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
#[error("{0}")]
pub struct ProcessError(String);

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
) -> Result<String, ProcessError> {
    let program = command.get_program().to_string_lossy().to_string();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ProcessError(format!("não foi possível iniciar '{program}': {error}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(text) = stdin_text {
            let _ = stdin.write_all(text.as_bytes());
        }
    }
    let stdout = capture(child.stdout.take(), limits.maximum_output_bytes, true);
    let stderr = capture(child.stderr.take(), limits.maximum_output_bytes, false);
    let status = wait_for_exit(&mut child, &stdout, limits)?;

    let stdout_bytes = join_output(stdout, limits)?;
    let stderr_bytes = join_output(stderr, limits)?;
    if !status.success() {
        let message = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        return Err(ProcessError(if message.is_empty() {
            format!("processo terminou com falha ({status})")
        } else {
            message
        }));
    }
    String::from_utf8(stdout_bytes)
        .map_err(|_| ProcessError("saída do processo não é UTF-8 válido".to_string()))
}

fn wait_for_exit(
    child: &mut Child,
    stdout: &CapturedOutput,
    limits: &ProcessLimits,
) -> Result<ExitStatus, ProcessError> {
    let deadline = Instant::now() + limits.timeout;
    loop {
        if stdout.overflowed.load(Ordering::SeqCst) {
            terminate(child);
            return Err(overflow_error(limits));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                terminate(child);
                return Err(ProcessError(format!(
                    "processo excedeu o tempo limite de {}s",
                    limits.timeout.as_secs()
                )));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                return Err(ProcessError(format!(
                    "falha ao aguardar o processo: {error}"
                )))
            }
        }
    }
}

fn overflow_error(limits: &ProcessLimits) -> ProcessError {
    ProcessError(format!(
        "saída excedeu o limite de {} bytes",
        limits.maximum_output_bytes
    ))
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
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
        .map_err(|_| ProcessError("falha ao ler a saída do processo".to_string()))?;
    if output.overflowed.load(Ordering::SeqCst) {
        return Err(overflow_error(limits));
    }
    Ok(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    fn limits(timeout_seconds: u64, maximum_output_bytes: usize) -> ProcessLimits {
        ProcessLimits {
            timeout: Duration::from_secs(timeout_seconds),
            maximum_output_bytes,
        }
    }

    #[test]
    fn run_process_returns_stdout() {
        let output = run_process(shell("printf olá"), None, &limits(5, 1024)).unwrap();
        assert_eq!(output, "olá");
    }

    #[test]
    fn run_process_enforces_custom_output_limit() {
        let error = run_process(
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
        let output = run_process(shell("cat"), Some("entrada"), &limits(5, 1024)).unwrap();
        assert_eq!(output, "entrada");
    }

    #[test]
    fn run_process_times_out() {
        let started = std::time::Instant::now();
        let error = run_process(shell("exec sleep 30"), None, &limits(1, 1024)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(error.to_string().contains("tempo"), "{error}");
    }

    #[test]
    fn run_process_reports_stderr_on_failure() {
        let error =
            run_process(shell("echo boom >&2; exit 3"), None, &limits(5, 1024)).unwrap_err();
        assert!(error.to_string().contains("boom"));
    }
}
