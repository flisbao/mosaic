use chrono::NaiveDateTime;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::application::pomodoro_notifications::AlertMessage;
use crate::domain::layout::{arrange, CellRect, GridLayout, TerminalSize, TOP_BAR_HEIGHT};
use crate::domain::pomodoro::{Phase, PomodoroStatus, PomodoroTimer};
use crate::presentation::app_state::{AppState, Mode, WidgetEntry};
use crate::presentation::text_fit::{flagged, truncate_to_width};
use crate::presentation::widget_view::render_blocks;

const HOME_TAB_NAME: &str = "Home";
const ENLARGE_WARNING: &str = "Amplie o terminal (mínimo 60x20)";
const LOADING_TEXT: &str = "carregando…";
const MODAL_MAXIMUM_WIDTH: u16 = 60;
const MODAL_FRAME_THICKNESS: u16 = 2;
const MINIMUM_DRAWABLE_SIDE: u16 = 3;
const SECONDS_PER_MINUTE: u64 = 60;
const ALERT_DISMISS_HINT: &str = "Qualquer tecla fecha";
const WRAP_CONTINUATION_INDENT: &str = "  ";
const SELECTION_MARKER: &str = "> ";
const NO_SELECTION_MARKER: &str = "  ";

pub fn render(frame: &mut Frame, state: &AppState, spans: &[u16], now: NaiveDateTime) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    render_top_bar(frame, state, now, area);
    if state.active_tab() == 0 {
        render_home(frame, state, spans, area);
    } else {
        render_detail(frame, state, area);
    }
    render_notice(frame, state, area);
    render_modal(frame, state, area);
    render_alert(frame, state, area);
}

fn render_top_bar(frame: &mut Frame, state: &AppState, now: NaiveDateTime, area: Rect) {
    let bar = Rect::new(area.x, area.y, area.width, TOP_BAR_HEIGHT.min(area.height));
    let mut spans = vec![Span::styled(
        format!(" {}  {} ", now.format("%H:%M"), now.format("%d/%m/%Y")),
        Style::default().fg(Color::Cyan),
    )];
    let mut names = vec![HOME_TAB_NAME.to_string()];
    names.extend(
        state
            .tab_entry_indexes()
            .iter()
            .map(|&index| entry_title(&state.entries()[index])),
    );
    for (position, name) in names.iter().enumerate() {
        let style = if position == state.active_tab() {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(format!(" {name} "), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), bar);
    if let Some(timer) = state.pomodoro() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                pomodoro_text(timer),
                Style::default().fg(Color::Green),
            ))
            .alignment(Alignment::Right),
            bar,
        );
    }
}

fn pomodoro_text(timer: &PomodoroTimer) -> String {
    let seconds = timer.remaining().as_secs();
    let phase = match timer.phase() {
        Phase::Focus => "foco",
        Phase::ShortBreak => "descanso curto",
        Phase::LongBreak => "descanso longo",
    };
    let suffix = match timer.status() {
        PomodoroStatus::Running => "",
        PomodoroStatus::Paused => " (pausado)",
        PomodoroStatus::Stopped => " (parado)",
    };
    format!(
        "🍅 {:02}:{:02} {phase}{suffix} ",
        seconds / SECONDS_PER_MINUTE,
        seconds % SECONDS_PER_MINUTE
    )
}

fn render_home(frame: &mut Frame, state: &AppState, spans: &[u16], area: Rect) {
    let size = TerminalSize {
        width: area.width,
        height: area.height,
    };
    match arrange(spans, size) {
        GridLayout::Grid(placements) => {
            for placement in placements {
                let selected = placement.widget_index == state.selected_item();
                let entry = &state.entries()[placement.widget_index];
                render_box(frame, entry, to_rect(area, placement.area), selected, None);
            }
        }
        GridLayout::Compact(placement) => {
            if let Some(entry) = state.entries().first() {
                let warning = Some(ENLARGE_WARNING);
                render_box(frame, entry, to_rect(area, placement.area), true, warning);
            }
        }
        GridLayout::Empty => {}
    }
}

fn render_box(
    frame: &mut Frame,
    entry: &WidgetEntry,
    area: Rect,
    selected: bool,
    warning: Option<&str>,
) {
    if area.width < MINIMUM_DRAWABLE_SIDE || area.height < MINIMUM_DRAWABLE_SIDE {
        return;
    }
    let border = if selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let block = Block::bordered()
        .title(truncate_to_width(
            &entry_title(entry),
            area.width as usize - 2,
        ))
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);
    let width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(warning) = warning {
        lines.push(Line::styled(
            truncate_to_width(warning, width),
            Style::default().fg(Color::Yellow),
        ));
    }
    lines.extend(box_lines(entry, width));
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn box_lines(entry: &WidgetEntry, width: usize) -> Vec<Line<'static>> {
    let record = &entry.record;
    if record.is_refreshing() {
        return vec![Line::from(LOADING_TEXT)];
    }
    match (record.snapshot(), record.last_error()) {
        (Some(snapshot), Some(message)) => {
            let mut lines = vec![failure_line(&format!("⚠ falha: {message}"), width)];
            lines.extend(render_blocks(&snapshot.summary.display_blocks(), width));
            lines
        }
        (Some(snapshot), None) => render_blocks(&snapshot.summary.display_blocks(), width),
        (None, Some(message)) => message
            .lines()
            .map(|line| failure_line(line, width))
            .collect(),
        (None, None) => vec![Line::from(LOADING_TEXT)],
    }
}

fn failure_line(text: &str, width: usize) -> Line<'static> {
    Line::styled(
        truncate_to_width(text, width),
        Style::default().fg(Color::Red),
    )
}

fn render_detail(frame: &mut Frame, state: &AppState, area: Rect) {
    let content = Rect::new(
        area.x,
        area.y + TOP_BAR_HEIGHT.min(area.height),
        area.width,
        area.height.saturating_sub(TOP_BAR_HEIGHT),
    );
    if content.width < MINIMUM_DRAWABLE_SIDE || content.height < MINIMUM_DRAWABLE_SIDE {
        return;
    }
    let Some(&entry_index) = state.tab_entry_indexes().get(state.active_tab() - 1) else {
        return;
    };
    let entry = &state.entries()[entry_index];
    let block = Block::bordered().title(truncate_to_width(
        &entry_title(entry),
        content.width as usize - 2,
    ));
    let inner = block.inner(content);
    let width = inner.width as usize;
    let (lines, selected_row) = detail_lines(entry, state.selected_item(), width);
    let scroll = selected_row.saturating_sub(inner.height.saturating_sub(1) as usize);
    frame.render_widget(block, content);
    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);
}

fn detail_lines(entry: &WidgetEntry, selected: usize, width: usize) -> (Vec<Line<'static>>, usize) {
    let Some(snapshot) = entry.record.snapshot() else {
        return (box_lines(entry, width), 0);
    };
    let Some(sections) = &snapshot.detail else {
        return (Vec::new(), 0);
    };
    let mut lines = Vec::new();
    let mut item_index = 0;
    let mut selected_row = 0;
    for section in sections {
        lines.push(Line::styled(
            truncate_to_width(&section.heading, width),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        lines.extend(render_blocks(&section.display_blocks(), width));
        for item in &section.items {
            let is_selected = item_index == selected;
            if is_selected {
                selected_row = lines.len();
            }
            let marker = if is_selected {
                SELECTION_MARKER
            } else {
                NO_SELECTION_MARKER
            };
            let style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            let wrap_width = width.saturating_sub(WRAP_CONTINUATION_INDENT.width());
            for (position, piece) in wrap_to_width(&flagged(item.flag, &item.text), wrap_width)
                .into_iter()
                .enumerate()
            {
                let prefix = if position == 0 {
                    marker
                } else {
                    WRAP_CONTINUATION_INDENT
                };
                lines.push(Line::styled(format!("{prefix}{piece}"), style));
            }
            item_index += 1;
        }
    }
    (lines, selected_row)
}

fn render_notice(frame: &mut Frame, state: &AppState, area: Rect) {
    let Some(notice) = state.notice() else {
        return;
    };
    if area.height < 2 {
        return;
    }
    let row = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    frame.render_widget(Clear, row);
    frame.render_widget(
        Paragraph::new(Line::styled(
            truncate_to_width(notice, area.width as usize),
            Style::default().fg(Color::Yellow),
        )),
        row,
    );
}

fn render_modal(frame: &mut Frame, state: &AppState, area: Rect) {
    let (title, lines) = match state.mode() {
        Mode::Browsing => return,
        Mode::ConfirmingSend { text, .. } => (
            "Enviar à IA",
            vec![
                text.clone(),
                String::new(),
                "[s] confirma  [n] cancela".to_string(),
            ],
        ),
        Mode::ChoosingAction { selected } => ("Ações", action_menu_lines(state, *selected)),
        Mode::Confirming(proposed) => {
            let mut lines = vec![proposed.describe()];
            if let Some(item_id) = proposed.item_id() {
                lines.push(format!("Item: {item_id}"));
            }
            lines.push(String::new());
            lines.push("[s] confirma  [n] cancela".to_string());
            ("Confirmar ação", lines)
        }
    };
    draw_popup(frame, area, title, &lines);
}

fn render_alert(frame: &mut Frame, state: &AppState, area: Rect) {
    let Some(AlertMessage { title, body }) = state.alert() else {
        return;
    };
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    lines.push(String::new());
    lines.push(ALERT_DISMISS_HINT.to_string());
    draw_popup(frame, area, title, &lines);
}

fn draw_popup(frame: &mut Frame, area: Rect, title: &str, lines: &[String]) {
    let width = area.width.min(MODAL_MAXIMUM_WIDTH);
    let height = (lines.len() as u16 + MODAL_FRAME_THICKNESS).min(area.height);
    if width < MINIMUM_DRAWABLE_SIDE || height < MINIMUM_DRAWABLE_SIDE {
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let block = Block::bordered()
        .title(title.to_string())
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(popup);
    let text: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(truncate_to_width(line, inner.width as usize)))
        .collect();
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(text), inner);
}

fn action_menu_lines(state: &AppState, selected: usize) -> Vec<String> {
    state
        .available_actions()
        .iter()
        .enumerate()
        .map(|(position, action)| {
            let marker = if position == selected {
                SELECTION_MARKER
            } else {
                NO_SELECTION_MARKER
            };
            format!("{marker}{}", action.label)
        })
        .collect()
}

fn entry_title(entry: &WidgetEntry) -> String {
    match entry.record.snapshot() {
        Some(snapshot) => snapshot.summary.title.clone(),
        None => entry.id.as_str().to_string(),
    }
}

fn to_rect(origin: Rect, cell: CellRect) -> Rect {
    Rect::new(
        origin.x + cell.x,
        origin.y + cell.y,
        cell.width,
        cell.height,
    )
}

fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(character);
        used += character_width;
    }
    lines.push(current);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget::{
        ActionScope, ActionSpec, DetailItem, DetailSection, Flag, Summary, SummaryLine, WidgetId,
        WidgetRecord, WidgetSnapshot,
    };
    use crate::presentation::app_state::{AppState, WidgetEntry};
    use crate::presentation::key_input::KeyInput;
    use chrono::NaiveDate;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 17)
            .unwrap()
            .and_hms_opt(9, 41, 0)
            .unwrap()
    }

    fn snapshot(title: &str, lines: Vec<SummaryLine>) -> WidgetSnapshot {
        WidgetSnapshot {
            mini: None,
            alerts: vec![],
            summary: Summary {
                blocks: vec![],
                title: title.to_string(),
                lines,
            },
            detail: None,
            actions: vec![],
        }
    }

    fn plain_line(text: &str) -> SummaryLine {
        SummaryLine {
            text: text.to_string(),
            flag: None,
        }
    }

    fn ready(snapshot: WidgetSnapshot) -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(Ok(snapshot), now());
        record
    }

    fn failed(message: &str) -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(Err(message.to_string()), now());
        record
    }

    fn entry(id: &str, record: WidgetRecord, has_tab: bool) -> WidgetEntry {
        WidgetEntry {
            id: WidgetId::new(id),
            record,
            has_tab,
        }
    }

    fn kanban_snapshot() -> WidgetSnapshot {
        let mut kanban = snapshot("Kanban", vec![plain_line("inbox 3")]);
        kanban.detail = Some(vec![DetailSection {
            blocks: vec![],
            heading: "inbox".to_string(),
            items: vec![
                DetailItem {
                    id: Some("inbox/a.md".to_string()),
                    text: "Tarefa A".to_string(),
                    flag: Some(Flag::Warning),
                    meta: vec![],
                },
                DetailItem {
                    id: Some("inbox/b.md".to_string()),
                    text: "Tarefa B".to_string(),
                    flag: None,
                    meta: vec![],
                },
            ],
        }]);
        kanban.actions = vec![ActionSpec {
            id: "move".to_string(),
            label: "Mover para done".to_string(),
            description: Some("Mover o cartão selecionado para done".to_string()),
            scope: ActionScope::Item,
            params_json: "{}".to_string(),
        }];
        kanban
    }

    fn kanban_entry() -> WidgetEntry {
        entry("kanban", ready(kanban_snapshot()), true)
    }

    fn draw(state: &AppState, width: u16, height: u16) -> Vec<String> {
        let spans = vec![1; state.entries().len()];
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, state, &spans, now()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|row| {
                (0..width)
                    .map(|column| buffer[(column, row)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn screen(state: &AppState, width: u16, height: u16) -> String {
        draw(state, width, height).join("\n")
    }

    fn press(state: &mut AppState, keys: &[KeyInput]) {
        for key in keys {
            state.handle_key(*key);
        }
    }

    #[test]
    fn top_bar_shows_clock_and_date() {
        let state = AppState::new(vec![kanban_entry()]);
        let first_row = draw(&state, 100, 30).remove(0);
        assert!(first_row.contains("09:41"), "{first_row}");
        assert!(first_row.contains("17/10/2026"), "{first_row}");
    }

    #[test]
    fn home_draws_one_box_per_widget_title() {
        let state = AppState::new(vec![
            kanban_entry(),
            entry("agenda", ready(snapshot("Agenda", vec![])), false),
            entry("time", ready(snapshot("Time", vec![])), false),
        ]);
        let text = screen(&state, 100, 31);
        for title in ["Kanban", "Agenda", "Time"] {
            assert!(text.contains(title), "{title}");
        }
    }

    #[test]
    fn failure_after_success_shows_error_line_above_old_snapshot() {
        let mut record = ready(snapshot("Kanban", vec![plain_line("inbox 3")]));
        record.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let state = AppState::new(vec![entry("kanban", record, false)]);
        let rows = draw(&state, 100, 30);
        let failure = rows
            .iter()
            .position(|row| row.contains("⚠ falha: fora do ar"));
        let old_data = rows.iter().position(|row| row.contains("inbox 3"));
        assert!(failure.is_some(), "{rows:?}");
        assert!(old_data.is_some(), "{rows:?}");
        assert!(failure < old_data, "{rows:?}");
    }

    #[test]
    fn ia_failure_after_idle_snapshot_is_shown() {
        let mut record = ready(snapshot("IA", vec![plain_line("Sem recomendação ainda")]));
        record.apply(
            Err::<WidgetSnapshot, _>("modelo indisponível".to_string()),
            now(),
        );
        let state = AppState::new(vec![entry("ia", record, true)]);
        assert!(screen(&state, 100, 30).contains("falha: modelo indisponível"));
    }

    #[test]
    fn failed_widget_shows_error_inside_its_box() {
        let state = AppState::new(vec![entry("kanban", failed("plugin quebrou"), false)]);
        assert!(screen(&state, 100, 30).contains("plugin quebrou"));
    }

    #[test]
    fn flag_symbol_is_rendered_before_line_text() {
        let line = SummaryLine {
            text: "inbox 3".to_string(),
            flag: Some(Flag::Updated),
        };
        let state = AppState::new(vec![entry(
            "kanban",
            ready(snapshot("Kanban", vec![line])),
            false,
        )]);
        assert!(screen(&state, 100, 30).contains("● inbox 3"));
    }

    #[test]
    fn confirmation_dialog_shows_description_and_keys() {
        let mut state = AppState::new(vec![kanban_entry()]);
        press(
            &mut state,
            &[
                KeyInput::Tab,
                KeyInput::Down,
                KeyInput::Character('a'),
                KeyInput::Enter,
            ],
        );
        let text = screen(&state, 100, 30);
        assert!(
            text.contains("Mover o cartão selecionado para done"),
            "{text}"
        );
        assert!(text.contains("inbox/b.md"));
        assert!(text.contains("[s] confirma"));
        assert!(text.contains("[n] cancela"));
    }

    #[test]
    fn action_menu_lists_action_labels() {
        let mut state = AppState::new(vec![kanban_entry()]);
        press(&mut state, &[KeyInput::Tab, KeyInput::Character('a')]);
        assert!(screen(&state, 100, 30).contains("Mover para done"));
    }

    #[test]
    fn compact_mode_shows_enlarge_warning() {
        let state = AppState::new(vec![kanban_entry()]);
        assert!(screen(&state, 50, 15).contains("Amplie"));
    }

    #[test]
    fn zero_sized_area_does_not_panic() {
        let mut state = AppState::new(vec![kanban_entry()]);
        for width in 0..=14u16 {
            for height in 0..=14u16 {
                draw(&state, width, height);
            }
        }
        press(
            &mut state,
            &[KeyInput::Tab, KeyInput::Character('a'), KeyInput::Enter],
        );
        for (width, height) in [(0, 0), (1, 1), (3, 3), (10, 4), (60, 20)] {
            draw(&state, width, height);
        }
    }

    #[test]
    fn wide_characters_are_truncated_with_ellipsis_inside_box() {
        let state = AppState::new(vec![entry(
            "kanban",
            ready(snapshot("Kanban", vec![plain_line(&"日本語".repeat(15))])),
            false,
        )]);
        let rows = draw(&state, 60, 20);
        let row = rows
            .iter()
            .find(|row| row.contains('…'))
            .expect("sem reticências");
        assert!(row.trim_end().ends_with('│'), "{row}");
    }

    #[test]
    fn detail_tab_lists_sections_and_items() {
        let mut state = AppState::new(vec![kanban_entry()]);
        press(&mut state, &[KeyInput::Tab, KeyInput::Down]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("inbox"));
        assert!(text.contains("  ⚠ Tarefa A"), "{text}");
        assert!(text.contains("> Tarefa B"), "{text}");
    }

    #[test]
    fn notice_is_rendered_on_the_last_row() {
        let mut state = AppState::new(vec![kanban_entry()]);
        state.show_notice("Cartão movido".to_string());
        let rows = draw(&state, 100, 30);
        assert!(rows[29].contains("Cartão movido"));
    }
    fn pomodoro_timer() -> crate::domain::pomodoro::PomodoroTimer {
        crate::domain::pomodoro::PomodoroTimer::new(crate::domain::pomodoro::PomodoroSettings {
            focus: std::time::Duration::from_secs(25 * 60),
            short_break: std::time::Duration::from_secs(5 * 60),
            long_break: std::time::Duration::from_secs(15 * 60),
            cycles_before_long_break: 4,
            suggestions: vec!["Beber água".to_string()],
        })
    }

    fn app_with_pomodoro() -> AppState {
        AppState::new(vec![kanban_entry()]).with_pomodoro(pomodoro_timer())
    }

    #[test]
    fn top_bar_shows_running_pomodoro_remaining_time() {
        let mut state = app_with_pomodoro();
        press(&mut state, &[KeyInput::Character('p')]);
        state.advance_pomodoro(std::time::Duration::from_secs(7 * 60));
        let first_row = draw(&state, 100, 30).remove(0);
        assert!(first_row.contains("18:00"), "{first_row}");
        assert!(first_row.contains("foco"), "{first_row}");
        assert!(!first_row.contains("pausado"));
    }

    #[test]
    fn top_bar_marks_paused_and_stopped_pomodoro() {
        let mut state = app_with_pomodoro();
        let stopped = draw(&state, 100, 30).remove(0);
        assert!(
            stopped.contains("25:00") && stopped.contains("(parado)"),
            "{stopped}"
        );
        press(
            &mut state,
            &[KeyInput::Character('p'), KeyInput::Character('p')],
        );
        let paused = draw(&state, 100, 30).remove(0);
        assert!(paused.contains("(pausado)"), "{paused}");
    }

    #[test]
    fn top_bar_without_pomodoro_has_no_timer_text() {
        let state = AppState::new(vec![kanban_entry()]);
        let first_row = draw(&state, 100, 30).remove(0);
        assert!(!first_row.contains("foco") && !first_row.contains("parado"));
    }

    #[test]
    fn alert_box_shows_title_body_and_dismiss_hint() {
        let mut state = app_with_pomodoro();
        press(&mut state, &[KeyInput::Character('p')]);
        state.advance_pomodoro(std::time::Duration::from_secs(25 * 60));
        let text = screen(&state, 100, 30);
        assert!(text.contains("Foco concluído"), "{text}");
        assert!(text.contains("Beber água"));
        assert!(text.contains("Próximo: descanso curto"));
        assert!(text.contains("Qualquer tecla fecha"));
    }

    #[test]
    fn send_confirmation_modal_shows_text_and_keys() {
        let mut state = AppState::new(vec![kanban_entry()]);
        state.ask_send_confirmation(
            "Enviar 3 itens (~40 tokens, estimativa) para claude?".to_string(),
            crate::application::recommendation::RecommendationPlan {
                prompt: "p".to_string(),
                item_count: 3,
                estimated_tokens: 40,
            },
        );
        let text = screen(&state, 100, 30);
        assert!(
            text.contains("Enviar 3 itens (~40 tokens, estimativa) para claude?"),
            "{text}"
        );
        assert!(text.contains("[s] confirma"));
        assert!(text.contains("[n] cancela"));
    }

    #[test]
    fn wrap_to_width_splits_on_width_and_keeps_all_text() {
        let text = "uma frase bem longa para quebrar em varias linhas";
        let lines = wrap_to_width(text, 10);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|line| line.width() <= 10));
        assert_eq!(lines.concat(), text);
    }

    #[test]
    fn wrap_to_width_handles_wide_characters_and_zero_width() {
        let lines = wrap_to_width("日本語日本語", 5);
        assert!(lines.iter().all(|line| line.width() <= 5));
        assert_eq!(lines.concat(), "日本語日本語");
        assert_eq!(wrap_to_width("", 5), vec![String::new()]);
        assert!(wrap_to_width("abc", 0).is_empty());
        assert_eq!(wrap_to_width("日", 1), vec!["日".to_string()]);
    }

    #[test]
    fn long_detail_item_is_wrapped_not_truncated() {
        let sentence = format!("{} FIM-DA-FRASE", "palavra ".repeat(30));
        let mut state = AppState::new(vec![entry(
            "ia",
            ready(WidgetSnapshot {
                mini: None,
                alerts: vec![],
                summary: Summary {
                    blocks: vec![],
                    title: "IA".to_string(),
                    lines: vec![],
                },
                detail: Some(vec![DetailSection {
                    blocks: vec![],
                    heading: "Recomendação".to_string(),
                    items: vec![DetailItem {
                        id: None,
                        text: sentence,
                        flag: None,
                        meta: vec![],
                    }],
                }]),
                actions: vec![],
            }),
            true,
        )]);
        press(&mut state, &[KeyInput::Tab]);
        assert!(screen(&state, 100, 30).contains("FIM-DA-FRASE"));
    }

    #[test]
    fn small_terminal_with_alert_does_not_panic() {
        let mut state = app_with_pomodoro();
        press(&mut state, &[KeyInput::Character('p')]);
        state.advance_pomodoro(std::time::Duration::from_secs(25 * 60));
        for width in 0..=14u16 {
            for height in 0..=14u16 {
                draw(&state, width, height);
            }
        }
        let mut modal = AppState::new(vec![kanban_entry()]);
        modal.ask_send_confirmation(
            "x".repeat(200),
            crate::application::recommendation::RecommendationPlan {
                prompt: String::new(),
                item_count: 0,
                estimated_tokens: 0,
            },
        );
        for width in 0..=14u16 {
            for height in 0..=14u16 {
                draw(&modal, width, height);
            }
        }
    }

    use crate::domain::blocks;
    use crate::domain::widget_content;

    fn widget_line(text: &str) -> blocks::lines::WidgetLine {
        blocks::lines::WidgetLine {
            text: text.to_string(),
            tone: widget_content::Tone::Normal,
            flag: None,
        }
    }

    fn widget_cell(text: &str) -> blocks::table::Cell {
        blocks::table::Cell {
            text: text.to_string(),
            tone: widget_content::Tone::Normal,
            flag: None,
        }
    }

    fn entry_with_blocks(blocks: Vec<blocks::Block>) -> WidgetEntry {
        let mut widget_snapshot = snapshot("Agenda", vec![]);
        widget_snapshot.summary.blocks = blocks;
        entry("agenda", ready(widget_snapshot), true)
    }

    fn rows_and_stat_blocks() -> Vec<blocks::Block> {
        vec![
            blocks::Block::Rows(vec![blocks::rows::Row {
                time: Some("09:00".to_string()),
                text: "Daily".to_string(),
                tags: vec![blocks::rows::Tag {
                    text: "agora".to_string(),
                    tone: widget_content::Tone::Warn,
                }],
                tone: widget_content::Tone::Normal,
                marker: Some('▶'),
                flag: None,
            }]),
            blocks::Block::Stat {
                items: vec![blocks::stat::StatItem {
                    value: "3".to_string(),
                    label: "reuniões".to_string(),
                    tone: widget_content::Tone::Ok,
                }],
                hint: Some("hoje".to_string()),
            },
        ]
    }

    #[test]
    fn home_box_renders_rows_and_stat_blocks() {
        let state = AppState::new(vec![entry_with_blocks(rows_and_stat_blocks())]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("▶ 09:00 Daily"), "{text}");
        assert!(text.contains("[agora]"), "{text}");
        assert!(text.contains("3 reuniões"), "{text}");
        assert!(text.contains("hoje"), "{text}");
    }

    #[test]
    fn home_box_renders_a_table() {
        let table = blocks::Block::Table {
            columns: vec![blocks::table::Column {
                title: "Nome".to_string(),
                align: widget_content::Align::Left,
            }],
            rows: vec![vec![widget_cell("ana")]],
        };
        let state = AppState::new(vec![entry_with_blocks(vec![table])]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("Nome"), "{text}");
        assert!(text.contains("ana"), "{text}");
    }

    #[test]
    fn legacy_lines_still_render_exactly_as_before() {
        let state = AppState::new(vec![entry(
            "kanban",
            ready(snapshot(
                "Kanban",
                vec![
                    plain_line("inbox 3"),
                    SummaryLine {
                        text: "atenção".to_string(),
                        flag: Some(Flag::Warning),
                    },
                ],
            )),
            false,
        )]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("inbox 3"), "{text}");
        assert!(text.contains("⚠ atenção"), "{text}");
    }

    #[test]
    fn detail_section_shows_blocks_before_items() {
        let mut snapshot = kanban_snapshot();
        let sections = snapshot.detail.as_mut().unwrap();
        sections[0].blocks = vec![blocks::Block::Lines(vec![widget_line("RESUMO DA SEÇÃO")])];
        let mut state = AppState::new(vec![entry("kanban", ready(snapshot), true)]);
        press(&mut state, &[KeyInput::Tab]);
        let rows = draw(&state, 100, 30);
        let position = |needle: &str| rows.iter().position(|row| row.contains(needle)).unwrap();
        assert!(position("inbox") < position("RESUMO DA SEÇÃO"));
        assert!(position("RESUMO DA SEÇÃO") < position("Tarefa A"));
    }

    #[test]
    fn selection_ignores_block_content() {
        let mut snapshot = kanban_snapshot();
        snapshot.detail.as_mut().unwrap()[0].blocks = vec![blocks::Block::Lines(vec![
            widget_line("a"),
            widget_line("b"),
        ])];
        let mut state = AppState::new(vec![entry("kanban", ready(snapshot), true)]);
        press(&mut state, &[KeyInput::Tab, KeyInput::Down]);
        assert_eq!(state.selected_item(), 1);
        press(&mut state, &[KeyInput::Down]);
        assert_eq!(state.selected_item(), 1);
    }

    #[test]
    fn small_terminal_with_blocks_does_not_panic() {
        let wide = "日本語".repeat(40);
        let columns = (0..10)
            .map(|_| blocks::table::Column {
                title: wide.clone(),
                align: widget_content::Align::Right,
            })
            .collect();
        let rows = (0..200)
            .map(|_| (0..10).map(|_| widget_cell(&wide)).collect())
            .collect();
        let mut blocks = rows_and_stat_blocks();
        blocks.push(blocks::Block::Table { columns, rows });
        blocks.push(blocks::Block::Lines(vec![widget_line(&wide); 200]));
        let mut state = AppState::new(vec![entry_with_blocks(blocks)]);
        for size in 0..=14 {
            draw(&state, size, size);
        }
        press(&mut state, &[KeyInput::Tab]);
        for size in 0..=14 {
            draw(&state, size, size);
        }
    }
}
