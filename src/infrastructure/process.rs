use std::collections::HashSet;
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::application::ports::Cancellable;
use thiserror::Error;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const READ_CHUNK_BYTES: usize = 8192;
const MAXIMUM_ERROR_BYTES: usize = 16_384;
const CANCEL_RELEASE_LIMIT: Duration = Duration::from_secs(2);
#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("não foi possível iniciar '{program}': {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("falha ao aguardar o processo: {0}")]
    Wait(#[source] std::io::Error),
    #[error("processo excedeu o tempo limite de {limit_seconds}s")]
    TimedOut { limit_seconds: u64 },
    #[error("saída excedeu o limite de {limit_bytes} bytes")]
    OutputTooLarge { limit_bytes: usize },
    #[error("processo cancelado")]
    Cancelled,
    #[error("tempo limite inválido")]
    InvalidTimeout,
    #[error("falha ao ler a saída do processo")]
    OutputUnreadable,
    #[error("{}", failure_description(.status, .stderr))]
    ExitedWithFailure { status: ExitStatus, stderr: String },
    #[error("saída do processo não é UTF-8 válido")]
    InvalidUtf8,
}

impl ProcessError {
    pub fn program_not_found(&self) -> bool {
        matches!(self, Self::Spawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    }
}

fn failure_description(status: &ExitStatus, stderr: &str) -> String {
    if stderr.is_empty() {
        format!("processo terminou com falha ({status})")
    } else {
        stderr.to_string()
    }
}

#[cfg(unix)]
const LOWEST_SAFE_PROCESS_GROUP_ID: i32 = 2;
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

pub trait HasCancelFlag {
    fn cancel_flag(&self) -> &CancelFlag;
}

impl<T: HasCancelFlag + Send + Sync> Cancellable for T {
    fn cancel(&self) {
        self.cancel_flag().cancel();
    }

    fn request_cancel(&self) {
        self.cancel_flag().request_cancel();
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
    command: Command,
    stdin_text: Option<&str>,
    limits: &ProcessLimits,
    cancel: &CancelFlag,
) -> Result<String, ProcessError> {
    if cancel.is_cancelled() {
        return Err(ProcessError::Cancelled);
    }
    let deadline = deadline_after(limits.timeout)?;
    let mut child = spawn_isolated(command, stdin_text.is_some())?;
    feed_stdin(&mut child, stdin_text);
    cancel.register_child(child.id());
    let stdout = capture(
        child.stdout.take(),
        limits.maximum_output_bytes,
        OverflowPolicy::Fail,
    );
    let stderr = capture(
        child.stderr.take(),
        limits.maximum_output_bytes.min(MAXIMUM_ERROR_BYTES),
        OverflowPolicy::Truncate,
    );
    let group_id = child.id();
    let status = wait_for_exit(&mut child, &stdout, limits, deadline, cancel)?;

    let stdout_bytes = join_output(stdout, limits, deadline, cancel)
        .inspect_err(|_| signal_process_group(group_id))?;
    let stderr_bytes = join_output(stderr, limits, deadline, cancel)
        .inspect_err(|_| signal_process_group(group_id))?;
    if !status.success() {
        return Err(failure_error(status, &stderr_bytes));
    }
    String::from_utf8(stdout_bytes).map_err(|_| ProcessError::InvalidUtf8)
}

fn deadline_after(timeout: Duration) -> Result<Instant, ProcessError> {
    Instant::now()
        .checked_add(timeout)
        .ok_or(ProcessError::InvalidTimeout)
}

fn spawn_isolated(mut command: Command, has_stdin: bool) -> Result<Child, ProcessError> {
    let program = command.get_program().to_string_lossy().to_string();
    isolate_process_group(&mut command);
    command
        .stdin(if has_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ProcessError::Spawn { program, source })
}

fn feed_stdin(child: &mut Child, stdin_text: Option<&str>) {
    if let (Some(text), Some(mut stdin)) = (stdin_text.map(str::to_string), child.stdin.take()) {
        thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
}

fn failure_error(status: ExitStatus, stderr_bytes: &[u8]) -> ProcessError {
    let stderr = String::from_utf8_lossy(stderr_bytes).trim().to_string();
    ProcessError::ExitedWithFailure { status, stderr }
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
            Ok(Some(_)) => return Err(ProcessError::Cancelled),
            Ok(None) if cancel.is_cancelled() => {
                cancel.terminate(child);
                return Err(ProcessError::Cancelled);
            }
            Ok(None) if Instant::now() >= deadline => {
                cancel.terminate(child);
                return Err(timeout_error(limits));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                cancel.terminate(child);
                return Err(ProcessError::Wait(error));
            }
        }
    }
}

fn overflow_error(limits: &ProcessLimits) -> ProcessError {
    ProcessError::OutputTooLarge {
        limit_bytes: limits.maximum_output_bytes,
    }
}

fn timeout_error(limits: &ProcessLimits) -> ProcessError {
    ProcessError::TimedOut {
        limit_seconds: limits.timeout.as_secs(),
    }
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
    if let Some(group) = safe_process_group(pid) {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
}

#[cfg(unix)]
fn safe_process_group(pid: u32) -> Option<rustix::process::Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|group_id| *group_id >= LOWEST_SAFE_PROCESS_GROUP_ID)
        .and_then(rustix::process::Pid::from_raw)
}

#[cfg(not(unix))]
fn signal_process_group(_pid: u32) {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverflowPolicy {
    Fail,
    Truncate,
}

fn capture(
    stream: Option<impl Read + Send + 'static>,
    maximum_bytes: usize,
    policy: OverflowPolicy,
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
                        if policy == OverflowPolicy::Fail {
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
            return Err(ProcessError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timeout_error(limits));
        }
        match output.receiver.recv_timeout(remaining.min(POLL_INTERVAL)) {
            Ok(bytes) => break bytes,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ProcessError::OutputUnreadable);
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
    const LEADER_REAP_MARGIN: Duration = Duration::from_millis(200);
    const IDLE_CANCEL_BUDGET: Duration = Duration::from_millis(100);

    #[test]
    fn types_exposing_a_cancel_flag_are_cancellable() {
        use crate::application::ports::Cancellable;

        struct Holder(CancelFlag);

        impl HasCancelFlag for Holder {
            fn cancel_flag(&self) -> &CancelFlag {
                &self.0
            }
        }

        let holder = Holder(CancelFlag::default());
        Cancellable::request_cancel(&holder);
        assert!(holder.0.is_cancelled());
    }

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

    #[test]
    fn failure_error_prefers_stderr_and_falls_back_to_the_status() {
        let status = shell("exit 3").status().unwrap();

        assert_eq!(failure_error(status, b"  boom \n").to_string(), "boom");
        assert!(failure_error(status, b"")
            .to_string()
            .contains("processo terminou com falha"));
    }

    #[test]
    fn deadline_after_rejects_an_unrepresentable_timeout() {
        assert!(deadline_after(Duration::MAX).is_err());
        assert!(deadline_after(Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn capture_truncate_discards_oversized_chunks_and_fail_flags_the_overflow() {
        let data = vec![b'a'; READ_CHUNK_BYTES * 2];

        let truncated = capture(
            Some(std::io::Cursor::new(data.clone())),
            10,
            OverflowPolicy::Truncate,
        );
        assert!(truncated
            .receiver
            .recv_timeout(WAIT_LIMIT)
            .unwrap()
            .is_empty());
        assert!(!truncated.overflowed.load(Ordering::SeqCst));

        let failed = capture(Some(std::io::Cursor::new(data)), 10, OverflowPolicy::Fail);
        failed.receiver.recv_timeout(WAIT_LIMIT).unwrap();
        assert!(failed.overflowed.load(Ordering::SeqCst));
    }

    fn is_gone(pid: &str) -> bool {
        let signalable = Command::new("kill")
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
    fn timeout_kills_group_members_after_leader_exits() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let script = format!("sleep 30 & echo $! > '{}'; echo '{{}}'", pid_file.display());
        let error = run(shell(&script), None, &limits(1, 1024)).unwrap_err();
        assert!(error.to_string().contains("tempo"), "{error}");
        assert!(wait_until_gone(&read_grandchild_pid(&pid_file)));
    }

    #[test]
    fn signal_process_group_kills_the_whole_group_without_an_external_program() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let mut command = shell(&grandchild_script(&pid_file));
        isolate_process_group(&mut command);
        let mut leader = command.spawn().unwrap();
        assert!(wait_until(|| pid_file_is_written(&pid_file)));
        signal_process_group(leader.id());
        leader.wait().unwrap();
        assert!(wait_until_gone(&read_grandchild_pid(&pid_file)));
    }

    fn orphan_script(pid_file: &Path, tail: &str) -> String {
        format!("sleep 30 & echo $! > '{}'; {tail}", pid_file.display())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_kills_descendants_that_outlive_the_leader() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let leader_file = directory.path().join("leader.txt");
        let script = format!(
            "echo $$ > '{}'; {}",
            leader_file.display(),
            orphan_script(&pid_file, "echo pronto")
        );
        let flag = CancelFlag::default();
        let ready_pid_file = pid_file.clone();
        let ready_leader_file = leader_file.clone();
        let cancel_thread = cancel_when_ready(&flag, move || {
            let leader_exited = pid_file_is_written(&ready_pid_file)
                && pid_file_is_written(&ready_leader_file)
                && !process_is_running(&read_grandchild_pid(&ready_leader_file));
            if leader_exited {
                thread::sleep(LEADER_REAP_MARGIN);
            }
            leader_exited
        });
        let error = run_process(shell(&script), None, &limits(60, 1024), &flag).unwrap_err();
        cancel_thread.join().unwrap();
        assert!(error.to_string().contains("cancelado"), "{error}");
        assert!(wait_until_gone(&read_grandchild_pid(&pid_file)));
    }

    #[test]
    fn overflow_kills_descendants_that_outlive_the_leader() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let script = orphan_script(&pid_file, "head -c 2000 /dev/zero | tr '\\0' a");
        let error = run(shell(&script), None, &limits(5, 100)).unwrap_err();
        assert!(error.to_string().contains("excedeu o limite"), "{error}");
        assert!(wait_until_gone(&read_grandchild_pid(&pid_file)));
    }

    #[test]
    fn successful_run_does_not_kill_detached_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let script = format!(
            "sleep 30 >/dev/null 2>&1 </dev/null & echo $! > '{}'; echo pronto",
            pid_file.display()
        );
        let output = run(shell(&script), None, &limits(5, 1024)).unwrap();
        assert_eq!(output.trim(), "pronto");
        let detached = read_grandchild_pid(&pid_file);
        assert!(
            !is_gone(&detached),
            "o descendente foi morto no caminho de sucesso"
        );
        let _ = Command::new("kill").args(["-KILL", &detached]).output();
    }

    #[test]
    fn safe_process_group_never_targets_unsafe_ids() {
        assert!(safe_process_group(0).is_none());
        assert!(safe_process_group(1).is_none());
        assert!(safe_process_group(u32::MAX).is_none());
        assert!(safe_process_group(1234).is_some());
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
    fn stderr_capture_is_capped_below_the_stdout_limit() {
        let error = run(
            shell("head -c 100000 /dev/zero | tr '\\0' a >&2; exit 1"),
            None,
            &limits(5, 1_048_576),
        )
        .unwrap_err();
        assert!(
            error.to_string().len() <= MAXIMUM_ERROR_BYTES,
            "{}",
            error.to_string().len()
        );
    }

    #[test]
    fn run_process_without_stdin_text_gives_the_child_a_closed_stdin() {
        let started = Instant::now();
        let output = run(shell("cat"), None, &limits(5, 1024)).unwrap();
        assert_eq!(output, "");
        assert!(started.elapsed() < Duration::from_secs(2));
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
        assert!(error.program_not_found(), "{error}");
        let failure = run(shell("exit 3"), None, &limits(5, 1024)).unwrap_err();
        assert!(
            matches!(failure, ProcessError::ExitedWithFailure { .. }),
            "{failure}"
        );
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

    #[test]
    fn failures_are_classified_by_variant() {
        let timed_out = run(shell("exec sleep 30"), None, &limits(1, 1024)).unwrap_err();
        assert!(matches!(
            timed_out,
            ProcessError::TimedOut { limit_seconds: 1 }
        ));

        let too_large = run(
            shell("head -c 1000 /dev/zero | tr '\\0' 'a'"),
            None,
            &limits(5, 100),
        )
        .unwrap_err();
        assert!(matches!(
            too_large,
            ProcessError::OutputTooLarge { limit_bytes: 100 }
        ));

        let failed = run(shell("echo boom >&2; exit 3"), None, &limits(5, 1024)).unwrap_err();
        assert!(matches!(failed, ProcessError::ExitedWithFailure { .. }));

        let invalid = run(shell("printf '\\377\\376'"), None, &limits(5, 1024)).unwrap_err();
        assert!(matches!(invalid, ProcessError::InvalidUtf8));
    }

    #[test]
    fn spawn_failure_keeps_the_io_error_as_source() {
        use std::error::Error;
        let error = run(
            Command::new("programa-que-nao-existe-xyz"),
            None,
            &limits(5, 1024),
        )
        .unwrap_err();
        assert!(error.source().is_some());
        assert!(error.program_not_found());
    }
}
