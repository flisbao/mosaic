use std::ffi::OsString;
use std::process::Command;

const ALWAYS_INHERITED_NAMES: [&str; 10] = [
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "TERM", "TZ", "LANG", "LANGUAGE", "TMPDIR",
];
const ALWAYS_INHERITED_PREFIXES: [&str; 2] = ["LC_", "XDG_"];
const NON_INTERACTIVE_DEFAULTS: [(&str, &str); 2] = [
    ("GIT_TERMINAL_PROMPT", "0"),
    ("SSH_ASKPASS_REQUIRE", "never"),
];

pub fn restricted_environment(
    host: impl IntoIterator<Item = (OsString, OsString)>,
    extra_names: &[String],
) -> Vec<(OsString, OsString)> {
    let mut environment: Vec<(OsString, OsString)> = host
        .into_iter()
        .filter(|(name, _)| {
            name.to_str()
                .is_some_and(|name| is_inherited(name, extra_names))
        })
        .collect();
    for (name, value) in NON_INTERACTIVE_DEFAULTS {
        environment.retain(|(existing, _)| existing != name);
        environment.push((OsString::from(name), OsString::from(value)));
    }
    environment
}

fn is_inherited(name: &str, extra_names: &[String]) -> bool {
    ALWAYS_INHERITED_NAMES.contains(&name)
        || ALWAYS_INHERITED_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || extra_names.iter().any(|extra| extra == name)
}

pub fn restrict_inherited_environment(command: &mut Command, inherit_env: Option<&[String]>) {
    if let Some(extra_names) = inherit_env {
        command.env_clear();
        command.envs(restricted_environment(std::env::vars_os(), extra_names));
    }
}

pub fn is_valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_environment_keeps_only_basics_and_listed_names() {
        let environment = restricted_environment(
            [("PATH", "path"), ("TOKEN", "secret"), ("LC_ALL", "C")]
                .into_iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value))),
            &["TOKEN".to_string()],
        );
        assert!(environment.iter().any(|(name, _)| name == "TOKEN"));
        assert!(!environment.iter().any(|(name, _)| name == "AWS_SECRET"));
        assert!(environment
            .iter()
            .any(|(name, _)| name == "GIT_TERMINAL_PROMPT"));
    }

    #[test]
    fn variable_names_are_validated() {
        assert!(is_valid_variable_name("JIRA_TOKEN_1"));
        assert!(is_valid_variable_name("_x"));
        assert!(!is_valid_variable_name(""));
        assert!(!is_valid_variable_name("A=B"));
        assert!(!is_valid_variable_name("A B"));
    }
}
