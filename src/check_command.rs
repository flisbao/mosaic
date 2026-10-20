use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use mosaic::application::check_report::{build_check_report, SnapshotParseFailure};
use mosaic::application::launch_spec::ProviderCommand;
use mosaic::domain::widget::WidgetSnapshot;
use mosaic::infrastructure::config::{
    config_directory, config_file_path, parse_config, read_config_file, ConfigError,
};
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::protocol_report::parse_snapshot_report;
use mosaic::presentation::widget_view::{lines_to_plain_text, render_blocks};
use thiserror::Error;

const CHECK_USAGE: &str = "uso: mosaic check <comando> [args...] [--env NOME=valor]...";
const CHECK_ENV_FLAG: &str = "--env";
const CHECK_TIMEOUT_SECONDS: u64 = 10;
const CHECK_PREVIEW_WIDTH: usize = 60;
const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
const CHECK_CONFIG_FLAG: &str = "--config";
const CHECK_CONFIG_USAGE: &str = "uso: mosaic check --config [caminho]";

#[derive(Debug, Error, PartialEq)]
enum CheckUsageError {
    #[error("{CHECK_USAGE}")]
    MissingCommand,
    #[error("{}", env_without_value_message())]
    EnvWithoutValue,
    #[error("{}", invalid_variable_message(.0))]
    InvalidVariable(String),
    #[error("{CHECK_CONFIG_USAGE}")]
    TooManyPaths,
    #[error(transparent)]
    ConfigDirectory(#[from] ConfigError),
}

fn env_without_value_message() -> String {
    format!("{CHECK_ENV_FLAG} precisa de NOME=valor\n{CHECK_USAGE}")
}

fn invalid_variable_message(assignment: &str) -> String {
    format!("variável inválida: {assignment}\n{CHECK_USAGE}")
}

#[derive(Debug)]
struct CheckInvocation {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

fn parse_check_arguments(arguments: &[String]) -> Result<CheckInvocation, CheckUsageError> {
    let mut command = None;
    let mut args = Vec::new();
    let mut env = Vec::new();
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        if argument != CHECK_ENV_FLAG {
            match command {
                None => command = Some(argument.clone()),
                Some(_) => args.push(argument.clone()),
            }
            continue;
        }
        let assignment = remaining.next().ok_or(CheckUsageError::EnvWithoutValue)?;
        match assignment.split_once('=') {
            Some((name, value)) if !name.is_empty() => {
                env.push((name.to_string(), value.to_string()))
            }
            _ => return Err(CheckUsageError::InvalidVariable(assignment.clone())),
        }
    }
    let command = command.ok_or(CheckUsageError::MissingCommand)?;
    Ok(CheckInvocation { command, args, env })
}

fn check_preview(snapshot: &WidgetSnapshot) -> Vec<String> {
    let blocks = snapshot.summary.display_blocks();
    lines_to_plain_text(&render_blocks(&blocks, CHECK_PREVIEW_WIDTH))
}

pub fn run(arguments: &[String]) -> ExitCode {
    if arguments.first().map(String::as_str) == Some(CHECK_CONFIG_FLAG) {
        return run_check_config(&arguments[1..]);
    }
    let invocation = match parse_check_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    let command = ProviderCommand {
        program: invocation.command,
        args: invocation.args,
        env: invocation.env,
        timeout: Duration::from_secs(CHECK_TIMEOUT_SECONDS),
    };
    let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let runner = PluginRunner::new(command, working_directory);
    let parse = |json: &str| {
        parse_snapshot_report(json).map_err(|error| SnapshotParseFailure(error.to_string()))
    };
    let outcome = build_check_report(runner.raw_snapshot_output(), &parse, &check_preview);
    println!("{}", outcome.text);
    ExitCode::from(u8::try_from(outcome.exit_code).unwrap_or(EXIT_CODE_FAILURE))
}

fn run_check_config(arguments: &[String]) -> ExitCode {
    let path = match config_path_argument(arguments) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    match read_config_file(&path).and_then(|text| parse_config(&text)) {
        Ok(config) => {
            for warning in &config.warnings {
                println!("aviso: {warning}");
            }
            println!("configuração válida: {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("mosaic: {error}");
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn config_path_argument(arguments: &[String]) -> Result<PathBuf, CheckUsageError> {
    match arguments {
        [] => Ok(config_file_path(&config_directory()?)),
        [path] => Ok(PathBuf::from(path)),
        _ => Err(CheckUsageError::TooManyPaths),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_command_arguments_and_environment() {
        let invocation = parse_check_arguments(&arguments(&[
            "python3", "jira.py", "--env", "A=b", "my-tasks",
        ]))
        .unwrap();
        assert_eq!(invocation.command, "python3");
        assert_eq!(invocation.args, vec!["jira.py", "my-tasks"]);
        assert_eq!(invocation.env, vec![("A".to_string(), "b".to_string())]);
    }

    #[test]
    fn env_flag_without_value_is_a_usage_error() {
        let error = parse_check_arguments(&arguments(&["cmd", "--env"])).unwrap_err();
        assert_eq!(error, CheckUsageError::EnvWithoutValue);
        assert!(
            error.to_string().contains("precisa de NOME=valor"),
            "{error}"
        );
    }

    #[test]
    fn env_with_empty_name_is_rejected() {
        let error = parse_check_arguments(&arguments(&["cmd", "--env", "=valor"])).unwrap_err();
        assert_eq!(
            error,
            CheckUsageError::InvalidVariable("=valor".to_string())
        );
        assert!(error.to_string().contains("variável inválida"), "{error}");
    }

    #[test]
    fn missing_command_is_a_usage_error() {
        let error = parse_check_arguments(&[]).unwrap_err();
        assert_eq!(error, CheckUsageError::MissingCommand);
        assert_eq!(error.to_string(), CHECK_USAGE);
        assert_eq!(
            parse_check_arguments(&arguments(&["--env", "A=b"])).unwrap_err(),
            CheckUsageError::MissingCommand
        );
    }

    #[test]
    fn config_path_argument_accepts_zero_or_one_path_only() {
        assert_eq!(
            config_path_argument(&arguments(&["/tmp/x.toml"])).unwrap(),
            std::path::PathBuf::from("/tmp/x.toml")
        );
        let error = config_path_argument(&arguments(&["a", "b"])).unwrap_err();
        assert_eq!(error, CheckUsageError::TooManyPaths);
        assert_eq!(error.to_string(), CHECK_CONFIG_USAGE);
    }
}
