use std::collections::BTreeMap;

use thiserror::Error;

pub const LIST_SEPARATOR: char = ',';

#[derive(Debug, Clone, PartialEq)]
pub struct SetupManifest {
    pub plugin: String,
    pub script: String,
    pub default_id: String,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub steps: Vec<Step>,
    pub inherit_rules: Vec<InheritRule>,
    pub presets: Vec<Preset>,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Question(Question),
    Secret(SecretQuestion),
}

impl Step {
    pub fn condition(&self) -> Option<&Condition> {
        match self {
            Step::Question(question) => question.condition.as_ref(),
            Step::Secret(secret) => secret.condition.as_ref(),
        }
    }

    pub fn is_advanced(&self) -> bool {
        match self {
            Step::Question(question) => question.advanced,
            Step::Secret(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub variable: String,
    pub any_of: Vec<String>,
}

impl Condition {
    pub fn holds(&self, values: &BTreeMap<String, String>) -> bool {
        values
            .get(&self.variable)
            .is_some_and(|value| self.any_of.contains(value))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Requirement {
    Optional,
    Always,
    When(Condition),
}

impl Requirement {
    pub fn applies(&self, values: &BTreeMap<String, String>) -> bool {
        match self {
            Requirement::Optional => false,
            Requirement::Always => true,
            Requirement::When(condition) => condition.holds(values),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub value: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnswerKind {
    Text,
    List,
    Integer { minimum: u64, maximum: u64 },
    Choice(Vec<Choice>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Argument(String),
    Variable(String),
}

impl Target {
    pub fn key(&self) -> &str {
        match self {
            Target::Argument(flag) => flag,
            Target::Variable(name) => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub target: Target,
    pub prompt: String,
    pub kind: AnswerKind,
    pub requirement: Requirement,
    pub default: Option<String>,
    pub advanced: bool,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecretQuestion {
    pub prompt: String,
    pub environment_key: String,
    pub file_key: String,
    pub names_are_list: bool,
    pub allow_both: bool,
    pub requirement: Requirement,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InheritRule {
    pub name: String,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub text: String,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresetInput {
    pub name: String,
    pub prompt: String,
    pub quote_list: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub description: String,
    pub widget_id: Option<String>,
    pub arguments: Vec<(String, String)>,
    pub inputs: Vec<PresetInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnswerError {
    #[error("responda um número de {minimum} a {maximum}")]
    NotAnInteger { minimum: u64, maximum: u64 },
    #[error("o valor precisa estar entre {minimum} e {maximum}")]
    OutOfRange { minimum: u64, maximum: u64 },
    #[error("escolha um número de 1 a {count} ou um dos valores listados")]
    UnknownChoice { count: usize },
    #[error("informe pelo menos um item")]
    EmptyList,
    #[error("\"{0}\" não é um nome de variável de ambiente válido")]
    InvalidEnvironmentName(String),
}

impl AnswerKind {
    pub fn normalize(&self, text: &str) -> Result<String, AnswerError> {
        let text = text.trim();
        match self {
            AnswerKind::Text => Ok(text.to_string()),
            AnswerKind::List => normalize_list(text),
            AnswerKind::Integer { minimum, maximum } => normalize_integer(text, *minimum, *maximum),
            AnswerKind::Choice(choices) => normalize_choice(text, choices),
        }
    }
}

pub fn normalize_list(text: &str) -> Result<String, AnswerError> {
    let items = split_list(text);
    if items.is_empty() {
        return Err(AnswerError::EmptyList);
    }
    Ok(items.join(&LIST_SEPARATOR.to_string()))
}

pub fn split_list(text: &str) -> Vec<String> {
    text.split(LIST_SEPARATOR)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_integer(text: &str, minimum: u64, maximum: u64) -> Result<String, AnswerError> {
    let value: u64 = text
        .parse()
        .map_err(|_| AnswerError::NotAnInteger { minimum, maximum })?;
    if value < minimum || value > maximum {
        return Err(AnswerError::OutOfRange { minimum, maximum });
    }
    Ok(value.to_string())
}

fn normalize_choice(text: &str, choices: &[Choice]) -> Result<String, AnswerError> {
    if let Some(choice) = choices.iter().find(|choice| choice.value == text) {
        return Ok(choice.value.clone());
    }
    let position: Option<usize> = text.parse().ok();
    match position {
        Some(number) if (1..=choices.len()).contains(&number) => {
            Ok(choices[number - 1].value.clone())
        }
        _ => Err(AnswerError::UnknownChoice {
            count: choices.len(),
        }),
    }
}

pub fn ensure_environment_name(name: &str) -> Result<(), AnswerError> {
    let mut characters = name.chars();
    let starts_well = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    if starts_well && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_') {
        Ok(())
    } else {
        Err(AnswerError::InvalidEnvironmentName(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn method_choices() -> AnswerKind {
        AnswerKind::Choice(vec![
            Choice {
                value: "azure-cli".to_string(),
                description: "usa o az login".to_string(),
            },
            Choice {
                value: "device-code".to_string(),
                description: "login no terminal".to_string(),
            },
        ])
    }

    #[test]
    fn condition_holds_only_when_the_variable_has_one_of_the_values() {
        let condition = Condition {
            variable: "METHOD".to_string(),
            any_of: vec!["a".to_string(), "b".to_string()],
        };
        assert!(condition.holds(&values(&[("METHOD", "b")])));
        assert!(!condition.holds(&values(&[("METHOD", "c")])));
        assert!(!condition.holds(&values(&[])));
    }

    #[test]
    fn requirement_applies_always_never_or_by_condition() {
        let condition = Condition {
            variable: "METHOD".to_string(),
            any_of: vec!["a".to_string()],
        };
        let current = values(&[("METHOD", "a")]);
        assert!(Requirement::Always.applies(&current));
        assert!(!Requirement::Optional.applies(&current));
        assert!(Requirement::When(condition).applies(&current));
    }

    #[test]
    fn list_answers_are_trimmed_and_rejoined() {
        assert_eq!(
            AnswerKind::List.normalize(" a ,b,, c ").unwrap(),
            "a,b,c".to_string()
        );
        assert_eq!(
            AnswerKind::List.normalize(" , ").unwrap_err(),
            AnswerError::EmptyList
        );
    }

    #[test]
    fn integer_answers_must_be_inside_the_range() {
        let kind = AnswerKind::Integer {
            minimum: 1,
            maximum: 90,
        };
        assert_eq!(kind.normalize(" 14 ").unwrap(), "14");
        assert_eq!(
            kind.normalize("0").unwrap_err(),
            AnswerError::OutOfRange {
                minimum: 1,
                maximum: 90
            }
        );
        assert_eq!(
            kind.normalize("muitos").unwrap_err(),
            AnswerError::NotAnInteger {
                minimum: 1,
                maximum: 90
            }
        );
    }

    #[test]
    fn choice_accepts_the_position_or_the_value() {
        let kind = method_choices();
        assert_eq!(kind.normalize("2").unwrap(), "device-code");
        assert_eq!(kind.normalize("azure-cli").unwrap(), "azure-cli");
        assert_eq!(
            kind.normalize("3").unwrap_err(),
            AnswerError::UnknownChoice { count: 2 }
        );
        assert_eq!(
            kind.normalize("pat").unwrap_err(),
            AnswerError::UnknownChoice { count: 2 }
        );
    }

    #[test]
    fn environment_names_follow_the_shell_convention() {
        assert!(ensure_environment_name("JIRA_TOKEN_EMPRESA").is_ok());
        assert!(ensure_environment_name("_x1").is_ok());
        assert!(ensure_environment_name("1abc").is_err());
        assert!(ensure_environment_name("A-B").is_err());
        assert!(ensure_environment_name("").is_err());
    }
}
