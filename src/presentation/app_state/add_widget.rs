use crate::application::plugin_setup::SetupResult;
use crate::application::ports::SetupCatalogSnapshot;
use crate::application::setup_session::{
    Answered, ChoiceOption, Prompt, PromptInput, SetupSession, SetupState,
};
use crate::domain::language::Language;
use crate::domain::plugin_setup::InstalledPlugin;
use crate::presentation::app_state::text_field::TextField;
use crate::presentation::key_input::KeyInput;

#[derive(Debug, Clone, PartialEq)]
pub struct NewWidget {
    pub widget_id: String,
    pub blocks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlowOutcome {
    Stay,
    Close,
    Save(NewWidget),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceRow {
    pub answer: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answering {
    plugin_name: String,
    session: SetupSession,
    field: TextField,
    selected_row: usize,
    error: Option<String>,
}

impl Answering {
    fn start(
        plugin: &InstalledPlugin,
        snapshot: &SetupCatalogSnapshot,
        language: Language,
    ) -> Self {
        let session =
            SetupSession::new_in(plugin.manifest.clone(), snapshot.setup_options(), language);
        let mut answering = Self {
            plugin_name: plugin.name.clone(),
            session,
            field: TextField::default(),
            selected_row: 0,
            error: None,
        };
        answering.reset_input();
        answering
    }

    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }

    pub fn state(&self) -> SetupState<'_> {
        self.session.state()
    }

    pub fn field(&self) -> &TextField {
        &self.field
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn selected_row(&self) -> usize {
        self.selected_row
    }

    pub fn rows(&self) -> Vec<ChoiceRow> {
        let SetupState::Asking(prompt) = self.session.state() else {
            return Vec::new();
        };
        let PromptInput::Choice(options) = &prompt.input else {
            return Vec::new();
        };
        let blank = prompt.blank.iter().map(|label| ChoiceRow {
            answer: String::new(),
            label: label.clone(),
            description: String::new(),
        });
        blank.chain(options.iter().map(choice_row)).collect()
    }

    fn is_typing(&self) -> bool {
        matches!(
            self.session.state(),
            SetupState::Asking(Prompt {
                input: PromptInput::Text,
                ..
            })
        )
    }

    fn reset_input(&mut self) {
        self.field.clear();
        self.error = None;
        self.selected_row = 0;
    }

    fn submit(&mut self, text: &str) -> Option<SetupResult> {
        match self.session.answer(text) {
            Answered::Rejected(message) => {
                self.error = Some(message);
                None
            }
            Answered::Advanced => {
                self.reset_input();
                match self.session.state() {
                    SetupState::Finished(result) => Some(result.clone()),
                    SetupState::Asking(_) => None,
                }
            }
        }
    }

    fn move_selection(&mut self, key: KeyInput) {
        let last = self.rows().len().saturating_sub(1);
        self.selected_row = match key {
            KeyInput::Up => self.selected_row.saturating_sub(1),
            KeyInput::Down => (self.selected_row + 1).min(last),
            KeyInput::Home => 0,
            KeyInput::End => last,
            _ => self.selected_row,
        };
    }

    fn handle_key(&mut self, key: KeyInput) -> AnsweringOutcome {
        if matches!(key, KeyInput::Escape | KeyInput::Quit) {
            return AnsweringOutcome::Cancel;
        }
        if self.is_typing() {
            return self.handle_text_key(key);
        }
        match key {
            KeyInput::Enter => {
                let answer = self
                    .rows()
                    .get(self.selected_row)
                    .map(|row| row.answer.clone())
                    .unwrap_or_default();
                self.finished_or_stay(answer)
            }
            other => {
                self.move_selection(other);
                AnsweringOutcome::Stay
            }
        }
    }

    fn handle_text_key(&mut self, key: KeyInput) -> AnsweringOutcome {
        if key == KeyInput::Enter {
            let text = self.field.text().to_string();
            return self.finished_or_stay(text);
        }
        if self.field.handle_key(key) {
            self.error = None;
        }
        AnsweringOutcome::Stay
    }

    fn finished_or_stay(&mut self, answer: String) -> AnsweringOutcome {
        match self.submit(&answer) {
            Some(result) => AnsweringOutcome::Review(Box::new(result)),
            None => AnsweringOutcome::Stay,
        }
    }
}

enum AnsweringOutcome {
    Stay,
    Cancel,
    Review(Box<SetupResult>),
}

fn choice_row(option: &ChoiceOption) -> ChoiceRow {
    ChoiceRow {
        answer: option.answer.clone(),
        label: option.label.clone(),
        description: option.description.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    plugin_name: String,
    result: SetupResult,
    blocks: Vec<String>,
    error: Option<String>,
}

impl Review {
    fn new(plugin_name: String, result: SetupResult) -> Self {
        Self {
            blocks: result.toml_blocks(),
            plugin_name,
            result,
            error: None,
        }
    }

    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }

    pub fn blocks(&self) -> &[String] {
        &self.blocks
    }

    pub fn notes(&self) -> &[String] {
        &self.result.notes
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddWidgetStep {
    ChoosingPlugin { selected: usize },
    Answering(Box<Answering>),
    Reviewing(Box<Review>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddWidgetFlow {
    snapshot: SetupCatalogSnapshot,
    step: AddWidgetStep,
    language: Language,
}

impl AddWidgetFlow {
    pub fn open(snapshot: SetupCatalogSnapshot, language: Language) -> Self {
        Self {
            snapshot,
            step: AddWidgetStep::ChoosingPlugin { selected: 0 },
            language,
        }
    }

    pub fn step(&self) -> &AddWidgetStep {
        &self.step
    }

    pub fn plugins(&self) -> &[InstalledPlugin] {
        &self.snapshot.plugins
    }

    pub fn problems(&self) -> &[String] {
        &self.snapshot.problems
    }

    pub fn is_typing(&self) -> bool {
        matches!(&self.step, AddWidgetStep::Answering(answering) if answering.is_typing())
    }

    pub fn show_save_error(&mut self, message: String) {
        if let AddWidgetStep::Reviewing(review) = &mut self.step {
            review.error = Some(message);
        }
    }

    pub fn handle_key(&mut self, key: KeyInput) -> FlowOutcome {
        match &mut self.step {
            AddWidgetStep::ChoosingPlugin { selected } => {
                let last = self.snapshot.plugins.len().saturating_sub(1);
                match key {
                    KeyInput::Escape | KeyInput::Quit => FlowOutcome::Close,
                    KeyInput::Up => {
                        *selected = selected.saturating_sub(1);
                        FlowOutcome::Stay
                    }
                    KeyInput::Down => {
                        *selected = (*selected + 1).min(last);
                        FlowOutcome::Stay
                    }
                    KeyInput::Home => {
                        *selected = 0;
                        FlowOutcome::Stay
                    }
                    KeyInput::End => {
                        *selected = last;
                        FlowOutcome::Stay
                    }
                    KeyInput::Enter => {
                        let chosen = self.snapshot.plugins.get(*selected).cloned();
                        if let Some(plugin) = chosen {
                            self.step = AddWidgetStep::Answering(Box::new(Answering::start(
                                &plugin,
                                &self.snapshot,
                                self.language,
                            )));
                        }
                        FlowOutcome::Stay
                    }
                    _ => FlowOutcome::Stay,
                }
            }
            AddWidgetStep::Answering(answering) => match answering.handle_key(key) {
                AnsweringOutcome::Stay => FlowOutcome::Stay,
                AnsweringOutcome::Cancel => FlowOutcome::Close,
                AnsweringOutcome::Review(result) => {
                    let plugin_name = answering.plugin_name.clone();
                    self.step =
                        AddWidgetStep::Reviewing(Box::new(Review::new(plugin_name, *result)));
                    FlowOutcome::Stay
                }
            },
            AddWidgetStep::Reviewing(review) => match key {
                KeyInput::Escape | KeyInput::Quit => FlowOutcome::Close,
                KeyInput::Enter => FlowOutcome::Save(NewWidget {
                    widget_id: review.result.draft.id.clone(),
                    blocks: review.blocks.clone(),
                }),
                _ => FlowOutcome::Stay,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plugin_setup::{
        AnswerKind, Choice, InstalledPlugin, Question, Requirement, SetupManifest, Step, Target,
    };

    fn question(name: &str, requirement: Requirement, kind: AnswerKind) -> Step {
        Step::Question(Question {
            target: Target::Variable(name.to_string()),
            prompt: format!("pergunta {name}"),
            kind,
            requirement,
            default: None,
            advanced: false,
            condition: None,
        })
    }

    fn plugin(name: &str, steps: Vec<Step>) -> InstalledPlugin {
        InstalledPlugin {
            name: name.to_string(),
            version: Some("0.0.4".to_string()),
            manifest: SetupManifest {
                plugin: name.to_string(),
                script: format!("plugins/{name}/{name}.py"),
                default_id: name.to_string(),
                interval_seconds: 120,
                timeout_seconds: 30,
                steps,
                inherit_rules: Vec::new(),
                presets: Vec::new(),
                notes: Vec::new(),
            },
        }
    }

    fn method_choices() -> AnswerKind {
        AnswerKind::Choice(vec![
            Choice {
                value: "cli".to_string(),
                description: "usa o CLI".to_string(),
            },
            Choice {
                value: "token".to_string(),
                description: "usa token".to_string(),
            },
        ])
    }

    fn snapshot(plugins: Vec<InstalledPlugin>) -> SetupCatalogSnapshot {
        SetupCatalogSnapshot {
            plugins,
            problems: Vec::new(),
            widget_ids: vec!["existente".to_string()],
            declared_tabs: Vec::new(),
        }
    }

    fn demo_flow() -> AddWidgetFlow {
        AddWidgetFlow::open(
            snapshot(vec![
                plugin("alfa", Vec::new()),
                plugin(
                    "demo",
                    vec![question("DEMO_SITE", Requirement::Always, AnswerKind::Text)],
                ),
            ]),
            Language::BrazilianPortuguese,
        )
    }

    fn press(flow: &mut AddWidgetFlow, keys: &[KeyInput]) -> FlowOutcome {
        let mut outcome = FlowOutcome::Stay;
        for key in keys {
            outcome = flow.handle_key(*key);
        }
        outcome
    }

    fn type_text(flow: &mut AddWidgetFlow, text: &str) {
        for character in text.chars() {
            assert_eq!(
                flow.handle_key(KeyInput::Character(character)),
                FlowOutcome::Stay
            );
        }
    }

    fn answering(flow: &AddWidgetFlow) -> &Answering {
        match flow.step() {
            AddWidgetStep::Answering(answering) => answering,
            other => panic!("esperava perguntas, veio {other:?}"),
        }
    }

    fn prompt_title(flow: &AddWidgetFlow) -> String {
        match answering(flow).state() {
            SetupState::Asking(prompt) => prompt.title.clone(),
            SetupState::Finished(_) => panic!("já terminou"),
        }
    }

    #[test]
    fn it_opens_on_the_plugin_list_and_moves_within_it() {
        let mut flow = demo_flow();
        assert_eq!(flow.step(), &AddWidgetStep::ChoosingPlugin { selected: 0 });
        press(&mut flow, &[KeyInput::Up]);
        assert_eq!(flow.step(), &AddWidgetStep::ChoosingPlugin { selected: 0 });
        press(&mut flow, &[KeyInput::Down, KeyInput::Down]);
        assert_eq!(flow.step(), &AddWidgetStep::ChoosingPlugin { selected: 1 });
        press(&mut flow, &[KeyInput::Home]);
        assert_eq!(flow.step(), &AddWidgetStep::ChoosingPlugin { selected: 0 });
        press(&mut flow, &[KeyInput::End]);
        assert_eq!(flow.step(), &AddWidgetStep::ChoosingPlugin { selected: 1 });
    }

    #[test]
    fn choosing_a_plugin_starts_its_interview_with_the_existing_ids() {
        let mut flow = demo_flow();
        press(&mut flow, &[KeyInput::Down, KeyInput::Enter]);
        assert_eq!(answering(&flow).plugin_name(), "demo");
        assert_eq!(prompt_title(&flow), "Id do widget");
        assert!(flow.is_typing());
        type_text(&mut flow, "existente");
        press(&mut flow, &[KeyInput::Enter]);
        assert!(answering(&flow)
            .error()
            .is_some_and(|message| message.contains("Já existe")));
    }

    #[test]
    fn without_installed_plugins_only_escape_leaves() {
        let mut flow = AddWidgetFlow::open(snapshot(Vec::new()), Language::BrazilianPortuguese);
        assert_eq!(press(&mut flow, &[KeyInput::Enter]), FlowOutcome::Stay);
        assert_eq!(press(&mut flow, &[KeyInput::Down]), FlowOutcome::Stay);
        assert_eq!(press(&mut flow, &[KeyInput::Escape]), FlowOutcome::Close);
    }

    #[test]
    fn typing_editing_and_submitting_a_text_answer() {
        let mut flow = demo_flow();
        press(&mut flow, &[KeyInput::Enter]);
        type_text(&mut flow, "alfaa");
        press(&mut flow, &[KeyInput::Backspace]);
        assert_eq!(answering(&flow).field().text(), "alfa");
        press(&mut flow, &[KeyInput::Enter]);
        assert_eq!(prompt_title(&flow), "Aba do widget");
        assert_eq!(answering(&flow).field().text(), "");
        assert_eq!(answering(&flow).error(), None);
    }

    #[test]
    fn letters_that_are_vim_keys_are_typed_not_navigated() {
        let mut flow = demo_flow();
        press(&mut flow, &[KeyInput::Enter]);
        assert!(flow.is_typing());
        type_text(&mut flow, "hjkl-q");
        assert_eq!(answering(&flow).field().text(), "hjkl-q");
    }

    #[test]
    fn a_rejected_answer_keeps_the_prompt_and_the_text_and_typing_clears_the_message() {
        let mut flow = demo_flow();
        press(&mut flow, &[KeyInput::Enter]);
        type_text(&mut flow, "Meu Widget");
        press(&mut flow, &[KeyInput::Enter]);
        assert_eq!(prompt_title(&flow), "Id do widget");
        assert_eq!(answering(&flow).field().text(), "Meu Widget");
        assert!(answering(&flow)
            .error()
            .is_some_and(|message| message.contains("letras minúsculas")));
        type_text(&mut flow, "x");
        assert_eq!(answering(&flow).error(), None);
    }

    #[test]
    fn a_choice_prompt_lists_a_blank_row_first_and_submits_the_chosen_one() {
        let mut choice = question("DEMO_METHOD", Requirement::Optional, method_choices());
        if let Step::Question(question) = &mut choice {
            question.default = Some("cli".to_string());
        }
        let mut flow = AddWidgetFlow::open(
            snapshot(vec![plugin("demo", vec![choice])]),
            Language::BrazilianPortuguese,
        );
        press(&mut flow, &[KeyInput::Enter; 3]);
        assert!(!flow.is_typing());
        let rows = answering(&flow).rows();
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, vec!["cli (padrão)", "cli", "token"]);
        assert_eq!(answering(&flow).selected_row(), 0);
        press(&mut flow, &[KeyInput::Down, KeyInput::Down, KeyInput::Down]);
        assert_eq!(answering(&flow).selected_row(), 2);
        press(&mut flow, &[KeyInput::Enter]);
        assert_eq!(
            prompt_title(&flow),
            "Intervalo entre atualizações, em segundos"
        );
    }

    #[test]
    fn a_required_choice_has_no_blank_row_and_vim_keys_move_the_selection() {
        let choice = question("DEMO_METHOD", Requirement::Always, method_choices());
        let mut flow = AddWidgetFlow::open(
            snapshot(vec![plugin("demo", vec![choice])]),
            Language::BrazilianPortuguese,
        );
        press(&mut flow, &[KeyInput::Enter; 3]);
        assert_eq!(answering(&flow).rows().len(), 2);
        press(&mut flow, &[KeyInput::Down]);
        assert_eq!(answering(&flow).selected_row(), 1);
    }

    #[test]
    fn finishing_the_interview_leads_to_the_review_and_enter_asks_to_save() {
        let mut flow = demo_flow();
        press(
            &mut flow,
            &[
                KeyInput::Down,
                KeyInput::Enter,
                KeyInput::Enter,
                KeyInput::Enter,
            ],
        );
        type_text(&mut flow, "empresa.exemplo");
        press(
            &mut flow,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        let AddWidgetStep::Reviewing(review) = flow.step() else {
            panic!("esperava a revisão");
        };
        assert!(review.blocks()[0].starts_with("[[widgets]]\nid = \"demo\""));
        assert!(review.blocks()[0].contains("DEMO_SITE = \"empresa.exemplo\""));
        let outcome = press(&mut flow, &[KeyInput::Enter]);
        let FlowOutcome::Save(widget) = outcome else {
            panic!("esperava gravar");
        };
        assert_eq!(widget.widget_id, "demo");
        assert_eq!(widget.blocks.len(), 1);
    }

    #[test]
    fn a_save_error_is_shown_in_the_review_and_the_flow_stays_open() {
        let mut flow = AddWidgetFlow::open(
            snapshot(vec![plugin("demo", Vec::new())]),
            Language::BrazilianPortuguese,
        );
        press(&mut flow, &[KeyInput::Enter; 5]);
        assert!(matches!(
            flow.step(),
            AddWidgetStep::Reviewing(review) if review.error().is_none()
        ));
        flow.show_save_error("disco cheio".to_string());
        let AddWidgetStep::Reviewing(review) = flow.step() else {
            panic!("esperava a revisão");
        };
        assert_eq!(review.error(), Some("disco cheio"));
    }

    #[test]
    fn escape_and_control_c_cancel_at_every_step() {
        for cancel in [KeyInput::Escape, KeyInput::Quit] {
            let mut list = demo_flow();
            assert_eq!(press(&mut list, &[cancel]), FlowOutcome::Close);
            let mut asking = demo_flow();
            press(&mut asking, &[KeyInput::Enter]);
            assert_eq!(press(&mut asking, &[cancel]), FlowOutcome::Close);
            let mut reviewing = AddWidgetFlow::open(
                snapshot(vec![plugin("demo", Vec::new())]),
                Language::BrazilianPortuguese,
            );
            press(&mut reviewing, &[KeyInput::Enter; 5]);
            assert!(matches!(reviewing.step(), AddWidgetStep::Reviewing(_)));
            assert_eq!(press(&mut reviewing, &[cancel]), FlowOutcome::Close);
        }
    }
}
