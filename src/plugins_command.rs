use crate::remote_plugins_command::{execute_remote_install, RemoteInstallOptions};
use mosaic::application::plugin_catalog::{resolve_plugin_source, OFFICIAL_PLUGINS};
use mosaic::domain::plugin_package::GitHubSource;
use mosaic::infrastructure::git_plugin_fetcher::GitPluginFetcher;
use mosaic::infrastructure::stream_prompter::StreamPrompter;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mosaic::infrastructure::app_paths::config_directory;
use mosaic::infrastructure::plugins_installer::{
    available_plugins, install_plugins, Overwrite, PluginOutcome, PluginsError,
};
use mosaic::infrastructure::skills_installer::{link_skill_directory, SkillOutcome};
use thiserror::Error;

use crate::plugin_setup_command;

const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
const PLUGINS_USAGE: &str = "uso: mosaic plugins install owner/repo[/subdir] | NOME... [--yes] [--to DIR] [--force] [--no-skill] [--skills-dir DIR] | mosaic plugins install --from DIR [NOME...] | mosaic plugins list [--from DIR] | mosaic plugins setup NOME [--config-dir DIR] [--advanced] [--print]\nexemplo: mosaic plugins install flisbao/mosaic/plugins/jira\npacote antigo: mosaic plugins install --from /usr/share/mosaic/plugins";
const INSTALL_SUBCOMMAND: &str = "install";
const LIST_SUBCOMMAND: &str = "list";
const SETUP_SUBCOMMAND: &str = "setup";
const FROM_FLAG: &str = "--from";
const TO_FLAG: &str = "--to";
const FORCE_FLAG: &str = "--force";
const NO_SKILL_FLAG: &str = "--no-skill";
const SKILLS_DIR_FLAG: &str = "--skills-dir";
const SKILL_FILE_NAME: &str = "SKILL.md";
const SKILL_NAME_PREFIX: &str = "mosaic-plugin-";
const CLAUDE_DIRECTORY_NAME: &str = ".claude";
const CLAUDE_SKILLS_FOLDER: &str = "skills";
const END_OF_FLAGS: &str = "--";
const YES_FLAG: &str = "--yes";
const INSTALLED_PLUGINS_FOLDER: &str = "plugins";

#[derive(Debug, Error, PartialEq)]
enum PluginsUsageError {
    #[error("{PLUGINS_USAGE}")]
    UnknownSubcommand,
    #[error("{0} precisa de DIRETORIO\n{PLUGINS_USAGE}")]
    FlagWithoutValue(&'static str),
    #[error("argumento desconhecido: {0}\n{PLUGINS_USAGE}")]
    UnknownFlag(String),
    #[error("não foi possível descobrir a pasta de configuração; use {TO_FLAG}\n{PLUGINS_USAGE}")]
    NoConfigDirectory,
    #[error("nome de plugin inválido ou fontes incompatíveis; use uma referência owner/repo[/subdir], atalhos oficiais, ou --from DIR\n{PLUGINS_USAGE}")]
    InvalidSources,
}

#[derive(Debug, PartialEq)]
enum SkillLinking {
    Disabled,
    WhenClaudeCodeExists(PathBuf),
    Into(PathBuf),
}

#[derive(Debug, PartialEq)]
enum InstallSource {
    Local {
        directory: PathBuf,
        names: Vec<String>,
    },
    Remote(Vec<GitHubSource>),
}

#[derive(Debug, PartialEq)]
struct InstallInvocation {
    source: InstallSource,
    target: PathBuf,
    overwrite: Overwrite,
    skill_linking: SkillLinking,
    assume_yes: bool,
}

fn parse_install_arguments(arguments: &[String]) -> Result<InstallInvocation, PluginsUsageError> {
    let mut source = None;
    let mut assume_yes = false;
    let mut target = None;
    let mut overwrite = Overwrite::Keep;
    let mut skills_directory = None;
    let mut link_skills = true;
    let mut names = Vec::new();
    let mut flags_ended = false;
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        if flags_ended || !argument.starts_with('-') {
            names.push(argument.clone());
            continue;
        }
        match argument.as_str() {
            END_OF_FLAGS => flags_ended = true,
            YES_FLAG => assume_yes = true,
            FORCE_FLAG => overwrite = Overwrite::Replace,
            NO_SKILL_FLAG => link_skills = false,
            SKILLS_DIR_FLAG => {
                skills_directory = Some(directory_value(&mut remaining, SKILLS_DIR_FLAG)?)
            }
            FROM_FLAG => source = Some(directory_value(&mut remaining, FROM_FLAG)?),
            TO_FLAG => target = Some(directory_value(&mut remaining, TO_FLAG)?),
            other => return Err(PluginsUsageError::UnknownFlag(other.to_string())),
        }
    }
    Ok(InstallInvocation {
        source: resolve_install_source(source, names)?,
        target: match target {
            Some(target) => target,
            None => default_target()?,
        },
        overwrite,
        skill_linking: skill_linking(link_skills, skills_directory),
        assume_yes,
    })
}

fn resolve_install_source(
    directory: Option<PathBuf>,
    names: Vec<String>,
) -> Result<InstallSource, PluginsUsageError> {
    if let Some(directory) = directory {
        if names.iter().any(|name| name.contains('/')) {
            return Err(PluginsUsageError::InvalidSources);
        }
        return Ok(InstallSource::Local { directory, names });
    }
    if names.is_empty()
        || (names.len() > 1
            && !names
                .iter()
                .all(|name| OFFICIAL_PLUGINS.contains(&name.as_str())))
    {
        return Err(PluginsUsageError::InvalidSources);
    }
    let mut sources = Vec::new();
    for name in names {
        let source = resolve_plugin_source(&name).map_err(|_| PluginsUsageError::InvalidSources)?;
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    Ok(InstallSource::Remote(sources))
}

fn skill_linking(link_skills: bool, skills_directory: Option<PathBuf>) -> SkillLinking {
    match (link_skills, skills_directory) {
        (false, _) => SkillLinking::Disabled,
        (true, Some(directory)) => SkillLinking::Into(directory),
        (true, None) => match dirs::home_dir() {
            Some(home) => SkillLinking::WhenClaudeCodeExists(home.join(CLAUDE_DIRECTORY_NAME)),
            None => SkillLinking::Disabled,
        },
    }
}

fn directory_value<'a>(
    remaining: &mut impl Iterator<Item = &'a String>,
    flag: &'static str,
) -> Result<PathBuf, PluginsUsageError> {
    remaining
        .next()
        .filter(|value| !value.starts_with('-'))
        .map(PathBuf::from)
        .ok_or(PluginsUsageError::FlagWithoutValue(flag))
}

fn parse_list_arguments(arguments: &[String]) -> Result<Option<PathBuf>, PluginsUsageError> {
    match arguments {
        [] => Ok(None),
        [flag, value] if flag == FROM_FLAG && !value.starts_with('-') => {
            Ok(Some(PathBuf::from(value)))
        }
        [flag, ..] if flag == FROM_FLAG => Err(PluginsUsageError::FlagWithoutValue(FROM_FLAG)),
        [other, ..] => Err(PluginsUsageError::UnknownFlag(other.clone())),
    }
}

fn default_target() -> Result<PathBuf, PluginsUsageError> {
    config_directory()
        .map(|directory| directory.join(INSTALLED_PLUGINS_FOLDER))
        .ok_or(PluginsUsageError::NoConfigDirectory)
}

pub fn run(arguments: &[String]) -> ExitCode {
    match arguments.split_first() {
        Some((subcommand, rest)) if subcommand == INSTALL_SUBCOMMAND => run_install(rest),
        Some((subcommand, rest)) if subcommand == LIST_SUBCOMMAND => run_list(rest),
        Some((subcommand, rest)) if subcommand == SETUP_SUBCOMMAND => {
            plugin_setup_command::run(rest)
        }
        _ => usage_failure(&PluginsUsageError::UnknownSubcommand),
    }
}

fn usage_failure(error: &PluginsUsageError) -> ExitCode {
    eprintln!("{error}");
    ExitCode::from(EXIT_CODE_USAGE)
}

fn run_install(arguments: &[String]) -> ExitCode {
    let invocation = match parse_install_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => return usage_failure(&error),
    };
    match &invocation.source {
        InstallSource::Local { directory, names } => {
            run_local_install(&invocation, directory, names)
        }
        InstallSource::Remote(sources) => {
            let options = RemoteInstallOptions {
                target: invocation.target.clone(),
                overwrite: invocation.overwrite,
                assume_yes: invocation.assume_yes,
            };
            let mut prompter = StreamPrompter::new(io::stdin().lock(), io::stdout());
            match execute_remote_install(
                sources,
                &options,
                &GitPluginFetcher::new(),
                &mut prompter,
                &mut io::stdout(),
            ) {
                Ok(outcomes) => {
                    link_skills(&invocation, &outcomes);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    link_skills(&invocation, error.completed());
                    eprintln!("mosaic: {}", error.diagnostic());
                    ExitCode::from(EXIT_CODE_FAILURE)
                }
            }
        }
    }
}

fn run_local_install(
    invocation: &InstallInvocation,
    directory: &Path,
    names: &[String],
) -> ExitCode {
    let mut outcomes = Vec::new();
    let result = install_plugins(
        directory,
        &invocation.target,
        names,
        invocation.overwrite,
        |outcome| {
            report_outcome(&outcome);
            outcomes.push(outcome);
        },
    );
    link_skills(invocation, &outcomes);
    match result {
        Ok(()) => {
            println!("{}", summary_line(&outcomes, &invocation.target));
            ExitCode::SUCCESS
        }
        Err(PluginsError::InvalidName(name)) => {
            eprintln!("mosaic: nome de plugin inválido: {name}\n{PLUGINS_USAGE}");
            ExitCode::from(EXIT_CODE_USAGE)
        }
        Err(error) => failure(&error),
    }
}

fn link_skills(invocation: &InstallInvocation, outcomes: &[PluginOutcome]) {
    let skills_directory = match &invocation.skill_linking {
        SkillLinking::Disabled => return,
        SkillLinking::Into(directory) => directory.clone(),
        SkillLinking::WhenClaudeCodeExists(claude_directory) => {
            if !claude_directory.is_dir() {
                return;
            }
            claude_directory.join(CLAUDE_SKILLS_FOLDER)
        }
    };
    for outcome in outcomes {
        let name = outcome_name(outcome);
        let plugin_directory = invocation.target.join(name);
        if !plugin_directory.join(SKILL_FILE_NAME).is_file() {
            continue;
        }
        let plugin_directory = match fs::canonicalize(&plugin_directory) {
            Ok(directory) => directory,
            Err(error) => {
                eprintln!("aviso: não consegui resolver a pasta da skill de {name}: {error}");
                continue;
            }
        };
        let skill_name = format!("{SKILL_NAME_PREFIX}{name}");
        match link_skill_directory(&plugin_directory, &skills_directory, &skill_name) {
            Ok(SkillOutcome::Linked(skill)) => println!("skill instalada: {skill}"),
            Ok(SkillOutcome::AlreadyLinked(skill)) => println!("skill já instalada: {skill}"),
            Ok(SkillOutcome::Conflict(skill)) => eprintln!(
                "aviso: {} já existe e não é a skill deste plugin; remova-a para instalar",
                skills_directory.join(skill).display()
            ),
            Err(error) => eprintln!("aviso: não instalei a skill de {name}: {error}"),
        }
    }
}

fn outcome_name(outcome: &PluginOutcome) -> &str {
    match outcome {
        PluginOutcome::Installed(name)
        | PluginOutcome::Replaced(name)
        | PluginOutcome::AlreadyInstalled(name) => name,
    }
}

fn run_list(arguments: &[String]) -> ExitCode {
    let source = match parse_list_arguments(arguments) {
        Ok(Some(source)) => source,
        Ok(None) => {
            OFFICIAL_PLUGINS.iter().for_each(|name| println!("{name}"));
            return ExitCode::SUCCESS;
        }
        Err(error) => return usage_failure(&error),
    };
    match available_plugins(&source) {
        Ok(names) => {
            names.iter().for_each(|name| println!("{name}"));
            ExitCode::SUCCESS
        }
        Err(error) => failure(&error),
    }
}

fn failure(error: &PluginsError) -> ExitCode {
    eprintln!("mosaic: {error}");
    ExitCode::from(EXIT_CODE_FAILURE)
}

fn report_outcome(outcome: &PluginOutcome) {
    match outcome {
        PluginOutcome::Installed(name) => println!("instalado: {name}"),
        PluginOutcome::Replaced(name) => println!("substituído: {name}"),
        PluginOutcome::AlreadyInstalled(name) => {
            println!("já instalado: {name} (use {FORCE_FLAG} para substituir)")
        }
    }
}

fn summary_line(outcomes: &[PluginOutcome], target: &Path) -> String {
    let written = outcomes
        .iter()
        .filter(|outcome| !matches!(outcome, PluginOutcome::AlreadyInstalled(_)))
        .count();
    let directory = target.display();
    match written {
        0 => format!("nenhum plugin instalado em {directory}"),
        1 => format!("1 plugin instalado em {directory}"),
        count => format!("{count} plugins instalados em {directory}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn summary_counts_only_written_plugins_and_agrees_in_number() {
        let target = Path::new("/destino");
        let installed = |name: &str| PluginOutcome::Installed(name.to_string());
        let kept = PluginOutcome::AlreadyInstalled("jira".to_string());
        assert_eq!(
            summary_line(&[], target),
            "nenhum plugin instalado em /destino"
        );
        assert_eq!(
            summary_line(&[kept, installed("github")], target),
            "1 plugin instalado em /destino"
        );
        assert_eq!(
            summary_line(&[installed("a"), installed("b")], target),
            "2 plugins instalados em /destino"
        );
    }

    #[test]
    fn parses_from_to_force_and_names() {
        let invocation = parse_install_arguments(&arguments(&[
            "--from", "/a", "--to", "/b", "--force", "jira", "github",
        ]))
        .unwrap();
        assert_eq!(
            invocation.source,
            InstallSource::Local {
                directory: PathBuf::from("/a"),
                names: arguments(&["jira", "github"])
            }
        );
        assert_eq!(invocation.target, PathBuf::from("/b"));
        assert_eq!(invocation.overwrite, Overwrite::Replace);
    }

    #[test]
    fn remote_defaults_use_official_sources_and_preserve_argument_order() {
        assert!(parse_install_arguments(&[]).is_err());
        let invocation =
            parse_install_arguments(&arguments(&["jira", "github", "jira", "--yes"])).unwrap();
        assert_eq!(
            invocation.source,
            InstallSource::Remote(vec![
                resolve_plugin_source("jira").unwrap(),
                resolve_plugin_source("github").unwrap()
            ])
        );
        assert!(invocation.target.ends_with("mosaic/plugins"));
        assert_eq!(invocation.overwrite, Overwrite::Keep);
        assert!(invocation.assume_yes);
        for args in [
            vec!["jira", "owner/repo"],
            vec!["owner/repo", "a/b"],
            vec!["--to", "--force", "jira"],
            vec!["--from", "/a", "owner/repo"],
        ] {
            assert!(parse_install_arguments(&arguments(&args)).is_err());
        }
    }

    #[test]
    fn an_unknown_flag_is_a_usage_error() {
        assert_eq!(
            parse_install_arguments(&arguments(&["--x"])).unwrap_err(),
            PluginsUsageError::UnknownFlag("--x".to_string())
        );
    }

    #[test]
    fn a_flag_without_value_is_a_usage_error() {
        assert_eq!(
            parse_install_arguments(&arguments(&["--to"])).unwrap_err(),
            PluginsUsageError::FlagWithoutValue(TO_FLAG)
        );
    }

    #[test]
    fn arguments_after_the_double_dash_are_names() {
        let invocation =
            parse_install_arguments(&arguments(&["--from", "/a", "--to", "/b", "--", "--force"]))
                .unwrap();
        assert_eq!(
            invocation.source,
            InstallSource::Local {
                directory: PathBuf::from("/a"),
                names: arguments(&["--force"])
            }
        );
        assert_eq!(invocation.overwrite, Overwrite::Keep);
    }

    #[test]
    fn list_without_from_uses_the_official_catalog() {
        assert_eq!(parse_list_arguments(&[]).unwrap(), None);
        assert_eq!(
            parse_list_arguments(&arguments(&["--from", "/a"])).unwrap(),
            Some(PathBuf::from("/a"))
        );
    }

    #[test]
    fn list_rejects_other_arguments() {
        assert_eq!(
            parse_list_arguments(&arguments(&["--to", "/b"])).unwrap_err(),
            PluginsUsageError::UnknownFlag("--to".to_string())
        );
        assert_eq!(
            parse_list_arguments(&arguments(&["--from"])).unwrap_err(),
            PluginsUsageError::FlagWithoutValue(FROM_FLAG)
        );
    }
}
