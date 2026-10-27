use std::collections::BTreeMap;

use thiserror::Error;

use crate::application::ports::{PromptError, Prompter};
use crate::domain::plugin_setup::{
    ensure_environment_name, normalize_list, split_list, AnswerKind, Condition, Preset, Question,
    SecretQuestion, SetupManifest, Step, Target,
};
use crate::domain::widget_draft::WidgetDraft;

const MINIMUM_INTERVAL_SECONDS: u64 = 30;
const MAXIMUM_INTERVAL_SECONDS: u64 = 86_400;
const MINIMUM_TIMEOUT_SECONDS: u64 = 1;
const MAXIMUM_TIMEOUT_SECONDS: u64 = 3_600;
const SECRET_FROM_ENVIRONMENT: &str = "1";
const SECRET_FROM_FILE: &str = "2";
const SECRET_FROM_BOTH: &str = "3";

#[derive(Debug, Error, PartialEq)]
pub enum SetupError {
    #[error(transparent)]
    Prompt(#[from] PromptError),
}

#[derive(Debug, Clone, Default)]
pub struct SetupOptions {
    pub include_advanced: bool,
    pub existing_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupResult {
    pub draft: WidgetDraft,
    pub notes: Vec<String>,
}

pub fn run_setup(
    manifest: &SetupManifest,
    options: &SetupOptions,
    prompter: &mut dyn Prompter,
) -> Result<SetupResult, SetupError> {
    let preset = choose_preset(manifest, prompter)?;
    let preset_values = match &preset {
        Some(preset) => resolve_preset(preset, prompter)?,
        None => BTreeMap::new(),
    };
    let widget_id = ask_widget_id(manifest, preset.as_ref(), options, prompter)?;
    let mut interview = Interview::new(manifest, preset_values);
    for step in &manifest.steps {
        interview.run_step(step, options, prompter)?;
    }
    let interval_seconds = ask_seconds(
        "Intervalo entre atualizações, em segundos",
        manifest.interval_seconds,
        MINIMUM_INTERVAL_SECONDS..=MAXIMUM_INTERVAL_SECONDS,
        prompter,
    )?;
    let timeout_seconds = ask_seconds(
        "Tempo máximo de cada execução, em segundos",
        manifest.timeout_seconds,
        MINIMUM_TIMEOUT_SECONDS..=MAXIMUM_TIMEOUT_SECONDS,
        prompter,
    )?;
    Ok(interview.finish(widget_id, interval_seconds, timeout_seconds))
}

struct Interview<'a> {
    manifest: &'a SetupManifest,
    preset_values: BTreeMap<String, String>,
    values: BTreeMap<String, String>,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    inherit_env: Vec<String>,
}

impl<'a> Interview<'a> {
    fn new(manifest: &'a SetupManifest, preset_values: BTreeMap<String, String>) -> Self {
        let defaults = manifest.steps.iter().filter_map(|step| match step {
            Step::Question(question) => question
                .default
                .clone()
                .map(|default| (question.target.key().to_string(), default)),
            Step::Secret(_) => None,
        });
        Self {
            manifest,
            values: defaults.collect(),
            preset_values,
            arguments: vec![manifest.script.clone()],
            environment: Vec::new(),
            inherit_env: Vec::new(),
        }
    }

    fn run_step(
        &mut self,
        step: &Step,
        options: &SetupOptions,
        prompter: &mut dyn Prompter,
    ) -> Result<(), SetupError> {
        let holds = step
            .condition()
            .is_none_or(|condition| condition.holds(&self.values));
        if !holds || (step.is_advanced() && !options.include_advanced) {
            return Ok(());
        }
        match step {
            Step::Question(question) => self.ask_question(question, prompter),
            Step::Secret(secret) => self.ask_secret(secret, prompter),
        }
    }

    fn ask_question(
        &mut self,
        question: &Question,
        prompter: &mut dyn Prompter,
    ) -> Result<(), SetupError> {
        let key = question.target.key().to_string();
        let preset_value = self.preset_values.get(&key).cloned();
        let default = preset_value.clone().or_else(|| question.default.clone());
        let required = question.requirement.applies(&self.values);
        let Some(answer) = ask_value(question, default.as_deref(), required, prompter)? else {
            return Ok(());
        };
        let was_typed = answer.typed || preset_value.is_some();
        self.values.insert(key.clone(), answer.value.clone());
        if was_typed {
            match &question.target {
                Target::Argument(flag) => {
                    self.arguments.push(flag.clone());
                    self.arguments.push(answer.value);
                }
                Target::Variable(name) => self.environment.push((name.clone(), answer.value)),
            }
        }
        Ok(())
    }

    fn ask_secret(
        &mut self,
        secret: &SecretQuestion,
        prompter: &mut dyn Prompter,
    ) -> Result<(), SetupError> {
        let required = secret.requirement.applies(&self.values);
        let Some(source) = ask_secret_source(secret, required, prompter)? else {
            return Ok(());
        };
        if source != SECRET_FROM_FILE {
            let names = ask_secret_names(secret, prompter)?;
            self.values
                .insert(secret.environment_key.clone(), names.clone());
            self.inherit_env.extend(split_list(&names));
            self.environment
                .push((secret.environment_key.clone(), names));
        }
        if source != SECRET_FROM_ENVIRONMENT {
            let path =
                ask_required_text(&format!("Caminho do arquivo ({})", secret.prompt), prompter)?;
            self.values.insert(secret.file_key.clone(), path.clone());
            self.environment.push((secret.file_key.clone(), path));
        }
        Ok(())
    }

    fn finish(
        mut self,
        widget_id: String,
        interval_seconds: u64,
        timeout_seconds: u64,
    ) -> SetupResult {
        for rule in &self.manifest.inherit_rules {
            if holds(rule.condition.as_ref(), &self.values) {
                self.inherit_env.push(rule.name.clone());
            }
        }
        let mut seen = Vec::new();
        self.inherit_env.retain(|name| {
            let first_time = !seen.contains(name);
            seen.push(name.clone());
            first_time
        });
        let notes = self
            .manifest
            .notes
            .iter()
            .filter(|note| holds(note.condition.as_ref(), &self.values))
            .map(|note| note.text.clone())
            .collect();
        SetupResult {
            draft: WidgetDraft {
                id: widget_id,
                arguments: self.arguments,
                interval_seconds,
                timeout_seconds,
                inherit_env: self.inherit_env,
                environment: self.environment,
            },
            notes,
        }
    }
}

fn holds(condition: Option<&Condition>, values: &BTreeMap<String, String>) -> bool {
    condition.is_none_or(|condition| condition.holds(values))
}

struct Answer {
    value: String,
    typed: bool,
}

fn ask_value(
    question: &Question,
    default: Option<&str>,
    required: bool,
    prompter: &mut dyn Prompter,
) -> Result<Option<Answer>, SetupError> {
    if let AnswerKind::Choice(choices) = &question.kind {
        let listing: Vec<String> = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                format!("  {}) {}: {}", index + 1, choice.value, choice.description)
            })
            .collect();
        prompter.tell(&format!("{}\n{}", question.prompt, listing.join("\n")));
    }
    let label = question_label(question, default, required);
    loop {
        let line = prompter.ask(&label)?;
        let line = line.trim();
        if line.is_empty() {
            match default {
                Some(default) => {
                    return Ok(Some(Answer {
                        value: default.to_string(),
                        typed: false,
                    }))
                }
                None if required => {
                    prompter.tell("Esta resposta é obrigatória.");
                    continue;
                }
                None => return Ok(None),
            }
        }
        match question.kind.normalize(line) {
            Ok(value) => return Ok(Some(Answer { value, typed: true })),
            Err(error) => prompter.tell(&error.to_string()),
        }
    }
}

fn question_label(question: &Question, default: Option<&str>, required: bool) -> String {
    let hint = match &question.kind {
        AnswerKind::List => " (separados por vírgula)".to_string(),
        AnswerKind::Integer { minimum, maximum } => format!(" ({minimum} a {maximum})"),
        AnswerKind::Choice(_) | AnswerKind::Text => String::new(),
    };
    let prompt = match question.kind {
        AnswerKind::Choice(_) => "Escolha",
        _ => question.prompt.as_str(),
    };
    let suffix = match (default, required) {
        (Some(default), _) => format!(" [{default}]"),
        (None, true) => " (obrigatório)".to_string(),
        (None, false) => " (Enter para pular)".to_string(),
    };
    format!("{prompt}{hint}{suffix}: ")
}

fn ask_secret_source(
    secret: &SecretQuestion,
    required: bool,
    prompter: &mut dyn Prompter,
) -> Result<Option<String>, SetupError> {
    let mut listing = format!(
        "{}\nO valor nunca é pedido aqui: o plugin lê de uma variável de ambiente ou de um arquivo.\n  {SECRET_FROM_ENVIRONMENT}) nome de variável de ambiente\n  {SECRET_FROM_FILE}) caminho de um arquivo",
        secret.prompt
    );
    let mut valid = vec![SECRET_FROM_ENVIRONMENT, SECRET_FROM_FILE];
    if secret.allow_both {
        listing.push_str(&format!("\n  {SECRET_FROM_BOTH}) os dois"));
        valid.push(SECRET_FROM_BOTH);
    }
    prompter.tell(&listing);
    let suffix = if required {
        " (obrigatório)"
    } else {
        " (Enter para pular)"
    };
    loop {
        let line = prompter.ask(&format!("Escolha{suffix}: "))?;
        let line = line.trim();
        if line.is_empty() && !required {
            return Ok(None);
        }
        if let Some(chosen) = valid.iter().find(|candidate| **candidate == line) {
            return Ok(Some((*chosen).to_string()));
        }
        prompter.tell("Responda com um dos números listados.");
    }
}

fn ask_secret_names(
    secret: &SecretQuestion,
    prompter: &mut dyn Prompter,
) -> Result<String, SetupError> {
    loop {
        let label = if secret.names_are_list {
            "Nomes das variáveis (separados por vírgula)"
        } else {
            "Nome da variável"
        };
        let line = ask_required_text(label, prompter)?;
        let names = if secret.names_are_list {
            normalize_list(&line)
        } else {
            Ok(line.trim().to_string())
        };
        let checked = names.and_then(|names| {
            split_list(&names)
                .iter()
                .try_for_each(|name| ensure_environment_name(name))
                .map(|()| names)
        });
        match checked {
            Ok(names) => return Ok(names),
            Err(error) => prompter.tell(&error.to_string()),
        }
    }
}

fn ask_required_text(label: &str, prompter: &mut dyn Prompter) -> Result<String, SetupError> {
    loop {
        let line = prompter.ask(&format!("{label}: "))?;
        let line = line.trim();
        if !line.is_empty() {
            return Ok(line.to_string());
        }
        prompter.tell("Esta resposta é obrigatória.");
    }
}

fn ask_seconds(
    label: &str,
    default: u64,
    allowed: std::ops::RangeInclusive<u64>,
    prompter: &mut dyn Prompter,
) -> Result<u64, SetupError> {
    let kind = AnswerKind::Integer {
        minimum: *allowed.start(),
        maximum: *allowed.end(),
    };
    loop {
        let line = prompter.ask(&format!("{label} [{default}]: "))?;
        if line.trim().is_empty() {
            return Ok(default);
        }
        match kind.normalize(&line).map(|value| value.parse::<u64>()) {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => prompter.tell(&error.to_string()),
            Err(error) => prompter.tell(&error.to_string()),
        }
    }
}

fn ask_widget_id(
    manifest: &SetupManifest,
    preset: Option<&Preset>,
    options: &SetupOptions,
    prompter: &mut dyn Prompter,
) -> Result<String, SetupError> {
    let default = preset
        .and_then(|preset| preset.widget_id.clone())
        .unwrap_or_else(|| manifest.default_id.clone());
    loop {
        let line = prompter.ask(&format!("Id do widget [{default}]: "))?;
        let id = match line.trim() {
            "" => default.clone(),
            typed => typed.to_string(),
        };
        if !is_valid_widget_id(&id) {
            prompter.tell("Use só letras minúsculas, números, '-' e '_'.");
        } else if options.existing_ids.contains(&id) {
            prompter.tell(&format!("Já existe um widget com o id \"{id}\"."));
        } else {
            return Ok(id);
        }
    }
}

fn is_valid_widget_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
}

fn choose_preset(
    manifest: &SetupManifest,
    prompter: &mut dyn Prompter,
) -> Result<Option<Preset>, SetupError> {
    if manifest.presets.is_empty() {
        return Ok(None);
    }
    let listing: Vec<String> = manifest
        .presets
        .iter()
        .enumerate()
        .map(|(index, preset)| format!("  {}) {}: {}", index + 1, preset.name, preset.description))
        .collect();
    prompter.tell(&format!("Modelos prontos:\n{}", listing.join("\n")));
    loop {
        let line = prompter.ask("Modelo (Enter para montar do zero): ")?;
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let chosen = line
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| manifest.presets.get(index))
            .or_else(|| manifest.presets.iter().find(|preset| preset.name == line));
        match chosen {
            Some(preset) => return Ok(Some(preset.clone())),
            None => prompter.tell("Modelo desconhecido; responda com um dos números listados."),
        }
    }
}

fn resolve_preset(
    preset: &Preset,
    prompter: &mut dyn Prompter,
) -> Result<BTreeMap<String, String>, SetupError> {
    let mut inputs = Vec::new();
    for input in &preset.inputs {
        let line = ask_required_text(&input.prompt, prompter)?;
        let value = if input.quote_list {
            split_list(&line)
                .iter()
                .map(|item| format!("\"{item}\""))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            line
        };
        inputs.push((format!("{{{}}}", input.name), value));
    }
    Ok(preset
        .arguments
        .iter()
        .map(|(flag, template)| {
            let filled = inputs
                .iter()
                .fold(template.clone(), |text, (placeholder, value)| {
                    text.replace(placeholder, value)
                });
            (flag.clone(), filled)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plugin_setup::{Choice, InheritRule, Note, PresetInput, Requirement};

    struct ScriptedPrompter {
        answers: std::vec::IntoIter<String>,
        asked: Vec<String>,
        told: Vec<String>,
    }

    impl ScriptedPrompter {
        fn new(answers: &[&str]) -> Self {
            Self {
                answers: answers
                    .iter()
                    .map(|answer| answer.to_string())
                    .collect::<Vec<_>>()
                    .into_iter(),
                asked: Vec::new(),
                told: Vec::new(),
            }
        }
    }

    impl Prompter for ScriptedPrompter {
        fn ask(&mut self, question: &str) -> Result<String, PromptError> {
            self.asked.push(question.to_string());
            self.answers.next().ok_or(PromptError::EndOfInput)
        }

        fn tell(&mut self, message: &str) {
            self.told.push(message.to_string());
        }
    }

    fn condition(variable: &str, any_of: &[&str]) -> Condition {
        Condition {
            variable: variable.to_string(),
            any_of: any_of.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn variable(name: &str, requirement: Requirement) -> Question {
        Question {
            target: Target::Variable(name.to_string()),
            prompt: format!("pergunta {name}"),
            kind: AnswerKind::Text,
            requirement,
            default: None,
            advanced: false,
            condition: None,
        }
    }

    fn manifest(steps: Vec<Step>) -> SetupManifest {
        SetupManifest {
            plugin: "demo".to_string(),
            script: "plugins/demo/demo.py".to_string(),
            default_id: "demo".to_string(),
            interval_seconds: 120,
            timeout_seconds: 30,
            steps,
            inherit_rules: Vec::new(),
            presets: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn token_secret(requirement: Requirement) -> SecretQuestion {
        SecretQuestion {
            prompt: "Token do demo".to_string(),
            environment_key: "DEMO_TOKEN_ENV".to_string(),
            file_key: "DEMO_TOKEN_FILE".to_string(),
            names_are_list: false,
            allow_both: false,
            requirement,
            condition: None,
        }
    }

    fn run(
        manifest: &SetupManifest,
        answers: &[&str],
    ) -> (Result<SetupResult, SetupError>, ScriptedPrompter) {
        let mut prompter = ScriptedPrompter::new(answers);
        let result = run_setup(manifest, &SetupOptions::default(), &mut prompter);
        (result, prompter)
    }

    #[test]
    fn enter_everywhere_keeps_the_manifest_defaults_and_writes_nothing_optional() {
        let (result, _) = run(
            &manifest(vec![Step::Question(variable(
                "DEMO_TITLE",
                Requirement::Optional,
            ))]),
            &["", "", "", ""],
        );
        let draft = result.unwrap().draft;
        assert_eq!(draft.id, "demo");
        assert_eq!(draft.arguments, vec!["plugins/demo/demo.py"]);
        assert!(draft.environment.is_empty());
        assert_eq!((draft.interval_seconds, draft.timeout_seconds), (120, 30));
    }

    #[test]
    fn a_required_question_repeats_until_it_has_an_answer() {
        let steps = vec![Step::Question(variable("DEMO_SITE", Requirement::Always))];
        let (result, prompter) = run(&manifest(steps), &["", "", "empresa.atlassian.net", "", ""]);
        let draft = result.unwrap().draft;
        assert_eq!(
            draft.environment,
            vec![("DEMO_SITE".to_string(), "empresa.atlassian.net".to_string())]
        );
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("obrigatória")));
    }

    #[test]
    fn an_invalid_answer_explains_and_asks_again() {
        let mut days = variable("DEMO_DAYS", Requirement::Optional);
        days.kind = AnswerKind::Integer {
            minimum: 1,
            maximum: 90,
        };
        let (result, prompter) = run(
            &manifest(vec![Step::Question(days)]),
            &["", "0", "14", "", ""],
        );
        let draft = result.unwrap().draft;
        assert_eq!(
            draft.environment,
            vec![("DEMO_DAYS".to_string(), "14".to_string())]
        );
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("entre 1 e 90")));
    }

    #[test]
    fn arguments_follow_the_script_in_manifest_order() {
        let mut query = variable("--jql", Requirement::Always);
        query.target = Target::Argument("--jql".to_string());
        let mut title = variable("--title", Requirement::Optional);
        title.target = Target::Argument("--title".to_string());
        let (result, _) = run(
            &manifest(vec![Step::Question(query), Step::Question(title)]),
            &["", "project = X", "Meu painel", "", ""],
        );
        assert_eq!(
            result.unwrap().draft.arguments,
            vec![
                "plugins/demo/demo.py",
                "--jql",
                "project = X",
                "--title",
                "Meu painel"
            ]
        );
    }

    #[test]
    fn a_question_is_skipped_when_its_condition_does_not_hold() {
        let mut method = variable("DEMO_METHOD", Requirement::Optional);
        method.kind = AnswerKind::Choice(vec![
            Choice {
                value: "cli".to_string(),
                description: "usa o CLI".to_string(),
            },
            Choice {
                value: "secret".to_string(),
                description: "usa segredo".to_string(),
            },
        ]);
        method.default = Some("cli".to_string());
        let mut tenant = variable("DEMO_TENANT", Requirement::Always);
        tenant.condition = Some(condition("DEMO_METHOD", &["secret"]));
        let manifest = manifest(vec![Step::Question(method), Step::Question(tenant)]);
        let (skipped, _) = run(&manifest, &["", "", "", ""]);
        assert!(skipped.unwrap().draft.environment.is_empty());
        let (asked, _) = run(&manifest, &["", "2", "meu-tenant", "", ""]);
        assert_eq!(
            asked.unwrap().draft.environment,
            vec![
                ("DEMO_METHOD".to_string(), "secret".to_string()),
                ("DEMO_TENANT".to_string(), "meu-tenant".to_string()),
            ]
        );
    }

    #[test]
    fn advanced_questions_only_appear_when_requested() {
        let mut state = variable("DEMO_STATE_DIR", Requirement::Optional);
        state.advanced = true;
        let manifest = manifest(vec![Step::Question(state)]);
        let (plain, prompter) = run(&manifest, &["", "", ""]);
        assert!(plain.unwrap().draft.environment.is_empty());
        assert!(!prompter
            .asked
            .iter()
            .any(|text| text.contains("DEMO_STATE_DIR")));
        let mut prompter = ScriptedPrompter::new(&["", "/tmp/estado", "", ""]);
        let options = SetupOptions {
            include_advanced: true,
            existing_ids: Vec::new(),
        };
        let result = run_setup(&manifest, &options, &mut prompter).unwrap();
        assert_eq!(
            result.draft.environment,
            vec![("DEMO_STATE_DIR".to_string(), "/tmp/estado".to_string())]
        );
    }

    #[test]
    fn a_secret_by_variable_stores_only_the_name_and_inherits_it() {
        let steps = vec![Step::Secret(token_secret(Requirement::Always))];
        let (result, prompter) = run(&manifest(steps), &["", "1", "DEMO_TOKEN_EMPRESA", "", ""]);
        let result = result.unwrap();
        assert_eq!(
            result.draft.environment,
            vec![(
                "DEMO_TOKEN_ENV".to_string(),
                "DEMO_TOKEN_EMPRESA".to_string()
            )]
        );
        assert_eq!(result.draft.inherit_env, vec!["DEMO_TOKEN_EMPRESA"]);
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("nunca é pedido")));
    }

    #[test]
    fn a_secret_by_file_stores_the_path_and_does_not_inherit() {
        let steps = vec![Step::Secret(token_secret(Requirement::Always))];
        let (result, _) = run(&manifest(steps), &["", "2", "~/.segredos/demo", "", ""]);
        let result = result.unwrap();
        assert_eq!(
            result.draft.environment,
            vec![(
                "DEMO_TOKEN_FILE".to_string(),
                "~/.segredos/demo".to_string()
            )]
        );
        assert!(result.draft.inherit_env.is_empty());
    }

    #[test]
    fn a_secret_variable_name_must_be_a_valid_name() {
        let steps = vec![Step::Secret(token_secret(Requirement::Always))];
        let (result, prompter) = run(
            &manifest(steps),
            &["", "1", "ghp_abc 123", "DEMO_OK", "", ""],
        );
        assert_eq!(result.unwrap().draft.inherit_env, vec!["DEMO_OK"]);
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("não é um nome de variável")));
    }

    #[test]
    fn an_optional_secret_can_be_skipped() {
        let steps = vec![Step::Secret(token_secret(Requirement::Optional))];
        let (result, _) = run(&manifest(steps), &["", "", "", ""]);
        assert!(result.unwrap().draft.environment.is_empty());
    }

    #[test]
    fn a_list_secret_can_use_variables_and_a_file_together() {
        let mut secret = token_secret(Requirement::Always);
        secret.names_are_list = true;
        secret.allow_both = true;
        let (result, _) = run(
            &manifest(vec![Step::Secret(secret)]),
            &["", "3", "LINK_A, LINK_B", "/links.txt", "", ""],
        );
        let draft = result.unwrap().draft;
        assert_eq!(draft.inherit_env, vec!["LINK_A", "LINK_B"]);
        assert_eq!(
            draft.environment,
            vec![
                ("DEMO_TOKEN_ENV".to_string(), "LINK_A,LINK_B".to_string()),
                ("DEMO_TOKEN_FILE".to_string(), "/links.txt".to_string()),
            ]
        );
    }

    #[test]
    fn inherit_rules_and_notes_follow_their_conditions() {
        let mut source = variable("DEMO_SOURCE", Requirement::Always);
        source.kind = AnswerKind::Choice(vec![
            Choice {
                value: "ics".to_string(),
                description: "link".to_string(),
            },
            Choice {
                value: "goa".to_string(),
                description: "GNOME".to_string(),
            },
        ]);
        let mut manifest = manifest(vec![Step::Question(source)]);
        manifest.inherit_rules = vec![InheritRule {
            name: "DBUS_SESSION_BUS_ADDRESS".to_string(),
            condition: Some(condition("DEMO_SOURCE", &["goa"])),
        }];
        manifest.notes = vec![
            Note {
                text: "só para goa".to_string(),
                condition: Some(condition("DEMO_SOURCE", &["goa"])),
            },
            Note {
                text: "sempre".to_string(),
                condition: None,
            },
        ];
        let (goa, _) = run(&manifest, &["", "goa", "", ""]);
        let goa = goa.unwrap();
        assert_eq!(goa.draft.inherit_env, vec!["DBUS_SESSION_BUS_ADDRESS"]);
        assert_eq!(goa.notes, vec!["só para goa", "sempre"]);
        let (ics, _) = run(&manifest, &["", "ics", "", ""]);
        let ics = ics.unwrap();
        assert!(ics.draft.inherit_env.is_empty());
        assert_eq!(ics.notes, vec!["sempre"]);
    }

    #[test]
    fn the_widget_id_must_be_valid_and_unused() {
        let manifest = manifest(Vec::new());
        let options = SetupOptions {
            include_advanced: false,
            existing_ids: vec!["demo".to_string()],
        };
        let mut prompter = ScriptedPrompter::new(&["", "Meu Widget", "demo-dois", "", ""]);
        let result = run_setup(&manifest, &options, &mut prompter).unwrap();
        assert_eq!(result.draft.id, "demo-dois");
        assert!(prompter.told.iter().any(|text| text.contains("Já existe")));
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("letras minúsculas")));
    }

    #[test]
    fn interval_and_timeout_are_validated() {
        let (result, prompter) = run(&manifest(Vec::new()), &["", "5", "60", "0", "20"]);
        let draft = result.unwrap().draft;
        assert_eq!((draft.interval_seconds, draft.timeout_seconds), (60, 20));
        assert_eq!(prompter.told.len(), 2);
    }

    #[test]
    fn a_preset_fills_defaults_replaces_inputs_and_is_written() {
        let mut query = variable("--jql", Requirement::Always);
        query.target = Target::Argument("--jql".to_string());
        let mut manifest = manifest(vec![Step::Question(query)]);
        manifest.presets = vec![Preset {
            name: "time".to_string(),
            description: "issues do time".to_string(),
            widget_id: Some("demo-time".to_string()),
            arguments: vec![("--jql".to_string(), "assignee in ({emails})".to_string())],
            inputs: vec![PresetInput {
                name: "emails".to_string(),
                prompt: "E-mails".to_string(),
                quote_list: true,
            }],
        }];
        let (result, _) = run(&manifest, &["1", "ana@x.com, bia@x.com", "", "", "", ""]);
        let draft = result.unwrap().draft;
        assert_eq!(draft.id, "demo-time");
        assert_eq!(
            draft.arguments,
            vec![
                "plugins/demo/demo.py",
                "--jql",
                "assignee in (\"ana@x.com\", \"bia@x.com\")"
            ]
        );
    }

    #[test]
    fn an_unknown_preset_is_asked_again_and_enter_builds_from_scratch() {
        let mut manifest = manifest(Vec::new());
        manifest.presets = vec![Preset {
            name: "um".to_string(),
            description: "d".to_string(),
            widget_id: None,
            arguments: Vec::new(),
            inputs: Vec::new(),
        }];
        let (result, prompter) = run(&manifest, &["9", "", "", "", ""]);
        assert!(result.is_ok());
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("Modelo desconhecido")));
    }

    #[test]
    fn running_out_of_input_is_reported() {
        let steps = vec![Step::Question(variable("DEMO_SITE", Requirement::Always))];
        let (result, _) = run(&manifest(steps), &[""]);
        assert_eq!(
            result.unwrap_err(),
            SetupError::Prompt(PromptError::EndOfInput)
        );
    }
}
