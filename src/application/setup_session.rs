use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use crate::application::plugin_setup::{SetupOptions, SetupResult, TabContext};
use crate::application::setup_messages as messages;
use crate::domain::language::{Language, LocalizedMessage};
use crate::domain::plugin_setup::{
    ensure_environment_name, normalize_list, split_list, AnswerError, AnswerKind, Condition,
    Preset, Question, SecretQuestion, SetupManifest, Step, Target,
};
use crate::domain::widget_draft::{WidgetDraft, JAVASCRIPT_PLUGIN_COMMAND, PLUGIN_COMMAND};

const MINIMUM_INTERVAL_SECONDS: u64 = 30;
const MAXIMUM_INTERVAL_SECONDS: u64 = 86_400;
const MINIMUM_TIMEOUT_SECONDS: u64 = 1;
const MAXIMUM_TIMEOUT_SECONDS: u64 = 3_600;
const SECRET_FROM_ENVIRONMENT: &str = "1";
const SECRET_FROM_FILE: &str = "2";
const SECRET_FROM_BOTH: &str = "3";

#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceOption {
    pub answer: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptInput {
    Text,
    Choice(Vec<ChoiceOption>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub title: String,
    pub explanation: Option<String>,
    pub introduction: Option<String>,
    pub label: String,
    pub input: PromptInput,
    pub default: Option<String>,
    pub blank: Option<String>,
    pub hint: Option<String>,
    pub required: bool,
}

impl Prompt {
    fn new(title: &str, label: String) -> Self {
        Self {
            title: title.to_string(),
            explanation: None,
            introduction: None,
            label,
            input: PromptInput::Text,
            default: None,
            blank: None,
            hint: None,
            required: false,
        }
    }

    fn required_text(title: &str, label: String) -> Self {
        Self {
            required: true,
            ..Self::new(title, label)
        }
    }

    fn choice(title: &str, label: String, options: Vec<ChoiceOption>) -> Self {
        Self {
            input: PromptInput::Choice(options),
            ..Self::new(title, label)
        }
    }

    fn introduced_by(mut self, introduction: String) -> Self {
        self.introduction = Some(introduction);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answered {
    Advanced,
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetupState<'session> {
    Asking(&'session Prompt),
    Finished(&'session SetupResult),
}

#[derive(Debug, Clone, PartialEq)]
enum Progress {
    Asking(Prompt),
    Finished(SetupResult),
}

#[derive(Debug, Clone, PartialEq)]
enum TabChoice {
    Keep,
    Existing(String),
    New(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SecretSource {
    Environment,
    File,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Preset,
    PresetInput { index: usize },
    WidgetId,
    FirstTab,
    DeclaredTab,
    NewTabName,
    Question { step: usize },
    SecretSource { step: usize },
    SecretNames { step: usize, source: SecretSource },
    SecretFile { step: usize },
    Interval,
    Timeout,
    Finished,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupSession {
    manifest: SetupManifest,
    options: SetupOptions,
    phase: Phase,
    progress: Progress,
    preset: Option<Preset>,
    preset_inputs: Vec<(String, String)>,
    widget_id: String,
    tab_choice: TabChoice,
    interval_seconds: u64,
    interview: Interview,
    language: Language,
}

impl SetupSession {
    pub fn new(manifest: SetupManifest, options: SetupOptions) -> Self {
        Self::new_in(manifest, options, Language::BrazilianPortuguese)
    }

    pub fn new_in(manifest: SetupManifest, options: SetupOptions, language: Language) -> Self {
        let interview = Interview::new(&manifest);
        let interval_seconds = manifest.interval_seconds;
        let (phase, prompt) = first_prompt(&manifest, language);
        Self {
            manifest,
            options,
            phase,
            progress: Progress::Asking(prompt),
            preset: None,
            preset_inputs: Vec::new(),
            widget_id: String::new(),
            tab_choice: TabChoice::Keep,
            interval_seconds,
            interview,
            language,
        }
    }

    pub fn state(&self) -> SetupState<'_> {
        match &self.progress {
            Progress::Asking(prompt) => SetupState::Asking(prompt),
            Progress::Finished(result) => SetupState::Finished(result),
        }
    }

    pub fn answer(&mut self, text: &str) -> Answered {
        let line = text.trim();
        match self.phase.clone() {
            Phase::Preset => self.answer_preset(line),
            Phase::PresetInput { index } => self.answer_preset_input(index, line),
            Phase::WidgetId => self.answer_widget_id(line),
            Phase::FirstTab => self.answer_first_tab(line),
            Phase::DeclaredTab => self.answer_declared_tab(line),
            Phase::NewTabName => self.answer_new_tab_name(line),
            Phase::Question { step } => self.answer_question(step, line),
            Phase::SecretSource { step } => self.answer_secret_source(step, line),
            Phase::SecretNames { step, source } => self.answer_secret_names(step, source, line),
            Phase::SecretFile { step } => self.answer_secret_file(step, line),
            Phase::Interval => self.answer_interval(line),
            Phase::Timeout => self.answer_timeout(line),
            Phase::Finished => Answered::Advanced,
        }
    }

    fn ask(&mut self, phase: Phase, prompt: Prompt) -> Answered {
        self.phase = phase;
        self.progress = Progress::Asking(prompt);
        Answered::Advanced
    }

    fn answer_preset(&mut self, line: &str) -> Answered {
        if line.is_empty() {
            self.begin_widget_id();
            return Answered::Advanced;
        }
        let presets = &self.manifest.presets;
        let chosen = line
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| presets.get(index))
            .or_else(|| presets.iter().find(|preset| preset.name == line))
            .cloned();
        let Some(preset) = chosen else {
            return Answered::Rejected(messages::unknown_preset(self.language).to_string());
        };
        self.preset = Some(preset);
        self.begin_preset_input(0);
        Answered::Advanced
    }

    fn begin_preset_input(&mut self, index: usize) {
        let prompt = self
            .preset
            .as_ref()
            .and_then(|preset| preset.inputs.get(index))
            .map(|input| Prompt::required_text(&input.prompt, format!("{}: ", input.prompt)));
        match prompt {
            Some(prompt) => {
                self.ask(Phase::PresetInput { index }, prompt);
            }
            None => {
                self.interview.preset_values = self.resolved_preset_values();
                self.begin_widget_id();
            }
        }
    }

    fn answer_preset_input(&mut self, index: usize, line: &str) -> Answered {
        if line.is_empty() {
            return Answered::Rejected(messages::required_answer(self.language).to_string());
        }
        let Some(input) = self
            .preset
            .as_ref()
            .and_then(|preset| preset.inputs.get(index))
        else {
            return Answered::Advanced;
        };
        let value = if input.quote_list {
            split_list(line)
                .iter()
                .map(|item| format!("\"{item}\""))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            line.to_string()
        };
        self.preset_inputs
            .push((format!("{{{}}}", input.name), value));
        self.begin_preset_input(index + 1);
        Answered::Advanced
    }

    fn resolved_preset_values(&self) -> BTreeMap<String, String> {
        let Some(preset) = &self.preset else {
            return BTreeMap::new();
        };
        preset
            .arguments
            .iter()
            .map(|(flag, template)| {
                let filled = self
                    .preset_inputs
                    .iter()
                    .fold(template.clone(), |text, (placeholder, value)| {
                        text.replace(placeholder, value)
                    });
                (flag.clone(), filled)
            })
            .collect()
    }

    fn default_widget_id(&self) -> String {
        self.preset
            .as_ref()
            .and_then(|preset| preset.widget_id.clone())
            .unwrap_or_else(|| self.manifest.default_id.clone())
    }

    fn begin_widget_id(&mut self) {
        let prompt = widget_id_prompt(&self.default_widget_id(), self.language);
        self.ask(Phase::WidgetId, prompt);
    }

    fn answer_widget_id(&mut self, line: &str) -> Answered {
        let id = if line.is_empty() {
            self.default_widget_id()
        } else {
            line.to_string()
        };
        if !is_valid_widget_id(&id) {
            return Answered::Rejected(messages::invalid_widget_id(self.language).to_string());
        }
        if self.options.existing_ids.contains(&id) {
            return Answered::Rejected(messages::widget_id_taken(self.language, &id));
        }
        self.widget_id = id;
        self.begin_tab();
        Answered::Advanced
    }

    fn begin_tab(&mut self) {
        let Some(context) = self.options.tab_context.clone() else {
            self.begin_steps(0);
            return;
        };
        match context.declared.first() {
            Some(first) => {
                let prompt = declared_tab_prompt(first, &context, self.language);
                self.ask(Phase::DeclaredTab, prompt);
            }
            None => {
                self.ask(Phase::FirstTab, first_tab_prompt(&context, self.language));
            }
        }
    }

    fn answer_first_tab(&mut self, line: &str) -> Answered {
        if !line.is_empty() {
            self.tab_choice = TabChoice::New(line.to_string());
        }
        self.begin_steps(0);
        Answered::Advanced
    }

    fn answer_declared_tab(&mut self, line: &str) -> Answered {
        let Some(context) = self.options.tab_context.clone() else {
            return Answered::Advanced;
        };
        if line.is_empty() {
            self.begin_steps(0);
            return Answered::Advanced;
        }
        let chosen = line
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| context.declared.get(index))
            .or_else(|| context.declared.iter().find(|name| name.as_str() == line));
        if let Some(name) = chosen {
            self.tab_choice = TabChoice::Existing(name.clone());
            self.begin_steps(0);
            return Answered::Advanced;
        }
        if line == (context.declared.len() + 1).to_string() {
            return self.ask(
                Phase::NewTabName,
                Prompt::required_text(
                    messages::new_tab_name_title(self.language),
                    messages::new_tab_name_label(self.language).to_string(),
                ),
            );
        }
        Answered::Rejected(messages::unknown_number(self.language).to_string())
    }

    fn answer_new_tab_name(&mut self, line: &str) -> Answered {
        if line.is_empty() {
            return Answered::Rejected(messages::required_answer(self.language).to_string());
        }
        let taken = self
            .options
            .tab_context
            .as_ref()
            .is_some_and(|context| context.declared.iter().any(|name| name == line));
        if taken {
            return Answered::Rejected(messages::tab_name_taken(self.language, line));
        }
        self.tab_choice = TabChoice::New(line.to_string());
        self.begin_steps(0);
        Answered::Advanced
    }

    fn begin_steps(&mut self, from: usize) {
        for index in from..self.manifest.steps.len() {
            let step = &self.manifest.steps[index];
            let holds = step
                .condition()
                .is_none_or(|condition| condition.holds(&self.interview.values));
            if !holds || (step.is_advanced() && !self.options.include_advanced) {
                continue;
            }
            let (phase, prompt) = match step {
                Step::Question(question) => (
                    Phase::Question { step: index },
                    self.question_prompt(question),
                ),
                Step::Secret(secret) => (
                    Phase::SecretSource { step: index },
                    secret_source_prompt(
                        secret,
                        secret.requirement.applies(&self.interview.values),
                        self.language,
                    ),
                ),
            };
            self.ask(phase, prompt);
            return;
        }
        self.begin_interval();
    }

    fn question_at(&self, step: usize) -> Option<&Question> {
        match self.manifest.steps.get(step) {
            Some(Step::Question(question)) => Some(question),
            _ => None,
        }
    }

    fn secret_at(&self, step: usize) -> Option<&SecretQuestion> {
        match self.manifest.steps.get(step) {
            Some(Step::Secret(secret)) => Some(secret),
            _ => None,
        }
    }

    fn question_default(&self, question: &Question) -> Option<String> {
        self.interview
            .preset_values
            .get(question.target.key())
            .cloned()
            .or_else(|| question.default.clone())
    }

    fn question_prompt(&self, question: &Question) -> Prompt {
        let default = self.question_default(question);
        let required = question.requirement.applies(&self.interview.values);
        question_prompt(question, default, required, self.language)
    }

    fn answer_question(&mut self, step: usize, line: &str) -> Answered {
        let Some(question) = self.question_at(step).cloned() else {
            return Answered::Advanced;
        };
        let key = question.target.key().to_string();
        let preset_value = self.interview.preset_values.get(&key).cloned();
        let default = self.question_default(&question);
        let required = question.requirement.applies(&self.interview.values);
        let (value, typed) = if line.is_empty() {
            match default {
                Some(default) => (default, false),
                None if required => {
                    return Answered::Rejected(messages::required_answer(self.language).to_string())
                }
                None => {
                    self.begin_steps(step + 1);
                    return Answered::Advanced;
                }
            }
        } else {
            match question.kind.normalize(line) {
                Ok(value) => (value, true),
                Err(error) => return Answered::Rejected(error.message(self.language)),
            }
        };
        self.interview.record_answer(
            &question.target,
            key,
            value,
            typed || preset_value.is_some(),
        );
        self.begin_steps(step + 1);
        Answered::Advanced
    }

    fn answer_secret_source(&mut self, step: usize, line: &str) -> Answered {
        let Some(secret) = self.secret_at(step).cloned() else {
            return Answered::Advanced;
        };
        let required = secret.requirement.applies(&self.interview.values);
        if line.is_empty() && !required {
            self.begin_steps(step + 1);
            return Answered::Advanced;
        }
        let source = match line {
            SECRET_FROM_ENVIRONMENT => SecretSource::Environment,
            SECRET_FROM_FILE => SecretSource::File,
            SECRET_FROM_BOTH if secret.allow_both => SecretSource::Both,
            _ => return Answered::Rejected(messages::unknown_number(self.language).to_string()),
        };
        match source {
            SecretSource::File => self.begin_secret_file(step, &secret),
            SecretSource::Environment | SecretSource::Both => self.ask(
                Phase::SecretNames { step, source },
                secret_names_prompt(&secret, self.language),
            ),
        }
    }

    fn begin_secret_file(&mut self, step: usize, secret: &SecretQuestion) -> Answered {
        self.ask(
            Phase::SecretFile { step },
            Prompt {
                explanation: Some(secret.prompt.clone()),
                ..Prompt::required_text(
                    messages::file_path_title(self.language),
                    messages::file_path_label(self.language, &secret.prompt),
                )
            },
        )
    }

    fn answer_secret_names(&mut self, step: usize, source: SecretSource, line: &str) -> Answered {
        let Some(secret) = self.secret_at(step).cloned() else {
            return Answered::Advanced;
        };
        if line.is_empty() {
            return Answered::Rejected(messages::required_answer(self.language).to_string());
        }
        let names = if secret.names_are_list {
            normalize_list(line)
        } else {
            Ok(line.to_string())
        };
        let checked = names.and_then(|names| {
            split_list(&names)
                .iter()
                .try_for_each(|name| ensure_environment_name(name))
                .map(|()| names)
        });
        let names = match checked {
            Ok(names) => names,
            Err(error) => return Answered::Rejected(error.message(self.language)),
        };
        self.interview.record_secret_names(&secret, names);
        if source == SecretSource::Both {
            self.begin_secret_file(step, &secret)
        } else {
            self.begin_steps(step + 1);
            Answered::Advanced
        }
    }

    fn answer_secret_file(&mut self, step: usize, line: &str) -> Answered {
        let Some(secret) = self.secret_at(step).cloned() else {
            return Answered::Advanced;
        };
        if line.is_empty() {
            return Answered::Rejected(messages::required_answer(self.language).to_string());
        }
        self.interview.record_secret_file(&secret, line.to_string());
        self.begin_steps(step + 1);
        Answered::Advanced
    }

    fn begin_interval(&mut self) {
        let prompt = seconds_prompt(
            messages::interval_title(self.language),
            self.manifest.interval_seconds,
            MINIMUM_INTERVAL_SECONDS..=MAXIMUM_INTERVAL_SECONDS,
            self.language,
        );
        self.ask(Phase::Interval, prompt);
    }

    fn answer_interval(&mut self, line: &str) -> Answered {
        match seconds_answer(
            line,
            self.manifest.interval_seconds,
            MINIMUM_INTERVAL_SECONDS..=MAXIMUM_INTERVAL_SECONDS,
            self.language,
        ) {
            Ok(seconds) => {
                self.interval_seconds = seconds;
                let prompt = seconds_prompt(
                    messages::timeout_title(self.language),
                    self.manifest.timeout_seconds,
                    MINIMUM_TIMEOUT_SECONDS..=MAXIMUM_TIMEOUT_SECONDS,
                    self.language,
                );
                self.ask(Phase::Timeout, prompt)
            }
            Err(message) => Answered::Rejected(message),
        }
    }

    fn answer_timeout(&mut self, line: &str) -> Answered {
        match seconds_answer(
            line,
            self.manifest.timeout_seconds,
            MINIMUM_TIMEOUT_SECONDS..=MAXIMUM_TIMEOUT_SECONDS,
            self.language,
        ) {
            Ok(seconds) => {
                self.finish(seconds);
                Answered::Advanced
            }
            Err(message) => Answered::Rejected(message),
        }
    }

    fn finish(&mut self, timeout_seconds: u64) {
        let mut result = self.interview.finish(
            &self.manifest,
            self.widget_id.clone(),
            self.interval_seconds,
            timeout_seconds,
        );
        apply_tab_choice(
            &mut result,
            self.tab_choice.clone(),
            self.options.tab_context.as_ref(),
            self.language,
        );
        self.phase = Phase::Finished;
        self.progress = Progress::Finished(result);
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Interview {
    preset_values: BTreeMap<String, String>,
    values: BTreeMap<String, String>,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    inherit_env: Vec<String>,
}

impl Interview {
    fn new(manifest: &SetupManifest) -> Self {
        let defaults = manifest.steps.iter().filter_map(|step| match step {
            Step::Question(question) => question
                .default
                .clone()
                .map(|default| (question.target.key().to_string(), default)),
            Step::Secret(_) => None,
        });
        Self {
            preset_values: BTreeMap::new(),
            values: defaults.collect(),
            arguments: vec![manifest.script.clone()],
            environment: Vec::new(),
            inherit_env: Vec::new(),
        }
    }

    fn record_answer(&mut self, target: &Target, key: String, value: String, written: bool) {
        self.values.insert(key, value.clone());
        if !written {
            return;
        }
        match target {
            Target::Argument(flag) => {
                self.arguments.push(flag.clone());
                self.arguments.push(value);
            }
            Target::Variable(name) => self.environment.push((name.clone(), value)),
        }
    }

    fn record_secret_names(&mut self, secret: &SecretQuestion, names: String) {
        self.values
            .insert(secret.environment_key.clone(), names.clone());
        self.inherit_env.extend(split_list(&names));
        self.environment
            .push((secret.environment_key.clone(), names));
    }

    fn record_secret_file(&mut self, secret: &SecretQuestion, path: String) {
        self.values.insert(secret.file_key.clone(), path.clone());
        self.environment.push((secret.file_key.clone(), path));
    }

    fn finish(
        &self,
        manifest: &SetupManifest,
        widget_id: String,
        interval_seconds: u64,
        timeout_seconds: u64,
    ) -> SetupResult {
        let mut inherit_env = self.inherit_env.clone();
        for rule in &manifest.inherit_rules {
            if holds(rule.condition.as_ref(), &self.values) {
                inherit_env.push(rule.name.clone());
            }
        }
        let mut seen = Vec::new();
        inherit_env.retain(|name| {
            let first_time = !seen.contains(name);
            seen.push(name.clone());
            first_time
        });
        let notes = manifest
            .notes
            .iter()
            .filter(|note| holds(note.condition.as_ref(), &self.values))
            .map(|note| note.text.clone())
            .collect();
        SetupResult {
            draft: WidgetDraft {
                id: widget_id,
                command: if manifest.script.ends_with(".js") {
                    JAVASCRIPT_PLUGIN_COMMAND
                } else {
                    PLUGIN_COMMAND
                }
                .to_string(),
                arguments: self.arguments.clone(),
                interval_seconds,
                timeout_seconds,
                tab: None,
                inherit_env,
                environment: self.environment.clone(),
            },
            notes,
            new_tab: None,
        }
    }
}

fn holds(condition: Option<&Condition>, values: &BTreeMap<String, String>) -> bool {
    condition.is_none_or(|condition| condition.holds(values))
}

fn apply_tab_choice(
    result: &mut SetupResult,
    choice: TabChoice,
    context: Option<&TabContext>,
    language: Language,
) {
    match choice {
        TabChoice::Keep => {}
        TabChoice::Existing(name) => result.draft.tab = Some(name),
        TabChoice::New(name) => {
            result.draft.tab = Some(name.clone());
            result.new_tab = Some(name);
            if let Some(context) = context
                .filter(|context| context.declared.is_empty() && context.existing_widgets > 0)
            {
                result
                    .notes
                    .push(messages::first_tab_note(language, context.existing_widgets));
            }
        }
    }
}

fn option_listing(options: &[ChoiceOption]) -> String {
    options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            if option.description.is_empty() {
                format!("  {}) {}", index + 1, option.label)
            } else {
                format!("  {}) {}: {}", index + 1, option.label, option.description)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plain_option(answer: &str, label: &str) -> ChoiceOption {
    ChoiceOption {
        answer: answer.to_string(),
        label: label.to_string(),
        description: String::new(),
    }
}

fn first_prompt(manifest: &SetupManifest, language: Language) -> (Phase, Prompt) {
    if manifest.presets.is_empty() {
        return (
            Phase::WidgetId,
            widget_id_prompt(&manifest.default_id, language),
        );
    }
    let options = manifest
        .presets
        .iter()
        .map(|preset| ChoiceOption {
            answer: preset.name.clone(),
            label: preset.name.clone(),
            description: preset.description.clone(),
        })
        .collect();
    (Phase::Preset, preset_prompt(options, language))
}

fn widget_id_prompt(default: &str, language: Language) -> Prompt {
    Prompt {
        default: Some(default.to_string()),
        hint: Some(messages::widget_id_hint(language).to_string()),
        ..Prompt::new(
            messages::widget_id_title(language),
            messages::widget_id_label(language, default),
        )
    }
}

fn preset_prompt(options: Vec<ChoiceOption>, language: Language) -> Prompt {
    let introduction = messages::presets_introduction(language, &option_listing(&options));
    Prompt {
        blank: Some(messages::from_scratch_label(language).to_string()),
        ..Prompt::choice(
            messages::preset_title(language),
            messages::preset_label(language).to_string(),
            options,
        )
    }
    .introduced_by(introduction)
}

fn first_tab_prompt(context: &TabContext, language: Language) -> Prompt {
    let explanation = messages::first_tab_explanation(language, context.existing_widgets > 0);
    Prompt {
        explanation: Some(explanation.clone()),
        ..Prompt::new(
            messages::widget_tab_title(language),
            messages::first_tab_label(language).to_string(),
        )
    }
    .introduced_by(explanation)
}

fn declared_tab_prompt(first: &str, context: &TabContext, language: Language) -> Prompt {
    let mut options: Vec<ChoiceOption> = context
        .declared
        .iter()
        .map(|name| plain_option(name, name))
        .collect();
    options.push(plain_option(
        &(context.declared.len() + 1).to_string(),
        messages::new_tab_label(language),
    ));
    let introduction = messages::panel_tabs_introduction(language, &option_listing(&options));
    Prompt {
        blank: Some(messages::first_tab_default_label(language, first)),
        ..Prompt::choice(
            messages::widget_tab_title(language),
            messages::declared_tab_label(language, first),
            options,
        )
    }
    .introduced_by(introduction)
}

fn question_prompt(
    question: &Question,
    default: Option<String>,
    required: bool,
    language: Language,
) -> Prompt {
    let hint = match &question.kind {
        AnswerKind::List => Some(messages::list_hint(language).to_string()),
        AnswerKind::Integer { minimum, maximum } => {
            Some(messages::integer_hint(language, *minimum, *maximum))
        }
        AnswerKind::Choice(_) | AnswerKind::Text => None,
    };
    let label = question_label(question, default.as_deref(), required, language);
    let mut prompt = match &question.kind {
        AnswerKind::Choice(choices) => {
            let options: Vec<ChoiceOption> = choices
                .iter()
                .map(|choice| ChoiceOption {
                    answer: choice.value.clone(),
                    label: choice.value.clone(),
                    description: choice.description.clone(),
                })
                .collect();
            let introduction = format!("{}\n{}", question.prompt, option_listing(&options));
            Prompt::choice(&question.prompt, label, options).introduced_by(introduction)
        }
        _ => Prompt::new(&question.prompt, label),
    };
    prompt.blank = match (&question.kind, &default) {
        (AnswerKind::Choice(_), Some(default)) => Some(messages::default_marker(language, default)),
        (AnswerKind::Choice(_), None) if !required => {
            Some(messages::skip_label(language).to_string())
        }
        _ => None,
    };
    prompt.default = default;
    prompt.hint = hint;
    prompt.required = required;
    prompt
}

fn question_label(
    question: &Question,
    default: Option<&str>,
    required: bool,
    language: Language,
) -> String {
    let hint = match &question.kind {
        AnswerKind::List => format!(" ({})", messages::list_hint(language)),
        AnswerKind::Integer { minimum, maximum } => {
            messages::integer_label_hint(language, *minimum, *maximum)
        }
        AnswerKind::Choice(_) | AnswerKind::Text => String::new(),
    };
    let prompt = match question.kind {
        AnswerKind::Choice(_) => messages::choose_label(language),
        _ => question.prompt.as_str(),
    };
    let suffix = match (default, required) {
        (Some(default), _) => format!(" [{default}]"),
        (None, true) => messages::required_suffix(language).to_string(),
        (None, false) => messages::skip_suffix(language).to_string(),
    };
    format!("{prompt}{hint}{suffix}: ")
}

fn secret_source_prompt(secret: &SecretQuestion, required: bool, language: Language) -> Prompt {
    let mut options = vec![
        plain_option(
            SECRET_FROM_ENVIRONMENT,
            messages::secret_from_environment(language),
        ),
        plain_option(SECRET_FROM_FILE, messages::secret_from_file(language)),
    ];
    if secret.allow_both {
        options.push(plain_option(
            SECRET_FROM_BOTH,
            messages::secret_from_both(language),
        ));
    }
    let explanation = messages::secret_explanation(language);
    let introduction = format!(
        "{}\n{explanation}\n{}",
        secret.prompt,
        option_listing(&options)
    );
    let suffix = if required {
        messages::required_suffix(language)
    } else {
        messages::skip_suffix(language)
    };
    Prompt {
        explanation: Some(explanation.to_string()),
        blank: (!required).then(|| messages::skip_label(language).to_string()),
        required,
        ..Prompt::choice(
            &secret.prompt,
            format!("{}{suffix}: ", messages::choose_label(language)),
            options,
        )
    }
    .introduced_by(introduction)
}

fn secret_names_prompt(secret: &SecretQuestion, language: Language) -> Prompt {
    let (title, label, hint) = if secret.names_are_list {
        (
            messages::variable_names_title(language),
            messages::variable_names_label(language),
            Some(messages::list_hint(language).to_string()),
        )
    } else {
        (
            messages::variable_name_title(language),
            messages::variable_name_title(language),
            None,
        )
    };
    Prompt {
        explanation: Some(secret.prompt.clone()),
        hint,
        ..Prompt::required_text(title, format!("{label}: "))
    }
}

fn seconds_prompt(
    title: &str,
    default: u64,
    allowed: RangeInclusive<u64>,
    language: Language,
) -> Prompt {
    Prompt {
        default: Some(default.to_string()),
        hint: Some(messages::seconds_hint(
            language,
            *allowed.start(),
            *allowed.end(),
        )),
        ..Prompt::new(title, format!("{title} [{default}]: "))
    }
}

fn seconds_answer(
    line: &str,
    default: u64,
    allowed: RangeInclusive<u64>,
    language: Language,
) -> Result<u64, String> {
    if line.is_empty() {
        return Ok(default);
    }
    let kind = AnswerKind::Integer {
        minimum: *allowed.start(),
        maximum: *allowed.end(),
    };
    let normalized = kind
        .normalize(line)
        .map_err(|error| error.message(language))?;
    normalized.parse().map_err(|_| {
        AnswerError::NotAnInteger {
            minimum: *allowed.start(),
            maximum: *allowed.end(),
        }
        .message(language)
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::plugin_setup::{SetupOptions, TabContext};
    use crate::domain::plugin_setup::{
        AnswerKind, Choice, Preset, PresetInput, Question, Requirement, SecretQuestion,
        SetupManifest, Step, Target,
    };

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

    fn secret(requirement: Requirement) -> SecretQuestion {
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

    fn session(manifest: SetupManifest) -> SetupSession {
        SetupSession::new(manifest, SetupOptions::default())
    }

    fn current(session: &SetupSession) -> Prompt {
        match session.state() {
            SetupState::Asking(prompt) => prompt.clone(),
            SetupState::Finished(_) => panic!("a sessão já terminou"),
        }
    }

    fn advance(session: &mut SetupSession, answer: &str) {
        assert_eq!(session.answer(answer), Answered::Advanced, "{answer}");
    }

    fn rejection(session: &mut SetupSession, answer: &str) -> String {
        match session.answer(answer) {
            Answered::Rejected(message) => message,
            Answered::Advanced => panic!("{answer} deveria ser rejeitada"),
        }
    }

    fn labels_until_finished(session: &mut SetupSession, answers: &[&str]) -> Vec<String> {
        let mut labels = Vec::new();
        for answer in answers {
            labels.push(current(session).label);
            advance(session, answer);
        }
        labels
    }

    #[test]
    fn the_first_prompt_asks_for_the_widget_id_with_the_manifest_default() {
        let prompt = current(&session(manifest(Vec::new())));
        assert_eq!(prompt.title, "Id do widget");
        assert_eq!(prompt.label, "Id do widget [demo]: ");
        assert_eq!(prompt.default.as_deref(), Some("demo"));
        assert_eq!(prompt.input, PromptInput::Text);
        assert_eq!(prompt.introduction, None);
    }

    #[test]
    fn the_prompts_follow_the_terminal_order_and_labels() {
        let mut session = session(manifest(vec![Step::Question(variable(
            "DEMO_TITLE",
            Requirement::Optional,
        ))]));
        let labels = labels_until_finished(&mut session, &["", "", "", ""]);
        assert_eq!(
            labels,
            vec![
                "Id do widget [demo]: ",
                "pergunta DEMO_TITLE (Enter para pular): ",
                "Intervalo entre atualizações, em segundos [120]: ",
                "Tempo máximo de cada execução, em segundos [30]: ",
            ]
        );
        assert!(matches!(session.state(), SetupState::Finished(_)));
    }

    #[test]
    fn a_rejected_answer_keeps_the_same_prompt_and_explains() {
        let mut session = session(manifest(Vec::new()));
        let before = current(&session);
        assert!(rejection(&mut session, "Meu Widget").contains("letras minúsculas"));
        assert_eq!(current(&session), before);
        advance(&mut session, "meu-widget");
        assert_eq!(
            current(&session).title,
            "Intervalo entre atualizações, em segundos"
        );
    }

    #[test]
    fn a_used_widget_id_is_rejected() {
        let options = SetupOptions {
            existing_ids: vec!["demo".to_string()],
            ..SetupOptions::default()
        };
        let mut session = SetupSession::new(manifest(Vec::new()), options);
        assert!(rejection(&mut session, "").contains("Já existe um widget"));
    }

    #[test]
    fn a_required_question_rejects_a_blank_answer() {
        let mut session = session(manifest(vec![Step::Question(variable(
            "DEMO_SITE",
            Requirement::Always,
        ))]));
        advance(&mut session, "");
        let prompt = current(&session);
        assert!(prompt.required);
        assert!(prompt.label.ends_with("(obrigatório): "));
        assert!(rejection(&mut session, "  ").contains("obrigatória"));
        advance(&mut session, "empresa.exemplo");
    }

    #[test]
    fn an_integer_question_explains_the_range_and_rejects_values_outside_it() {
        let mut days = variable("DEMO_DAYS", Requirement::Optional);
        days.kind = AnswerKind::Integer {
            minimum: 1,
            maximum: 90,
        };
        let mut session = session(manifest(vec![Step::Question(days)]));
        advance(&mut session, "");
        assert_eq!(current(&session).hint.as_deref(), Some("de 1 a 90"));
        assert!(rejection(&mut session, "0").contains("entre 1 e 90"));
        advance(&mut session, "14");
    }

    #[test]
    fn a_choice_question_lists_its_options_and_a_blank_row_for_the_default() {
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
        let mut session = session(manifest(vec![Step::Question(method)]));
        advance(&mut session, "");
        let prompt = current(&session);
        assert_eq!(prompt.title, "pergunta DEMO_METHOD");
        assert_eq!(prompt.label, "Escolha [cli]: ");
        let PromptInput::Choice(options) = &prompt.input else {
            panic!("esperava uma escolha");
        };
        let answers: Vec<&str> = options
            .iter()
            .map(|option| option.answer.as_str())
            .collect();
        assert_eq!(answers, vec!["cli", "secret"]);
        assert_eq!(options[1].description, "usa segredo");
        assert_eq!(prompt.blank.as_deref(), Some("cli (padrão)"));
        assert_eq!(
            prompt.introduction.as_deref(),
            Some("pergunta DEMO_METHOD\n  1) cli: usa o CLI\n  2) secret: usa segredo")
        );
        assert!(rejection(&mut session, "pat").contains("escolha um número"));
        advance(&mut session, "secret");
    }

    #[test]
    fn a_secret_asks_for_the_source_then_only_the_name_or_the_path() {
        let mut session = session(manifest(vec![Step::Secret(secret(Requirement::Always))]));
        advance(&mut session, "");
        let source = current(&session);
        assert_eq!(source.title, "Token do demo");
        assert!(source
            .explanation
            .as_deref()
            .is_some_and(|text| text.contains("nunca é pedido")));
        let PromptInput::Choice(options) = &source.input else {
            panic!("esperava uma escolha");
        };
        assert_eq!(options.len(), 2);
        assert!(rejection(&mut session, "9").contains("números listados"));
        advance(&mut session, "1");
        assert_eq!(current(&session).label, "Nome da variável: ");
        assert!(rejection(&mut session, "ghp abc").contains("não é um nome de variável"));
        advance(&mut session, "DEMO_TOKEN_EMPRESA");
        advance(&mut session, "");
        advance(&mut session, "");
        let SetupState::Finished(result) = session.state() else {
            panic!("esperava o fim");
        };
        assert_eq!(result.draft.inherit_env, vec!["DEMO_TOKEN_EMPRESA"]);
    }

    #[test]
    fn declared_tabs_become_options_with_a_blank_row_for_the_first_tab() {
        let options = SetupOptions {
            tab_context: Some(TabContext {
                declared: vec!["Work".to_string(), "Home".to_string()],
                existing_widgets: 2,
            }),
            ..SetupOptions::default()
        };
        let mut session = SetupSession::new(manifest(Vec::new()), options);
        advance(&mut session, "");
        let prompt = current(&session);
        assert_eq!(prompt.label, "Aba [Enter = Work, a primeira]: ");
        assert_eq!(prompt.blank.as_deref(), Some("Work (a primeira aba)"));
        let PromptInput::Choice(options) = &prompt.input else {
            panic!("esperava uma escolha");
        };
        let answers: Vec<&str> = options
            .iter()
            .map(|option| option.answer.as_str())
            .collect();
        assert_eq!(answers, vec!["Work", "Home", "3"]);
        advance(&mut session, "3");
        assert_eq!(current(&session).label, "Nome da aba nova: ");
        assert!(rejection(&mut session, "Work").contains("Já existe uma aba"));
        advance(&mut session, "Estudos");
        advance(&mut session, "");
        advance(&mut session, "");
        let SetupState::Finished(result) = session.state() else {
            panic!("esperava o fim");
        };
        assert_eq!(result.new_tab.as_deref(), Some("Estudos"));
        assert_eq!(result.draft.tab.as_deref(), Some("Estudos"));
    }

    #[test]
    fn presets_come_first_and_their_inputs_are_required() {
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
        let mut session = session(manifest);
        let first = current(&session);
        assert_eq!(first.label, "Modelo (Enter para montar do zero): ");
        assert_eq!(first.blank.as_deref(), Some("montar do zero"));
        assert!(rejection(&mut session, "9").contains("Modelo desconhecido"));
        advance(&mut session, "time");
        assert_eq!(current(&session).label, "E-mails: ");
        assert!(rejection(&mut session, "").contains("obrigatória"));
        advance(&mut session, "ana@x.com, bia@x.com");
        assert_eq!(current(&session).default.as_deref(), Some("demo-time"));
        advance(&mut session, "");
        advance(&mut session, "");
        advance(&mut session, "");
        advance(&mut session, "");
        let SetupState::Finished(result) = session.state() else {
            panic!("esperava o fim");
        };
        assert_eq!(
            result.draft.arguments,
            vec![
                "plugins/demo/demo.py",
                "--jql",
                "assignee in (\"ana@x.com\", \"bia@x.com\")"
            ]
        );
    }

    #[test]
    fn once_finished_further_answers_change_nothing() {
        let mut session = session(manifest(Vec::new()));
        for answer in ["", "", ""] {
            advance(&mut session, answer);
        }
        let SetupState::Finished(before) = session.state() else {
            panic!("esperava o fim");
        };
        let before = before.clone();
        assert_eq!(session.answer("qualquer"), Answered::Advanced);
        assert_eq!(session.state(), SetupState::Finished(&before));
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;
    use crate::infrastructure::setup_manifest::parse_manifest_in;

    const MANIFEST: &str = r#"
plugin = "demo"
script = "plugins/demo/demo.py"
default_id = "demo"
interval_seconds = 60
timeout_seconds = 5

[[steps]]
kind = "variable"
name = "DEMO_COUNT"
prompt = "Quantidade"
prompt_en = "Count"
type = "integer"
min = 1
max = 9
required = true
"#;

    fn session(language: Language) -> SetupSession {
        let manifest = parse_manifest_in(MANIFEST, language).unwrap();
        SetupSession::new_in(manifest, SetupOptions::default(), language)
    }

    fn prompt_of(session: &SetupSession) -> Prompt {
        match session.state() {
            SetupState::Asking(prompt) => prompt.clone(),
            SetupState::Finished(_) => panic!("finished"),
        }
    }

    #[test]
    fn the_english_interview_asks_in_english() {
        let mut session = session(Language::English);
        let first = prompt_of(&session);
        assert_eq!(first.title, "Widget id");
        assert_eq!(first.label, "Widget id [demo]: ");
        assert_eq!(
            session.answer("Bad Id"),
            Answered::Rejected("Use only lowercase letters, numbers, '-' and '_'.".to_string())
        );
        session.answer("");
        let count = prompt_of(&session);
        assert_eq!(count.title, "Count");
        assert_eq!(count.label, "Count (1 to 9) (required): ");
        assert_eq!(
            session.answer("12"),
            Answered::Rejected("the value must be between 1 and 9".to_string())
        );
        session.answer("3");
        let interval = prompt_of(&session);
        assert_eq!(interval.title, "Interval between refreshes, in seconds");
        assert_eq!(interval.hint.as_deref(), Some("30 to 86400"));
    }

    #[test]
    fn the_portuguese_interview_keeps_the_original_texts() {
        let mut session = session(Language::BrazilianPortuguese);
        let first = prompt_of(&session);
        assert_eq!(first.label, "Id do widget [demo]: ");
        session.answer("");
        let count = prompt_of(&session);
        assert_eq!(count.label, "Quantidade (1 a 9) (obrigatório): ");
    }
}
