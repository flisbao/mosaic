use std::ffi::OsStr;
use std::process::Command;

use crate::application::ports::ReleaseError;
use crate::domain::language::{Language, LocalizedMessage};
use crate::infrastructure::process::ProcessError;

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

pub fn external_command(program: &OsStr) -> Command {
    let mut command = Command::new(program);
    command.env_clear();
    for name in INHERITED_ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

pub fn release_error(tool: &str, error: ProcessError, language: Language) -> ReleaseError {
    if error.program_not_found() {
        return ReleaseError::ToolMissing(language.pick(
            format!("'{tool}' was not found; install it to use this command"),
            format!("'{tool}' não foi encontrado; instale-o para usar este comando"),
        ));
    }
    if matches!(error, ProcessError::Cancelled) {
        return ReleaseError::Cancelled;
    }
    ReleaseError::Failed(sanitized_diagnostic(&format!(
        "{tool}: {}",
        error.message(language)
    )))
}

fn sanitized_diagnostic(text: &str) -> String {
    text.chars()
        .take(MAXIMUM_DIAGNOSTIC_CHARACTERS)
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::language::Language;

    #[test]
    fn only_the_documented_environment_is_inherited() {
        let command = external_command(OsStr::new("curl"));
        for (name, _) in command.get_envs() {
            let name = name.to_str().unwrap();
            assert!(INHERITED_ENVIRONMENT.contains(&name), "{name}");
        }
    }

    #[test]
    fn diagnostics_are_bounded_and_free_of_control_characters() {
        let message = format!("\u{1b}[31m{}", "x".repeat(2000));
        let sanitized = sanitized_diagnostic(&message);
        assert_eq!(sanitized.chars().count(), MAXIMUM_DIAGNOSTIC_CHARACTERS);
        assert!(!sanitized.chars().any(char::is_control));
    }

    #[test]
    fn a_missing_program_and_a_cancellation_have_their_own_errors() {
        let missing = ProcessError::Spawn {
            program: "curl".into(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        assert!(matches!(
            release_error("curl", missing, Language::BrazilianPortuguese),
            ReleaseError::ToolMissing(_)
        ));
        assert_eq!(
            release_error(
                "curl",
                ProcessError::Cancelled,
                Language::BrazilianPortuguese
            ),
            ReleaseError::Cancelled
        );
        assert!(matches!(
            release_error(
                "curl",
                ProcessError::OutputUnreadable,
                Language::BrazilianPortuguese
            ),
            ReleaseError::Failed(_)
        ));
    }
}
