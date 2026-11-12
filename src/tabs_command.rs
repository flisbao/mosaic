use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use mosaic::application::ports::{PromptError, Prompter};
use mosaic::display_in_portuguese;
use mosaic::domain::language::{Language, LocalizedMessage};
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
const LIST_SUBCOMMAND: &str = "list";
const ADD_SUBCOMMAND: &str = "add";
const CONFIG_DIR_FLAG: &str = "--config-dir";

fn tabs_usage(language: Language) -> &'static str {
    language.pick(
        "usage: mosaic tabs list [--config-dir DIRECTORY] | mosaic tabs add NAME [--config-dir DIRECTORY]",
        "uso: mosaic tabs list [--config-dir DIRETORIO] | mosaic tabs add NOME [--config-dir DIRETORIO]",
    )
}

#[derive(Debug, Error, PartialEq)]
enum TabsUsageError {
    UnknownSubcommand,
    FlagWithoutValue,
    UnknownArgument(String),
    MissingName,
    NoConfigDirectory,
}

impl LocalizedMessage for TabsUsageError {
    fn message(&self, language: Language) -> String {
        let usage = tabs_usage(language);
        match self {
            Self::UnknownSubcommand => usage.to_string(),
            Self::FlagWithoutValue => language.pick(
                format!("{CONFIG_DIR_FLAG} needs DIRECTORY\n{usage}"),
                format!("{CONFIG_DIR_FLAG} precisa de DIRETORIO\n{usage}"),
            ),
            Self::UnknownArgument(argument) => language.pick(
                format!("unknown argument: {argument}\n{usage}"),
                format!("argumento desconhecido: {argument}\n{usage}"),
            ),
            Self::MissingName => language.pick(
                format!("add needs the tab NAME\n{usage}"),
                format!("add precisa do NOME da aba\n{usage}"),
            ),
            Self::NoConfigDirectory => language.pick(
                format!("could not find the configuration folder; use {CONFIG_DIR_FLAG}\n{usage}"),
                format!("não foi possível descobrir a pasta de configuração; use {CONFIG_DIR_FLAG}\n{usage}"),
            ),
        }
    }
}

display_in_portuguese!(TabsUsageError);

#[derive(Debug, Error)]
enum TabsFailure {
    Config(#[from] AppendError),
    Duplicate(String),
    BlankName,
    Output(String),
    Prompt(#[from] PromptError),
}

impl LocalizedMessage for TabsFailure {
    fn message(&self, language: Language) -> String {
        match self {
            Self::Config(error) => error.message(language),
            Self::Duplicate(name) => language.pick(
                format!("a tab named \"{name}\" already exists"),
                format!("já existe uma aba chamada \"{name}\""),
            ),
            Self::BlankName => language.pick(
                "the tab name cannot be empty".to_string(),
                "o nome da aba não pode ser vazio".to_string(),
            ),
            Self::Output(reason) => language.pick(
                format!("could not write to the output: {reason}"),
                format!("não consegui escrever na saída: {reason}"),
            ),
            Self::Prompt(error) => error.to_string(),
        }
    }
}

display_in_portuguese!(TabsFailure);

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
    let language = Language::detect_from_environment();
    let invocation = match parse_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{}", error.message(language));
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    let mut prompter = StreamPrompter::new(io::stdin().lock(), io::stdout());
    match execute(&invocation, &mut prompter, &mut io::stdout(), language) {
        Ok(()) => ExitCode::SUCCESS,
        Err(TabsFailure::Prompt(PromptError::EndOfInput)) => {
            eprintln!(
                "{}",
                language.pick(
                    "mosaic: cancelled (the input ended); nothing was written",
                    "mosaic: cancelado (a entrada terminou); nada foi gravado",
                )
            );
            ExitCode::from(EXIT_CODE_FAILURE)
        }
        Err(error) => {
            eprintln!("mosaic: {}", error.message(language));
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn execute(
    invocation: &TabsInvocation,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
    language: Language,
) -> Result<(), TabsFailure> {
    let overview = inspect_config(&invocation.config_directory, language)?;
    match &invocation.action {
        Action::List => list(&overview, output, language),
        Action::Add(name) => add(
            invocation,
            &overview,
            name.trim(),
            prompter,
            output,
            language,
        ),
    }
}

fn list(
    overview: &ConfigOverview,
    output: &mut impl Write,
    language: Language,
) -> Result<(), TabsFailure> {
    if overview.declared_tabs.is_empty() {
        write_line(
            output,
            &language.pick(
                format!(
                    "No tab declared: the {} widgets live in a single tab. Create the first one with: mosaic tabs add NAME",
                    overview.widget_ids.len()
                ),
                format!(
                    "Nenhuma aba declarada: os {} widgets ficam numa aba única. Crie a primeira com: mosaic tabs add NOME",
                    overview.widget_ids.len()
                ),
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
    language: Language,
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
            &language.pick(
                format!(
                    "Note: mosaic.toml declares no tabs. When you declare the first one, the {} existing widgets without 'tab' move into it.",
                    overview.widget_ids.len()
                ),
                format!(
                    "Atenção: o mosaic.toml não declara abas. Ao declarar a primeira, os {} widgets que já existem sem 'tab' passam a ficar nela.",
                    overview.widget_ids.len()
                ),
            ),
        )?;
        let question = language.pick(
            "Create the tab anyway? [Y/n]: ",
            "Criar a aba mesmo assim? [S/n]: ",
        );
        if !confirm(prompter, question, language)? {
            return write_line(
                output,
                language.pick("Nothing was written.", "Nada foi gravado."),
            );
        }
    }
    append_blocks(
        &invocation.config_directory,
        &[tab_toml_block(name)],
        language,
    )?;
    write_line(
        output,
        &language.pick(
            format!(
                "Tab \"{name}\" created in {}. To put a widget in it, use 'tab = \"{name}\"' in the widget or choose the tab in: mosaic plugins setup NAME. Press R in mosaic to reload.",
                config_file_path(&invocation.config_directory).display()
            ),
            format!(
                "Aba \"{name}\" criada em {}. Para pôr um widget nela, use 'tab = \"{name}\"' no widget ou escolha a aba em: mosaic plugins setup NOME. Aperte R no mosaic para recarregar.",
                config_file_path(&invocation.config_directory).display()
            ),
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
