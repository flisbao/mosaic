use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::application::ports::{ActionExecutor, SourceError, WidgetSource};
use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::widget::WidgetSnapshot;
use crate::infrastructure::config::WidgetSettings;
use crate::infrastructure::plugin_protocol::parse_snapshot;
use crate::infrastructure::process::{run_process, ProcessLimits};

pub const MAXIMUM_PLUGIN_OUTPUT_BYTES: usize = 1_048_576;

pub struct PluginRunner {
    settings: WidgetSettings,
    working_directory: PathBuf,
}

impl PluginRunner {
    pub fn new(settings: WidgetSettings, working_directory: PathBuf) -> Self {
        Self {
            settings,
            working_directory,
        }
    }

    fn run(&self, extra_args: &[&str], stdin_text: Option<&str>) -> Result<String, SourceError> {
        let mut command = Command::new(&self.settings.command);
        command
            .args(&self.settings.args)
            .args(extra_args)
            .envs(self.settings.env.iter().map(|(key, value)| (key, value)))
            .current_dir(&self.working_directory);
        let limits = ProcessLimits {
            timeout: Duration::from_secs(self.settings.timeout_seconds),
            maximum_output_bytes: MAXIMUM_PLUGIN_OUTPUT_BYTES,
        };
        run_process(command, stdin_text, &limits)
            .map_err(|error| SourceError::Failed(error.to_string()))
    }

    pub fn raw_snapshot_output(&self) -> Result<String, SourceError> {
        self.run(&["snapshot"], None)
    }
}

impl WidgetSource for PluginRunner {
    fn snapshot(&self) -> Result<WidgetSnapshot, SourceError> {
        let output = self.raw_snapshot_output()?;
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
            interval_adjusted: false,
            timeout_seconds,
            span: 1,
            tab: false,
            llm_sections: vec![],
            llm_max_items: 20,
            llm_max_characters_per_item: 120,
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
