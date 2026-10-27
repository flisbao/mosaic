use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::domain::plugin_setup::{
    AnswerKind, Choice, Condition, InheritRule, Note, Preset, PresetInput, Question, Requirement,
    SecretQuestion, SetupManifest, Step, Target,
};

pub const MANIFEST_FILE_NAME: &str = "setup.toml";
const KIND_VARIABLE: &str = "variable";
const KIND_ARGUMENT: &str = "argument";
const KIND_SECRET: &str = "secret";
const TYPE_TEXT: &str = "text";
const TYPE_LIST: &str = "list";
const TYPE_INTEGER: &str = "integer";
const TYPE_CHOICE: &str = "choice";

#[derive(Debug, Error, PartialEq)]
pub enum ManifestError {
    #[error("{path}: {reason}")]
    Unreadable { path: String, reason: String },
    #[error("{path}: {reason}")]
    Invalid { path: String, reason: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    plugin: String,
    script: String,
    default_id: String,
    interval_seconds: u64,
    timeout_seconds: u64,
    #[serde(default)]
    #[allow(dead_code)]
    testing_only: Vec<String>,
    #[serde(default)]
    steps: Vec<RawStep>,
    #[serde(default)]
    inherit: Vec<RawInherit>,
    #[serde(default)]
    notes: Vec<RawNote>,
    #[serde(default)]
    presets: Vec<RawPreset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCondition {
    variable: String,
    #[serde(rename = "in")]
    any_of: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChoice {
    value: String,
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    kind: String,
    name: Option<String>,
    flag: Option<String>,
    prompt: String,
    #[serde(rename = "type")]
    answer_type: Option<String>,
    min: Option<u64>,
    max: Option<u64>,
    #[serde(default)]
    choices: Vec<RawChoice>,
    #[serde(default)]
    required: bool,
    required_when: Option<RawCondition>,
    default: Option<String>,
    #[serde(default)]
    advanced: bool,
    when: Option<RawCondition>,
    environment_key: Option<String>,
    file_key: Option<String>,
    #[serde(default)]
    list: bool,
    #[serde(default)]
    both: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInherit {
    name: String,
    when: Option<RawCondition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNote {
    text: String,
    when: Option<RawCondition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPresetInput {
    name: String,
    prompt: String,
    #[serde(default)]
    quote_list: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPreset {
    name: String,
    description: String,
    id: Option<String>,
    #[serde(default)]
    arguments: BTreeMap<String, String>,
    #[serde(default)]
    inputs: Vec<RawPresetInput>,
}

pub fn read_manifest(path: &Path) -> Result<SetupManifest, ManifestError> {
    let text = fs::read_to_string(path).map_err(|error| ManifestError::Unreadable {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    parse_manifest(&text).map_err(|reason| ManifestError::Invalid {
        path: path.display().to_string(),
        reason,
    })
}

pub fn parse_manifest(text: &str) -> Result<SetupManifest, String> {
    let raw: RawManifest = toml::from_str(text).map_err(|error| error.message().to_string())?;
    let steps = raw
        .steps
        .into_iter()
        .map(convert_step)
        .collect::<Result<Vec<_>, _>>()?;
    ensure_conditions_point_backwards(&steps)?;
    let presets: Vec<Preset> = raw.presets.into_iter().map(convert_preset).collect();
    ensure_preset_flags_exist(&presets, &steps)?;
    Ok(SetupManifest {
        plugin: raw.plugin,
        script: raw.script,
        default_id: raw.default_id,
        interval_seconds: raw.interval_seconds,
        timeout_seconds: raw.timeout_seconds,
        steps,
        inherit_rules: raw.inherit.into_iter().map(convert_inherit).collect(),
        presets,
        notes: raw.notes.into_iter().map(convert_note).collect(),
    })
}

fn convert_condition(raw: RawCondition) -> Condition {
    Condition {
        variable: raw.variable,
        any_of: raw.any_of,
    }
}

fn convert_step(raw: RawStep) -> Result<Step, String> {
    match raw.kind.as_str() {
        KIND_SECRET => convert_secret(raw).map(Step::Secret),
        KIND_VARIABLE | KIND_ARGUMENT => convert_question(raw).map(Step::Question),
        other => Err(format!("tipo de passo desconhecido: {other}")),
    }
}

fn requirement_of(required: bool, required_when: Option<RawCondition>) -> Requirement {
    match (required, required_when) {
        (true, _) => Requirement::Always,
        (false, Some(condition)) => Requirement::When(convert_condition(condition)),
        (false, None) => Requirement::Optional,
    }
}

fn convert_secret(raw: RawStep) -> Result<SecretQuestion, String> {
    let environment_key = raw
        .environment_key
        .ok_or("passo secret precisa de environment_key")?;
    let file_key = raw.file_key.ok_or("passo secret precisa de file_key")?;
    Ok(SecretQuestion {
        prompt: raw.prompt,
        environment_key,
        file_key,
        names_are_list: raw.list,
        allow_both: raw.both,
        requirement: requirement_of(raw.required, raw.required_when),
        condition: raw.when.map(convert_condition),
    })
}

fn convert_question(raw: RawStep) -> Result<Question, String> {
    let target = match (raw.kind.as_str(), raw.name, raw.flag) {
        (KIND_VARIABLE, Some(name), None) => Target::Variable(name),
        (KIND_ARGUMENT, None, Some(flag)) => Target::Argument(flag),
        (KIND_VARIABLE, _, _) => return Err("passo variable precisa só de name".to_string()),
        _ => return Err("passo argument precisa só de flag".to_string()),
    };
    let kind = convert_kind(
        raw.answer_type.as_deref(),
        raw.min,
        raw.max,
        raw.choices,
        target.key(),
    )?;
    Ok(Question {
        target,
        prompt: raw.prompt,
        kind,
        requirement: requirement_of(raw.required, raw.required_when),
        default: raw.default,
        advanced: raw.advanced,
        condition: raw.when.map(convert_condition),
    })
}

fn convert_kind(
    answer_type: Option<&str>,
    minimum: Option<u64>,
    maximum: Option<u64>,
    choices: Vec<RawChoice>,
    key: &str,
) -> Result<AnswerKind, String> {
    match answer_type.unwrap_or(TYPE_TEXT) {
        TYPE_TEXT => Ok(AnswerKind::Text),
        TYPE_LIST => Ok(AnswerKind::List),
        TYPE_INTEGER => match (minimum, maximum) {
            (Some(minimum), Some(maximum)) if minimum <= maximum => {
                Ok(AnswerKind::Integer { minimum, maximum })
            }
            _ => Err(format!("{key}: integer precisa de min <= max")),
        },
        TYPE_CHOICE if !choices.is_empty() => Ok(AnswerKind::Choice(
            choices
                .into_iter()
                .map(|choice| Choice {
                    value: choice.value,
                    description: choice.description,
                })
                .collect(),
        )),
        TYPE_CHOICE => Err(format!("{key}: choice precisa de choices")),
        other => Err(format!("{key}: type desconhecido: {other}")),
    }
}

fn convert_inherit(raw: RawInherit) -> InheritRule {
    InheritRule {
        name: raw.name,
        condition: raw.when.map(convert_condition),
    }
}

fn convert_note(raw: RawNote) -> Note {
    Note {
        text: raw.text,
        condition: raw.when.map(convert_condition),
    }
}

fn convert_preset(raw: RawPreset) -> Preset {
    Preset {
        name: raw.name,
        description: raw.description,
        widget_id: raw.id,
        arguments: raw.arguments.into_iter().collect(),
        inputs: raw
            .inputs
            .into_iter()
            .map(|input| PresetInput {
                name: input.name,
                prompt: input.prompt,
                quote_list: input.quote_list,
            })
            .collect(),
    }
}

fn question_keys(step: &Step) -> Vec<&str> {
    match step {
        Step::Question(question) => vec![question.target.key()],
        Step::Secret(secret) => vec![&secret.environment_key, &secret.file_key],
    }
}

fn ensure_conditions_point_backwards(steps: &[Step]) -> Result<(), String> {
    let mut earlier_keys: Vec<&str> = Vec::new();
    for step in steps {
        let referenced = step
            .condition()
            .into_iter()
            .chain(requirement_condition(step));
        for condition in referenced {
            if !earlier_keys.contains(&condition.variable.as_str()) {
                return Err(format!(
                    "condição usa {} antes de essa pergunta existir",
                    condition.variable
                ));
            }
        }
        earlier_keys.extend(question_keys(step));
    }
    Ok(())
}

fn requirement_condition(step: &Step) -> Option<&Condition> {
    let requirement = match step {
        Step::Question(question) => &question.requirement,
        Step::Secret(secret) => &secret.requirement,
    };
    match requirement {
        Requirement::When(condition) => Some(condition),
        _ => None,
    }
}

fn ensure_preset_flags_exist(presets: &[Preset], steps: &[Step]) -> Result<(), String> {
    let flags: Vec<&str> = steps
        .iter()
        .filter_map(|step| match step {
            Step::Question(question) => match &question.target {
                Target::Argument(flag) => Some(flag.as_str()),
                Target::Variable(_) => None,
            },
            Step::Secret(_) => None,
        })
        .collect();
    for preset in presets {
        for (flag, _) in &preset.arguments {
            if !flags.contains(&flag.as_str()) {
                return Err(format!(
                    "preset {} usa {flag}, que o plugin não pergunta",
                    preset.name
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
plugin = "demo"
script = "plugins/demo/demo.py"
default_id = "demo"
interval_seconds = 120
timeout_seconds = 30
testing_only = ["DEMO_URL"]

[[steps]]
kind = "variable"
name = "DEMO_METHOD"
prompt = "Método"
type = "choice"
default = "a"
choices = [{ value = "a", description = "primeiro" }, { value = "b", description = "segundo" }]

[[steps]]
kind = "variable"
name = "DEMO_TENANT"
prompt = "Tenant"
when = { variable = "DEMO_METHOD", in = ["b"] }
required_when = { variable = "DEMO_METHOD", in = ["b"] }

[[steps]]
kind = "argument"
flag = "--days"
prompt = "Dias"
type = "integer"
min = 1
max = 90
default = "14"
advanced = true

[[steps]]
kind = "secret"
prompt = "Token"
environment_key = "DEMO_TOKEN_ENV"
file_key = "DEMO_TOKEN_FILE"
required = true

[[inherit]]
name = "DBUS_SESSION_BUS_ADDRESS"
when = { variable = "DEMO_METHOD", in = ["a"] }

[[notes]]
text = "faça o login"

[[presets]]
name = "curto"
description = "poucos dias"
id = "demo-curto"
arguments = { "--days" = "3" }
"#;

    #[test]
    fn a_complete_manifest_is_converted_to_the_domain() {
        let manifest = parse_manifest(MINIMAL).unwrap();
        assert_eq!(manifest.plugin, "demo");
        assert_eq!(manifest.steps.len(), 4);
        assert_eq!(manifest.inherit_rules.len(), 1);
        assert_eq!(manifest.notes[0].text, "faça o login");
        assert_eq!(
            manifest.presets[0].arguments,
            vec![("--days".to_string(), "3".to_string())]
        );
        match &manifest.steps[2] {
            Step::Question(question) => {
                assert_eq!(question.target, Target::Argument("--days".to_string()));
                assert!(question.advanced);
                assert_eq!(
                    question.kind,
                    AnswerKind::Integer {
                        minimum: 1,
                        maximum: 90
                    }
                );
            }
            other => panic!("esperava uma pergunta, veio {other:?}"),
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = MINIMAL.replace(
            "default_id = \"demo\"",
            "default_id = \"demo\"\nsurprise = 1",
        );
        assert!(parse_manifest(&text).is_err());
    }

    #[test]
    fn a_condition_cannot_point_to_a_later_question() {
        let text = r#"
plugin = "demo"
script = "s"
default_id = "d"
interval_seconds = 30
timeout_seconds = 5

[[steps]]
kind = "variable"
name = "A"
prompt = "a"
when = { variable = "B", in = ["x"] }

[[steps]]
kind = "variable"
name = "B"
prompt = "b"
"#;
        assert!(parse_manifest(text)
            .unwrap_err()
            .contains("antes de essa pergunta existir"));
    }

    #[test]
    fn integer_without_a_range_and_choice_without_options_are_rejected() {
        let integer = MINIMAL.replace("min = 1\n", "");
        assert!(parse_manifest(&integer).unwrap_err().contains("min <= max"));
        let choice = MINIMAL.replace(
            "choices = [{ value = \"a\", description = \"primeiro\" }, { value = \"b\", description = \"segundo\" }]",
            "",
        );
        assert!(parse_manifest(&choice)
            .unwrap_err()
            .contains("precisa de choices"));
    }

    #[test]
    fn a_preset_cannot_set_a_flag_the_plugin_does_not_ask() {
        let text = MINIMAL.replace("\"--days\" = \"3\"", "\"--other\" = \"3\"");
        assert!(parse_manifest(&text).unwrap_err().contains("--other"));
    }

    #[test]
    fn an_unreadable_file_names_the_path() {
        let error = read_manifest(Path::new("/nao/existe/setup.toml")).unwrap_err();
        assert!(matches!(error, ManifestError::Unreadable { .. }));
        assert!(error.to_string().contains("/nao/existe/setup.toml"));
    }
}
