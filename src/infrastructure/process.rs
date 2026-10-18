use std::collections::HashSet;
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const READ_CHUNK_BYTES: usize = 8192;
const CANCEL_RELEASE_LIMIT: Duration = Duration::from_secs(2);
const INVALID_TIMEOUT_MESSAGE: &str = "tempo limite inválido";

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ProcessError {
    message: String,
    program_not_found: bool,
}

impl ProcessError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            program_not_found: false,
        }
    }

    pub fn program_not_found(&self) -> bool {
        self.program_not_found
    }
}

#[cfg(unix)]
const LOWEST_SAFE_PROCESS_GROUP_ID: i32 = 2;
#[cfg(unix)]
const ABSOLUTE_KILL_PATH: &str = "/bin/kill";

#[derive(Default)]
struct CancelState {
    cancelled: AtomicBool,
    /// Invariant: a pid is listed only while its child is unreaped. Reaping, signalling
    /// and forgetting happen under this lock, so a listed pid is never a recycled group id.
    active_children: Mutex<HashSet<u32>>,
    released: Condvar,
}

#[derive(Clone, Default)]
pub struct CancelFlag(Arc<CancelState>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.request_cancel();
        let children = self.lock_active_children();
        let _ = self
            .0
            .released
            .wait_timeout_while(children, CANCEL_RELEASE_LIMIT, |children| {
                !children.is_empty()
            });
    }

    pub fn request_cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
        for pid in self.lock_active_children().iter() {
            signal_process_group(*pid);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
    }

    fn lock_active_children(&self) -> MutexGuard<'_, HashSet<u32>> {
        self.0
            .active_children
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn register_child(&self, pid: u32) {
        self.lock_active_children().insert(pid);
    }

    fn try_reap(&self, child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
        let mut children = self.lock_active_children();
        let status = child.try_wait()?;
        if status.is_some() {
            self.forget_child(&mut children, child.id());
        }
        Ok(status)
    }

    fn terminate(&self, child: &mut Child) {
        let mut children = self.lock_active_children();
        signal_process_group(child.id());
        let _ = child.kill();
        let _ = child.wait();
        self.forget_child(&mut children, child.id());
    }

    fn forget_child(&self, children: &mut HashSet<u32>, pid: u32) {
        children.remove(&pid);
        self.0.released.notify_all();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub maximum_output_bytes: usize,
}

struct CapturedOutput {
    receiver: mpsc::Receiver<Vec<u8>>,
    overflowed: Arc<AtomicBool>,
}

pub fn run_process(
    mut command: Command,
    stdin_text: Option<&str>,
    limits: &ProcessLimits,
    cancel: &CancelFlag,
) -> Result<String, ProcessError> {
    if cancel.is_cancelled() {
        return Err(cancelled_error());
    }
    let program = command.get_program().to_string_lossy().to_string();
    let deadline = Instant::now()
        .checked_add(limits.timeout)
        .ok_or_else(|| ProcessError::new(INVALID_TIMEOUT_MESSAGE))?;
    isolate_process_group(&mut command);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ProcessError {
            message: format!("não foi possível iniciar '{program}': {error}"),
            program_not_found: error.kind() == std::io::ErrorKind::NotFound,
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        let text = stdin_text.map(str::to_string);
        thread::spawn(move || {
            if let Some(text) = text {
                let _ = stdin.write_all(text.as_bytes());
            }
        });
    }
    cancel.register_child(child.id());
    let stdout = capture(child.stdout.take(), limits.maximum_output_bytes, true);
    let stderr = capture(child.stderr.take(), limits.maximum_output_bytes, false);
    let status = wait_for_exit(&mut child, &stdout, limits, deadline, cancel)?;

    let stdout_bytes = join_output(stdout, limits, deadline, cancel)?;
    let stderr_bytes = join_output(stderr, limits, deadline, cancel)?;
    if !status.success() {
        let message = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        return Err(ProcessError::new(if message.is_empty() {
            format!("processo terminou com falha ({status})")
        } else {
            message
        }));
    }
    String::from_utf8(stdout_bytes)
        .map_err(|_| ProcessError::new("saída do processo não é UTF-8 válido".to_string()))
}

fn wait_for_exit(
    child: &mut Child,
    stdout: &CapturedOutput,
    limits: &ProcessLimits,
    deadline: Instant,
    cancel: &CancelFlag,
) -> Result<ExitStatus, ProcessError> {
    loop {
        if stdout.overflowed.load(Ordering::SeqCst) {
            cancel.terminate(child);
            return Err(overflow_error(limits));
        }
        match cancel.try_reap(child) {
            Ok(Some(status)) if status.success() || !cancel.is_cancelled() => return Ok(status),
            Ok(Some(_)) => return Err(cancelled_error()),
            Ok(None) if cancel.is_cancelled() => {
                cancel.terminate(child);
                return Err(cancelled_error());
            }
            Ok(None) if Instant::now() >= deadline => {
                cancel.terminate(child);
                return Err(timeout_error(limits));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                cancel.terminate(child);
                return Err(ProcessError::new(format!(
                    "falha ao aguardar o processo: {error}"
                )));
            }
        }
    }
}

fn overflow_error(limits: &ProcessLimits) -> ProcessError {
    ProcessError::new(format!(
        "saída excedeu o limite de {} bytes",
        limits.maximum_output_bytes
    ))
}

fn cancelled_error() -> ProcessError {
    ProcessError::new("processo cancelado")
}

fn timeout_error(limits: &ProcessLimits) -> ProcessError {
    ProcessError::new(format!(
        "processo excedeu o tempo limite de {}s",
        limits.timeout.as_secs()
    ))
}

#[cfg(unix)]
fn isolate_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn isolate_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn signal_process_group(pid: u32) {
    if let Some(target) = process_group_argument(pid) {
        let _ = kill_command().args(["-KILL", "--", &target]).output();
    }
}

#[cfg(unix)]
fn kill_command() -> Command {
    let absolute = std::path::Path::new(ABSOLUTE_KILL_PATH);
    let program = if absolute.exists() {
        ABSOLUTE_KILL_PATH
    } else {
        "kill"
    };
    Command::new(program)
}

#[cfg(not(unix))]
fn signal_process_group(_pid: u32) {}

#[cfg(unix)]
fn process_group_argument(pid: u32) -> Option<String> {
    let group_id = i32::try_from(pid).ok()?;
    (group_id >= LOWEST_SAFE_PROCESS_GROUP_ID).then(|| format!("-{group_id}"))
}

fn capture(
    stream: Option<impl Read + Send + 'static>,
    maximum_bytes: usize,
    enforce_limit: bool,
) -> CapturedOutput {
    let (sender, receiver) = mpsc::channel();
    let overflowed = Arc::new(AtomicBool::new(false));
    let overflow_flag = Arc::clone(&overflowed);
    thread::spawn(move || {
        let mut collected = Vec::new();
        let Some(mut stream) = stream else {
            let _ = sender.send(collected);
            return;
        };
        let mut chunk = [0u8; READ_CHUNK_BYTES];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if collected.len() + count > maximum_bytes {
                        if enforce_limit {
                            overflow_flag.store(true, Ordering::SeqCst);
                            break;
                        }
                        continue;
                    }
                    collected.extend_from_slice(&chunk[..count]);
                }
            }
        }
        let _ = sender.send(collected);
    });
    CapturedOutput {
        receiver,
        overflowed,
    }
}

fn join_output(
    output: CapturedOutput,
    limits: &ProcessLimits,
    deadline: Instant,
    cancel: &CancelFlag,
) -> Result<Vec<u8>, ProcessError> {
    let bytes = loop {
        if cancel.is_cancelled() {
            return Err(cancelled_error());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timeout_error(limits));
        }
        match output.receiver.recv_timeout(remaining.min(POLL_INTERVAL)) {
            Ok(bytes) => break bytes,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ProcessError::new("falha ao ler a saída do processo"));
            }
        }
    };
    if output.overflowed.load(Ordering::SeqCst) {
        return Err(overflow_error(limits));
    }
    Ok(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    const WAIT_LIMIT: Duration = Duration::from_secs(5);
    const CANCEL_DELAY: Duration = Duration::from_millis(100);
    const SPAWN_CHECK_INTERVAL: Duration = Duration::from_millis(10);
    const DESCENDANT_CHECK_INTERVAL: Duration = Duration::from_millis(20);
    const DESCENDANT_EXIT_LIMIT: Duration = Duration::from_secs(5);
    const GRANDCHILD_PIPE_HOLD_SECONDS: u64 = 3;
    const PIPE_HOLD_CANCEL_BUDGET: Duration = Duration::from_secs(1);
    const IDLE_CANCEL_BUDGET: Duration = Duration::from_millis(100);

    fn wait_until(condition: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + WAIT_LIMIT;
        while !condition() && Instant::now() < deadline {
            thread::sleep(SPAWN_CHECK_INTERVAL);
        }
        condition()
    }

    fn poison_active_children(flag: &CancelFlag) {
        let poisoner = flag.clone();
        let _ = thread::spawn(move || {
            let _guard = poisoner.0.active_children.lock().unwrap();
            panic!("envenenando o mutex de cancelamento");
        })
        .join();
    }

    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    fn run(
        command: Command,
        stdin_text: Option<&str>,
        limits: &ProcessLimits,
    ) -> Result<String, ProcessError> {
        run_process(command, stdin_text, limits, &CancelFlag::default())
    }

    fn limits(timeout_seconds: u64, maximum_output_bytes: usize) -> ProcessLimits {
        ProcessLimits {
            timeout: Duration::from_secs(timeout_seconds),
            maximum_output_bytes,
        }
    }

    fn grandchild_script(pid_file: &Path) -> String {
        format!("sleep 30 & echo $! > '{}'; wait", pid_file.display())
    }

    fn pid_file_is_written(path: &Path) -> bool {
        fs::read_to_string(path).is_ok_and(|content| content.ends_with('\n'))
    }

    fn cancel_when_ready(
        flag: &CancelFlag,
        ready: impl Fn() -> bool + Send + 'static,
    ) -> thread::JoinHandle<()> {
        let canceller = flag.clone();
        thread::spawn(move || {
            let deadline = Instant::now() + WAIT_LIMIT;
            while !ready() && Instant::now() < deadline {
                thread::sleep(SPAWN_CHECK_INTERVAL);
            }
            canceller.cancel();
        })
    }

    fn is_gone(pid: &str) -> bool {
        let signalable = kill_command()
            .args(["-0", pid])
            .output()
            .is_ok_and(|output| output.status.success());
        if !signalable {
            return true;
        }
        Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output()
            .is_ok_and(|output| {
                let state = String::from_utf8_lossy(&output.stdout);
                let state = state.trim();
                state.is_empty() || state.starts_with('Z')
            })
    }

    fn wait_until_gone(pid: &str) -> bool {
        let deadline = Instant::now() + DESCENDANT_EXIT_LIMIT;
        while Instant::now() < deadline {
            if is_gone(pid) {
                return true;
            }
            thread::sleep(DESCENDANT_CHECK_INTERVAL);
        }
        is_gone(pid)
    }

    fn read_grandchild_pid(pid_file: &Path) -> String {
        fs::read_to_string(pid_file).unwrap().trim().to_string()
    }

    #[test]
    fn cancel_kills_a_running_process_quickly() {
        let flag = CancelFlag::default();
        let canceller = flag.clone();
        let cancel_thread = thread::spawn(move || {
            thread::sleep(CANCEL_DELAY);
            canceller.cancel();
        });
        let started = Instant::now();
        let error =
            run_process(shell("exec sleep 30"), None, &limits(60, 1024), &flag).unwrap_err();
        cancel_thread.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(error.to_string().contains("cancelado"), "{error}");
    }

    #[test]
    fn cancel_kills_grandchildren() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let flag = CancelFlag::default();
        let ready_file = pid_file.clone();
        let cancel_thread = cancel_when_ready(&flag, move || pid_file_is_written(&ready_file));
        let error = run_process(
            shell(&grandchild_script(&pid_file)),
            None,
            &limits(60, 1024),
            &flag,
        )
        .unwrap_err();
        cancel_thread.join().unwrap();
        assert!(error.to_string().contains("cancelado"), "{error}");
        let grandchild = read_grandchild_pid(&pid_file);
        assert!(wait_until_gone(&grandchild));
    }

    #[cfg(target_os = "linux")]
    fn process_is_running(pid: &str) -> bool {
        fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(')')
                .is_some_and(|(_, state)| !state.trim_start().starts_with('Z'))
        })
    }

    fn active_child_count(flag: &CancelFlag) -> usize {
        flag.0
            .active_children
            .lock()
            .map_or(0, |children| children.len())
    }

    fn spawn_run(
        flag: &CancelFlag,
        pid_file: &Path,
    ) -> thread::JoinHandle<Result<String, ProcessError>> {
        let script = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());
        let runner_flag = flag.clone();
        thread::spawn(move || run_process(shell(&script), None, &limits(60, 1024), &runner_flag))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_stops_every_concurrent_run_sharing_the_flag() {
        let directory = tempfile::tempdir().unwrap();
        let first_pid_file = directory.path().join("first.txt");
        let second_pid_file = directory.path().join("second.txt");
        let flag = CancelFlag::default();
        let first = spawn_run(&flag, &first_pid_file);
        let second = spawn_run(&flag, &second_pid_file);
        assert!(
            wait_until(|| active_child_count(&flag) == 2),
            "dois processos deveriam estar rastreados"
        );
        let first_pid = read_grandchild_pid(&first_pid_file);
        let second_pid = read_grandchild_pid(&second_pid_file);

        let started = Instant::now();
        flag.cancel();

        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(
            !process_is_running(&first_pid),
            "primeiro processo sobreviveu"
        );
        assert!(
            !process_is_running(&second_pid),
            "segundo processo sobreviveu"
        );
        assert!(first.join().unwrap().is_err());
        assert!(second.join().unwrap().is_err());
    }

    #[test]
    fn reaped_children_leave_the_registry_on_every_exit_path() {
        let flag = CancelFlag::default();
        run_process(shell("exit 0"), None, &limits(5, 1024), &flag).unwrap();
        assert_eq!(active_child_count(&flag), 0);
        let _ = run_process(shell("exec sleep 30"), None, &limits(1, 1024), &flag);
        assert_eq!(active_child_count(&flag), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_returns_after_active_process_is_terminated() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let flag = CancelFlag::default();
        let script = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());
        let runner_flag = flag.clone();
        let runner = thread::spawn(move || {
            run_process(shell(&script), None, &limits(60, 1024), &runner_flag)
        });
        let deadline = Instant::now() + WAIT_LIMIT;
        while !(pid_file_is_written(&pid_file) && (active_child_count(&flag) > 0))
            && Instant::now() < deadline
        {
            thread::sleep(SPAWN_CHECK_INTERVAL);
        }
        let pid = read_grandchild_pid(&pid_file);
        assert!(process_is_running(&pid));

        flag.cancel();

        assert!(
            !process_is_running(&pid),
            "cancel() deve retornar após o término do processo"
        );
        assert!(runner.join().unwrap().is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_does_not_wait_for_pipe_held_after_the_child_exited() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let flag = CancelFlag::default();
        let script = format!(
            "setsid sleep {GRANDCHILD_PIPE_HOLD_SECONDS} & echo $$ > '{}'; sleep 0.2",
            pid_file.display()
        );
        let runner_flag = flag.clone();
        let runner = thread::spawn(move || {
            run_process(shell(&script), None, &limits(60, 1024), &runner_flag)
        });
        wait_until(|| pid_file_is_written(&pid_file));
        let child = read_grandchild_pid(&pid_file);
        wait_until(|| !process_is_running(&child));

        let started = Instant::now();
        flag.cancel();
        let elapsed = started.elapsed();

        assert!(
            elapsed < PIPE_HOLD_CANCEL_BUDGET,
            "cancel() esperou {elapsed:?} pelo neto que segura o pipe"
        );
        assert!(runner.join().unwrap().is_err());
    }

    #[test]
    fn cancel_without_active_process_returns_immediately() {
        let flag = CancelFlag::default();

        let started = Instant::now();
        flag.cancel();

        assert!(started.elapsed() < IDLE_CANCEL_BUDGET);
        assert!(flag.is_cancelled());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_still_kills_the_process_after_the_lock_is_poisoned() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let flag = CancelFlag::default();
        let script = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());
        let runner_flag = flag.clone();
        let runner = thread::spawn(move || {
            run_process(shell(&script), None, &limits(60, 1024), &runner_flag)
        });
        wait_until(|| pid_file_is_written(&pid_file) && (active_child_count(&flag) > 0));
        poison_active_children(&flag);
        let pid = read_grandchild_pid(&pid_file);

        flag.cancel();

        assert!(!process_is_running(&pid));
        assert!(runner.join().unwrap().is_err());
    }

    #[test]
    fn run_after_cancel_fails_without_spawning() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("spawned.txt");
        let flag = CancelFlag::default();
        flag.cancel();
        let script = format!("touch '{}'", marker.display());
        let error = run_process(shell(&script), None, &limits(5, 1024), &flag).unwrap_err();
        assert!(error.to_string().contains("cancelado"), "{error}");
        assert!(!marker.exists());
    }

    #[test]
    fn timeout_also_kills_grandchildren() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let started = Instant::now();
        let error = run(shell(&grandchild_script(&pid_file)), None, &limits(2, 1024)).unwrap_err();
        assert!(started.elapsed() < WAIT_LIMIT);
        assert!(error.to_string().contains("tempo"), "{error}");
        assert!(wait_until_gone(&read_grandchild_pid(&pid_file)));
    }

    #[test]
    fn process_group_argument_never_targets_unsafe_ids() {
        assert_eq!(process_group_argument(0), None);
        assert_eq!(process_group_argument(1), None);
        assert_eq!(process_group_argument(u32::MAX), None);
        assert_eq!(process_group_argument(1234), Some("-1234".to_string()));
    }

    #[test]
    fn run_process_returns_stdout() {
        let output = run(shell("printf olá"), None, &limits(5, 1024)).unwrap();
        assert_eq!(output, "olá");
    }

    #[test]
    fn run_process_enforces_custom_output_limit() {
        let error = run(
            shell("head -c 1000 /dev/zero | tr '\\0' 'a'"),
            None,
            &limits(5, 100),
        )
        .unwrap_err();
        assert!(error.to_string().contains("excedeu o limite"), "{error}");
        assert!(error.to_string().contains("100"), "{error}");
    }

    #[test]
    fn run_process_passes_stdin() {
        let output = run(shell("cat"), Some("entrada"), &limits(5, 1024)).unwrap();
        assert_eq!(output, "entrada");
    }

    #[test]
    fn run_process_times_out() {
        let started = std::time::Instant::now();
        let error = run(shell("exec sleep 30"), None, &limits(1, 1024)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(error.to_string().contains("tempo"), "{error}");
    }

    #[test]
    fn run_process_times_out_even_when_stdin_is_never_read() {
        let started = std::time::Instant::now();
        let input = "a".repeat(300_000);
        let error = run(shell("exec sleep 30"), Some(&input), &limits(1, 1024)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(error.to_string().contains("tempo"), "{error}");
    }

    #[test]
    fn run_process_flags_a_missing_program() {
        let error = run(
            Command::new("programa-que-nao-existe-xyz"),
            None,
            &limits(5, 1024),
        )
        .unwrap_err();
        assert!(error.program_not_found());
        let failure = run(shell("exit 3"), None, &limits(5, 1024)).unwrap_err();
        assert!(!failure.program_not_found());
    }

    #[test]
    fn run_process_reports_stderr_on_failure() {
        let error = run(shell("echo boom >&2; exit 3"), None, &limits(5, 1024)).unwrap_err();
        assert!(error.to_string().contains("boom"));
    }

    #[test]
    fn descendant_holding_the_pipe_does_not_block_past_the_timeout() {
        let started = Instant::now();
        let error = run(
            shell("setsid sleep 30 & echo pronto"),
            None,
            &limits(1, 1024),
        )
        .unwrap_err();
        assert!(started.elapsed() < WAIT_LIMIT, "{:?}", started.elapsed());
        assert!(error.to_string().contains("tempo"), "{error}");
    }

    #[test]
    fn unrepresentable_timeout_is_an_error_not_a_panic() {
        let limits = ProcessLimits {
            timeout: Duration::MAX,
            maximum_output_bytes: 1024,
        };
        let error = run(shell("exit 0"), None, &limits).unwrap_err();
        assert!(
            error.to_string().contains("tempo limite inválido"),
            "{error}"
        );
    }
}
