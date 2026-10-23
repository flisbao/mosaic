use std::path::PathBuf;
use std::process::ExitCode;

use mosaic::infrastructure::skills_installer::{install_skills, SkillOutcome};
use thiserror::Error;

const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
const SKILLS_USAGE: &str = "uso: mosaic skills install [--from DIRETORIO] [--to DIRETORIO]";
const INSTALL_SUBCOMMAND: &str = "install";
const FROM_FLAG: &str = "--from";
const TO_FLAG: &str = "--to";
const PACKAGED_SKILLS_DIRECTORY: &str = "/usr/share/mosaic/skills";
const CLAUDE_SKILLS_RELATIVE_PATH: &str = ".claude/skills";

#[derive(Debug, Error, PartialEq)]
enum SkillsUsageError {
    #[error("{SKILLS_USAGE}")]
    UnknownSubcommand,
    #[error("{0} precisa de DIRETORIO\n{SKILLS_USAGE}")]
    FlagWithoutValue(&'static str),
    #[error("argumento desconhecido: {0}\n{SKILLS_USAGE}")]
    UnknownArgument(String),
    #[error("não foi possível descobrir a pasta pessoal; use {TO_FLAG}\n{SKILLS_USAGE}")]
    NoHomeDirectory,
}

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
    match arguments.split_first() {
        Some((subcommand, rest)) if subcommand == INSTALL_SUBCOMMAND => run_install(rest),
        _ => {
            eprintln!("{}", SkillsUsageError::UnknownSubcommand);
            ExitCode::from(EXIT_CODE_USAGE)
        }
    }
}

fn run_install(arguments: &[String]) -> ExitCode {
    let invocation = match parse_install_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    let outcomes = match install_skills(&invocation.source, &invocation.target) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            eprintln!("mosaic: {error}");
            return ExitCode::from(EXIT_CODE_FAILURE);
        }
    };
    report(&outcomes, &invocation)
}

fn report(outcomes: &[SkillOutcome], invocation: &InstallInvocation) -> ExitCode {
    let mut conflicts = 0;
    for outcome in outcomes {
        match outcome {
            SkillOutcome::Linked(name) => println!("instalada: {name}"),
            SkillOutcome::AlreadyLinked(name) => println!("já instalada: {name}"),
            SkillOutcome::Conflict(name) => {
                conflicts += 1;
                eprintln!(
                    "conflito: {} já existe e não é a skill do Mosaic; remova-a para instalar",
                    invocation.target.join(name).display()
                );
            }
        }
    }
    println!(
        "{} skills em {}",
        outcomes.len() - conflicts,
        invocation.target.display()
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
