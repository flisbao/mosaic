use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const READ_CHUNK_BYTES: usize = 8192;

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

#[derive(Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub maximum_output_bytes: usize,
}

struct CapturedOutput {
    handle: JoinHandle<Vec<u8>>,
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
    let stdout = capture(child.stdout.take(), limits.maximum_output_bytes, true);
    let stderr = capture(child.stderr.take(), limits.maximum_output_bytes, false);
    let status = wait_for_exit(&mut child, &stdout, limits, cancel)?;

    let stdout_bytes = join_output(stdout, limits)?;
    let stderr_bytes = join_output(stderr, limits)?;
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
    cancel: &CancelFlag,
) -> Result<ExitStatus, ProcessError> {
    let deadline = Instant::now() + limits.timeout;
    loop {
        if stdout.overflowed.load(Ordering::SeqCst) {
            terminate(child);
            return Err(overflow_error(limits));
        }
        if cancel.is_cancelled() {
            terminate(child);
            return Err(cancelled_error());
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                terminate(child);
                return Err(timeout_error(limits));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                return Err(ProcessError::new(format!(
                    "falha ao aguardar o processo: {error}"
                )))
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

fn terminate(child: &mut Child) {
    signal_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn signal_process_group(pid: u32) {
    if let Some(target) = process_group_argument(pid) {
        let _ = Command::new("kill").args(["-KILL", "--", &target]).output();
    }
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
    let overflowed = Arc::new(AtomicBool::new(false));
    let overflow_flag = Arc::clone(&overflowed);
    let handle = thread::spawn(move || {
        let mut collected = Vec::new();
        let Some(mut stream) = stream else {
            return collected;
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
        collected
    });
    CapturedOutput { handle, overflowed }
}

fn join_output(output: CapturedOutput, limits: &ProcessLimits) -> Result<Vec<u8>, ProcessError> {
    let bytes = output
        .handle
        .join()
        .map_err(|_| ProcessError::new("falha ao ler a saída do processo".to_string()))?;
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

    fn process_exists(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .status()
            .is_ok_and(|status| status.success())
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
        assert!(!process_exists(&grandchild));
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
        assert!(!process_exists(&read_grandchild_pid(&pid_file)));
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
}
