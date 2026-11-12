use thiserror::Error;

use crate::application::ports::{PromptError, Prompter};
use crate::application::setup_session::{Answered, SetupSession, SetupState};
use crate::display_in_portuguese;
use crate::domain::language::{Language, LocalizedMessage};
use crate::domain::plugin_setup::SetupManifest;
use crate::domain::widget_draft::{tab_toml_block, WidgetDraft};

#[derive(Debug, Error, PartialEq)]
pub enum SetupError {
    Prompt(#[from] PromptError),
}

impl LocalizedMessage for SetupError {
    fn message(&self, language: Language) -> String {
        match self {
            Self::Prompt(error) => error.message(language),
        }
    }
}

display_in_portuguese!(SetupError);

#[derive(Debug, Clone, PartialEq)]
pub struct TabContext {
    pub declared: Vec<String>,
    pub existing_widgets: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetupOptions {
    pub include_advanced: bool,
    pub existing_ids: Vec<String>,
    pub tab_context: Option<TabContext>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupResult {
    pub draft: WidgetDraft,
    pub notes: Vec<String>,
    pub new_tab: Option<String>,
}

impl SetupResult {
    pub fn toml_blocks(&self) -> Vec<String> {
        self.new_tab
            .iter()
            .map(|name| tab_toml_block(name))
            .chain(std::iter::once(self.draft.to_toml_block()))
            .collect()
    }
}

pub fn run_setup(
    manifest: &SetupManifest,
    options: &SetupOptions,
    prompter: &mut dyn Prompter,
    language: Language,
) -> Result<SetupResult, SetupError> {
    let mut session = SetupSession::new_in(manifest.clone(), options.clone(), language);
    let mut introduce = true;
    loop {
        let prompt = match session.state() {
            SetupState::Finished(result) => return Ok(result.clone()),
            SetupState::Asking(prompt) => prompt.clone(),
        };
        if let Some(introduction) = prompt.introduction.as_deref().filter(|_| introduce) {
            prompter.tell(introduction);
        }
        let line = prompter.ask(&prompt.label)?;
        introduce = match session.answer(&line) {
            Answered::Advanced => true,
            Answered::Rejected(message) => {
                prompter.tell(&message);
                false
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plugin_setup::{
        AnswerKind, Choice, Condition, InheritRule, Note, Preset, PresetInput, Question,
        Requirement, SecretQuestion, Step, Target,
    };

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
            present: None,
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
        let result = run_setup(
            manifest,
            &SetupOptions::default(),
            &mut prompter,
            Language::BrazilianPortuguese,
        );
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
            ..SetupOptions::default()
        };
        let result = run_setup(
            &manifest,
            &options,
            &mut prompter,
            Language::BrazilianPortuguese,
        )
        .unwrap();
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
            existing_ids: vec!["demo".to_string()],
            ..SetupOptions::default()
        };
        let mut prompter = ScriptedPrompter::new(&["", "Meu Widget", "demo-dois", "", ""]);
        let result = run_setup(
            &manifest,
            &options,
            &mut prompter,
            Language::BrazilianPortuguese,
        )
        .unwrap();
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

    fn run_with_tabs(
        context: TabContext,
        answers: &[&str],
    ) -> (Result<SetupResult, SetupError>, ScriptedPrompter) {
        let mut prompter = ScriptedPrompter::new(answers);
        let options = SetupOptions {
            tab_context: Some(context),
            ..SetupOptions::default()
        };
        let result = run_setup(
            &manifest(Vec::new()),
            &options,
            &mut prompter,
            Language::BrazilianPortuguese,
        );
        (result, prompter)
    }

    fn declared(names: &[&str], existing_widgets: usize) -> TabContext {
        TabContext {
            declared: names.iter().map(|name| name.to_string()).collect(),
            existing_widgets,
        }
    }

    #[test]
    fn enter_on_the_tab_question_writes_no_tab_key() {
        let (result, _) = run_with_tabs(declared(&["Work", "Home"], 3), &["", "", "", ""]);
        let result = result.unwrap();
        assert_eq!(result.draft.tab, None);
        assert_eq!(result.new_tab, None);
    }

    #[test]
    fn a_declared_tab_is_chosen_by_number_or_name() {
        let (by_number, _) = run_with_tabs(declared(&["Work", "Home"], 1), &["", "2", "", ""]);
        assert_eq!(by_number.unwrap().draft.tab.as_deref(), Some("Home"));
        let (by_name, _) = run_with_tabs(declared(&["Work", "Home"], 1), &["", "Work", "", ""]);
        assert_eq!(by_name.unwrap().draft.tab.as_deref(), Some("Work"));
    }

    #[test]
    fn creating_a_new_tab_next_to_declared_ones_asks_for_a_free_name() {
        let (result, prompter) = run_with_tabs(
            declared(&["Work", "Home"], 1),
            &["", "3", "Work", "Estudos", "", ""],
        );
        let result = result.unwrap();
        assert_eq!(result.draft.tab.as_deref(), Some("Estudos"));
        assert_eq!(result.new_tab.as_deref(), Some("Estudos"));
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("Já existe uma aba")));
        assert!(result.notes.is_empty());
    }

    #[test]
    fn the_first_declared_tab_warns_that_existing_widgets_move_into_it() {
        let (result, prompter) = run_with_tabs(declared(&[], 4), &["", "Trabalho", "", ""]);
        let result = result.unwrap();
        assert_eq!(result.new_tab.as_deref(), Some("Trabalho"));
        assert!(result.notes.iter().any(|note| note.contains("4 widgets")));
        assert!(prompter
            .told
            .iter()
            .any(|text| text.contains("ainda não declara abas")));
    }

    #[test]
    fn without_declared_tabs_enter_keeps_the_single_tab() {
        let (result, _) = run_with_tabs(declared(&[], 0), &["", "", "", ""]);
        let result = result.unwrap();
        assert_eq!(result.draft.tab, None);
        assert!(result.notes.is_empty());
    }

    #[test]
    fn the_blocks_to_write_put_a_new_tab_before_the_widget() {
        let (result, _) = run_with_tabs(declared(&[], 0), &["", "Trabalho", "", ""]);
        let blocks = result.unwrap().toml_blocks();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "[[tabs]]\nname = \"Trabalho\"\n");
        assert!(blocks[1].starts_with("[[widgets]]\nid = \"demo\""));
        let (without_tab, _) = run(&manifest(Vec::new()), &["", "", ""]);
        assert_eq!(without_tab.unwrap().toml_blocks().len(), 1);
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
