use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::application::ports::{ActionExecutor, SourceError, WidgetSource};
use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::widget::WidgetSnapshot;
use crate::infrastructure::config::WidgetSettings;
use crate::infrastructure::plugin_protocol::parse_snapshot;

pub const MAXIMUM_PLUGIN_OUTPUT_BYTES: usize = 1_048_576;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const READ_CHUNK_BYTES: usize = 8192;

pub struct PluginRunner {
    settings: WidgetSettings,
    working_directory: PathBuf,
}

struct CapturedOutput {
    handle: JoinHandle<Vec<u8>>,
    overflowed: Arc<AtomicBool>,
}

impl PluginRunner {
    pub fn new(settings: WidgetSettings, working_directory: PathBuf) -> Self {
        Self {
            settings,
            working_directory,
        }
    }

    fn run(&self, extra_args: &[&str], stdin_text: Option<&str>) -> Result<String, SourceError> {
        let mut child = self.spawn(extra_args)?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Some(text) = stdin_text {
                let _ = stdin.write_all(text.as_bytes());
            }
        }
        let stdout = capture(child.stdout.take(), true);
        let stderr = capture(child.stderr.take(), false);
        let status = self.wait_for_exit(&mut child, &stdout)?;

        let stdout_bytes = join_output(stdout)?;
        let stderr_bytes = join_output(stderr)?;
        if !status.success() {
            let message = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            return Err(SourceError::Failed(if message.is_empty() {
                format!("plugin terminou com falha ({status})")
            } else {
                message
            }));
        }
        String::from_utf8(stdout_bytes)
            .map_err(|_| SourceError::Failed("saída do plugin não é UTF-8 válido".to_string()))
    }

    fn spawn(&self, extra_args: &[&str]) -> Result<Child, SourceError> {
        Command::new(&self.settings.command)
            .args(&self.settings.args)
            .args(extra_args)
            .envs(self.settings.env.iter().map(|(key, value)| (key, value)))
            .current_dir(&self.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                SourceError::Failed(format!(
                    "não foi possível iniciar '{}': {error}",
                    self.settings.command
                ))
            })
    }

    fn wait_for_exit(
        &self,
        child: &mut Child,
        stdout: &CapturedOutput,
    ) -> Result<std::process::ExitStatus, SourceError> {
        let deadline = Instant::now() + Duration::from_secs(self.settings.timeout_seconds);
        loop {
            if stdout.overflowed.load(Ordering::SeqCst) {
                terminate(child);
                return Err(SourceError::Failed(
                    "saída do plugin excedeu o limite de 1 MiB".to_string(),
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) if Instant::now() >= deadline => {
                    terminate(child);
                    return Err(SourceError::Failed(format!(
                        "plugin excedeu o tempo limite de {}s",
                        self.settings.timeout_seconds
                    )));
                }
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(error) => {
                    return Err(SourceError::Failed(format!(
                        "falha ao aguardar o plugin: {error}"
                    )))
                }
            }
        }
    }
}

impl WidgetSource for PluginRunner {
    fn snapshot(&self) -> Result<WidgetSnapshot, SourceError> {
        let output = self.run(&["snapshot"], None)?;
        parse_snapshot(&output).map_err(|error| SourceError::Failed(error.to_string()))
    }
}

impl ActionExecutor for PluginRunner {
    fn execute(&self, action: &ConfirmedAction) -> Result<ActionOutcome, SourceError> {
        let params = action.params_json_with_item();
        let output = self.run(&["action", action.action_id()], Some(&params))?;
        Ok(ActionOutcome {
            message: output.trim().to_string(),
        })
    }
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn capture(stream: Option<impl Read + Send + 'static>, enforce_limit: bool) -> CapturedOutput {
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
                    if collected.len() + count > MAXIMUM_PLUGIN_OUTPUT_BYTES {
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

fn join_output(output: CapturedOutput) -> Result<Vec<u8>, SourceError> {
    let bytes = output
        .handle
        .join()
        .map_err(|_| SourceError::Failed("falha ao ler a saída do plugin".to_string()))?;
    if output.overflowed.load(Ordering::SeqCst) {
        return Err(SourceError::Failed(
            "saída do plugin excedeu o limite de 1 MiB".to_string(),
        ));
    }
    Ok(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::domain::action::ProposedAction;
    use crate::domain::widget::{ActionScope, ActionSpec, WidgetId};

    fn runner(directory: &Path, script: &str, timeout_seconds: u64) -> PluginRunner {
        fs::write(directory.join("plugin.sh"), script).unwrap();
        let settings = WidgetSettings {
            id: WidgetId::new("test"),
            command: "sh".to_string(),
            args: vec!["plugin.sh".to_string()],
            env: vec![("KANBAN_DIR".to_string(), "/tmp/kanban".to_string())],
            interval_seconds: 30,
            timeout_seconds,
            span: 1,
            tab: false,
        };
        PluginRunner::new(settings, directory.to_path_buf())
    }

    fn snapshot_error(runner: &PluginRunner) -> String {
        runner.snapshot().unwrap_err().to_string()
    }

    #[test]
    fn valid_snapshot_is_returned() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            r#"printf '{"summary":{"title":"T","lines":[]}}'"#,
            5,
        );
        assert_eq!(runner.snapshot().unwrap().summary.title, "T");
    }

    #[test]
    fn nonzero_exit_reports_stderr() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "echo boom >&2; exit 3", 5);
        assert!(snapshot_error(&runner).contains("boom"));
    }

    #[test]
    fn slow_plugin_times_out_and_is_killed() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "exec sleep 30", 1);
        let started = Instant::now();
        let message = snapshot_error(&runner);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(message.contains("tempo"), "{message}");
    }

    #[test]
    fn output_over_one_mebibyte_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            "head -c 2000000 /dev/zero | tr '\\0' 'a'",
            5,
        );
        let started = Instant::now();
        let message = snapshot_error(&runner);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(message.contains("1 MiB"), "{message}");
    }

    #[test]
    fn invalid_utf8_output_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "printf '\\377\\376'", 5);
        assert!(snapshot_error(&runner).contains("UTF-8"));
    }

    #[test]
    fn action_receives_params_on_stdin_and_args() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            "echo \"$@\" > args.txt; cat > stdin.txt; echo feito",
            5,
        );
        let spec = ActionSpec {
            id: "move".to_string(),
            label: "Mover".to_string(),
            description: None,
            scope: ActionScope::Item,
            params_json: r#"{"to":"done"}"#.to_string(),
        };
        let action = ProposedAction::new(WidgetId::new("test"), spec, Some("a.md".to_string()))
            .unwrap()
            .confirm();

        let outcome = runner.execute(&action).unwrap();

        assert_eq!(outcome.message, "feito");
        let arguments = fs::read_to_string(directory.path().join("args.txt")).unwrap();
        assert_eq!(arguments.trim(), "action move");
        let stdin = fs::read_to_string(directory.path().join("stdin.txt")).unwrap();
        assert!(stdin.contains(r#""item_id":"a.md""#));
        assert!(stdin.contains(r#""to":"done""#));
    }

    #[test]
    fn plugin_runs_in_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            r#"printf '{"summary":{"title":"%s","lines":[]}}' "$(pwd -P)""#,
            5,
        );
        let expected = fs::canonicalize(directory.path()).unwrap();
        assert_eq!(
            runner.snapshot().unwrap().summary.title,
            expected.to_string_lossy()
        );
    }

    #[test]
    fn environment_variables_are_passed() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            r#"printf '{"summary":{"title":"%s","lines":[]}}' "$KANBAN_DIR""#,
            5,
        );
        assert_eq!(runner.snapshot().unwrap().summary.title, "/tmp/kanban");
    }

    #[test]
    fn missing_command_reports_spawn_failure() {
        let directory = tempfile::tempdir().unwrap();
        let mut runner = runner(directory.path(), "", 5);
        runner.settings.command = "comando-que-nao-existe-xyz".to_string();
        assert!(snapshot_error(&runner).contains("comando-que-nao-existe-xyz"));
    }
}
