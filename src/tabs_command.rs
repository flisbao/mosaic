use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use mosaic::application::ports::{PromptError, Prompter};
use mosaic::domain::widget_draft::tab_toml_block;
use mosaic::infrastructure::app_paths::config_directory;
use mosaic::infrastructure::config::config_file_path;
use mosaic::infrastructure::config_appender::{
    append_blocks, inspect_config, AppendError, ConfigOverview,
};
use mosaic::infrastructure::stream_prompter::StreamPrompter;
use thiserror::Error;

use crate::confirmation::confirm;

const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
const TABS_USAGE: &str =
    "uso: mosaic tabs list [--config-dir DIRETORIO] | mosaic tabs add NOME [--config-dir DIRETORIO]";
const LIST_SUBCOMMAND: &str = "list";
const ADD_SUBCOMMAND: &str = "add";
const CONFIG_DIR_FLAG: &str = "--config-dir";

#[derive(Debug, Error, PartialEq)]
enum TabsUsageError {
    #[error("{TABS_USAGE}")]
    UnknownSubcommand,
    #[error("{CONFIG_DIR_FLAG} precisa de DIRETORIO\n{TABS_USAGE}")]
    FlagWithoutValue,
    #[error("argumento desconhecido: {0}\n{TABS_USAGE}")]
    UnknownArgument(String),
    #[error("add precisa do NOME da aba\n{TABS_USAGE}")]
    MissingName,
    #[error(
        "não foi possível descobrir a pasta de configuração; use {CONFIG_DIR_FLAG}\n{TABS_USAGE}"
    )]
    NoConfigDirectory,
}

#[derive(Debug, Error)]
enum TabsFailure {
    #[error(transparent)]
    Config(#[from] AppendError),
    #[error("já existe uma aba chamada \"{0}\"")]
    Duplicate(String),
    #[error("o nome da aba não pode ser vazio")]
    BlankName,
    #[error("não consegui escrever na saída: {0}")]
    Output(String),
    #[error(transparent)]
    Prompt(#[from] PromptError),
}

#[derive(Debug, PartialEq)]
enum Action {
    List,
    Add(String),
}

#[derive(Debug, PartialEq)]
struct TabsInvocation {
    action: Action,
    config_directory: PathBuf,
}

fn parse_arguments(arguments: &[String]) -> Result<TabsInvocation, TabsUsageError> {
    let (subcommand, rest) = arguments
        .split_first()
        .ok_or(TabsUsageError::UnknownSubcommand)?;
    let mut name = None;
    let mut directory = None;
    let mut remaining = rest.iter();
    while let Some(argument) = remaining.next() {
        match argument.as_str() {
            CONFIG_DIR_FLAG => {
                let value = remaining.next().ok_or(TabsUsageError::FlagWithoutValue)?;
                directory = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(TabsUsageError::UnknownArgument(other.to_string()))
            }
            other if name.is_none() && subcommand == ADD_SUBCOMMAND => {
                name = Some(other.to_string())
            }
            other => return Err(TabsUsageError::UnknownArgument(other.to_string())),
        }
    }
    let action = match subcommand.as_str() {
        LIST_SUBCOMMAND => Action::List,
        ADD_SUBCOMMAND => Action::Add(name.ok_or(TabsUsageError::MissingName)?),
        _ => return Err(TabsUsageError::UnknownSubcommand),
    };
    Ok(TabsInvocation {
        action,
        config_directory: match directory {
            Some(directory) => directory,
            None => config_directory().ok_or(TabsUsageError::NoConfigDirectory)?,
        },
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
        Err(TabsFailure::Prompt(PromptError::EndOfInput)) => {
            eprintln!("mosaic: cancelado (a entrada terminou); nada foi gravado");
            ExitCode::from(EXIT_CODE_FAILURE)
        }
        Err(error) => {
            eprintln!("mosaic: {error}");
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn execute(
    invocation: &TabsInvocation,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
) -> Result<(), TabsFailure> {
    let overview = inspect_config(&invocation.config_directory)?;
    match &invocation.action {
        Action::List => list(&overview, output),
        Action::Add(name) => add(invocation, &overview, name.trim(), prompter, output),
    }
}

fn list(overview: &ConfigOverview, output: &mut impl Write) -> Result<(), TabsFailure> {
    if overview.declared_tabs.is_empty() {
        write_line(
            output,
            &format!(
                "Nenhuma aba declarada: os {} widgets ficam numa aba única. Crie a primeira com: mosaic tabs add NOME",
                overview.widget_ids.len()
            ),
        )?;
        return Ok(());
    }
    for tab in &overview.tabs {
        write_line(
            output,
            &format!(
                "{} ({} widgets): {}",
                tab.name,
                tab.widget_ids.len(),
                tab.widget_ids.join(", ")
            ),
        )?;
    }
    Ok(())
}

fn add(
    invocation: &TabsInvocation,
    overview: &ConfigOverview,
    name: &str,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
) -> Result<(), TabsFailure> {
    if name.is_empty() {
        return Err(TabsFailure::BlankName);
    }
    if overview
        .declared_tabs
        .iter()
        .any(|existing| existing == name)
    {
        return Err(TabsFailure::Duplicate(name.to_string()));
    }
    if overview.declared_tabs.is_empty() && !overview.widget_ids.is_empty() {
        write_line(
            output,
            &format!(
                "Atenção: o mosaic.toml não declara abas. Ao declarar a primeira, os {} widgets que já existem sem 'tab' passam a ficar nela.",
                overview.widget_ids.len()
            ),
        )?;
        let question = "Criar a aba mesmo assim? [S/n]: ";
        if !confirm(prompter, question)? {
            return write_line(output, "Nada foi gravado.");
        }
    }
    append_blocks(&invocation.config_directory, &[tab_toml_block(name)])?;
    write_line(
        output,
        &format!(
            "Aba \"{name}\" criada em {}. Para pôr um widget nela, use 'tab = \"{name}\"' no widget ou escolha a aba em: mosaic plugins setup NOME. Aperte R no mosaic para recarregar.",
            config_file_path(&invocation.config_directory).display()
        ),
    )
}

fn write_line(output: &mut impl Write, text: &str) -> Result<(), TabsFailure> {
    writeln!(output, "{text}").map_err(|error| TabsFailure::Output(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn list_and_add_are_parsed_with_the_directory() {
        let list = parse_arguments(&arguments(&["list", "--config-dir", "/a"])).unwrap();
        assert_eq!(list.action, Action::List);
        let add = parse_arguments(&arguments(&["add", "Trabalho", "--config-dir", "/a"])).unwrap();
        assert_eq!(add.action, Action::Add("Trabalho".to_string()));
        assert_eq!(add.config_directory, PathBuf::from("/a"));
    }

    #[test]
    fn usage_errors_are_distinguished() {
        assert_eq!(
            parse_arguments(&[]).unwrap_err(),
            TabsUsageError::UnknownSubcommand
        );
        assert_eq!(
            parse_arguments(&arguments(&["add", "--config-dir", "/a"])).unwrap_err(),
            TabsUsageError::MissingName
        );
        assert_eq!(
            parse_arguments(&arguments(&["list", "extra", "--config-dir", "/a"])).unwrap_err(),
            TabsUsageError::UnknownArgument("extra".to_string())
        );
        assert_eq!(
            parse_arguments(&arguments(&["add", "x", "--config-dir"])).unwrap_err(),
            TabsUsageError::FlagWithoutValue
        );
    }
}
