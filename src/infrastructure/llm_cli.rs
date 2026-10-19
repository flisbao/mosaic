use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::application::llm::{LlmError, LlmRequest, LlmRunner};
use crate::application::ports::Cancellable;
use crate::infrastructure::private_directory::prepare_private_directory;
use crate::infrastructure::process::{run_process, CancelFlag, ProcessLimits};

pub const MAXIMUM_LLM_OUTPUT_BYTES: usize = 262_144;

const ARGUMENT_SEPARATOR: &str = "--";
const OPENCODE_CONFIG_VARIABLE: &str = "OPENCODE_CONFIG_CONTENT";
const OPENCODE_RESTRICTED_CONFIG: &str = r#"{"permission":{"*":"deny","external_directory":"deny","write":"deny","edit":"deny","bash":"deny","webfetch":"deny","read":"deny","grep":"deny","glob":"deny","list":"deny","patch":"deny","task":"deny","todowrite":"deny","todoread":"deny"},"tools":{"*":false,"write":false,"edit":false,"bash":false,"webfetch":false,"read":false,"grep":false,"glob":false,"list":false,"patch":false,"task":false,"todowrite":false,"todoread":false},"mcp":{}}"#;
const OPENCODE_STDIN_INSTRUCTION: &str = "Responda ao pedido recebido pela entrada padrão.";
const OPENCODE_CONFIG_HOME_VARIABLE: &str = "XDG_CONFIG_HOME";
const OPENCODE_ISOLATED_CONFIG_DIRECTORY: &str = "xdg-config";

pub struct ClaudeCli {
    program: String,
    working_directory: PathBuf,
    max_budget_usd: Option<f64>,
    timeout: Duration,
    cancel_flag: CancelFlag,
}

impl ClaudeCli {
    pub fn new(
        program: String,
        working_directory: PathBuf,
        max_budget_usd: Option<f64>,
        timeout: Duration,
    ) -> Self {
        Self {
            program,
            working_directory,
            max_budget_usd,
            timeout,
            cancel_flag: CancelFlag::default(),
        }
    }

    pub fn arguments(&self, request: &LlmRequest) -> Vec<String> {
        let mut arguments: Vec<String> = [
            "-p",
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--setting-sources",
            "",
            "--no-session-persistence",
        ]
        .iter()
        .map(|argument| argument.to_string())
        .collect();
        if let Some(model) = &request.model {
            arguments.extend(["--model".to_string(), model.clone()]);
        }
        if let Some(budget) = self.max_budget_usd {
            arguments.extend(["--max-budget-usd".to_string(), budget.to_string()]);
        }
        arguments
    }
}

impl ClaudeCli {
    pub fn environment(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}

impl LlmRunner for ClaudeCli {
    fn run(&self, request: &LlmRequest) -> Result<String, LlmError> {
        run_cli(
            &self.program,
            &self.working_directory,
            &self.arguments(request),
            &self.environment(),
            Some(&request.prompt),
            self.timeout,
            &self.cancel_flag,
        )
    }
}

impl Cancellable for ClaudeCli {
    fn cancel(&self) {
        self.cancel_flag.cancel();
    }

    fn request_cancel(&self) {
        self.cancel_flag.request_cancel();
    }
}

pub struct OpenCodeCli {
    program: String,
    working_directory: PathBuf,
    timeout: Duration,
    cancel_flag: CancelFlag,
}

impl OpenCodeCli {
    pub fn new(program: String, working_directory: PathBuf, timeout: Duration) -> Self {
        Self {
            program,
            working_directory,
            timeout,
            cancel_flag: CancelFlag::default(),
        }
    }

    pub fn arguments(&self, request: &LlmRequest) -> Vec<String> {
        let mut arguments = vec![
            "run".to_string(),
            "--dir".to_string(),
            self.working_directory.to_string_lossy().to_string(),
        ];
        if let Some(model) = &request.model {
            arguments.extend(["-m".to_string(), model.clone()]);
        }
        arguments.push(ARGUMENT_SEPARATOR.to_string());
        arguments.push(OPENCODE_STDIN_INSTRUCTION.to_string());
        arguments
    }
}

impl OpenCodeCli {
    pub fn environment(&self) -> Vec<(String, String)> {
        vec![
            (
                OPENCODE_CONFIG_VARIABLE.to_string(),
                OPENCODE_RESTRICTED_CONFIG.to_string(),
            ),
            (
                OPENCODE_CONFIG_HOME_VARIABLE.to_string(),
                self.working_directory
                    .join(OPENCODE_ISOLATED_CONFIG_DIRECTORY)
                    .to_string_lossy()
                    .to_string(),
            ),
        ]
    }
}

impl LlmRunner for OpenCodeCli {
    fn run(&self, request: &LlmRequest) -> Result<String, LlmError> {
        run_cli(
            &self.program,
            &self.working_directory,
            &self.arguments(request),
            &self.environment(),
            Some(&request.prompt),
            self.timeout,
            &self.cancel_flag,
        )
    }
}

impl Cancellable for OpenCodeCli {
    fn cancel(&self) {
        self.cancel_flag.cancel();
    }

    fn request_cancel(&self) {
        self.cancel_flag.request_cancel();
    }
}

fn run_cli(
    program: &str,
    working_directory: &PathBuf,
    arguments: &[String],
    environment: &[(String, String)],
    stdin_text: Option<&str>,
    timeout: Duration,
    cancel: &CancelFlag,
) -> Result<String, LlmError> {
    prepare_private_directory(working_directory)
        .map_err(|error| LlmError::Failed(error.to_string()))?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .envs(environment.iter().map(|(name, value)| (name, value)))
        .current_dir(working_directory);
    let limits = ProcessLimits {
        timeout,
        maximum_output_bytes: MAXIMUM_LLM_OUTPUT_BYTES,
    };
    run_process(command, stdin_text, &limits, cancel).map_err(|error| {
        if error.program_not_found() {
            LlmError::Unavailable(format!("`{program}` não encontrado no PATH"))
        } else {
            LlmError::Failed(error.to_string())
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;
    use crate::application::llm::{LlmError, LlmRequest, LlmRunner};
    use crate::application::ports::Cancellable;

    trait RunRetrying: LlmRunner {
        fn run_retrying(&self, request: &LlmRequest) -> Result<String, LlmError> {
            let mut result = self.run(request);
            for _ in 0..FORK_RACE_RETRIES {
                match &result {
                    Err(error) if error.to_string().contains("Text file busy") => {
                        std::thread::sleep(Duration::from_millis(20));
                        result = self.run(request);
                    }
                    _ => break,
                }
            }
            result
        }
    }

    impl<T: LlmRunner> RunRetrying for T {}

    const FORK_RACE_RETRIES: usize = 20;

    fn request(prompt: &str, model: Option<&str>) -> LlmRequest {
        LlmRequest {
            prompt: prompt.to_string(),
            model: model.map(str::to_string),
        }
    }

    fn fake_program(directory: &Path, body: &str) -> String {
        let path = directory.join("fake-cli");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().to_string()
    }

    fn claude(program: String, directory: &Path, timeout_seconds: u64) -> ClaudeCli {
        ClaudeCli::new(
            program,
            directory.join("neutro"),
            None,
            Duration::from_secs(timeout_seconds),
        )
    }

    fn opencode(program: String, directory: &Path, timeout_seconds: u64) -> OpenCodeCli {
        OpenCodeCli::new(
            program,
            directory.join("neutro"),
            Duration::from_secs(timeout_seconds),
        )
    }

    #[test]
    fn claude_arguments_disable_tools_and_context() {
        let cli = claude("claude".to_string(), Path::new("/tmp"), 5);
        let arguments = cli.arguments(&request("SEGREDO-DO-PROMPT", None));
        assert_eq!(
            arguments,
            vec![
                "-p",
                "--tools",
                "",
                "--strict-mcp-config",
                "--disable-slash-commands",
                "--setting-sources",
                "",
                "--no-session-persistence"
            ]
        );
    }

    #[test]
    fn claude_arguments_include_model_and_budget_when_set() {
        let cli = ClaudeCli::new(
            "claude".to_string(),
            "/tmp".into(),
            Some(0.05),
            Duration::from_secs(5),
        );
        let arguments = cli.arguments(&request("p", Some("haiku")));
        let joined = arguments.join(" ");
        assert!(joined.contains("--model haiku"), "{joined}");
        assert!(joined.contains("--max-budget-usd 0.05"), "{joined}");
    }

    #[test]
    fn opencode_arguments_use_dir_model_and_never_carry_the_prompt() {
        let cli = opencode("opencode".to_string(), Path::new("/tmp"), 5);
        let arguments = cli.arguments(&request("SEGREDO-DO-PROMPT", Some("anthropic/haiku")));
        assert_eq!(arguments[0], "run");
        let joined = arguments.join(" ");
        assert!(joined.contains("--dir /tmp/neutro"), "{joined}");
        assert!(joined.contains("-m anthropic/haiku"), "{joined}");
        assert!(!joined.contains("SEGREDO-DO-PROMPT"), "{joined}");
        let separator = arguments.iter().position(|a| a == "--").unwrap();
        assert_eq!(arguments[separator + 1], OPENCODE_STDIN_INSTRUCTION);
        assert_eq!(arguments.last().unwrap(), OPENCODE_STDIN_INSTRUCTION);
    }

    #[test]
    fn opencode_restricts_tools_through_the_config_environment() {
        let cli = opencode("opencode".to_string(), Path::new("/tmp"), 5);
        let environment = cli.environment();
        let value = |name: &str| {
            environment
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("{name} ausente"))
        };
        let config: serde_json::Value =
            serde_json::from_str(&value("OPENCODE_CONFIG_CONTENT")).unwrap();
        let tools = [
            "write",
            "edit",
            "bash",
            "webfetch",
            "read",
            "grep",
            "glob",
            "list",
            "patch",
            "task",
            "todowrite",
            "todoread",
        ];
        assert_eq!(config["permission"]["*"], "deny");
        assert_eq!(config["permission"]["external_directory"], "deny");
        for tool in tools {
            assert_eq!(config["permission"][tool], "deny", "permission {tool}");
            assert_eq!(config["tools"][tool], false, "tool {tool}");
        }
        assert_eq!(config["tools"]["*"], false);
        assert_eq!(config["mcp"], serde_json::json!({}));
        assert_eq!(value("XDG_CONFIG_HOME"), "/tmp/neutro/xdg-config");
    }

    #[test]
    fn opencode_environment_reaches_the_process_and_claude_has_none() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "printf '%s' \"$OPENCODE_CONFIG_CONTENT\"");
        let output = opencode(program, directory.path(), 5)
            .run_retrying(&request("p", None))
            .unwrap();
        assert!(output.contains("\"permission\""), "{output}");
        assert!(claude("claude".to_string(), directory.path(), 5)
            .environment()
            .is_empty());
    }

    #[test]
    fn refuses_a_symlinked_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let outside = directory.path().join("fora");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, directory.path().join("neutro")).unwrap();
        let program = fake_program(directory.path(), "echo nao-deveria-rodar");
        let error = claude(program, directory.path(), 5)
            .run_retrying(&request("p", None))
            .unwrap_err();
        assert!(matches!(error, LlmError::Failed(_)), "{error:?}");
    }

    #[test]
    fn claude_sends_prompt_on_stdin_and_returns_stdout() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "cat");
        let prompt = "- critérios com $VAR e \"aspas\" e `crase`";
        let output = claude(program, directory.path(), 5)
            .run_retrying(&request(prompt, None))
            .unwrap();
        assert_eq!(output, prompt);
    }

    #[test]
    fn opencode_receives_the_prompt_on_stdin_not_in_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "printf 'ARGS=%s;STDIN=' \"$*\"; cat");
        let prompt = "-x com $VAR e SEGREDO";
        let output = opencode(program, directory.path(), 5)
            .run_retrying(&request(prompt, None))
            .unwrap();
        assert!(output.ends_with(&format!("STDIN={prompt}")), "{output}");
        assert!(
            !output.split(";STDIN=").next().unwrap().contains("SEGREDO"),
            "{output}"
        );
    }

    #[test]
    fn runs_in_the_neutral_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "pwd -P");
        let output = claude(program, directory.path(), 5)
            .run_retrying(&request("p", None))
            .unwrap();
        let expected = fs::canonicalize(directory.path().join("neutro")).unwrap();
        assert_eq!(output.trim(), expected.to_string_lossy());
    }

    #[test]
    fn missing_program_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let error = claude("claude-que-nao-existe-xyz".to_string(), directory.path(), 5)
            .run_retrying(&request("p", None))
            .unwrap_err();
        assert!(matches!(error, LlmError::Unavailable(_)), "{error:?}");
        assert!(error.to_string().contains("claude-que-nao-existe-xyz"));
    }

    #[test]
    fn nonzero_exit_is_failed_with_stderr() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "echo sem credito >&2; exit 2");
        let error = claude(program, directory.path(), 5)
            .run_retrying(&request("p", None))
            .unwrap_err();
        assert_eq!(error, LlmError::Failed("sem credito".to_string()));
    }

    #[test]
    fn slow_cli_times_out() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "exec sleep 30");
        let started = std::time::Instant::now();
        let error = claude(program, directory.path(), 1)
            .run_retrying(&request("p", None))
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(matches!(error, LlmError::Failed(_)));
        assert!(error.to_string().contains("tempo"));
    }

    #[test]
    fn huge_output_is_failed() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(directory.path(), "head -c 300000 /dev/zero | tr '\\0' 'a'");
        let error = claude(program, directory.path(), 5)
            .run_retrying(&request("p", None))
            .unwrap_err();
        assert!(matches!(error, LlmError::Failed(_)));
        assert!(error.to_string().contains("excedeu o limite"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cancel_stops_the_running_llm_process_and_blocks_later_runs() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid.txt");
        let marker = directory.path().join("spawned.txt");
        let body = format!(
            "echo $$ > '{}'; touch '{}'; exec sleep 30",
            pid_file.display(),
            marker.display()
        );
        let cli = Arc::new(claude(
            fake_program(directory.path(), &body),
            directory.path(),
            60,
        ));
        let running = Arc::clone(&cli);
        let worker = std::thread::spawn(move || running.run(&request("prompt", None)));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pid_file.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid = fs::read_to_string(&pid_file).unwrap().trim().to_string();

        Cancellable::cancel(cli.as_ref());

        assert!(
            !is_signalable(&pid),
            "cancel() deve matar o processo do LLM antes de retornar"
        );
        let error = worker.join().unwrap().unwrap_err();
        assert!(error.to_string().contains("cancelado"), "{error}");

        fs::remove_file(&marker).unwrap();
        let later = cli.run(&request("prompt", None)).unwrap_err();
        assert!(later.to_string().contains("cancelado"), "{later}");
        assert!(
            !marker.exists(),
            "execução após cancel() não deve criar processo"
        );
    }

    fn is_signalable(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .status()
            .is_ok_and(|status| status.success())
    }
}
