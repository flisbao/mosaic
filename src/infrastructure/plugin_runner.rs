use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, PoisonError};

use crate::application::launch_spec::ProviderCommand;
use crate::application::ports::{ActionExecutor, SourceError, WidgetSource};
use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::widget::WidgetSnapshot;
use crate::domain::widget_content::{clean_text_limited, MAXIMUM_TEXT_CHARACTERS};
use crate::infrastructure::plugin_protocol::parse_snapshot;
use crate::infrastructure::process::{
    run_process, CancelFlag, HasCancelFlag, ProcessError, ProcessLimits,
};

pub const MAXIMUM_PLUGIN_OUTPUT_BYTES: usize = 1_048_576;

const ITEM_ID_KEY: &str = "item_id";

pub struct PluginRunner {
    command: ProviderCommand,
    working_directory: PathBuf,
    cancel_flag: CancelFlag,
    invocation_lock: Mutex<()>,
}

impl PluginRunner {
    pub fn new(command: ProviderCommand, working_directory: PathBuf) -> Self {
        Self {
            command,
            working_directory,
            cancel_flag: CancelFlag::default(),
            invocation_lock: Mutex::new(()),
        }
    }

    fn run(&self, extra_args: &[&str], stdin_text: Option<&str>) -> Result<String, SourceError> {
        let _one_invocation_at_a_time = self
            .invocation_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut command = Command::new(&self.command.program);
        command
            .args(&self.command.args)
            .args(extra_args)
            .envs(self.command.env.iter().map(|(key, value)| (key, value)))
            .current_dir(&self.working_directory);
        let limits = ProcessLimits {
            timeout: self.command.timeout,
            maximum_output_bytes: MAXIMUM_PLUGIN_OUTPUT_BYTES,
        };
        run_process(command, stdin_text, &limits, &self.cancel_flag).map_err(SourceError::from)
    }

    pub fn raw_snapshot_output(&self) -> Result<String, SourceError> {
        self.run(&["snapshot"], None)
    }
}

impl WidgetSource for PluginRunner {
    fn snapshot(&self) -> Result<WidgetSnapshot, SourceError> {
        let output = self.raw_snapshot_output()?;
        parse_snapshot(&output).map_err(|error| SourceError::InvalidSnapshot(error.to_string()))
    }
}

impl HasCancelFlag for PluginRunner {
    fn cancel_flag(&self) -> &CancelFlag {
        &self.cancel_flag
    }
}

impl ActionExecutor for PluginRunner {
    fn execute(&self, action: &ConfirmedAction) -> Result<ActionOutcome, SourceError> {
        let params = action_parameters(action);
        let output = self.run(&["action", action.action_id()], Some(&params))?;
        Ok(ActionOutcome {
            message: bounded_message(&output),
        })
    }
}

fn bounded_message(text: &str) -> String {
    clean_text_limited(text, MAXIMUM_TEXT_CHARACTERS)
}

impl From<ProcessError> for SourceError {
    fn from(error: ProcessError) -> Self {
        let message = bounded_message(&error.to_string());
        match error {
            ProcessError::TimedOut { .. } => Self::TimedOut(message),
            ProcessError::OutputTooLarge { .. } => Self::OutputTooLarge(message),
            ProcessError::Cancelled => Self::Cancelled,
            _ => Self::Failed(message),
        }
    }
}

fn action_parameters(action: &ConfirmedAction) -> String {
    let mut parameters = match serde_json::from_str::<serde_json::Value>(action.params_json()) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    if let Some(item_id) = action.item_id() {
        parameters.insert(
            ITEM_ID_KEY.to_string(),
            serde_json::Value::String(item_id.to_string()),
        );
    }
    serde_json::Value::Object(parameters).to_string()
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::application::ports::Cancellable;
    use crate::domain::action::ProposedAction;
    use crate::domain::widget::{ActionScope, ActionSpec, WidgetId};

    fn runner(directory: &Path, script: &str, timeout_seconds: u64) -> PluginRunner {
        fs::write(directory.join("plugin.sh"), script).unwrap();
        let command = ProviderCommand {
            program: "sh".to_string(),
            args: vec!["plugin.sh".to_string()],
            env: vec![("KANBAN_DIR".to_string(), "/tmp/kanban".to_string())],
            timeout: Duration::from_secs(timeout_seconds),
        };
        PluginRunner::new(command, directory.to_path_buf())
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
    fn raw_snapshot_output_returns_the_plugin_text_unparsed() {
        let directory = tempfile::tempdir().unwrap();
        let valid = runner(directory.path(), r#"printf '{"summary":{"title":"T"}}'"#, 5);
        assert_eq!(
            valid.raw_snapshot_output().unwrap(),
            r#"{"summary":{"title":"T"}}"#
        );
        let invalid = runner(directory.path(), "printf 'isto não é json'", 5);
        assert_eq!(invalid.raw_snapshot_output().unwrap(), "isto não é json");
    }

    #[test]
    fn raw_snapshot_output_reports_execution_failures() {
        let directory = tempfile::tempdir().unwrap();
        let failing = runner(directory.path(), "echo boom >&2; exit 3", 5);
        assert!(failing
            .raw_snapshot_output()
            .unwrap_err()
            .to_string()
            .contains("boom"));
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
        assert!(message.contains("excedeu o limite"), "{message}");
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

    fn move_action(params: &str, item: Option<&str>) -> ConfirmedAction {
        let spec = ActionSpec {
            id: "move".to_string(),
            label: "Mover".to_string(),
            description: None,
            scope: if item.is_some() {
                ActionScope::Item
            } else {
                ActionScope::Widget
            },
            params_json: params.to_string(),
        };
        ProposedAction::new(WidgetId::new("test"), spec, item.map(str::to_string))
            .unwrap()
            .confirm()
    }

    #[test]
    fn action_parameters_override_an_existing_item_id_with_a_single_key() {
        let json = action_parameters(&move_action(
            r#"{"to":"done","item_id":"velho"}"#,
            Some("novo.md"),
        ));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["item_id"], "novo.md");
        assert_eq!(value["to"], "done");
        assert_eq!(json.matches("item_id").count(), 1, "{json}");
    }

    #[test]
    fn action_parameters_escape_the_item_id() {
        let json = action_parameters(&move_action("{}", Some(r#"a"b\c"#)));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["item_id"], r#"a"b\c"#);
    }

    #[test]
    fn action_parameters_without_item_keep_the_params() {
        let json = action_parameters(&move_action(r#"{"x":1}"#, None));
        assert_eq!(json, r#"{"x":1}"#);
    }

    #[test]
    fn action_parameters_ignore_non_object_params() {
        assert_eq!(action_parameters(&move_action("[1]", None)), "{}");
    }

    #[test]
    fn huge_stderr_becomes_a_bounded_message() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            "head -c 100000 /dev/zero | tr '\\0' a >&2; exit 1",
            5,
        );
        let message = snapshot_error(&runner);
        assert!(
            message.chars().count() <= MAXIMUM_TEXT_CHARACTERS,
            "{} caracteres",
            message.chars().count()
        );
    }

    #[test]
    fn control_and_bidi_characters_are_removed_from_stderr() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(
            directory.path(),
            "printf 'falha\\342\\200\\256oculta\\n' >&2; exit 1",
            5,
        );
        let message = snapshot_error(&runner);
        assert!(!message.contains('\u{202e}'), "{message:?}");
        assert!(message.contains("falha"), "{message:?}");
    }

    #[test]
    fn huge_action_output_becomes_a_bounded_message() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "head -c 100000 /dev/zero | tr '\\0' a", 5);
        let spec = ActionSpec {
            id: "move".to_string(),
            label: "Mover".to_string(),
            description: None,
            scope: ActionScope::Widget,
            params_json: "{}".to_string(),
        };
        let action = ProposedAction::new(WidgetId::new("test"), spec, None)
            .unwrap()
            .confirm();
        let outcome = runner.execute(&action).unwrap();
        assert!(outcome.message.chars().count() <= MAXIMUM_TEXT_CHARACTERS);
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
    fn plugin_runner_cancel_delegates_to_flag() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "exec sleep 30", 60);
        runner.cancel();
        assert!(runner.cancel_flag.is_cancelled());
        assert!(snapshot_error(&runner).contains("cancelado"));
    }

    #[test]
    fn missing_command_reports_spawn_failure() {
        let directory = tempfile::tempdir().unwrap();
        let mut runner = runner(directory.path(), "", 5);
        runner.command.program = "comando-que-nao-existe-xyz".to_string();
        assert!(snapshot_error(&runner).contains("comando-que-nao-existe-xyz"));
    }

    #[test]
    fn timeout_maps_to_the_timed_out_source_error() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "exec sleep 30", 1);
        assert!(matches!(
            runner.snapshot().unwrap_err(),
            SourceError::TimedOut(_)
        ));
    }

    #[test]
    fn malformed_json_maps_to_the_invalid_snapshot_source_error() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "printf 'isto não é json'", 5);
        assert!(matches!(
            runner.snapshot().unwrap_err(),
            SourceError::InvalidSnapshot(_)
        ));
    }

    #[test]
    fn cancel_maps_to_the_cancelled_source_error() {
        let directory = tempfile::tempdir().unwrap();
        let runner = runner(directory.path(), "exec sleep 30", 60);
        runner.cancel();
        assert_eq!(runner.snapshot().unwrap_err(), SourceError::Cancelled);
    }

    #[test]
    fn snapshot_and_action_never_overlap() {
        let directory = tempfile::tempdir().unwrap();
        let script = r#"if ! mkdir busy.lock 2>/dev/null; then echo overlap >> overlaps.txt; fi
sleep 0.3
rmdir busy.lock 2>/dev/null
case "$1" in
  snapshot) printf '{"summary":{"title":"T","lines":[]} }';;
  *) cat > /dev/null; echo feito;;
esac"#;
        let runner = Arc::new(runner(directory.path(), script, 5));
        let snapshot_runner = Arc::clone(&runner);
        let snapshot_thread = thread::spawn(move || snapshot_runner.snapshot().unwrap());
        thread::sleep(Duration::from_millis(100));

        runner.execute(&move_action("{}", None)).unwrap();
        snapshot_thread.join().unwrap();

        assert!(!directory.path().join("overlaps.txt").exists());
    }
}
