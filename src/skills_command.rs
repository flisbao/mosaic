use std::path::PathBuf;
use std::process::ExitCode;

use mosaic::display_in_portuguese;
use mosaic::domain::language::{Language, LocalizedMessage};
use mosaic::infrastructure::skills_installer::{install_skills, SkillOutcome};
use thiserror::Error;

const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
const INSTALL_SUBCOMMAND: &str = "install";
const FROM_FLAG: &str = "--from";
const TO_FLAG: &str = "--to";
const PACKAGED_SKILLS_DIRECTORY: &str = "/usr/share/mosaic/skills";
const CLAUDE_SKILLS_RELATIVE_PATH: &str = ".claude/skills";

fn skills_usage(language: Language) -> &'static str {
    language.pick(
        "usage: mosaic skills install [--from DIRECTORY] [--to DIRECTORY]",
        "uso: mosaic skills install [--from DIRETORIO] [--to DIRETORIO]",
    )
}

#[derive(Debug, Error, PartialEq)]
enum SkillsUsageError {
    UnknownSubcommand,
    FlagWithoutValue(&'static str),
    UnknownArgument(String),
    NoHomeDirectory,
}

impl LocalizedMessage for SkillsUsageError {
    fn message(&self, language: Language) -> String {
        let usage = skills_usage(language);
        match self {
            Self::UnknownSubcommand => usage.to_string(),
            Self::FlagWithoutValue(flag) => language.pick(
                format!("{flag} needs DIRECTORY\n{usage}"),
                format!("{flag} precisa de DIRETORIO\n{usage}"),
            ),
            Self::UnknownArgument(argument) => language.pick(
                format!("unknown argument: {argument}\n{usage}"),
                format!("argumento desconhecido: {argument}\n{usage}"),
            ),
            Self::NoHomeDirectory => language.pick(
                format!("could not find the home folder; use {TO_FLAG}\n{usage}"),
                format!("não foi possível descobrir a pasta pessoal; use {TO_FLAG}\n{usage}"),
            ),
        }
    }
}

display_in_portuguese!(SkillsUsageError);

#[derive(Debug, PartialEq)]
struct InstallInvocation {
    source: PathBuf,
    target: PathBuf,
}

fn parse_install_arguments(arguments: &[String]) -> Result<InstallInvocation, SkillsUsageError> {
    let mut source = None;
    let mut target = None;
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        let (slot, flag) = match argument.as_str() {
            FROM_FLAG => (&mut source, FROM_FLAG),
            TO_FLAG => (&mut target, TO_FLAG),
            other => return Err(SkillsUsageError::UnknownArgument(other.to_string())),
        };
        let value = remaining
            .next()
            .ok_or(SkillsUsageError::FlagWithoutValue(flag))?;
        *slot = Some(PathBuf::from(value));
    }
    let target = match target {
        Some(target) => target,
        None => dirs::home_dir()
            .map(|home| home.join(CLAUDE_SKILLS_RELATIVE_PATH))
            .ok_or(SkillsUsageError::NoHomeDirectory)?,
    };
    Ok(InstallInvocation {
        source: source.unwrap_or_else(|| PathBuf::from(PACKAGED_SKILLS_DIRECTORY)),
        target,
    })
}

pub fn run(arguments: &[String]) -> ExitCode {
    let language = Language::detect_from_environment();
    match arguments.split_first() {
        Some((subcommand, rest)) if subcommand == INSTALL_SUBCOMMAND => run_install(rest, language),
        _ => {
            eprintln!("{}", SkillsUsageError::UnknownSubcommand.message(language));
            ExitCode::from(EXIT_CODE_USAGE)
        }
    }
}

fn run_install(arguments: &[String], language: Language) -> ExitCode {
    let invocation = match parse_install_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{}", error.message(language));
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    let outcomes = match install_skills(&invocation.source, &invocation.target) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            eprintln!("mosaic: {}", error.message(language));
            return ExitCode::from(EXIT_CODE_FAILURE);
        }
    };
    report(&outcomes, &invocation, language)
}

fn report(
    outcomes: &[SkillOutcome],
    invocation: &InstallInvocation,
    language: Language,
) -> ExitCode {
    let mut conflicts = 0;
    for outcome in outcomes {
        match outcome {
            SkillOutcome::Linked(name) => {
                println!("{}: {name}", language.pick("installed", "instalada"))
            }
            SkillOutcome::AlreadyLinked(name) => {
                println!(
                    "{}: {name}",
                    language.pick("already installed", "já instalada")
                )
            }
            SkillOutcome::Conflict(name) => {
                conflicts += 1;
                let path = invocation.target.join(name).display().to_string();
                eprintln!(
                    "{}",
                    language.pick(
                        format!("conflict: {path} already exists and is not the Mosaic skill; remove it to install"),
                        format!("conflito: {path} já existe e não é a skill do Mosaic; remova-a para instalar"),
                    )
                );
            }
        }
    }
    println!(
        "{}",
        language.pick(
            format!(
                "{} skills in {}",
                outcomes.len() - conflicts,
                invocation.target.display()
            ),
            format!(
                "{} skills em {}",
                outcomes.len() - conflicts,
                invocation.target.display()
            ),
        )
    );
    if conflicts == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_CODE_FAILURE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn flags_override_the_default_directories() {
        let invocation =
            parse_install_arguments(&arguments(&["--from", "/a", "--to", "/b"])).unwrap();
        assert_eq!(invocation.source, PathBuf::from("/a"));
        assert_eq!(invocation.target, PathBuf::from("/b"));
    }

    #[test]
    fn defaults_come_from_the_package_and_the_home_directory() {
        let invocation = parse_install_arguments(&[]).unwrap();
        assert_eq!(invocation.source, PathBuf::from(PACKAGED_SKILLS_DIRECTORY));
        assert!(invocation.target.ends_with(CLAUDE_SKILLS_RELATIVE_PATH));
    }

    #[test]
    fn a_flag_without_value_is_a_usage_error() {
        assert_eq!(
            parse_install_arguments(&arguments(&["--to"])).unwrap_err(),
            SkillsUsageError::FlagWithoutValue(TO_FLAG)
        );
    }

    #[test]
    fn an_unknown_argument_is_a_usage_error() {
        assert_eq!(
            parse_install_arguments(&arguments(&["--x"])).unwrap_err(),
            SkillsUsageError::UnknownArgument("--x".to_string())
        );
    }
}
