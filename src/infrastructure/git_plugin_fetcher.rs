use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::process::{run_process, CancelFlag, ProcessLimits};
use crate::application::ports::{PluginFetchError, PluginFetcher};
use crate::domain::language::{Language, LocalizedMessage};
use crate::domain::plugin_package::GitHubSource;

const CLONE_TIMEOUT: Duration = Duration::from_secs(120);
const MAXIMUM_CLONE_OUTPUT_BYTES: usize = 1_048_576;
const MAXIMUM_DIAGNOSTIC_CHARACTERS: usize = 500;
const INHERITED_ENVIRONMENT: [&str; 15] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];
const FIXED_ENVIRONMENT: [(&str, &str); 5] = [
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_LFS_SKIP_SMUDGE", "1"),
];
const GIT_CONFIGURATION: [&str; 7] = [
    "core.hooksPath=/dev/null",
    "credential.helper=",
    "core.askPass=",
    "protocol.allow=never",
    "protocol.https.allow=always",
    "http.followRedirects=false",
    "core.autocrlf=false",
];

#[derive(Default)]
pub struct GitPluginFetcher {
    language: Language,
}

impl GitPluginFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }
}

impl PluginFetcher for GitPluginFetcher {
    fn fetch(&self, source: &GitHubSource, destination: &Path) -> Result<(), PluginFetchError> {
        fetch_with_program(OsStr::new("git"), source, destination, self.language)
    }
}

fn clone_limits() -> ProcessLimits {
    ProcessLimits {
        timeout: CLONE_TIMEOUT,
        maximum_output_bytes: MAXIMUM_CLONE_OUTPUT_BYTES,
    }
}

fn build_clone_command(source: &GitHubSource, destination: &Path, template: &Path) -> Command {
    let mut command = Command::new("git");
    command.env_clear();
    for name in INHERITED_ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in FIXED_ENVIRONMENT {
        command.env(name, value);
    }
    for configuration in GIT_CONFIGURATION {
        command.args(["-c", configuration]);
    }
    command
        .args([
            "clone",
            "--depth",
            "1",
            "--single-branch",
            "--no-tags",
            "--no-recurse-submodules",
            "--template",
        ])
        .arg(template)
        .arg("--")
        .arg(source.repository_url())
        .arg(destination)
        .current_dir(destination.parent().unwrap_or_else(|| Path::new("/")));
    command
}

fn fetch_with_program(
    program: &OsStr,
    source: &GitHubSource,
    destination: &Path,
    language: Language,
) -> Result<(), PluginFetchError> {
    let template =
        tempfile::tempdir().map_err(|error| PluginFetchError::Failed(error.to_string()))?;
    let configured = build_clone_command(source, destination, template.path());
    let mut command = Command::new(program);
    command.args(configured.get_args()).env_clear();
    for (key, value) in configured.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        }
    }
    if let Some(directory) = configured.get_current_dir() {
        command.current_dir(directory);
    }
    run_process(command, None, &clone_limits(), &CancelFlag::default())
        .map(|_| ())
        .map_err(|error| {
            if error.program_not_found() {
                PluginFetchError::Unavailable(
                    language
                        .pick(
                            "Git not found; install git to download plugins, or use --from DIR",
                            "Git não encontrado; instale git para baixar plugins, ou use --from DIR",
                        )
                        .into(),
                )
            } else {
                let diagnostic: String = error
                    .message(language)
                    .chars()
                    .take(MAXIMUM_DIAGNOSTIC_CHARACTERS)
                    .map(|character| {
                        if character.is_control() {
                            '?'
                        } else {
                            character
                        }
                    })
                    .collect();
                PluginFetchError::Failed(diagnostic)
            }
        })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn clone_command_is_noninteractive_and_restricted_to_https() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("repo");
        let template = root.path().join("empty-template");
        let command = build_clone_command(
            &GitHubSource::parse("owner/repo/nested").unwrap(),
            &destination,
            &template,
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        for expected in [
            "--depth",
            "1",
            "--single-branch",
            "--no-tags",
            "--no-recurse-submodules",
            "https://github.com/owner/repo.git",
            "core.hooksPath=/dev/null",
            "credential.helper=",
            "core.askPass=",
            "protocol.allow=never",
            "protocol.https.allow=always",
            "http.followRedirects=false",
        ] {
            assert!(args.contains(&expected), "{expected}");
        }
        assert_eq!(command.get_current_dir(), Some(root.path()));
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment[std::ffi::OsStr::new("GIT_TERMINAL_PROMPT")],
            Some(std::ffi::OsStr::new("0"))
        );
        assert_eq!(
            environment[std::ffi::OsStr::new("GIT_CONFIG_GLOBAL")],
            Some(std::ffi::OsStr::new("/dev/null"))
        );
        for (name, _) in command.get_envs() {
            let name = name.to_str().unwrap();
            assert!(
                INHERITED_ENVIRONMENT.contains(&name)
                    || FIXED_ENVIRONMENT.iter().any(|(key, _)| key == &name),
                "{name}"
            );
        }
        assert_eq!(clone_limits().timeout, std::time::Duration::from_secs(120));
        assert_eq!(clone_limits().maximum_output_bytes, 1_048_576);
    }

    #[test]
    fn git_errors_are_sanitized_and_bounded_and_missing_git_is_distinct() {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("git");
        fs::write(
            &program,
            format!(
                "#!/bin/sh\nprintf '\\033[31m{}' >&2\nexit 1\n",
                "x".repeat(2000)
            ),
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let source = GitHubSource::parse("owner/repo").unwrap();
        let error = fetch_with_program(
            program.as_os_str(),
            &source,
            &root.path().join("repo"),
            Language::BrazilianPortuguese,
        )
        .unwrap_err();
        match error {
            PluginFetchError::Failed(message) => {
                assert!(message.chars().count() <= 500);
                assert!(!message.chars().any(char::is_control));
            }
            other => panic!("{other}"),
        }
        assert!(matches!(
            fetch_with_program(
                root.path().join("missing").as_os_str(),
                &source,
                &root.path().join("repo"),
                Language::BrazilianPortuguese
            ),
            Err(PluginFetchError::Unavailable(_))
        ));
    }
}
