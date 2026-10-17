use crate::domain::action::{ConfirmedAction, ProposedAction};
use crate::domain::widget::{ActionScope, ActionSpec, DetailItem, WidgetId, WidgetState};
use crate::presentation::key_input::KeyInput;

const HOME_TAB: usize = 0;
const NO_ACTIONS_NOTICE: &str = "Nenhuma ação disponível aqui";
const NO_DETAIL_NOTICE: &str = "Este widget ainda não tem detalhe para mostrar";

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetEntry {
    pub id: WidgetId,
    pub state: WidgetState,
    pub has_tab: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Browsing,
    ChoosingAction { selected: usize },
    Confirming(ProposedAction),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Quit,
    RefreshAll,
    Execute(ConfirmedAction),
}

pub struct AppState {
    entries: Vec<WidgetEntry>,
    tabbed_entries: Vec<usize>,
    active_tab: usize,
    selected_item: usize,
    mode: Mode,
    notice: Option<String>,
}

impl AppState {
    pub fn new(entries: Vec<WidgetEntry>) -> Self {
        let tabbed_entries = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.has_tab)
            .map(|(index, _)| index)
            .collect();
        Self {
            entries,
            tabbed_entries,
            active_tab: HOME_TAB,
            selected_item: 0,
            mode: Mode::Browsing,
            notice: None,
        }
    }

    pub fn entries(&self) -> &[WidgetEntry] {
        &self.entries
    }

    pub fn tab_entry_indexes(&self) -> &[usize] {
        &self.tabbed_entries
    }

    pub fn active_tab(&self) -> usize {
        self.active_tab
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

    pub fn update_widget(&mut self, id: &WidgetId, state: WidgetState) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| &entry.id == id) {
            entry.state = state;
        }
        if matches!(self.mode, Mode::ChoosingAction { .. }) {
            self.mode = Mode::Browsing;
        }
        self.selected_item = self.selected_item.min(self.item_count().saturating_sub(1));
    }

    pub fn detail_items(&self) -> Vec<&DetailItem> {
        let Some(index) = self.active_entry_index() else {
            return Vec::new();
        };
        match &self.entries[index].state {
            WidgetState::Ready(snapshot) => snapshot
                .detail
                .iter()
                .flatten()
                .flat_map(|section| section.items.iter())
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn available_actions(&self) -> Vec<ActionSpec> {
        let Some(index) = self.active_entry_index() else {
            return Vec::new();
        };
        let WidgetState::Ready(snapshot) = &self.entries[index].state else {
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
        match std::mem::replace(&mut self.mode, Mode::Browsing) {
            Mode::Browsing => self.handle_browsing_key(key),
            Mode::ChoosingAction { selected } => {
                self.handle_action_menu_key(key, selected);
                None
            }
            Mode::Confirming(proposed) => self.handle_confirmation_key(key, proposed),
        }
    }

    fn handle_browsing_key(&mut self, key: KeyInput) -> Option<Command> {
        match key {
            KeyInput::Tab => self.switch_tab(1),
            KeyInput::BackTab => self.switch_tab(self.tab_count() - 1),
            KeyInput::Down => self.move_selection(1),
            KeyInput::Up => self.move_selection(-1),
            KeyInput::Enter => self.open_selected_widget_tab(),
            KeyInput::Character('a') => self.open_action_menu(),
            KeyInput::Character('r') => return Some(Command::RefreshAll),
            KeyInput::Character('q') => return Some(Command::Quit),
            _ => {}
        }
        None
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
            KeyInput::Character('s') => Some(Command::Execute(proposed.confirm())),
            KeyInput::Escape | KeyInput::Character('n') => None,
            _ => {
                self.mode = Mode::Confirming(proposed);
                None
            }
        }
    }

    fn propose(&mut self, spec: Option<&ActionSpec>) -> Option<ProposedAction> {
        let spec = spec?.clone();
        let index = self.active_entry_index()?;
        let item_id = match spec.scope {
            ActionScope::Item => self.selected_detail_item_id(),
            ActionScope::Widget => None,
        };
        match ProposedAction::new(self.entries[index].id.clone(), spec, item_id) {
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

    fn open_selected_widget_tab(&mut self) {
        if self.active_tab != HOME_TAB {
            return;
        }
        let selected_entry = self.selected_item;
        if let Some(position) = self
            .tabbed_entries
            .iter()
            .position(|&index| index == selected_entry)
        {
            self.go_to_tab(position + 1);
        }
    }

    fn tab_count(&self) -> usize {
        1 + self.tabbed_entries.len()
    }

    fn switch_tab(&mut self, step: usize) {
        self.go_to_tab((self.active_tab + step) % self.tab_count());
    }

    fn go_to_tab(&mut self, tab: usize) {
        self.active_tab = tab;
        self.selected_item = 0;
        if tab != HOME_TAB && !self.has_detail() {
            self.notice = Some(NO_DETAIL_NOTICE.to_string());
        }
    }

    fn has_detail(&self) -> bool {
        match self
            .active_entry_index()
            .map(|index| &self.entries[index].state)
        {
            Some(WidgetState::Ready(snapshot)) => snapshot.detail.is_some(),
            _ => false,
        }
    }

    fn move_selection(&mut self, direction: i32) {
        let last = self.item_count().saturating_sub(1);
        self.selected_item = if direction > 0 {
            (self.selected_item + 1).min(last)
        } else {
            self.selected_item.saturating_sub(1)
        };
    }

    fn item_count(&self) -> usize {
        if self.active_tab == HOME_TAB {
            self.entries.len()
        } else {
            self.detail_items().len()
        }
    }

    fn active_entry_index(&self) -> Option<usize> {
        if self.active_tab == HOME_TAB {
            (self.selected_item < self.entries.len()).then_some(self.selected_item)
        } else {
            self.tabbed_entries.get(self.active_tab - 1).copied()
        }
    }

    fn selected_detail_item_id(&self) -> Option<String> {
        if self.active_tab == HOME_TAB {
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
    use crate::domain::widget::{
        ActionScope, ActionSpec, DetailItem, DetailSection, Summary, WidgetSnapshot,
    };

    fn action(id: &str, scope: ActionScope, description: &str) -> ActionSpec {
        ActionSpec {
            id: id.to_string(),
            label: id.to_string(),
            description: Some(description.to_string()),
            scope,
            params_json: r#"{"to":"done"}"#.to_string(),
        }
    }

    fn kanban_state() -> WidgetState {
        let item = |id: &str| DetailItem {
            id: Some(id.to_string()),
            text: id.to_string(),
            flag: None,
            meta: vec![],
        };
        WidgetState::Ready(WidgetSnapshot {
            summary: Summary {
                title: "Kanban".to_string(),
                lines: vec![],
            },
            detail: Some(vec![DetailSection {
                heading: "inbox".to_string(),
                items: vec![item("inbox/a.md"), item("inbox/b.md")],
            }]),
            actions: vec![
                action("move", ActionScope::Item, "Mover para done"),
                action("refresh", ActionScope::Widget, "Atualizar kanban"),
            ],
        })
    }

    fn entry(id: &str, state: WidgetState, has_tab: bool) -> WidgetEntry {
        WidgetEntry {
            id: WidgetId::new(id),
            state,
            has_tab,
        }
    }

    fn app() -> AppState {
        AppState::new(vec![
            entry("kanban", kanban_state(), true),
            entry("plain", WidgetState::Loading, false),
            entry("notes", WidgetState::Loading, true),
        ])
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

    fn app_confirming_move_of_second_item() -> AppState {
        let mut app = app();
        press(
            &mut app,
            &[
                KeyInput::Tab,
                KeyInput::Down,
                character('a'),
                KeyInput::Enter,
            ],
        );
        app
    }

    #[test]
    fn tab_cycles_through_home_and_tabbed_widgets() {
        let mut app = app();
        assert_eq!(app.active_tab(), 0);
        press(&mut app, &[KeyInput::Tab]);
        assert_eq!(app.active_tab(), 1);
        press(&mut app, &[KeyInput::Tab]);
        assert_eq!(app.active_tab(), 2);
        press(&mut app, &[KeyInput::Tab]);
        assert_eq!(app.active_tab(), 0);
        press(&mut app, &[KeyInput::BackTab]);
        assert_eq!(app.active_tab(), 2);
    }

    #[test]
    fn selection_is_clamped_to_item_count() {
        let mut app = app();
        press(&mut app, &[KeyInput::Down; 10]);
        assert_eq!(app.selected_item(), 2);
        press(&mut app, &[KeyInput::Tab]);
        press(&mut app, &[KeyInput::Down; 10]);
        assert_eq!(app.selected_item(), 1);
        press(&mut app, &[KeyInput::Up; 10]);
        assert_eq!(app.selected_item(), 0);
    }

    #[test]
    fn enter_on_home_opens_selected_widgets_tab() {
        let mut app = app();
        press(&mut app, &[KeyInput::Enter]);
        assert_eq!(app.active_tab(), 1);
        let mut app = AppState::new(vec![entry("plain", WidgetState::Loading, false)]);
        press(&mut app, &[KeyInput::Enter]);
        assert_eq!(app.active_tab(), 0);
    }

    #[test]
    fn action_key_lists_actions_for_selected_item() {
        let mut app = app();
        press(&mut app, &[KeyInput::Tab, KeyInput::Down, character('a')]);
        assert_eq!(app.mode(), &Mode::ChoosingAction { selected: 0 });
        let ids: Vec<String> = app.available_actions().into_iter().map(|a| a.id).collect();
        assert_eq!(ids, vec!["move".to_string(), "refresh".to_string()]);
    }

    #[test]
    fn home_lists_only_widget_scoped_actions() {
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
        press(&mut app, &[KeyInput::Tab, character('a')]);
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
    fn tab_for_widget_without_detail_shows_empty_notice() {
        let mut app = app();
        press(&mut app, &[KeyInput::Tab, KeyInput::Tab]);
        assert_eq!(app.active_tab(), 2);
        assert!(app.notice().is_some());
    }

    #[test]
    fn update_widget_clamps_selection_and_closes_action_menu() {
        let mut app = app();
        press(&mut app, &[KeyInput::Tab, KeyInput::Down, character('a')]);
        app.update_widget(&WidgetId::new("kanban"), WidgetState::Loading);
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
}
