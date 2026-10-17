use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::application::llm::{LlmError, LlmRequest, LlmRunner};
use crate::infrastructure::process::{run_process, ProcessLimits};

pub const MAXIMUM_LLM_OUTPUT_BYTES: usize = 262_144;

const ARGUMENT_SEPARATOR: &str = "--";

pub struct ClaudeCli {
    program: String,
    working_directory: PathBuf,
    max_budget_usd: Option<f64>,
    timeout: Duration,
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
        }
    }

    pub fn arguments(&self, request: &LlmRequest) -> Vec<String> {
        let mut arguments: Vec<String> = [
            "-p",
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
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

impl LlmRunner for ClaudeCli {
    fn run(&self, request: &LlmRequest) -> Result<String, LlmError> {
        run_cli(
            &self.program,
            &self.working_directory,
            &self.arguments(request),
            Some(&request.prompt),
            self.timeout,
        )
    }
}

pub struct OpenCodeCli {
    program: String,
    working_directory: PathBuf,
    timeout: Duration,
}

impl OpenCodeCli {
    pub fn new(program: String, working_directory: PathBuf, timeout: Duration) -> Self {
        Self {
            program,
            working_directory,
            timeout,
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
        arguments.push(request.prompt.clone());
        arguments
    }
}

impl LlmRunner for OpenCodeCli {
    fn run(&self, request: &LlmRequest) -> Result<String, LlmError> {
        run_cli(
            &self.program,
            &self.working_directory,
            &self.arguments(request),
            None,
            self.timeout,
        )
    }
}

fn run_cli(
    program: &str,
    working_directory: &PathBuf,
    arguments: &[String],
    stdin_text: Option<&str>,
    timeout: Duration,
) -> Result<String, LlmError> {
    fs::create_dir_all(working_directory).map_err(|error| {
        LlmError::Failed(format!(
            "não foi possível criar o diretório de trabalho da IA: {error}"
        ))
    })?;
    let mut command = Command::new(program);
    command.args(arguments).current_dir(working_directory);
    let limits = ProcessLimits {
        timeout,
        maximum_output_bytes: MAXIMUM_LLM_OUTPUT_BYTES,
    };
    run_process(command, stdin_text, &limits).map_err(|error| {
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

    use super::*;
    use crate::application::llm::{LlmError, LlmRequest, LlmRunner};

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
                "--disable-slash-commands"
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
    fn opencode_arguments_use_dir_model_and_double_dash_before_prompt() {
        let cli = opencode("opencode".to_string(), Path::new("/tmp"), 5);
        let arguments = cli.arguments(&request("faça isso", Some("anthropic/haiku")));
        assert_eq!(arguments[0], "run");
        let joined = arguments.join(" ");
        assert!(joined.contains("--dir /tmp/neutro"), "{joined}");
        assert!(joined.contains("-m anthropic/haiku"), "{joined}");
        let separator = arguments.iter().position(|a| a == "--").unwrap();
        assert_eq!(arguments[separator + 1], "faça isso");
        assert_eq!(arguments.last().unwrap(), "faça isso");
    }

    #[test]
    fn opencode_prompt_starting_with_dash_stays_after_double_dash() {
        let cli = opencode("opencode".to_string(), Path::new("/tmp"), 5);
        let arguments = cli.arguments(&request("--help me", None));
        let separator = arguments.iter().position(|a| a == "--").unwrap();
        assert_eq!(arguments[separator + 1], "--help me");
        assert_eq!(arguments.iter().filter(|a| *a == "--help me").count(), 1);
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
    fn opencode_passes_prompt_as_last_argument() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_program(
            directory.path(),
            "for last; do :; done; printf '%s' \"$last\"",
        );
        let prompt = "-x com $VAR";
        let output = opencode(program, directory.path(), 5)
            .run_retrying(&request(prompt, None))
            .unwrap();
        assert_eq!(output, prompt);
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
}
