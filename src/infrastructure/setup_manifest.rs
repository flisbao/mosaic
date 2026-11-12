use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::domain::language::Language;
use crate::domain::plugin_package::{PackageMetadata, PluginVersion};
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
    version: Option<String>,
    min_mosaic_version: Option<String>,
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
    #[serde(rename = "in", default)]
    any_of: Vec<String>,
    present: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChoice {
    value: String,
    description: String,
    description_en: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    kind: String,
    name: Option<String>,
    flag: Option<String>,
    prompt: String,
    prompt_en: Option<String>,
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
    text_en: Option<String>,
    when: Option<RawCondition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPresetInput {
    name: String,
    prompt: String,
    prompt_en: Option<String>,
    #[serde(default)]
    quote_list: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPreset {
    name: String,
    description: String,
    description_en: Option<String>,
    id: Option<String>,
    #[serde(default)]
    arguments: BTreeMap<String, String>,
    #[serde(default)]
    inputs: Vec<RawPresetInput>,
}

fn localized(portuguese: String, english: Option<String>, language: Language) -> String {
    match (language, english) {
        (Language::English, Some(english)) => english,
        _ => portuguese,
    }
}

pub fn read_manifest(path: &Path) -> Result<SetupManifest, ManifestError> {
    read_manifest_in(path, Language::BrazilianPortuguese)
}

pub fn read_manifest_in(path: &Path, language: Language) -> Result<SetupManifest, ManifestError> {
    read_manifest_with_version_in(path, language).map(|(manifest, _)| manifest)
}

pub fn read_manifest_with_version(
    path: &Path,
) -> Result<(SetupManifest, Option<PluginVersion>), ManifestError> {
    read_manifest_with_version_in(path, Language::BrazilianPortuguese)
}

pub fn read_manifest_with_version_in(
    path: &Path,
    language: Language,
) -> Result<(SetupManifest, Option<PluginVersion>), ManifestError> {
    let text = fs::read_to_string(path).map_err(|error| ManifestError::Unreadable {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    parse_document(&text, language)
        .map(|(manifest, metadata)| (manifest, metadata.map(|metadata| metadata.version)))
        .map_err(|reason| ManifestError::Invalid {
            path: path.display().to_string(),
            reason,
        })
}

#[derive(Debug)]
pub struct PackageManifest {
    pub setup: SetupManifest,
    pub metadata: PackageMetadata,
}

pub fn parse_manifest(text: &str) -> Result<SetupManifest, String> {
    parse_manifest_in(text, Language::BrazilianPortuguese)
}

pub fn parse_manifest_in(text: &str, language: Language) -> Result<SetupManifest, String> {
    parse_document(text, language).map(|(setup, _)| setup)
}

pub fn parse_package_manifest(text: &str) -> Result<PackageManifest, String> {
    parse_package_manifest_in(text, Language::BrazilianPortuguese)
}

pub fn parse_package_manifest_in(
    text: &str,
    language: Language,
) -> Result<PackageManifest, String> {
    let (setup, metadata) = parse_document(text, language)?;
    let metadata = metadata.ok_or_else(|| {
        language
            .pick(
                "a remote plugin needs version and min_mosaic_version",
                "plugin remoto precisa de version e min_mosaic_version",
            )
            .to_string()
    })?;
    Ok(PackageManifest { setup, metadata })
}

fn parse_document(
    text: &str,
    language: Language,
) -> Result<(SetupManifest, Option<PackageMetadata>), String> {
    let raw: RawManifest = toml::from_str(text).map_err(|error| error.message().to_string())?;
    let metadata = match (&raw.version, &raw.min_mosaic_version) {
        (None, None) => None,
        (Some(version), Some(minimum)) => Some(PackageMetadata {
            version: PluginVersion::parse(version).map_err(|error| error.to_string())?,
            min_mosaic_version: PluginVersion::parse(minimum).map_err(|error| error.to_string())?,
        }),
        _ => {
            return Err(language
                .pick(
                    "version and min_mosaic_version must appear together",
                    "version e min_mosaic_version precisam aparecer juntos",
                )
                .to_string())
        }
    };
    let steps = raw
        .steps
        .into_iter()
        .map(|step| convert_step(step, language))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_conditions_point_backwards(&steps, language)?;
    let presets: Vec<Preset> = raw
        .presets
        .into_iter()
        .map(|preset| convert_preset(preset, language))
        .collect();
    ensure_preset_flags_exist(&presets, &steps, language)?;
    Ok((
        SetupManifest {
            plugin: raw.plugin,
            script: raw.script,
            default_id: raw.default_id,
            interval_seconds: raw.interval_seconds,
            timeout_seconds: raw.timeout_seconds,
            steps,
            inherit_rules: raw
                .inherit
                .into_iter()
                .map(|rule| convert_inherit(rule, language))
                .collect::<Result<Vec<_>, _>>()?,
            presets,
            notes: raw
                .notes
                .into_iter()
                .map(|note| convert_note(note, language))
                .collect::<Result<Vec<_>, _>>()?,
        },
        metadata,
    ))
}

fn convert_condition(raw: RawCondition, language: Language) -> Result<Condition, String> {
    match (&raw.any_of.is_empty(), raw.present) {
        (true, None) => Err(language.pick(
            format!("condition on {} needs 'in' or 'present'", raw.variable),
            format!(
                "condição em {} precisa de 'in' ou de 'present'",
                raw.variable
            ),
        )),
        (false, Some(_)) => Err(language.pick(
            format!(
                "condition on {} cannot have both 'in' and 'present'",
                raw.variable
            ),
            format!(
                "condição em {} não pode ter 'in' e 'present' juntos",
                raw.variable
            ),
        )),
        _ => Ok(Condition {
            variable: raw.variable,
            any_of: raw.any_of,
            present: raw.present,
        }),
    }
}

fn convert_optional_condition(
    raw: Option<RawCondition>,
    language: Language,
) -> Result<Option<Condition>, String> {
    raw.map(|condition| convert_condition(condition, language))
        .transpose()
}

fn convert_step(raw: RawStep, language: Language) -> Result<Step, String> {
    match raw.kind.as_str() {
        KIND_SECRET => convert_secret(raw, language).map(Step::Secret),
        KIND_VARIABLE | KIND_ARGUMENT => convert_question(raw, language).map(Step::Question),
        other => Err(language.pick(
            format!("unknown step kind: {other}"),
            format!("tipo de passo desconhecido: {other}"),
        )),
    }
}

fn requirement_of(
    required: bool,
    required_when: Option<RawCondition>,
    language: Language,
) -> Result<Requirement, String> {
    match (required, required_when) {
        (true, _) => Ok(Requirement::Always),
        (false, Some(condition)) => convert_condition(condition, language).map(Requirement::When),
        (false, None) => Ok(Requirement::Optional),
    }
}

fn convert_secret(raw: RawStep, language: Language) -> Result<SecretQuestion, String> {
    let environment_key = raw.environment_key.ok_or_else(|| {
        language.pick(
            "secret step needs environment_key",
            "passo secret precisa de environment_key",
        )
    })?;
    let file_key = raw.file_key.ok_or_else(|| {
        language.pick(
            "secret step needs file_key",
            "passo secret precisa de file_key",
        )
    })?;
    Ok(SecretQuestion {
        prompt: localized(raw.prompt, raw.prompt_en, language),
        environment_key,
        file_key,
        names_are_list: raw.list,
        allow_both: raw.both,
        requirement: requirement_of(raw.required, raw.required_when, language)?,
        condition: convert_optional_condition(raw.when, language)?,
    })
}

fn convert_question(raw: RawStep, language: Language) -> Result<Question, String> {
    let target = match (raw.kind.as_str(), raw.name, raw.flag) {
        (KIND_VARIABLE, Some(name), None) => Target::Variable(name),
        (KIND_ARGUMENT, None, Some(flag)) => Target::Argument(flag),
        (KIND_VARIABLE, _, _) => {
            return Err(language
                .pick(
                    "variable step needs only name",
                    "passo variable precisa só de name",
                )
                .to_string())
        }
        _ => {
            return Err(language
                .pick(
                    "argument step needs only flag",
                    "passo argument precisa só de flag",
                )
                .to_string())
        }
    };
    let kind = convert_kind(
        raw.answer_type.as_deref(),
        raw.min,
        raw.max,
        raw.choices,
        target.key(),
        language,
    )?;
    Ok(Question {
        target,
        prompt: localized(raw.prompt, raw.prompt_en, language),
        kind,
        requirement: requirement_of(raw.required, raw.required_when, language)?,
        default: raw.default,
        advanced: raw.advanced,
        condition: convert_optional_condition(raw.when, language)?,
    })
}

fn convert_kind(
    answer_type: Option<&str>,
    minimum: Option<u64>,
    maximum: Option<u64>,
    choices: Vec<RawChoice>,
    key: &str,
    language: Language,
) -> Result<AnswerKind, String> {
    match answer_type.unwrap_or(TYPE_TEXT) {
        TYPE_TEXT => Ok(AnswerKind::Text),
        TYPE_LIST => Ok(AnswerKind::List),
        TYPE_INTEGER => match (minimum, maximum) {
            (Some(minimum), Some(maximum)) if minimum <= maximum => {
                Ok(AnswerKind::Integer { minimum, maximum })
            }
            _ => Err(language.pick(
                format!("{key}: integer needs min <= max"),
                format!("{key}: integer precisa de min <= max"),
            )),
        },
        TYPE_CHOICE if !choices.is_empty() => Ok(AnswerKind::Choice(
            choices
                .into_iter()
                .map(|choice| Choice {
                    value: choice.value,
                    description: localized(choice.description, choice.description_en, language),
                })
                .collect(),
        )),
        TYPE_CHOICE => Err(language.pick(
            format!("{key}: choice needs choices"),
            format!("{key}: choice precisa de choices"),
        )),
        other => Err(language.pick(
            format!("{key}: unknown type: {other}"),
            format!("{key}: type desconhecido: {other}"),
        )),
    }
}

fn convert_inherit(raw: RawInherit, language: Language) -> Result<InheritRule, String> {
    Ok(InheritRule {
        name: raw.name,
        condition: convert_optional_condition(raw.when, language)?,
    })
}

fn convert_note(raw: RawNote, language: Language) -> Result<Note, String> {
    Ok(Note {
        text: localized(raw.text, raw.text_en, language),
        condition: convert_optional_condition(raw.when, language)?,
    })
}

fn convert_preset(raw: RawPreset, language: Language) -> Preset {
    Preset {
        name: raw.name,
        description: localized(raw.description, raw.description_en, language),
        widget_id: raw.id,
        arguments: raw.arguments.into_iter().collect(),
        inputs: raw
            .inputs
            .into_iter()
            .map(|input| PresetInput {
                name: input.name,
                prompt: localized(input.prompt, input.prompt_en, language),
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

fn ensure_conditions_point_backwards(steps: &[Step], language: Language) -> Result<(), String> {
    let mut earlier_keys: Vec<&str> = Vec::new();
    for step in steps {
        let referenced = step
            .condition()
            .into_iter()
            .chain(requirement_condition(step));
        for condition in referenced {
            if !earlier_keys.contains(&condition.variable.as_str()) {
                return Err(language.pick(
                    format!(
                        "condition uses {} before that question exists",
                        condition.variable
                    ),
                    format!(
                        "condição usa {} antes de essa pergunta existir",
                        condition.variable
                    ),
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

fn ensure_preset_flags_exist(
    presets: &[Preset],
    steps: &[Step],
    language: Language,
) -> Result<(), String> {
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
                return Err(language.pick(
                    format!(
                        "preset {} uses {flag}, which the plugin does not ask",
                        preset.name
                    ),
                    format!(
                        "preset {} usa {flag}, que o plugin não pergunta",
                        preset.name
                    ),
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
    fn package_versions_are_required_only_for_remote_installation() {
        assert!(parse_manifest(MINIMAL).is_ok());
        assert!(parse_package_manifest(MINIMAL).is_err());
        let text = format!("version = \"0.0.4\"\nmin_mosaic_version = \"0.0.10\"\n{MINIMAL}");
        let package = parse_package_manifest(&text).unwrap();
        assert_eq!(package.metadata.version.to_string(), "0.0.4");
        assert_eq!(package.metadata.min_mosaic_version.to_string(), "0.0.10");
        assert_eq!(package.setup, parse_manifest(&text).unwrap());
    }

    #[test]
    fn one_version_field_is_invalid() {
        for field in ["version", "min_mosaic_version"] {
            let text = format!("{field} = \"0.0.4\"\n{MINIMAL}");
            assert!(parse_manifest(&text).is_err());
            assert!(parse_package_manifest(&text).is_err());
        }
    }

    #[test]
    fn malformed_versions_are_invalid_in_both_readers() {
        for bad in ["1.2", "01.2.3", "0.0.4-alpha"] {
            let text = format!("version = \"{bad}\"\nmin_mosaic_version = \"0.0.10\"\n{MINIMAL}");
            assert!(parse_manifest(&text).is_err());
            assert!(parse_package_manifest(&text).is_err());
        }
    }

    #[test]
    fn package_reader_preserves_setup_validation() {
        let text = format!("version = \"0.0.4\"\nmin_mosaic_version = \"0.0.10\"\n{MINIMAL}");
        assert!(
            parse_package_manifest(&text.replace("kind = \"variable\"", "kind = \"unknown\""))
                .is_err()
        );
        assert!(parse_package_manifest(
            &text.replace("variable = \"DEMO_METHOD\"", "variable = \"UNKNOWN\"")
        )
        .is_err());
        assert!(parse_package_manifest(&format!("unknown = 1\n{text}")).is_err());
    }

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
    fn a_presence_condition_is_read_and_a_vague_one_is_rejected() {
        let with_presence = MINIMAL.replace(
            "when = { variable = \"DEMO_METHOD\", in = [\"a\"] }",
            "when = { variable = \"DEMO_METHOD\", present = true }",
        );
        let manifest = parse_manifest(&with_presence).unwrap();
        assert_eq!(
            manifest.inherit_rules[0]
                .condition
                .as_ref()
                .unwrap()
                .present,
            Some(true)
        );
        let vague = MINIMAL.replace(
            "when = { variable = \"DEMO_METHOD\", in = [\"a\"] }",
            "when = { variable = \"DEMO_METHOD\" }",
        );
        assert!(parse_manifest(&vague)
            .unwrap_err()
            .contains("'in' ou de 'present'"));
        let both = MINIMAL.replace(
            "when = { variable = \"DEMO_METHOD\", in = [\"a\"] }",
            "when = { variable = \"DEMO_METHOD\", in = [\"a\"], present = true }",
        );
        assert!(parse_manifest(&both).unwrap_err().contains("juntos"));
    }

    #[test]
    fn an_unreadable_file_names_the_path() {
        let error = read_manifest(Path::new("/nao/existe/setup.toml")).unwrap_err();
        assert!(matches!(error, ManifestError::Unreadable { .. }));
        assert!(error.to_string().contains("/nao/existe/setup.toml"));
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    const BILINGUAL: &str = r#"
plugin = "demo"
script = "plugins/demo/demo.py"
default_id = "demo"
interval_seconds = 60
timeout_seconds = 5

[[steps]]
kind = "variable"
name = "DEMO_MODE"
prompt = "Modo"
prompt_en = "Mode"
type = "choice"
choices = [
  { value = "a", description = "Primeiro", description_en = "First" },
  { value = "b", description = "Segundo" },
]

[[notes]]
text = "Aviso"
text_en = "Notice"

[[presets]]
name = "quick"
description = "Rápido"
description_en = "Quick"
"#;

    fn first_prompt(manifest: &SetupManifest) -> &str {
        match &manifest.steps[0] {
            Step::Question(question) => &question.prompt,
            Step::Secret(secret) => &secret.prompt,
        }
    }

    #[test]
    fn english_uses_the_english_fields() {
        let manifest = parse_manifest_in(BILINGUAL, Language::English).unwrap();
        assert_eq!(first_prompt(&manifest), "Mode");
        assert_eq!(manifest.notes[0].text, "Notice");
        assert_eq!(manifest.presets[0].description, "Quick");
        match &manifest.steps[0] {
            Step::Question(question) => match &question.kind {
                AnswerKind::Choice(choices) => {
                    assert_eq!(choices[0].description, "First");
                    assert_eq!(choices[1].description, "Segundo");
                }
                other => panic!("{other:?}"),
            },
            Step::Secret(_) => panic!("expected a question"),
        }
    }

    #[test]
    fn portuguese_keeps_the_original_fields() {
        let manifest = parse_manifest_in(BILINGUAL, Language::BrazilianPortuguese).unwrap();
        assert_eq!(first_prompt(&manifest), "Modo");
        assert_eq!(manifest.notes[0].text, "Aviso");
        assert_eq!(manifest.presets[0].description, "Rápido");
    }

    #[test]
    fn a_manifest_with_only_the_old_fields_works_in_both_languages() {
        let old = BILINGUAL
            .replace("prompt_en = \"Mode\"\n", "")
            .replace(", description_en = \"First\"", "")
            .replace("text_en = \"Notice\"\n", "")
            .replace("description_en = \"Quick\"\n", "");
        for language in [Language::English, Language::BrazilianPortuguese] {
            let manifest = parse_manifest_in(&old, language).unwrap();
            assert_eq!(first_prompt(&manifest), "Modo");
            assert_eq!(manifest.notes[0].text, "Aviso");
        }
    }

    const HEADER: &str = "plugin = \"demo\"\nscript = \"plugins/demo/demo.py\"\ndefault_id = \"demo\"\ninterval_seconds = 60\ntimeout_seconds = 5\n";

    fn error_of(body: &str, language: Language) -> String {
        parse_manifest_in(&format!("{HEADER}{body}"), language).unwrap_err()
    }

    #[test]
    fn authoring_errors_are_translated() {
        let cases = [
            (
                "[[steps]]\nkind = \"nope\"\nprompt = \"x\"\n",
                "unknown step kind: nope",
                "tipo de passo desconhecido: nope",
            ),
            (
                "[[steps]]\nkind = \"variable\"\nname = \"A\"\nprompt = \"x\"\ntype = \"choice\"\n",
                "A: choice needs choices",
                "A: choice precisa de choices",
            ),
            (
                "[[steps]]\nkind = \"variable\"\nname = \"A\"\nprompt = \"x\"\nwhen = { variable = \"B\", in = [\"1\"] }\n",
                "condition uses B before that question exists",
                "condição usa B antes de essa pergunta existir",
            ),
            (
                "[[steps]]\nkind = \"variable\"\nname = \"A\"\nprompt = \"x\"\nwhen = { variable = \"B\" }\n",
                "condition on B needs 'in' or 'present'",
                "condição em B precisa de 'in' ou de 'present'",
            ),
            (
                "[[presets]]\nname = \"p\"\ndescription = \"d\"\narguments = { \"--x\" = \"1\" }\n",
                "preset p uses --x, which the plugin does not ask",
                "preset p usa --x, que o plugin não pergunta",
            ),
        ];
        for (body, english, portuguese) in cases {
            assert_eq!(error_of(body, Language::English), english);
            assert_eq!(error_of(body, Language::BrazilianPortuguese), portuguese);
        }
    }

    #[test]
    fn a_package_without_versions_is_refused_in_both_languages() {
        let text = HEADER.to_string();
        assert_eq!(
            parse_package_manifest_in(&text, Language::English).unwrap_err(),
            "a remote plugin needs version and min_mosaic_version"
        );
        assert_eq!(
            parse_package_manifest_in(&text, Language::BrazilianPortuguese).unwrap_err(),
            "plugin remoto precisa de version e min_mosaic_version"
        );
    }
}
