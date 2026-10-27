use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mosaic::application::plugin_setup::{run_setup, SetupError, SetupOptions, SetupResult};
use mosaic::application::ports::{PromptError, Prompter};
use mosaic::infrastructure::app_paths::config_directory;
use mosaic::infrastructure::config::{config_file_path, PLUGINS_DIRECTORY_NAME};
use mosaic::infrastructure::config_appender::{append_widget, existing_widget_ids, AppendError};
use mosaic::infrastructure::setup_manifest::{read_manifest, ManifestError, MANIFEST_FILE_NAME};
use mosaic::infrastructure::stream_prompter::StreamPrompter;
use thiserror::Error;

const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
pub const SETUP_USAGE: &str =
    "uso: mosaic plugins setup NOME [--config-dir DIRETORIO] [--advanced] [--print]";
const CONFIG_DIR_FLAG: &str = "--config-dir";
const ADVANCED_FLAG: &str = "--advanced";
const PRINT_FLAG: &str = "--print";
const AFFIRMATIVE_ANSWERS: [&str; 3] = ["s", "sim", "y"];
const NEGATIVE_ANSWERS: [&str; 3] = ["n", "nao", "não"];
const BLOCK_RULE: &str = "----------------------------------------";

#[derive(Debug, Error, PartialEq)]
enum SetupUsageError {
    #[error("{SETUP_USAGE}")]
    MissingName,
    #[error("{CONFIG_DIR_FLAG} precisa de DIRETORIO\n{SETUP_USAGE}")]
    FlagWithoutValue,
    #[error("argumento desconhecido: {0}\n{SETUP_USAGE}")]
    UnknownArgument(String),
    #[error(
        "não foi possível descobrir a pasta de configuração; use {CONFIG_DIR_FLAG}\n{SETUP_USAGE}"
    )]
    NoConfigDirectory,
}

#[derive(Debug, Error)]
enum SetupFailure {
    #[error("o plugin '{0}' não está instalado; rode: mosaic plugins install {0}")]
    NotInstalled(String),
    #[error("o plugin '{0}' não tem {MANIFEST_FILE_NAME}: reinstale com: mosaic plugins install --force {0}")]
    NoManifest(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Append(#[from] AppendError),
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error("não consegui escrever na saída: {0}")]
    Output(String),
}

#[derive(Debug, PartialEq)]
struct SetupInvocation {
    name: String,
    config_directory: PathBuf,
    include_advanced: bool,
    print_only: bool,
}

fn parse_arguments(arguments: &[String]) -> Result<SetupInvocation, SetupUsageError> {
    let mut name = None;
    let mut config_directory_argument = None;
    let mut include_advanced = false;
    let mut print_only = false;
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        match argument.as_str() {
            ADVANCED_FLAG => include_advanced = true,
            PRINT_FLAG => print_only = true,
            CONFIG_DIR_FLAG => {
                let value = remaining.next().ok_or(SetupUsageError::FlagWithoutValue)?;
                config_directory_argument = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(SetupUsageError::UnknownArgument(other.to_string()))
            }
            other if name.is_none() => name = Some(other.to_string()),
            other => return Err(SetupUsageError::UnknownArgument(other.to_string())),
        }
    }
    Ok(SetupInvocation {
        name: name.ok_or(SetupUsageError::MissingName)?,
        config_directory: match config_directory_argument {
            Some(directory) => directory,
            None => config_directory().ok_or(SetupUsageError::NoConfigDirectory)?,
        },
        include_advanced,
        print_only,
    })
}

pub fn run(arguments: &[String]) -> ExitCode {
    let invocation = match parse_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    let mut prompter = StreamPrompter::new(io::stdin().lock(), io::stdout());
    match execute(&invocation, &mut prompter, &mut io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(SetupFailure::Setup(SetupError::Prompt(PromptError::EndOfInput))) => {
            eprintln!("mosaic: configuração cancelada (a entrada terminou); nada foi gravado");
            ExitCode::from(EXIT_CODE_FAILURE)
        }
        Err(error) => {
            eprintln!("mosaic: {error}");
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn execute(
    invocation: &SetupInvocation,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
) -> Result<(), SetupFailure> {
    let manifest = load_manifest(&invocation.config_directory, &invocation.name)?;
    let existing_ids = if invocation.print_only {
        existing_widget_ids(&invocation.config_directory).unwrap_or_default()
    } else {
        existing_widget_ids(&invocation.config_directory)?
    };
    let options = SetupOptions {
        include_advanced: invocation.include_advanced,
        existing_ids,
    };
    let result = run_setup(&manifest, &options, prompter)?;
    show_block(&result, output)?;
    if invocation.print_only {
        return Ok(());
    }
    let config_path = config_file_path(&invocation.config_directory);
    if !confirm(prompter, &config_path)? {
        writeln_checked(output, "Nada foi gravado.")?;
        return Ok(());
    }
    append_widget(&invocation.config_directory, &result.draft.to_toml_block())?;
    writeln_checked(
        output,
        &format!(
            "Widget \"{}\" gravado em {}. Aperte R no mosaic para recarregar; \"mosaic check --config\" valida o arquivo.",
            result.draft.id,
            config_path.display()
        ),
    )
}

fn load_manifest(
    config_directory: &Path,
    name: &str,
) -> Result<mosaic::domain::plugin_setup::SetupManifest, SetupFailure> {
    let plugin_directory = config_directory.join(PLUGINS_DIRECTORY_NAME).join(name);
    if !plugin_directory.is_dir() {
        return Err(SetupFailure::NotInstalled(name.to_string()));
    }
    let manifest_path = plugin_directory.join(MANIFEST_FILE_NAME);
    if !manifest_path.is_file() {
        return Err(SetupFailure::NoManifest(name.to_string()));
    }
    Ok(read_manifest(&manifest_path)?)
}

fn show_block(result: &SetupResult, output: &mut impl Write) -> Result<(), SetupFailure> {
    writeln_checked(output, &format!("\n{BLOCK_RULE}"))?;
    write!(output, "{}", result.draft.to_toml_block())
        .map_err(|error| SetupFailure::Output(error.to_string()))?;
    writeln_checked(output, BLOCK_RULE)?;
    for note in &result.notes {
        writeln_checked(output, &format!("Atenção: {note}"))?;
    }
    Ok(())
}

fn confirm(prompter: &mut impl Prompter, config_path: &Path) -> Result<bool, SetupFailure> {
    loop {
        let question = format!("Gravar este widget em {}? [S/n]: ", config_path.display());
        let line = prompter
            .ask(&question)
            .map_err(|error| SetupFailure::Setup(SetupError::Prompt(error)))?;
        let answer = line.trim().to_lowercase();
        if answer.is_empty() || AFFIRMATIVE_ANSWERS.contains(&answer.as_str()) {
            return Ok(true);
        }
        if NEGATIVE_ANSWERS.contains(&answer.as_str()) {
            return Ok(false);
        }
        prompter.tell("Responda s ou n.");
    }
}

fn writeln_checked(output: &mut impl Write, text: &str) -> Result<(), SetupFailure> {
    writeln!(output, "{text}").map_err(|error| SetupFailure::Output(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn the_name_and_the_flags_are_parsed() {
        let invocation = parse_arguments(&arguments(&[
            "jira",
            "--advanced",
            "--print",
            "--config-dir",
            "/tmp/x",
        ]))
        .unwrap();
        assert_eq!(
            invocation,
            SetupInvocation {
                name: "jira".to_string(),
                config_directory: PathBuf::from("/tmp/x"),
                include_advanced: true,
                print_only: true,
            }
        );
    }

    #[test]
    fn a_missing_name_is_a_usage_error() {
        assert_eq!(
            parse_arguments(&arguments(&["--print", "--config-dir", "/tmp/x"])).unwrap_err(),
            SetupUsageError::MissingName
        );
    }

    #[test]
    fn a_flag_without_value_and_unknown_arguments_are_usage_errors() {
        assert_eq!(
            parse_arguments(&arguments(&["jira", "--config-dir"])).unwrap_err(),
            SetupUsageError::FlagWithoutValue
        );
        assert_eq!(
            parse_arguments(&arguments(&["jira", "--x", "--config-dir", "/a"])).unwrap_err(),
            SetupUsageError::UnknownArgument("--x".to_string())
        );
        assert_eq!(
            parse_arguments(&arguments(&["jira", "github", "--config-dir", "/a"])).unwrap_err(),
            SetupUsageError::UnknownArgument("github".to_string())
        );
    }
}
