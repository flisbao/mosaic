use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use chrono::NaiveDateTime;

use crate::application::pomodoro_notifications::{alert_for, AlertMessage};
use crate::application::ports::SourceError;
use crate::application::recommendation::RecommendationPlan;
use crate::application::recommendation_view::IA_WIDGET_ID;
use crate::domain::action::{ConfirmedAction, ProposedAction};
use crate::domain::pomodoro::PomodoroTimer;
use crate::domain::shortcuts::assign_shortcuts;
use crate::domain::widget::{
    ActionScope, ActionSpec, DetailItem, WidgetId, WidgetRecord, WidgetSnapshot,
};
use crate::presentation::key_input::KeyInput;

mod focus_navigation;

pub const MAXIMUM_QUEUED_ALERTS: usize = 20;
pub const PAGE_JUMP_ITEMS: usize = 10;
const NO_ACTIONS_NOTICE: &str = "Nenhuma ação disponível aqui";
const POMODORO_DISABLED_NOTICE: &str = "Pomodoro desligado na configuração";
const IA_BUSY_NOTICE: &str = "A IA já está processando um pedido";
const ACTION_PENDING_NOTICE: &str = "Ação em andamento";
const NO_DETAIL_NOTICE: &str = "Este widget ainda não tem detalhe para mostrar";

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetEntry {
    pub id: WidgetId,
    pub record: WidgetRecord,
    pub interval: Duration,
    pub max_items: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabView {
    pub name: String,
    pub entry_indexes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Browsing,
    ChoosingAction {
        selected: usize,
    },
    Confirming(ProposedAction),
    ConfirmingSend {
        text: String,
        plan: RecommendationPlan,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Quit,
    RefreshAll,
    ReloadConfig,
    Execute(ConfirmedAction),
    RequestRecommendation,
    SendRecommendation(RecommendationPlan),
}

pub struct AppState {
    entries: Vec<WidgetEntry>,
    tabs: Vec<TabView>,
    active_tab: usize,
    focus: usize,
    expanded: bool,
    selected_item: usize,
    mode: Mode,
    notice: Option<String>,
    pomodoro: Option<PomodoroTimer>,
    alerts: VecDeque<AlertMessage>,
    pending_actions: HashSet<WidgetId>,
}

impl AppState {
    pub fn new(entries: Vec<WidgetEntry>, tabs: Vec<TabView>) -> Self {
        Self {
            entries,
            tabs,
            active_tab: 0,
            focus: 0,
            expanded: false,
            selected_item: 0,
            mode: Mode::Browsing,
            notice: None,
            pomodoro: None,
            alerts: VecDeque::new(),
            pending_actions: HashSet::new(),
        }
    }

    pub fn replace_tabs(&mut self, entries: Vec<WidgetEntry>, tabs: Vec<TabView>) {
        let previous_focus = self.focused_entry();
        self.entries = entries;
        self.tabs = tabs;
        self.active_tab = self.active_tab.min(self.tabs.len().saturating_sub(1));
        let last_focus = self.active_entry_indexes().len().saturating_sub(1);
        self.focus = self.focus.min(last_focus);
        if self.focused_entry() != previous_focus {
            self.selected_item = 0;
        }
        if self.focused_entry().is_none() {
            self.expanded = false;
        }
        self.settle_after_widget_change();
    }

    pub fn with_pomodoro(mut self, timer: PomodoroTimer) -> Self {
        self.pomodoro = Some(timer);
        self
    }

    pub fn pomodoro(&self) -> Option<&PomodoroTimer> {
        self.pomodoro.as_ref()
    }

    pub fn alert(&self) -> Option<&AlertMessage> {
        self.alerts.front()
    }

    pub fn pending_alert_count(&self) -> usize {
        self.alerts.len()
    }

    pub fn show_alert(&mut self, message: AlertMessage) {
        if self.alerts.len() < MAXIMUM_QUEUED_ALERTS {
            self.alerts.push_back(message);
        }
    }

    pub fn advance_pomodoro(&mut self, elapsed: Duration) -> Option<AlertMessage> {
        let ended = self.pomodoro.as_mut()?.advance(elapsed)?;
        let alert = alert_for(&ended);
        self.show_alert(alert.clone());
        Some(alert)
    }

    pub fn ask_send_confirmation(&mut self, text: String, plan: RecommendationPlan) {
        self.mode = Mode::ConfirmingSend { text, plan };
    }

    pub fn entries(&self) -> &[WidgetEntry] {
        &self.entries
    }

    pub fn tabs(&self) -> &[TabView] {
        &self.tabs
    }

    pub fn active_tab(&self) -> usize {
        self.active_tab
    }

    pub fn active_entry_indexes(&self) -> &[usize] {
        match self.tabs.get(self.active_tab) {
            Some(tab) => &tab.entry_indexes,
            None => &[],
        }
    }

    pub fn focused_entry(&self) -> Option<usize> {
        self.active_entry_indexes()
            .get(self.focus)
            .copied()
            .filter(|&index| index < self.entries.len())
    }

    pub fn shortcut_of(&self, entry_index: usize) -> Option<char> {
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.entry_indexes.contains(&entry_index))?;
        let position = tab
            .entry_indexes
            .iter()
            .position(|&index| index == entry_index)?;
        assign_shortcuts(tab.entry_indexes.len())
            .get(position)
            .copied()
            .flatten()
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn selected_item(&self) -> usize {
        self.selected_item
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn show_notice(&mut self, text: String) {
        self.notice = Some(text);
    }

    pub fn mark_action_pending(&mut self, id: &WidgetId) {
        self.pending_actions.insert(id.clone());
    }

    pub fn finish_action(&mut self, id: &WidgetId) {
        self.pending_actions.remove(id);
    }

    pub fn update_widget(
        &mut self,
        id: &WidgetId,
        result: Result<WidgetSnapshot, SourceError>,
        now: NaiveDateTime,
    ) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| &entry.id == id) {
            entry.record.apply(result, now);
        }
        self.settle_after_widget_change();
    }

    pub fn begin_widget_refresh(&mut self, id: &WidgetId) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| &entry.id == id) {
            entry.record.begin_refresh();
        }
        self.settle_after_widget_change();
    }

    fn settle_after_widget_change(&mut self) {
        if matches!(self.mode, Mode::ChoosingAction { .. }) {
            self.mode = Mode::Browsing;
        }
        self.selected_item = self.selected_item.min(self.item_count().saturating_sub(1));
    }

    pub fn detail_items(&self) -> Vec<&DetailItem> {
        let Some(record) = self.focused_record() else {
            return Vec::new();
        };
        if record.is_failing() {
            return Vec::new();
        }
        let Some(snapshot) = record.snapshot() else {
            return Vec::new();
        };
        snapshot
            .detail
            .iter()
            .flatten()
            .flat_map(|section| section.items.iter())
            .collect()
    }

    pub fn available_actions(&self) -> Vec<ActionSpec> {
        let Some(record) = self.focused_record() else {
            return Vec::new();
        };
        if record.is_failing() {
            return Vec::new();
        }
        let Some(snapshot) = record.snapshot() else {
            return Vec::new();
        };
        let item_selected = self.selected_detail_item_id().is_some();
        snapshot
            .actions
            .iter()
            .filter(|action| action.scope == ActionScope::Widget || item_selected)
            .cloned()
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyInput) -> Option<Command> {
        self.notice = None;
        if self.alerts.pop_front().is_some() {
            return None;
        }
        match std::mem::replace(&mut self.mode, Mode::Browsing) {
            Mode::Browsing => self.handle_browsing_key(key),
            Mode::ChoosingAction { selected } => {
                self.handle_action_menu_key(key, selected);
                None
            }
            Mode::Confirming(proposed) => self.handle_confirmation_key(key, proposed),
            Mode::ConfirmingSend { text, plan } => {
                self.handle_send_confirmation_key(key, text, plan)
            }
        }
    }

    fn handle_browsing_key(&mut self, key: KeyInput) -> Option<Command> {
        match key {
            KeyInput::Character(letter) => self.handle_character(letter),
            KeyInput::Tab
            | KeyInput::BackTab
            | KeyInput::Up
            | KeyInput::Down
            | KeyInput::Left
            | KeyInput::Right
            | KeyInput::Enter
            | KeyInput::Escape
            | KeyInput::PageUp
            | KeyInput::PageDown
            | KeyInput::Home
            | KeyInput::End => {
                self.handle_navigation_key(key);
                None
            }
        }
    }

    fn handle_character(&mut self, letter: char) -> Option<Command> {
        match letter {
            'r' => Some(Command::RefreshAll),
            'R' => Some(Command::ReloadConfig),
            'q' => Some(Command::Quit),
            'i' => self.request_recommendation(),
            'p' => {
                self.toggle_pomodoro();
                None
            }
            'x' => {
                self.reset_pomodoro();
                None
            }
            'a' => {
                self.open_action_menu();
                None
            }
            other => {
                self.handle_selection_character(other);
                None
            }
        }
    }

    fn request_recommendation(&mut self) -> Option<Command> {
        if self.ia_is_loading() {
            self.notice = Some(IA_BUSY_NOTICE.to_string());
            None
        } else {
            Some(Command::RequestRecommendation)
        }
    }

    fn handle_action_menu_key(&mut self, key: KeyInput, selected: usize) {
        let actions = self.available_actions();
        let last = actions.len().saturating_sub(1);
        self.mode = match key {
            KeyInput::Down => Mode::ChoosingAction {
                selected: (selected + 1).min(last),
            },
            KeyInput::Up => Mode::ChoosingAction {
                selected: selected.saturating_sub(1),
            },
            KeyInput::Enter => match self.propose(actions.get(selected)) {
                Some(proposed) => Mode::Confirming(proposed),
                None => Mode::Browsing,
            },
            KeyInput::Escape | KeyInput::Character('n') => Mode::Browsing,
            _ => Mode::ChoosingAction { selected },
        };
    }

    fn handle_confirmation_key(
        &mut self,
        key: KeyInput,
        proposed: ProposedAction,
    ) -> Option<Command> {
        match key {
            KeyInput::Character('s') => {
                let confirmed = proposed.confirm();
                if !self.pending_actions.insert(confirmed.widget().clone()) {
                    self.show_notice(ACTION_PENDING_NOTICE.to_string());
                    return None;
                }
                Some(Command::Execute(confirmed))
            }
            KeyInput::Escape | KeyInput::Character('n') => None,
            _ => {
                self.mode = Mode::Confirming(proposed);
                None
            }
        }
    }

    fn handle_send_confirmation_key(
        &mut self,
        key: KeyInput,
        text: String,
        plan: RecommendationPlan,
    ) -> Option<Command> {
        match key {
            KeyInput::Character('s') => Some(Command::SendRecommendation(plan)),
            KeyInput::Escape | KeyInput::Character('n') => None,
            _ => {
                self.mode = Mode::ConfirmingSend { text, plan };
                None
            }
        }
    }

    fn ia_is_loading(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.id.as_str() == IA_WIDGET_ID && entry.record.is_refreshing())
    }

    fn toggle_pomodoro(&mut self) {
        match self.pomodoro.as_mut() {
            Some(timer) => timer.toggle(),
            None => self.notice = Some(POMODORO_DISABLED_NOTICE.to_string()),
        }
    }

    fn reset_pomodoro(&mut self) {
        match self.pomodoro.as_mut() {
            Some(timer) => timer.reset(),
            None => self.notice = Some(POMODORO_DISABLED_NOTICE.to_string()),
        }
    }

    fn propose(&mut self, spec: Option<&ActionSpec>) -> Option<ProposedAction> {
        let spec = spec?.clone();
        let entry_index = self.focused_entry()?;
        let widget_id = self.entries.get(entry_index)?.id.clone();
        let item_id = match spec.scope {
            ActionScope::Item => self.selected_detail_item_id(),
            ActionScope::Widget => None,
        };
        match ProposedAction::new(widget_id, spec, item_id) {
            Ok(proposed) => Some(proposed),
            Err(error) => {
                self.notice = Some(error.to_string());
                None
            }
        }
    }

    fn open_action_menu(&mut self) {
        if self.available_actions().is_empty() {
            self.notice = Some(NO_ACTIONS_NOTICE.to_string());
        } else {
            self.mode = Mode::ChoosingAction { selected: 0 };
        }
    }

    fn focused_record(&self) -> Option<&WidgetRecord> {
        let entry_index = self.focused_entry()?;
        self.entries.get(entry_index).map(|entry| &entry.record)
    }

    fn item_count(&self) -> usize {
        if self.expanded {
            self.detail_items().len()
        } else {
            0
        }
    }

    fn selected_detail_item_id(&self) -> Option<String> {
        if !self.expanded {
            return None;
        }
        self.detail_items()
            .get(self.selected_item)
            .and_then(|item| item.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::pomodoro::{PomodoroSettings, PomodoroStatus};
    use crate::domain::widget::{
        ActionScope, ActionSpec, DetailItem, DetailSection, Summary, WidgetSnapshot,
    };
    use chrono::NaiveDate;

    const LONG_LIST_LENGTH: usize = 25;
    const LAST_LONG_LIST_ITEM: usize = LONG_LIST_LENGTH - 1;
    const WIDGETS_IN_TAB: usize = 30;
    const LAST_FREE_LETTER_WIDGET: usize = 24;
    const FIRST_WIDGET_AFTER_LAST_LETTER: usize = LAST_FREE_LETTER_WIDGET + 1;

    fn action(id: &str, scope: ActionScope, description: &str) -> ActionSpec {
        ActionSpec {
            id: id.to_string(),
            label: id.to_string(),
            description: Some(description.to_string()),
            scope,
            params_json: r#"{"to":"done"}"#.to_string(),
        }
    }

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 18)
            .and_then(|date| date.and_hms_opt(12, 0, 0))
            .expect("data válida")
    }

    fn record_with_items(item_ids: Vec<String>) -> WidgetRecord {
        let items = item_ids
            .into_iter()
            .map(|id| DetailItem {
                id: Some(id.clone()),
                text: id,
                flag: None,
                meta: vec![],
            })
            .collect();
        let mut record = WidgetRecord::loading();
        record.apply::<String>(
            Ok(WidgetSnapshot {
                mini: None,
                alerts: vec![],
                summary: Summary {
                    blocks: vec![],
                    title: "Kanban".to_string(),
                    lines: vec![],
                },
                detail: Some(vec![DetailSection {
                    blocks: vec![],
                    heading: "inbox".to_string(),
                    items,
                }]),
                actions: vec![
                    action("move", ActionScope::Item, "Mover para done"),
                    action("refresh", ActionScope::Widget, "Atualizar kanban"),
                ],
            }),
            now(),
        );
        record
    }

    fn kanban_record() -> WidgetRecord {
        record_with_items(vec!["inbox/a.md".to_string(), "inbox/b.md".to_string()])
    }

    fn long_record() -> WidgetRecord {
        record_with_items(
            (0..LONG_LIST_LENGTH)
                .map(|number| format!("item-{number}"))
                .collect(),
        )
    }

    fn entry(id: &str, record: WidgetRecord) -> WidgetEntry {
        WidgetEntry {
            id: WidgetId::new(id),
            record,
            interval: Duration::from_secs(30),
            max_items: None,
        }
    }

    fn plain_entries(count: usize) -> Vec<WidgetEntry> {
        (0..count)
            .map(|number| entry(&format!("widget-{number}"), WidgetRecord::loading()))
            .collect()
    }

    fn tab(name: &str, entry_indexes: &[usize]) -> TabView {
        TabView {
            name: name.to_string(),
            entry_indexes: entry_indexes.to_vec(),
        }
    }

    fn app() -> AppState {
        AppState::new(
            vec![
                entry("kanban", kanban_record()),
                entry("plain", WidgetRecord::loading()),
                entry("notes", WidgetRecord::loading()),
            ],
            vec![tab("Principal", &[0, 1]), tab("Notas", &[2])],
        )
    }

    fn app_with_one_tab(entries: Vec<WidgetEntry>) -> AppState {
        let entry_indexes: Vec<usize> = (0..entries.len()).collect();
        AppState::new(entries, vec![tab("Principal", &entry_indexes)])
    }

    fn press(app: &mut AppState, keys: &[KeyInput]) -> Option<Command> {
        let mut last = None;
        for key in keys {
            last = app.handle_key(*key);
        }
        last
    }

    fn character(value: char) -> KeyInput {
        KeyInput::Character(value)
    }

    fn every_key() -> Vec<KeyInput> {
        vec![
            KeyInput::Tab,
            KeyInput::BackTab,
            KeyInput::Up,
            KeyInput::Down,
            KeyInput::Enter,
            KeyInput::Escape,
            KeyInput::Left,
            KeyInput::Right,
            KeyInput::PageUp,
            KeyInput::PageDown,
            KeyInput::Home,
            KeyInput::End,
            character('a'),
            character('A'),
            character('R'),
            character('r'),
            character('x'),
            character('P'),
            character('p'),
            character('i'),
            character('q'),
            character('0'),
            character('1'),
            character('9'),
            character('z'),
        ]
    }

    fn app_confirming_move_of_second_item() -> AppState {
        let mut app = app();
        press(
            &mut app,
            &[
                KeyInput::Enter,
                KeyInput::Down,
                character('a'),
                KeyInput::Enter,
            ],
        );
        app
    }

    fn app_with_pomodoro() -> AppState {
        app().with_pomodoro(PomodoroTimer::new(PomodoroSettings {
            focus: Duration::from_secs(25 * 60),
            short_break: Duration::from_secs(5 * 60),
            long_break: Duration::from_secs(15 * 60),
            cycles_before_long_break: 4,
            suggestions: vec!["Beber água".to_string()],
        }))
    }

    fn plan() -> RecommendationPlan {
        RecommendationPlan {
            prompt: "pedido".to_string(),
            item_count: 3,
            estimated_tokens: 40,
        }
    }

    fn message(title: &str) -> AlertMessage {
        AlertMessage {
            title: title.to_string(),
            body: String::new(),
        }
    }

    #[test]
    fn digit_keys_switch_tabs_and_ignore_out_of_range() {
        let mut app = AppState::new(
            plain_entries(3),
            vec![tab("Um", &[0]), tab("Dois", &[1]), tab("Três", &[2])],
        );
        press(&mut app, &[character('2')]);
        assert_eq!(app.active_tab(), 1);
        press(&mut app, &[character('9')]);
        assert_eq!(app.active_tab(), 1);
        press(&mut app, &[character('0')]);
        assert_eq!(app.active_tab(), 1);
        press(&mut app, &[character('3')]);
        assert_eq!(app.active_tab(), 2);
    }

    #[test]
    fn tab_wraps_around() {
        let mut app = AppState::new(
            plain_entries(3),
            vec![tab("Um", &[0]), tab("Dois", &[1]), tab("Três", &[2])],
        );
        press(&mut app, &[KeyInput::Tab; 3]);
        assert_eq!(app.active_tab(), 0);
        press(&mut app, &[KeyInput::BackTab]);
        assert_eq!(app.active_tab(), 2);
        press(&mut app, &[KeyInput::Tab]);
        assert_eq!(app.active_tab(), 0);
    }

    #[test]
    fn arrows_move_focus_in_grid_and_clamp() {
        let mut app = app_with_one_tab(plain_entries(3));
        press(&mut app, &[KeyInput::Down]);
        assert_eq!(app.focused_entry(), Some(1));
        press(&mut app, &[KeyInput::Right]);
        assert_eq!(app.focused_entry(), Some(2));
        press(&mut app, &[KeyInput::Down; 5]);
        assert_eq!(app.focused_entry(), Some(2));
        press(&mut app, &[KeyInput::Up; 10]);
        assert_eq!(app.focused_entry(), Some(0));
        press(&mut app, &[KeyInput::Left]);
        assert_eq!(app.focused_entry(), Some(0));
    }

    #[test]
    fn shift_letter_focuses_widget_of_current_tab_only() {
        let mut app = AppState::new(
            plain_entries(4),
            vec![tab("Um", &[0, 1]), tab("Dois", &[2, 3])],
        );
        press(&mut app, &[character('B')]);
        assert_eq!(app.focused_entry(), Some(1));
        press(&mut app, &[character('C')]);
        assert_eq!(app.focused_entry(), Some(1));
        press(&mut app, &[KeyInput::Tab, character('B')]);
        assert_eq!(app.focused_entry(), Some(3));
        press(&mut app, &[character('A')]);
        assert_eq!(app.focused_entry(), Some(2));
    }

    #[test]
    fn shortcuts_restart_at_a_per_tab() {
        let app = AppState::new(
            plain_entries(4),
            vec![tab("Um", &[0, 1]), tab("Dois", &[2, 3])],
        );
        assert_eq!(app.shortcut_of(0), Some('A'));
        assert_eq!(app.shortcut_of(1), Some('B'));
        assert_eq!(app.shortcut_of(2), Some('A'));
        assert_eq!(app.shortcut_of(3), Some('B'));
    }

    #[test]
    fn widgets_beyond_free_letters_have_no_shortcut_but_arrows_reach_them() {
        let mut app = app_with_one_tab(plain_entries(WIDGETS_IN_TAB));
        assert_eq!(app.shortcut_of(LAST_FREE_LETTER_WIDGET), Some('Z'));
        assert_eq!(app.shortcut_of(FIRST_WIDGET_AFTER_LAST_LETTER), None);
        press(&mut app, &[KeyInput::Down; WIDGETS_IN_TAB]);
        assert_eq!(app.focused_entry(), Some(WIDGETS_IN_TAB - 1));
        assert_eq!(app.shortcut_of(WIDGETS_IN_TAB - 1), None);
        press(&mut app, &[character('Z')]);
        assert_eq!(app.focused_entry(), Some(LAST_FREE_LETTER_WIDGET));
    }

    #[test]
    fn enter_expands_and_escape_collapses() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter]);
        assert!(app.is_expanded());
        press(&mut app, &[KeyInput::Escape]);
        assert!(!app.is_expanded());
        assert_eq!(app.focused_entry(), Some(0));
    }

    #[test]
    fn focus_mode_arrows_move_item_selection_and_clamp_at_ends() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter, KeyInput::Down]);
        assert_eq!(app.selected_item(), 1);
        press(&mut app, &[KeyInput::Down]);
        assert_eq!(app.selected_item(), 1);
        press(&mut app, &[KeyInput::Up; 3]);
        assert_eq!(app.selected_item(), 0);

        let mut long = app_with_one_tab(vec![entry("longa", long_record())]);
        press(&mut long, &[KeyInput::Enter, KeyInput::PageDown]);
        assert_eq!(long.selected_item(), PAGE_JUMP_ITEMS);
        press(&mut long, &[KeyInput::PageDown; 5]);
        assert_eq!(long.selected_item(), LAST_LONG_LIST_ITEM);
        press(&mut long, &[KeyInput::PageUp]);
        assert_eq!(long.selected_item(), LAST_LONG_LIST_ITEM - PAGE_JUMP_ITEMS);
        press(&mut long, &[KeyInput::Home]);
        assert_eq!(long.selected_item(), 0);
        press(&mut long, &[KeyInput::End]);
        assert_eq!(long.selected_item(), LAST_LONG_LIST_ITEM);
    }

    #[test]
    fn shift_letter_in_focus_mode_switches_widget_and_stays_expanded() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter, KeyInput::Down]);
        assert_eq!(app.selected_item(), 1);
        press(&mut app, &[character('B')]);
        assert_eq!(app.focused_entry(), Some(1));
        assert!(app.is_expanded());
        assert_eq!(app.selected_item(), 0);
        press(&mut app, &[character('A')]);
        assert_eq!(app.focused_entry(), Some(0));
        assert!(app.is_expanded());
    }

    #[test]
    fn r_uppercase_returns_reload_command_and_lowercase_refreshes_all() {
        let mut app = app();
        assert_eq!(app.handle_key(character('R')), Some(Command::ReloadConfig));
        assert_eq!(app.handle_key(character('r')), Some(Command::RefreshAll));
        press(&mut app, &[KeyInput::Enter]);
        assert_eq!(app.handle_key(character('R')), Some(Command::ReloadConfig));
    }

    #[test]
    fn x_resets_pomodoro_and_uppercase_p_does_nothing() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        app.advance_pomodoro(Duration::from_secs(60));
        let remaining_before = app.pomodoro().expect("pomodoro ligado").remaining();
        assert_eq!(app.handle_key(character('P')), None);
        let timer = app.pomodoro().expect("pomodoro ligado");
        assert_eq!(timer.status(), PomodoroStatus::Running);
        assert_eq!(timer.remaining(), remaining_before);
        assert_eq!(app.handle_key(character('x')), None);
        let timer = app.pomodoro().expect("pomodoro ligado");
        assert_eq!(timer.status(), PomodoroStatus::Stopped);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    fn assert_nothing_focused(app: &AppState, expected_tab: usize) {
        assert_eq!(app.focused_entry(), None);
        assert!(!app.is_expanded());
        assert_eq!(app.active_tab(), expected_tab);
    }

    #[test]
    fn empty_tab_does_not_panic_on_any_key() {
        for key in every_key() {
            let mut grid = AppState::new(plain_entries(2), vec![tab("Vazia", &[])]);
            grid.handle_key(key);
            assert_nothing_focused(&grid, 0);
        }
        let mut focused = app_with_one_tab(vec![entry("kanban", kanban_record())]);
        press(&mut focused, &[KeyInput::Enter]);
        focused.replace_tabs(
            vec![entry("kanban", kanban_record())],
            vec![tab("Vazia", &[])],
        );
        for key in every_key() {
            focused.handle_key(key);
            assert_nothing_focused(&focused, 0);
        }
        let mut without_tabs = AppState::new(vec![], vec![]);
        for key in every_key() {
            without_tabs.handle_key(key);
            assert_eq!(without_tabs.focused_entry(), None);
            assert!(!without_tabs.is_expanded());
        }
    }

    #[test]
    fn entry_index_outside_entries_never_gets_focus_or_expansion() {
        for key in every_key() {
            let mut app = AppState::new(plain_entries(1), vec![tab("X", &[99])]);
            app.handle_key(key);
            assert_nothing_focused(&app, 0);
        }
        let mut app = AppState::new(plain_entries(1), vec![tab("X", &[99])]);
        for key in every_key() {
            app.handle_key(key);
            assert_nothing_focused(&app, 0);
        }
    }

    #[test]
    fn replace_tabs_clamps_active_tab_and_focus_and_collapses_when_tab_is_empty() {
        let mut switched =
            AppState::new(plain_entries(4), vec![tab("A", &[0, 1, 2]), tab("B", &[3])]);
        press(&mut switched, &[KeyInput::Tab]);
        assert_eq!(switched.active_tab(), 1);
        switched.replace_tabs(plain_entries(2), vec![tab("Só", &[0, 1])]);
        assert_eq!(switched.active_tab(), 0);
        assert_eq!(switched.focused_entry(), Some(0));

        let mut shorter = app_with_one_tab(plain_entries(3));
        press(&mut shorter, &[KeyInput::Down, KeyInput::Down]);
        assert_eq!(shorter.focused_entry(), Some(2));
        shorter.replace_tabs(plain_entries(3), vec![tab("Curta", &[0])]);
        assert_eq!(shorter.focused_entry(), Some(0));

        let mut expanded = app();
        press(&mut expanded, &[KeyInput::Enter]);
        assert!(expanded.is_expanded());
        expanded.replace_tabs(
            vec![entry("kanban", kanban_record())],
            vec![tab("Vazia", &[])],
        );
        assert_eq!(expanded.focused_entry(), None);
        assert!(!expanded.is_expanded());
    }

    #[test]
    fn replace_tabs_resets_selected_item_when_focus_moves_to_another_widget() {
        let mut app = AppState::new(
            vec![
                entry("kanban", kanban_record()),
                entry("outro", kanban_record()),
            ],
            vec![tab("A", &[0, 1])],
        );
        press(&mut app, &[KeyInput::Enter, KeyInput::Down]);
        assert_eq!(app.selected_item(), 1);
        app.replace_tabs(
            vec![
                entry("kanban", kanban_record()),
                entry("outro", kanban_record()),
            ],
            vec![tab("A", &[1, 0])],
        );
        assert_eq!(app.focused_entry(), Some(1));
        assert_eq!(app.selected_item(), 0);
    }

    #[test]
    fn replace_tabs_after_rename_keeps_focus_on_the_same_widget() {
        let mut app = AppState::new(plain_entries(2), vec![tab("Antiga", &[0, 1])]);
        press(&mut app, &[KeyInput::Down]);
        app.replace_tabs(plain_entries(2), vec![tab("Nova", &[0, 1])]);
        assert_eq!(app.tabs()[0].name, "Nova");
        assert_eq!(app.focused_entry(), Some(1));
    }

    #[test]
    fn switching_tabs_resets_focus_and_selected_item() {
        let mut app = AppState::new(
            vec![
                entry("kanban", kanban_record()),
                entry("plain", WidgetRecord::loading()),
                entry("outra", WidgetRecord::loading()),
                entry("de_b", WidgetRecord::loading()),
            ],
            vec![tab("A", &[0, 1, 2]), tab("B", &[3])],
        );
        press(&mut app, &[KeyInput::Down, KeyInput::Down]);
        assert_eq!(app.focused_entry(), Some(2));
        press(&mut app, &[KeyInput::Tab]);
        assert_eq!(app.focused_entry(), Some(3));
        assert_eq!(app.selected_item(), 0);
        press(&mut app, &[KeyInput::BackTab]);
        assert_eq!(app.active_tab(), 0);
        assert_eq!(app.focused_entry(), Some(0));
        assert_eq!(app.selected_item(), 0);
    }

    #[test]
    fn r_uppercase_with_twenty_widgets_keeps_focus() {
        let mut app = app_with_one_tab(plain_entries(20));
        press(&mut app, &[KeyInput::Down; 5]);
        assert_eq!(app.handle_key(character('R')), Some(Command::ReloadConfig));
        assert_eq!(app.focused_entry(), Some(5));
    }

    #[test]
    fn tab_and_digits_do_not_switch_tabs_while_expanded() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter, KeyInput::Tab, character('2')]);
        assert_eq!(app.active_tab(), 0);
        assert!(app.is_expanded());
        assert_eq!(app.focused_entry(), Some(0));
    }

    #[test]
    fn digit_of_active_tab_keeps_focus() {
        let mut app = app();
        press(&mut app, &[KeyInput::Down, character('1')]);
        assert_eq!(app.active_tab(), 0);
        assert_eq!(app.focused_entry(), Some(1));
    }

    #[test]
    fn item_actions_use_selected_item_in_focus_mode() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter, KeyInput::Down, character('a')]);
        assert_eq!(app.mode(), &Mode::ChoosingAction { selected: 0 });
        let ids: Vec<String> = app.available_actions().into_iter().map(|a| a.id).collect();
        assert_eq!(ids, vec!["move".to_string(), "refresh".to_string()]);
        press(&mut app, &[KeyInput::Enter]);
        match app.mode() {
            Mode::Confirming(proposed) => assert_eq!(proposed.item_id(), Some("inbox/b.md")),
            other => panic!("esperava Confirming, veio {other:?}"),
        }
    }

    #[test]
    fn grid_action_key_lists_only_widget_scoped_actions() {
        let mut app = app();
        press(&mut app, &[character('a')]);
        let ids: Vec<String> = app.available_actions().into_iter().map(|a| a.id).collect();
        assert_eq!(ids, vec!["refresh".to_string()]);
    }

    #[test]
    fn action_key_without_actions_shows_notice() {
        let mut app = app();
        press(&mut app, &[KeyInput::Down, character('a')]);
        assert_eq!(app.mode(), &Mode::Browsing);
        assert!(app.notice().is_some());
    }

    #[test]
    fn enter_on_action_moves_to_confirming() {
        let app = app_confirming_move_of_second_item();
        match app.mode() {
            Mode::Confirming(proposed) => {
                assert_eq!(proposed.describe(), "Mover para done");
                assert_eq!(proposed.item_id(), Some("inbox/b.md"));
            }
            other => panic!("esperava Confirming, veio {other:?}"),
        }
    }

    #[test]
    fn second_confirmation_for_the_same_widget_is_blocked_while_pending() {
        let mut app = app_confirming_move_of_second_item();
        assert!(matches!(
            app.handle_key(character('s')),
            Some(Command::Execute(_))
        ));

        let mut again = app_confirming_move_of_second_item();
        again.mark_action_pending(&WidgetId::new("kanban"));
        assert_eq!(again.handle_key(character('s')), None);
        assert_eq!(again.notice(), Some("Ação em andamento"));
    }

    #[test]
    fn confirmation_works_again_after_the_action_finishes() {
        let mut next = app_confirming_move_of_second_item();
        next.mark_action_pending(&WidgetId::new("kanban"));
        next.finish_action(&WidgetId::new("kanban"));
        assert!(matches!(
            next.handle_key(character('s')),
            Some(Command::Execute(_))
        ));
    }

    #[test]
    fn confirm_key_emits_execute_command() {
        let mut app = app_confirming_move_of_second_item();
        match app.handle_key(character('s')) {
            Some(Command::Execute(confirmed)) => {
                assert_eq!(confirmed.action_id(), "move");
                assert_eq!(confirmed.item_id(), Some("inbox/b.md"));
                assert_eq!(confirmed.widget().as_str(), "kanban");
            }
            other => panic!("esperava Execute, veio {other:?}"),
        }
        assert_eq!(app.mode(), &Mode::Browsing);
    }

    #[test]
    fn cancel_key_returns_to_browsing_without_command() {
        for cancel in [character('n'), KeyInput::Escape] {
            let mut app = app_confirming_move_of_second_item();
            assert_eq!(app.handle_key(cancel), None);
            assert_eq!(app.mode(), &Mode::Browsing);
        }
    }

    #[test]
    fn confirm_key_outside_confirming_is_ignored() {
        let mut app = app();
        assert_eq!(app.handle_key(character('s')), None);
        press(&mut app, &[KeyInput::Enter, character('a')]);
        let mode_before = app.mode().clone();
        assert_eq!(app.handle_key(character('s')), None);
        assert_eq!(app.mode(), &mode_before);
    }

    #[test]
    fn quit_and_refresh_keys_emit_commands() {
        let mut app = app();
        assert_eq!(app.handle_key(character('q')), Some(Command::Quit));
        assert_eq!(app.handle_key(character('r')), Some(Command::RefreshAll));
    }

    #[test]
    fn quit_is_ignored_while_confirming() {
        let mut app = app_confirming_move_of_second_item();
        assert_eq!(app.handle_key(character('q')), None);
        assert!(matches!(app.mode(), Mode::Confirming(_)));
    }

    #[test]
    fn expanding_widget_without_detail_shows_notice() {
        let mut app = app();
        press(&mut app, &[KeyInput::Tab, KeyInput::Enter]);
        assert_eq!(app.active_tab(), 1);
        assert!(app.notice().is_some());
    }

    #[test]
    fn failing_widget_exposes_no_detail_items_or_actions() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter]);
        assert!(!app.detail_items().is_empty());
        assert!(!app.available_actions().is_empty());
        app.update_widget(
            &WidgetId::new("kanban"),
            Err(SourceError::Failed("fora do ar".to_string())),
            now(),
        );
        assert!(app.detail_items().is_empty());
        assert!(app.available_actions().is_empty());
    }

    #[test]
    fn begin_widget_refresh_closes_the_action_menu() {
        let mut app = app();
        press(&mut app, &[character('a')]);
        assert!(matches!(app.mode(), Mode::ChoosingAction { .. }));
        app.begin_widget_refresh(&WidgetId::new("kanban"));
        assert_eq!(app.mode(), &Mode::Browsing);
    }

    #[test]
    fn update_widget_clamps_selection_and_closes_action_menu() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter, KeyInput::Down, character('a')]);
        let emptied = WidgetSnapshot {
            mini: None,
            alerts: vec![],
            summary: Summary {
                blocks: vec![],
                title: "Kanban".to_string(),
                lines: vec![],
            },
            detail: None,
            actions: vec![],
        };
        app.update_widget(&WidgetId::new("kanban"), Ok(emptied), now());
        assert_eq!(app.mode(), &Mode::Browsing);
        assert_eq!(app.selected_item(), 0);
    }

    #[test]
    fn notice_is_cleared_by_next_key() {
        let mut app = app();
        app.show_notice("movido".to_string());
        assert_eq!(app.notice(), Some("movido"));
        press(&mut app, &[KeyInput::Down]);
        assert_eq!(app.notice(), None);
    }

    #[test]
    fn p_toggles_the_pomodoro() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        assert_eq!(
            app.pomodoro().map(PomodoroTimer::status),
            Some(PomodoroStatus::Running)
        );
        press(&mut app, &[character('p')]);
        assert_eq!(
            app.pomodoro().map(PomodoroTimer::status),
            Some(PomodoroStatus::Paused)
        );
    }

    #[test]
    fn p_without_pomodoro_shows_notice() {
        let mut app = app();
        press(&mut app, &[character('p')]);
        assert!(app.notice().expect("aviso").contains("Pomodoro desligado"));
    }

    #[test]
    fn advancing_past_a_phase_end_returns_alert_and_stores_it() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        assert_eq!(app.advance_pomodoro(Duration::from_secs(60)), None);
        let alert = app
            .advance_pomodoro(Duration::from_secs(25 * 60))
            .expect("aviso de fim de foco");
        assert_eq!(alert.title, "Foco concluído");
        assert_eq!(app.alert(), Some(&alert));
    }

    #[test]
    fn show_alert_queues_messages_in_order() {
        let mut app = app_with_pomodoro();
        app.show_alert(message("um"));
        app.show_alert(message("dois"));
        assert_eq!(app.pending_alert_count(), 2);
        assert_eq!(app.alert().map(|alert| alert.title.as_str()), Some("um"));
    }

    #[test]
    fn any_key_dismisses_only_the_front_alert() {
        let mut app = app_with_pomodoro();
        app.show_alert(message("um"));
        app.show_alert(message("dois"));
        assert_eq!(app.handle_key(character('q')), None);
        assert_eq!(app.alert().map(|alert| alert.title.as_str()), Some("dois"));
        assert_eq!(app.handle_key(character('q')), None);
        assert!(app.alert().is_none());
        assert_eq!(app.handle_key(character('q')), Some(Command::Quit));
    }

    #[test]
    fn queue_is_bounded_to_twenty() {
        let mut app = app_with_pomodoro();
        for number in 0..30 {
            app.show_alert(message(&number.to_string()));
        }
        assert_eq!(app.pending_alert_count(), MAXIMUM_QUEUED_ALERTS);
        assert_eq!(app.alert().map(|alert| alert.title.as_str()), Some("0"));
    }

    #[test]
    fn pomodoro_alert_and_widget_alerts_share_the_queue() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        app.show_alert(message("do widget"));
        app.advance_pomodoro(Duration::from_secs(25 * 60));
        assert_eq!(app.pending_alert_count(), 2);
        assert_eq!(
            app.alert().map(|alert| alert.title.as_str()),
            Some("do widget")
        );
    }

    #[test]
    fn any_key_dismisses_alert_and_is_consumed() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        app.advance_pomodoro(Duration::from_secs(25 * 60));
        assert_eq!(app.handle_key(character('q')), None);
        assert!(app.alert().is_none());
        assert_eq!(app.handle_key(character('q')), Some(Command::Quit));
    }

    #[test]
    fn alert_does_not_change_the_current_mode() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        press(
            &mut app,
            &[
                KeyInput::Enter,
                KeyInput::Down,
                character('a'),
                KeyInput::Enter,
            ],
        );
        app.advance_pomodoro(Duration::from_secs(25 * 60));
        assert!(app.alert().is_some());
        assert!(matches!(app.mode(), Mode::Confirming(_)));
        app.handle_key(KeyInput::Escape);
        assert!(matches!(app.mode(), Mode::Confirming(_)));
    }

    #[test]
    fn i_emits_request_recommendation_only_when_browsing() {
        let mut app = app();
        assert_eq!(
            app.handle_key(character('i')),
            Some(Command::RequestRecommendation)
        );
        let mut confirming = app_confirming_move_of_second_item();
        assert_eq!(confirming.handle_key(character('i')), None);
    }

    #[test]
    fn send_confirmation_s_emits_send_with_the_plan() {
        let mut app = app();
        app.ask_send_confirmation("Enviar 3 itens?".to_string(), plan());
        assert!(matches!(app.mode(), Mode::ConfirmingSend { .. }));
        assert_eq!(
            app.handle_key(character('s')),
            Some(Command::SendRecommendation(plan()))
        );
        assert_eq!(app.mode(), &Mode::Browsing);
    }

    #[test]
    fn send_confirmation_cancel_returns_to_browsing() {
        for cancel in [character('n'), KeyInput::Escape] {
            let mut app = app();
            app.ask_send_confirmation("Enviar?".to_string(), plan());
            assert_eq!(app.handle_key(cancel), None);
            assert_eq!(app.mode(), &Mode::Browsing);
        }
    }

    #[test]
    fn quit_is_ignored_while_confirming_send() {
        let mut app = app();
        app.ask_send_confirmation("Enviar?".to_string(), plan());
        assert_eq!(app.handle_key(character('q')), None);
        assert!(matches!(app.mode(), Mode::ConfirmingSend { .. }));
    }

    #[test]
    fn i_is_blocked_with_notice_while_the_ia_is_loading() {
        let mut app = AppState::new(
            vec![entry("ia", WidgetRecord::loading())],
            vec![tab("IA", &[0])],
        );
        app.begin_widget_refresh(&WidgetId::new("ia"));
        assert_eq!(app.handle_key(character('i')), None);
        assert!(app.notice().expect("aviso").contains("processando"));
        app.update_widget(
            &WidgetId::new("ia"),
            Err(SourceError::Failed("x".to_string())),
            now(),
        );
        assert_eq!(
            app.handle_key(character('i')),
            Some(Command::RequestRecommendation)
        );
    }
}
