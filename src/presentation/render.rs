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
use crate::presentation::focus_column::focus_column_lines;
use crate::presentation::text_fit::{flagged, truncate_to_width};
use crate::presentation::widget_box::{
    box_body_lines, entry_title, render_widget_box, BoxContext, BORDER_COLUMNS,
};
use crate::presentation::widget_view::render_blocks;

const FALLBACK_SPAN: u16 = 1;
const ENLARGE_WARNING: &str = "Amplie o terminal (mínimo 60x20)";
const FOCUS_COLUMN_WIDTH: u16 = 22;
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
    if state.is_expanded() {
        render_focus_mode(frame, state, now, area);
    } else {
        render_home(frame, state, spans, now, area);
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
    for (position, tab) in state.tabs().iter().enumerate() {
        let name = tab.name.as_str();
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

fn render_home(frame: &mut Frame, state: &AppState, spans: &[u16], now: NaiveDateTime, area: Rect) {
    let size = TerminalSize {
        width: area.width,
        height: area.height,
    };
    let entry_indexes = state.active_entry_indexes();
    let tab_spans: Vec<u16> = entry_indexes
        .iter()
        .map(|&index| spans.get(index).copied().unwrap_or(FALLBACK_SPAN))
        .collect();
    match arrange(&tab_spans, size) {
        GridLayout::Grid(placements) => {
            for placement in placements {
                let Some(&entry_index) = entry_indexes.get(placement.widget_index) else {
                    continue;
                };
                let context = BoxContext {
                    shortcut: state.shortcut_of(entry_index),
                    selected: Some(entry_index) == state.focused_entry(),
                    now,
                    warning: None,
                };
                if let Some(entry) = state.entries().get(entry_index) {
                    render_widget_box(frame, entry, to_rect(area, placement.area), &context);
                }
            }
        }
        GridLayout::Compact(placement) => {
            let first = entry_indexes.first().copied();
            if let Some(entry) = first.and_then(|index| state.entries().get(index)) {
                let context = BoxContext {
                    shortcut: first.and_then(|index| state.shortcut_of(index)),
                    selected: true,
                    now,
                    warning: Some(ENLARGE_WARNING),
                };
                render_widget_box(frame, entry, to_rect(area, placement.area), &context);
            }
        }
        GridLayout::Empty => {}
    }
}

fn render_focus_mode(frame: &mut Frame, state: &AppState, now: NaiveDateTime, area: Rect) {
    let content = content_area(area);
    if content.width < FOCUS_COLUMN_WIDTH.saturating_add(MINIMUM_DRAWABLE_SIDE) {
        render_detail(frame, state, now, content);
        return;
    }
    let column = Rect::new(content.x, content.y, FOCUS_COLUMN_WIDTH, content.height);
    let detail = Rect::new(
        content.x.saturating_add(FOCUS_COLUMN_WIDTH),
        content.y,
        content.width.saturating_sub(FOCUS_COLUMN_WIDTH),
        content.height,
    );
    render_focus_column(frame, state, column);
    render_detail(frame, state, now, detail);
}

fn render_focus_column(frame: &mut Frame, state: &AppState, area: Rect) {
    let entries: Vec<&WidgetEntry> = state
        .active_entry_indexes()
        .iter()
        .filter_map(|&index| state.entries().get(index))
        .collect();
    let focused = state
        .active_entry_indexes()
        .iter()
        .position(|&index| Some(index) == state.focused_entry());
    let lines = focus_column_lines(&entries, focused, usize::from(area.width));
    frame.render_widget(Paragraph::new(lines), area);
}

fn content_area(area: Rect) -> Rect {
    let top = TOP_BAR_HEIGHT.min(area.height);
    Rect::new(
        area.x,
        area.y.saturating_add(top),
        area.width,
        area.height.saturating_sub(top),
    )
}

fn render_detail(frame: &mut Frame, state: &AppState, now: NaiveDateTime, content: Rect) {
    if content.width < MINIMUM_DRAWABLE_SIDE || content.height < MINIMUM_DRAWABLE_SIDE {
        return;
    }
    let Some(entry) = state
        .focused_entry()
        .and_then(|index| state.entries().get(index))
    else {
        return;
    };
    let block = Block::bordered().title(truncate_to_width(
        &entry_title(entry),
        usize::from(content.width.saturating_sub(BORDER_COLUMNS)),
    ));
    let inner = block.inner(content);
    let width = usize::from(inner.width);
    let (lines, selected_row) = detail_lines(entry, state.selected_item(), width, now);
    let scroll = selected_row.saturating_sub(usize::from(inner.height.saturating_sub(1)));
    frame.render_widget(block, content);
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        inner,
    );
}

fn detail_lines(
    entry: &WidgetEntry,
    selected: usize,
    width: usize,
    now: NaiveDateTime,
) -> (Vec<Line<'static>>, usize) {
    let Some(snapshot) = entry.record.snapshot() else {
        return (box_body_lines(entry, width, now, true), 0);
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

fn to_rect(origin: Rect, cell: CellRect) -> Rect {
    Rect::new(
        origin.x.saturating_add(cell.x),
        origin.y.saturating_add(cell.y),
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
        ActionScope, ActionSpec, DetailItem, DetailSection, Flag, Mini, Summary, SummaryLine,
        WidgetId, WidgetRecord, WidgetSnapshot,
    };
    use crate::presentation::app_state::{AppState, TabView, WidgetEntry};
    use crate::presentation::key_input::KeyInput;
    use chrono::{NaiveDate, TimeDelta};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Modifier;
    use ratatui::style::{Color, Style};
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

    fn entry(id: &str, record: WidgetRecord) -> WidgetEntry {
        WidgetEntry {
            id: WidgetId::new(id),
            record,
            interval: std::time::Duration::from_secs(30),
            max_items: None,
        }
    }

    fn state_with(entries: Vec<WidgetEntry>) -> AppState {
        let entry_indexes = (0..entries.len()).collect();
        AppState::new(
            entries,
            vec![TabView {
                name: "Principal".to_string(),
                entry_indexes,
            }],
        )
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
        entry("kanban", ready(kanban_snapshot()))
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

    fn buffer_of(state: &AppState, width: u16, height: u16, at: NaiveDateTime) -> Buffer {
        let spans = vec![1; state.entries().len()];
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, state, &spans, at))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn ready_at(snapshot: WidgetSnapshot, at: NaiveDateTime) -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(Ok(snapshot), at);
        record
    }

    fn border_colors(buffer: &Buffer) -> Vec<Option<Color>> {
        let area = buffer.area;
        (0..area.height)
            .flat_map(|row| (0..area.width).map(move |column| (column, row)))
            .filter(|&(column, row)| buffer[(column, row)].symbol() == "┌")
            .map(|(column, row)| buffer[(column, row)].fg.into())
            .collect()
    }

    fn style_of_text(buffer: &Buffer, needle: &str) -> Option<Style> {
        let area = buffer.area;
        for row in 0..area.height {
            let symbols: Vec<&str> = (0..area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect();
            for start in 0..symbols.len() {
                if symbols[start..].concat().starts_with(needle) {
                    let column = u16::try_from(start).unwrap();
                    return Some(buffer[(column, row)].style());
                }
            }
        }
        None
    }

    #[test]
    fn failing_widget_with_old_data_has_red_border_and_age_line() {
        let old = ready_at(
            snapshot("Agenda", vec![plain_line("reuniões 2")]),
            now() - TimeDelta::minutes(45),
        );
        let mut failing = old;
        failing.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let state = state_with(vec![kanban_entry(), entry("agenda", failing)]);
        let buffer = buffer_of(&state, 100, 30, now());
        let colors = border_colors(&buffer);
        assert!(colors.contains(&Some(Color::Red)), "{colors:?}");
        let text = screen(&state, 100, 30);
        assert!(text.contains("sem atualizar há 45 min"), "{text}");
        assert!(text.contains("reuniões 2"), "{text}");
        assert!(!text.contains("⚠ falha: fora do ar"), "{text}");
    }

    #[test]
    fn first_load_failure_shows_only_red_message() {
        let state = state_with(vec![entry("kanban", failed("plugin quebrou"))]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("plugin quebrou"), "{text}");
        assert!(!text.contains("sem atualizar"), "{text}");
        assert!(!text.contains("carregando"), "{text}");
        let style = style_of_text(&buffer_of(&state, 100, 30, now()), "plugin quebrou");
        assert_eq!(style.and_then(|style| style.fg), Some(Color::Red));
    }

    #[test]
    fn refreshing_keeps_border_color_and_adds_indicator() {
        let mut refreshing = ready(snapshot("Kanban", vec![plain_line("inbox 3")]));
        refreshing.begin_refresh();
        let idle = state_with(vec![kanban_entry()]);
        let busy = state_with(vec![entry("kanban", refreshing)]);
        assert_eq!(
            border_colors(&buffer_of(&busy, 100, 30, now())),
            border_colors(&buffer_of(&idle, 100, 30, now())),
        );
        let text = screen(&busy, 100, 30);
        assert!(text.contains("[A] Kanban ⟳"), "{text}");
        assert!(text.contains("inbox 3"), "{text}");
    }

    #[test]
    fn stale_data_is_dimmed() {
        let interval_periods = 10;
        let stale_at = now() - TimeDelta::seconds(30 * interval_periods);
        let stale = state_with(vec![entry(
            "kanban",
            ready_at(snapshot("Kanban", vec![plain_line("inbox 3")]), stale_at),
        )]);
        let fresh = state_with(vec![kanban_entry()]);
        let stale_style = style_of_text(&buffer_of(&stale, 100, 30, now()), "inbox 3");
        let fresh_style = style_of_text(&buffer_of(&fresh, 100, 30, now()), "Tarefa");
        assert!(stale_style.is_some_and(|style| style.add_modifier.contains(Modifier::DIM)));
        assert!(!fresh_style.is_some_and(|style| style.add_modifier.contains(Modifier::DIM)));
    }

    #[test]
    fn widget_max_items_cuts_overflow_inside_box() {
        let lines = (1..=10)
            .map(|number| plain_line(&format!("linha {number}")))
            .collect();
        let mut capped = entry("agenda", ready(snapshot("Agenda", lines)));
        capped.max_items = Some(3);
        let text = screen(&state_with(vec![capped]), 100, 30);
        assert!(text.contains("linha 2"), "{text}");
        assert!(text.contains("+8 itens"), "{text}");
        assert!(!text.contains("linha 3"), "{text}");
    }

    #[test]
    fn focus_mode_lists_every_widget_of_the_tab_in_a_column() {
        let mut agenda = snapshot("Agenda", vec![]);
        agenda.mini = Some(Mini {
            value: "3".to_string(),
            text: "reuniões".to_string(),
            tone: widget_content::Tone::Ok,
        });
        let mut state = state_with(vec![kanban_entry(), entry("agenda", ready(agenda))]);
        press(&mut state, &[KeyInput::Enter]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("Kanban 2 itens"), "{text}");
        assert!(text.contains("Agenda 3 reuniões"), "{text}");
    }

    #[test]
    fn render_never_panics_on_zero_sized_frames() {
        let mut state = state_with(vec![kanban_entry()]);
        assert!(draw(&state, 0, 0).is_empty());
        assert_eq!(draw(&state, 1, 1).len(), 1);
        let rows = draw(&state, 59, 19);
        assert_eq!(rows.len(), 19);
        assert!(rows.iter().any(|row| row.contains("Amplie")), "{rows:?}");
        assert!(border_colors(&buffer_of(&state, 59, 19, now())).contains(&Some(Color::Cyan)));
        press(&mut state, &[KeyInput::Enter]);
        for (width, height) in [(0, 0), (1, 1), (59, 19)] {
            assert_eq!(draw(&state, width, height).len(), usize::from(height));
        }
    }

    #[test]
    fn top_bar_shows_clock_and_date() {
        let state = state_with(vec![kanban_entry()]);
        let first_row = draw(&state, 100, 30).remove(0);
        assert!(first_row.contains("09:41"), "{first_row}");
        assert!(first_row.contains("17/10/2026"), "{first_row}");
    }

    #[test]
    fn home_draws_one_box_per_widget_title() {
        let state = state_with(vec![
            kanban_entry(),
            entry("agenda", ready(snapshot("Agenda", vec![]))),
            entry("time", ready(snapshot("Time", vec![]))),
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
        let state = state_with(vec![entry("kanban", record)]);
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
        let state = state_with(vec![entry("ia", record)]);
        assert!(screen(&state, 100, 30).contains("falha: modelo indisponível"));
    }

    #[test]
    fn failed_widget_shows_error_inside_its_box() {
        let state = state_with(vec![entry("kanban", failed("plugin quebrou"))]);
        assert!(screen(&state, 100, 30).contains("plugin quebrou"));
    }

    #[test]
    fn flag_symbol_is_rendered_before_line_text() {
        let line = SummaryLine {
            text: "inbox 3".to_string(),
            flag: Some(Flag::Updated),
        };
        let state = state_with(vec![entry("kanban", ready(snapshot("Kanban", vec![line])))]);
        assert!(screen(&state, 100, 30).contains("● inbox 3"));
    }

    #[test]
    fn confirmation_dialog_shows_description_and_keys() {
        let mut state = state_with(vec![kanban_entry()]);
        press(
            &mut state,
            &[
                KeyInput::Enter,
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
        let mut state = state_with(vec![kanban_entry()]);
        press(&mut state, &[KeyInput::Enter, KeyInput::Character('a')]);
        assert!(screen(&state, 100, 30).contains("Mover para done"));
    }

    #[test]
    fn compact_mode_shows_enlarge_warning() {
        let state = state_with(vec![kanban_entry()]);
        assert!(screen(&state, 50, 15).contains("Amplie"));
    }

    #[test]
    fn zero_sized_area_does_not_panic() {
        let mut state = state_with(vec![kanban_entry()]);
        for width in 0..=14u16 {
            for height in 0..=14u16 {
                draw(&state, width, height);
            }
        }
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Character('a'), KeyInput::Enter],
        );
        for (width, height) in [(0, 0), (1, 1), (3, 3), (10, 4), (60, 20)] {
            draw(&state, width, height);
        }
    }

    #[test]
    fn wide_characters_are_truncated_with_ellipsis_inside_box() {
        let state = state_with(vec![entry(
            "kanban",
            ready(snapshot("Kanban", vec![plain_line(&"日本語".repeat(15))])),
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
        let mut state = state_with(vec![kanban_entry()]);
        press(&mut state, &[KeyInput::Enter, KeyInput::Down]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("inbox"));
        assert!(text.contains("  ⚠ Tarefa A"), "{text}");
        assert!(text.contains("> Tarefa B"), "{text}");
    }

    #[test]
    fn notice_is_rendered_on_the_last_row() {
        let mut state = state_with(vec![kanban_entry()]);
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
        state_with(vec![kanban_entry()]).with_pomodoro(pomodoro_timer())
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
        let state = state_with(vec![kanban_entry()]);
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
        let mut state = state_with(vec![kanban_entry()]);
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
        let mut state = state_with(vec![entry(
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
        )]);
        press(&mut state, &[KeyInput::Enter]);
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
        let mut modal = state_with(vec![kanban_entry()]);
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
        entry("agenda", ready(widget_snapshot))
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
        let state = state_with(vec![entry_with_blocks(rows_and_stat_blocks())]);
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
        let state = state_with(vec![entry_with_blocks(vec![table])]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("Nome"), "{text}");
        assert!(text.contains("ana"), "{text}");
    }

    #[test]
    fn legacy_lines_still_render_exactly_as_before() {
        let state = state_with(vec![entry(
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
        let mut state = state_with(vec![entry("kanban", ready(snapshot))]);
        press(&mut state, &[KeyInput::Enter]);
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
        let mut state = state_with(vec![entry("kanban", ready(snapshot))]);
        press(&mut state, &[KeyInput::Enter, KeyInput::Down]);
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
        let mut state = state_with(vec![entry_with_blocks(blocks)]);
        for size in 0..=14 {
            draw(&state, size, size);
        }
        press(&mut state, &[KeyInput::Enter]);
        for size in 0..=14 {
            draw(&state, size, size);
        }
    }
}
