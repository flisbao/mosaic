use std::collections::VecDeque;
use std::time::Duration;

use crate::application::pomodoro_notifications::{alert_for, AlertMessage};
use crate::application::recommendation::RecommendationPlan;
use crate::application::recommendation_view::IA_WIDGET_ID;
use crate::domain::action::{ConfirmedAction, ProposedAction};
use crate::domain::pomodoro::PomodoroTimer;
use crate::domain::widget::{ActionScope, ActionSpec, DetailItem, WidgetId, WidgetState};
use crate::presentation::key_input::KeyInput;

const HOME_TAB: usize = 0;
pub const MAXIMUM_QUEUED_ALERTS: usize = 20;
const NO_ACTIONS_NOTICE: &str = "Nenhuma ação disponível aqui";
const POMODORO_DISABLED_NOTICE: &str = "Pomodoro desligado na configuração";
const IA_BUSY_NOTICE: &str = "A IA já está processando um pedido";
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
    Execute(ConfirmedAction),
    RequestRecommendation,
    SendRecommendation(RecommendationPlan),
}

pub struct AppState {
    entries: Vec<WidgetEntry>,
    tabbed_entries: Vec<usize>,
    active_tab: usize,
    selected_item: usize,
    mode: Mode,
    notice: Option<String>,
    pomodoro: Option<PomodoroTimer>,
    alerts: VecDeque<AlertMessage>,
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
            pomodoro: None,
            alerts: VecDeque::new(),
        }
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
            KeyInput::Tab => self.switch_tab(1),
            KeyInput::BackTab => self.switch_tab(self.tab_count() - 1),
            KeyInput::Down => self.move_selection(1),
            KeyInput::Up => self.move_selection(-1),
            KeyInput::Enter => self.open_selected_widget_tab(),
            KeyInput::Character('a') => self.open_action_menu(),
            KeyInput::Character('r') => return Some(Command::RefreshAll),
            KeyInput::Character('i') if self.ia_is_loading() => {
                self.notice = Some(IA_BUSY_NOTICE.to_string())
            }
            KeyInput::Character('i') => return Some(Command::RequestRecommendation),
            KeyInput::Character('p') => self.toggle_pomodoro(),
            KeyInput::Character('P') => self.reset_pomodoro(),
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
            .any(|entry| entry.id.as_str() == IA_WIDGET_ID && entry.state == WidgetState::Loading)
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
            alerts: vec![],
            summary: Summary {
                blocks: vec![],
                title: "Kanban".to_string(),
                lines: vec![],
            },
            detail: Some(vec![DetailSection {
                blocks: vec![],
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
    fn pomodoro_settings() -> crate::domain::pomodoro::PomodoroSettings {
        crate::domain::pomodoro::PomodoroSettings {
            focus: std::time::Duration::from_secs(25 * 60),
            short_break: std::time::Duration::from_secs(5 * 60),
            long_break: std::time::Duration::from_secs(15 * 60),
            cycles_before_long_break: 4,
            suggestions: vec!["Beber água".to_string()],
        }
    }

    fn app_with_pomodoro() -> AppState {
        app().with_pomodoro(crate::domain::pomodoro::PomodoroTimer::new(
            pomodoro_settings(),
        ))
    }

    fn plan() -> RecommendationPlan {
        RecommendationPlan {
            prompt: "pedido".to_string(),
            item_count: 3,
            estimated_tokens: 40,
        }
    }

    #[test]
    fn p_toggles_the_pomodoro() {
        use crate::domain::pomodoro::PomodoroStatus;
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        assert_eq!(app.pomodoro().unwrap().status(), PomodoroStatus::Running);
        press(&mut app, &[character('p')]);
        assert_eq!(app.pomodoro().unwrap().status(), PomodoroStatus::Paused);
    }

    #[test]
    fn p_without_pomodoro_shows_notice() {
        let mut app = app();
        press(&mut app, &[character('p')]);
        assert!(app.notice().unwrap().contains("Pomodoro desligado"));
    }

    #[test]
    fn capital_p_resets_the_pomodoro() {
        use crate::domain::pomodoro::PomodoroStatus;
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        app.advance_pomodoro(std::time::Duration::from_secs(60));
        press(&mut app, &[character('P')]);
        let timer = app.pomodoro().unwrap();
        assert_eq!(timer.status(), PomodoroStatus::Stopped);
        assert_eq!(timer.remaining(), std::time::Duration::from_secs(25 * 60));
    }

    #[test]
    fn advancing_past_a_phase_end_returns_alert_and_stores_it() {
        let mut app = app_with_pomodoro();
        press(&mut app, &[character('p')]);
        assert_eq!(
            app.advance_pomodoro(std::time::Duration::from_secs(60)),
            None
        );
        let alert = app
            .advance_pomodoro(std::time::Duration::from_secs(25 * 60))
            .expect("aviso de fim de foco");
        assert_eq!(alert.title, "Foco concluído");
        assert_eq!(app.alert(), Some(&alert));
    }

    fn message(title: &str) -> AlertMessage {
        AlertMessage {
            title: title.to_string(),
            body: String::new(),
        }
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
        app.advance_pomodoro(std::time::Duration::from_secs(25 * 60));
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
        app.advance_pomodoro(std::time::Duration::from_secs(25 * 60));
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
                KeyInput::Tab,
                KeyInput::Down,
                character('a'),
                KeyInput::Enter,
            ],
        );
        app.advance_pomodoro(std::time::Duration::from_secs(25 * 60));
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
        let mut app = AppState::new(vec![entry("ia", WidgetState::Loading, true)]);
        assert_eq!(app.handle_key(character('i')), None);
        assert!(app.notice().unwrap().contains("processando"));
        app.update_widget(&WidgetId::new("ia"), WidgetState::Failed("x".to_string()));
        assert_eq!(
            app.handle_key(character('i')),
            Some(Command::RequestRecommendation)
        );
    }
}
