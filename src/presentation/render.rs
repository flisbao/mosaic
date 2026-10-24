use chrono::NaiveDateTime;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::application::pomodoro_notifications::phase_label;
use crate::application::ports::AlertMessage;
use crate::domain::pomodoro::{PomodoroStatus, PomodoroTimer, SECONDS_PER_MINUTE};
use crate::domain::widget::{DetailItem, DetailSection};
use crate::presentation::app_state::{AppState, Mode, WidgetEntry};
use crate::presentation::focus_column::focus_column_lines;
use crate::presentation::grid_layout::{
    arrange, CellRect, GridLayout, TerminalSize, MINIMUM_FULL_HEIGHT, MINIMUM_FULL_WIDTH,
    TOP_BAR_HEIGHT,
};
use crate::presentation::key_hints::{
    context_hints, footer_hints, global_hints, HintContext, KeyHint,
};
use crate::presentation::text_fit::{flagged, truncate_to_width};
use crate::presentation::theme_styles::ThemeStyles;
use crate::presentation::widget_box::{
    box_block, box_header, dim_if_stale, render_widget_box, BoxContext, Emphasis, FailureDetail,
};
use crate::presentation::widget_view::render_blocks_limited;

const FALLBACK_SPAN: u16 = 1;
const FOOTER_HEIGHT: u16 = 1;
const MINIMUM_HEIGHT_FOR_FOOTER: u16 = MINIMUM_FULL_HEIGHT + FOOTER_HEIGHT;
const FOOTER_LEADING_PADDING: &str = " ";
const FOOTER_SEPARATOR: &str = "  ";
const HELP_KEY_COLUMN_WIDTH: usize = 18;
const HELP_GLOBAL_HEADING: &str = "Em qualquer tela";
const HELP_DISMISS_HINT: &str = "Esc ou ? fecha esta ajuda";
const MAXIMUM_NUMBERED_TABS: usize = 9;
const CONFIRMATION_HINT: &str = "[s] confirma  [n] cancela";
const NOTICE_ROW_HEIGHT: u16 = 1;
const MINIMUM_HEIGHT_FOR_NOTICE: u16 = 2;
const FOCUS_COLUMN_WIDTH: u16 = 22;
const FOCUS_MINIMUM_WIDTH: u16 = FOCUS_COLUMN_WIDTH.saturating_add(MINIMUM_DRAWABLE_SIDE);
const MODAL_MAXIMUM_WIDTH: u16 = 60;
const MODAL_FRAME_THICKNESS: u16 = 2;
const MINIMUM_DRAWABLE_SIDE: u16 = 3;
const ALERT_DISMISS_HINT: &str = "Qualquer tecla fecha";
const WRAP_CONTINUATION_INDENT: &str = "  ";
const SELECTION_MARKER: &str = "> ";
const NO_SELECTION_MARKER: &str = "  ";
const HEADING_ROWS: usize = 1;
const WINDOW_PREALLOCATION_LIMIT: usize = 512;
const PRINTABLE_ASCII_START: u8 = 0x20;
const PRINTABLE_ASCII_END: u8 = 0x7f;

pub fn render(frame: &mut Frame, state: &AppState, spans: &[u16], now: NaiveDateTime) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    fill_background(frame, area, state.styles());
    render_top_bar(frame, state, now, area);
    let body = body_area(area);
    if state.is_expanded() {
        render_focus_mode(frame, state, now, body);
    } else {
        render_home(frame, state, spans, now, body);
    }
    render_footer(frame, state, area);
    render_notice(frame, state, area);
    render_modal(frame, state, area);
    render_alert(frame, state, area);
}

fn fill_background(frame: &mut Frame, area: Rect, styles: &ThemeStyles) {
    if styles.paints_background() {
        frame.render_widget(Block::new().style(styles.base()), area);
    }
}

fn clear_with_theme(frame: &mut Frame, area: Rect, styles: &ThemeStyles) {
    frame.render_widget(Clear, area);
    fill_background(frame, area, styles);
}

fn render_top_bar(frame: &mut Frame, state: &AppState, now: NaiveDateTime, area: Rect) {
    let styles = state.styles();
    let bar = Rect::new(area.x, area.y, area.width, TOP_BAR_HEIGHT.min(area.height));
    let mut spans = vec![Span::styled(
        format!(" {}  {} ", now.format("%H:%M"), now.format("%d/%m/%Y")),
        styles.accent(),
    )];
    for (position, tab) in state.tabs().iter().enumerate() {
        let style = if position == state.active_tab() {
            styles.active_tab()
        } else {
            styles.muted()
        };
        spans.push(Span::styled(tab_label(position, &tab.name), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), bar);
    if let Some(timer) = state.pomodoro() {
        frame.render_widget(
            Paragraph::new(Line::styled(pomodoro_text(timer), styles.ok()))
                .alignment(Alignment::Right),
            bar,
        );
    }
}

fn footer_row(area: Rect) -> Option<Rect> {
    if area.height < MINIMUM_HEIGHT_FOR_FOOTER {
        return None;
    }
    Some(Rect::new(
        area.x,
        area.y + area.height - FOOTER_HEIGHT,
        area.width,
        FOOTER_HEIGHT,
    ))
}

fn body_area(area: Rect) -> Rect {
    match footer_row(area) {
        Some(_) => Rect::new(area.x, area.y, area.width, area.height - FOOTER_HEIGHT),
        None => area,
    }
}

fn render_footer(frame: &mut Frame, state: &AppState, area: Rect) {
    let Some(row) = footer_row(area) else {
        return;
    };
    let context = HintContext::from_expanded(state.is_expanded());
    let key_style = state.styles().key_hint();
    let description_style = state.styles().muted();
    let mut spans = vec![Span::raw(FOOTER_LEADING_PADDING)];
    for hint in footer_hints(context, usize::from(area.width)) {
        spans.push(Span::styled(hint.keys, key_style));
        spans.push(Span::styled(
            format!(" {}{FOOTER_SEPARATOR}", hint.description),
            description_style,
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn tab_label(position: usize, name: &str) -> String {
    if position < MAXIMUM_NUMBERED_TABS {
        format!(" {} {name} ", position + 1)
    } else {
        format!(" {name} ")
    }
}

fn pomodoro_text(timer: &PomodoroTimer) -> String {
    let seconds = timer.remaining().as_secs();
    let phase = phase_label(timer.phase());
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
                    emphasis: if Some(entry_index) == state.focused_entry() {
                        Emphasis::Focused
                    } else {
                        Emphasis::Normal
                    },
                    now,
                    warning: None,
                    styles: state.styles(),
                };
                if let Some(entry) = state.entries().get(entry_index) {
                    render_widget_box(frame, entry, to_rect(area, placement.area), &context);
                }
            }
        }
        GridLayout::Compact(placement) => {
            let first = entry_indexes.first().copied();
            if let Some(entry) = first.and_then(|index| state.entries().get(index)) {
                let warning = enlarge_warning();
                let context = BoxContext {
                    shortcut: first.and_then(|index| state.shortcut_of(index)),
                    emphasis: Emphasis::Focused,
                    now,
                    warning: Some(&warning),
                    styles: state.styles(),
                };
                render_widget_box(frame, entry, to_rect(area, placement.area), &context);
            }
        }
        GridLayout::Empty => {}
    }
}

fn enlarge_warning() -> String {
    format!("Amplie o terminal (mínimo {MINIMUM_FULL_WIDTH}x{MINIMUM_FULL_HEIGHT})")
}

fn render_focus_mode(frame: &mut Frame, state: &AppState, now: NaiveDateTime, area: Rect) {
    let content = content_area(area);
    if content.width < FOCUS_MINIMUM_WIDTH {
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
    let lines = focus_column_lines(&entries, focused, usize::from(area.width), state.styles());
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
    let Some(index) = state.focused_entry() else {
        return;
    };
    let Some(entry) = state.entries().get(index) else {
        return;
    };
    let block = box_block(
        entry,
        state.shortcut_of(index),
        Emphasis::Focused,
        content.width,
        state.styles(),
    );
    let inner = block.inner(content);
    let width = usize::from(inner.width);
    let lines = detail_window(
        entry,
        state.selected_item(),
        width,
        usize::from(inner.height),
        now,
        state.styles(),
    );
    frame.render_widget(block, content);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn detail_window(
    entry: &WidgetEntry,
    selected: usize,
    width: usize,
    height: usize,
    now: NaiveDateTime,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let header = box_header(entry, width, now, FailureDetail::Shown, styles);
    let Some(sections) = entry
        .record
        .snapshot()
        .and_then(|snapshot| snapshot.detail.as_deref())
    else {
        return header;
    };
    let header_height = header.len();
    let selected_row =
        selected_row(sections, selected, width, styles).saturating_add(header_height);
    let scroll = selected_row.saturating_sub(height.saturating_sub(1));
    let mut window: Vec<Line<'static>> = header.into_iter().skip(scroll).take(height).collect();
    let remaining = height.saturating_sub(window.len());
    if remaining > 0 {
        let first_section_row = scroll.saturating_sub(header_height);
        let section_lines = section_window(
            sections,
            selected,
            width,
            first_section_row,
            remaining,
            styles,
        );
        window.extend(dim_if_stale(section_lines, entry, now));
    }
    window
}

fn item_wrap_width(width: usize) -> usize {
    width.saturating_sub(WRAP_CONTINUATION_INDENT.width())
}

fn item_height(item: &DetailItem, wrap_width: usize) -> usize {
    match item.flag {
        Some(flag) => wrapped_height(&flagged(Some(flag), &item.text), wrap_width),
        None => wrapped_height(&item.text, wrap_width),
    }
}

fn section_preamble_height(section: &DetailSection, width: usize, styles: &ThemeStyles) -> usize {
    HEADING_ROWS + render_blocks_limited(&section.display_blocks(), width, 0, styles).total
}

fn selected_row(
    sections: &[DetailSection],
    selected: usize,
    width: usize,
    styles: &ThemeStyles,
) -> usize {
    let wrap_width = item_wrap_width(width);
    let mut row = 0;
    let mut first_item_of_section = 0;
    for section in sections {
        row += section_preamble_height(section, width, styles);
        let last_item_of_section = first_item_of_section + section.items.len();
        if selected < last_item_of_section {
            let before = selected.saturating_sub(first_item_of_section);
            return row
                + section.items[..before]
                    .iter()
                    .map(|item| item_height(item, wrap_width))
                    .sum::<usize>();
        }
        row += section
            .items
            .iter()
            .map(|item| item_height(item, wrap_width))
            .sum::<usize>();
        first_item_of_section = last_item_of_section;
    }
    0
}

fn section_window(
    sections: &[DetailSection],
    selected: usize,
    width: usize,
    start: usize,
    count: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let end = start.saturating_add(count);
    let wrap_width = item_wrap_width(width);
    let mut window = Vec::with_capacity(count.min(WINDOW_PREALLOCATION_LIMIT));
    let mut row = 0;
    let mut item_index = 0;
    'sections: for section in sections {
        if row >= end {
            break;
        }
        let mut preamble = vec![section_heading_line(section, width)];
        preamble.extend(
            render_blocks_limited(&section.display_blocks(), width, usize::MAX, styles).lines,
        );
        push_visible(&mut window, &mut row, preamble, start, end);
        for item in &section.items {
            if row >= end {
                break 'sections;
            }
            let is_selected = item_index == selected;
            item_index += 1;
            let height = item_height(item, wrap_width);
            if row + height <= start {
                row += height;
                continue;
            }
            push_visible(
                &mut window,
                &mut row,
                item_lines(item, is_selected, width, styles),
                start,
                end,
            );
        }
    }
    window
}

fn push_visible(
    window: &mut Vec<Line<'static>>,
    row: &mut usize,
    lines: Vec<Line<'static>>,
    start: usize,
    end: usize,
) {
    for line in lines {
        if *row >= start && *row < end {
            window.push(line);
        }
        *row += 1;
    }
}

fn is_printable_ascii(text: &str) -> bool {
    text.bytes()
        .all(|byte| (PRINTABLE_ASCII_START..PRINTABLE_ASCII_END).contains(&byte))
}

fn wrapped_height(text: &str, width: usize) -> usize {
    if width == 0 {
        return 0;
    }
    if text.len() <= width {
        return 1;
    }
    if is_printable_ascii(text) {
        return text.len().div_ceil(width);
    }
    let mut lines = 1;
    let mut used = 0;
    let mut current_is_empty = true;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > width && !current_is_empty {
            lines += 1;
            used = 0;
        }
        current_is_empty = false;
        used += character_width;
    }
    lines
}

fn section_heading_line(section: &DetailSection, width: usize) -> Line<'static> {
    Line::styled(
        truncate_to_width(&section.heading, width),
        Style::default().add_modifier(Modifier::BOLD),
    )
}

struct ItemAppearance {
    marker: &'static str,
    style: Style,
}

impl ItemAppearance {
    fn for_selection(is_selected: bool, styles: &ThemeStyles) -> Self {
        if is_selected {
            Self {
                marker: SELECTION_MARKER,
                style: styles.accent(),
            }
        } else {
            Self {
                marker: NO_SELECTION_MARKER,
                style: Style::default(),
            }
        }
    }
}

fn item_lines(
    item: &DetailItem,
    is_selected: bool,
    width: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let appearance = ItemAppearance::for_selection(is_selected, styles);
    let wrap_width = width.saturating_sub(WRAP_CONTINUATION_INDENT.width());
    wrap_to_width(&flagged(item.flag, &item.text), wrap_width)
        .into_iter()
        .enumerate()
        .map(|(position, piece)| {
            let prefix = if position == 0 {
                appearance.marker
            } else {
                WRAP_CONTINUATION_INDENT
            };
            Line::styled(format!("{prefix}{piece}"), appearance.style)
        })
        .collect()
}

fn render_notice(frame: &mut Frame, state: &AppState, area: Rect) {
    let Some(notice) = state.notice() else {
        return;
    };
    let Some(row) = notice_row(area) else {
        return;
    };
    clear_with_theme(frame, row, state.styles());
    frame.render_widget(
        Paragraph::new(Line::styled(
            truncate_to_width(notice, area.width as usize),
            state.styles().warn(),
        )),
        row,
    );
}

fn notice_row(area: Rect) -> Option<Rect> {
    if area.height < MINIMUM_HEIGHT_FOR_NOTICE {
        return None;
    }
    Some(Rect::new(
        area.x,
        area.y + area.height - NOTICE_ROW_HEIGHT,
        area.width,
        NOTICE_ROW_HEIGHT,
    ))
}

fn render_modal(frame: &mut Frame, state: &AppState, area: Rect) {
    let (title, lines) = match state.mode() {
        Mode::Browsing => return,
        Mode::Helping => {
            let context = HintContext::from_expanded(state.is_expanded());
            (context.title(), help_lines(context))
        }
        Mode::ConfirmingSend { text, .. } => (
            "Enviar à IA",
            vec![text.clone(), String::new(), CONFIRMATION_HINT.to_string()],
        ),
        Mode::ChoosingAction { selected } => ("Ações", action_menu_lines(state, *selected)),
        Mode::Confirming(proposed) => {
            let mut lines = vec![proposed.describe()];
            if let Some(item_id) = proposed.item_id() {
                lines.push(format!("Item: {item_id}"));
            }
            lines.push(String::new());
            lines.push(CONFIRMATION_HINT.to_string());
            ("Confirmar ação", lines)
        }
    };
    draw_popup(frame, area, title, &lines, state.styles());
}

fn render_alert(frame: &mut Frame, state: &AppState, area: Rect) {
    let Some(AlertMessage { title, body }) = state.alert() else {
        return;
    };
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    lines.push(String::new());
    lines.push(ALERT_DISMISS_HINT.to_string());
    draw_popup(frame, area, title, &lines, state.styles());
}

fn draw_popup(frame: &mut Frame, area: Rect, title: &str, lines: &[String], styles: &ThemeStyles) {
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
        .border_style(styles.warn());
    let inner = block.inner(popup);
    let text: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(truncate_to_width(line, inner.width as usize)))
        .collect();
    clear_with_theme(frame, popup, styles);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(text), inner);
}

fn help_lines(context: HintContext) -> Vec<String> {
    let mut lines: Vec<String> = context_hints(context).iter().map(help_line).collect();
    lines.push(String::new());
    lines.push(HELP_GLOBAL_HEADING.to_string());
    lines.extend(global_hints().iter().map(help_line));
    lines.push(String::new());
    lines.push(HELP_DISMISS_HINT.to_string());
    lines
}

fn help_line(hint: &KeyHint) -> String {
    format!(
        "{:<width$}{}",
        hint.keys,
        hint.description,
        width = HELP_KEY_COLUMN_WIDTH
    )
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
        ActionScope, ActionSpec, DetailItem, DetailSection, Flag, Mini, Summary, WidgetId,
        WidgetRecord, WidgetSnapshot,
    };
    use crate::presentation::app_state::{AppState, TabView, WidgetEntry};
    use crate::presentation::key_input::KeyInput;
    use crate::presentation::theme_styles::ThemeStyles;
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

    fn plain(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn detail_item(text: &str) -> DetailItem {
        DetailItem {
            id: None,
            text: text.to_string(),
            flag: None,
            meta: vec![],
        }
    }

    #[test]
    fn item_lines_mark_the_selected_item_and_indent_continuations() {
        let item = detail_item("abcdefghij");

        let selected: Vec<String> = item_lines(&item, true, 8, &ThemeStyles::ansi())
            .iter()
            .map(plain)
            .collect();
        let normal: Vec<String> = item_lines(&item, false, 8, &ThemeStyles::ansi())
            .iter()
            .map(plain)
            .collect();

        assert_eq!(selected, vec!["> abcdef", "  ghij"]);
        assert_eq!(normal, vec!["  abcdef", "  ghij"]);
    }

    #[test]
    fn section_heading_line_is_truncated_to_the_width() {
        let section = DetailSection {
            heading: "cabeçalho muito longo".to_string(),
            items: vec![],
            blocks: vec![],
        };
        assert!(plain(&section_heading_line(&section, 8)).chars().count() <= 8);
    }

    #[test]
    fn enlarge_warning_is_built_from_the_layout_minimums() {
        assert_eq!(enlarge_warning(), "Amplie o terminal (mínimo 60x20)");
    }

    #[test]
    fn notice_row_is_the_last_row_and_absent_in_a_one_row_terminal() {
        assert_eq!(
            notice_row(Rect::new(2, 3, 40, 10)),
            Some(Rect::new(2, 12, 40, 1))
        );
        assert_eq!(notice_row(Rect::new(0, 0, 40, 1)), None);
    }

    fn snapshot(title: &str, lines: Vec<blocks::lines::WidgetLine>) -> WidgetSnapshot {
        let summary_blocks = if lines.is_empty() {
            vec![]
        } else {
            vec![blocks::Block::Lines(lines)]
        };
        WidgetSnapshot {
            summary: Summary {
                blocks: summary_blocks,
                ..Summary::titled(title)
            },
            ..WidgetSnapshot::titled(title)
        }
    }

    fn plain_line(text: &str) -> blocks::lines::WidgetLine {
        blocks::lines::WidgetLine {
            text: text.to_string(),
            tone: widget_content::Tone::Normal,
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
                    flag: Some(Flag::Attention),
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

    fn border_cells(buffer: &Buffer) -> Vec<(Option<Color>, Modifier)> {
        let area = buffer.area;
        (0..area.height)
            .flat_map(|row| (0..area.width).map(move |column| (column, row)))
            .filter(|&(column, row)| buffer[(column, row)].symbol() == "┌")
            .map(|(column, row)| {
                (
                    buffer[(column, row)].fg.into(),
                    buffer[(column, row)].modifier,
                )
            })
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

    fn two_tab_state() -> AppState {
        AppState::new(
            vec![
                entry("kanban", ready(kanban_snapshot())),
                entry(
                    "notes",
                    ready(snapshot("Notas", vec![plain_line("nota 1")])),
                ),
            ],
            vec![
                TabView {
                    name: "Alfa".to_string(),
                    entry_indexes: vec![0],
                },
                TabView {
                    name: "Beta".to_string(),
                    entry_indexes: vec![1],
                },
            ],
        )
    }

    #[test]
    fn tab_label_numbers_the_first_nine_tabs_only() {
        assert_eq!(tab_label(0, "PRs"), " 1 PRs ");
        assert_eq!(tab_label(8, "Nona"), " 9 Nona ");
        assert_eq!(tab_label(9, "Décima"), " Décima ");
    }

    #[test]
    fn top_bar_shows_numbered_tabs() {
        let top_bar = draw(&two_tab_state(), 80, 24)[0].clone();
        assert!(top_bar.contains("1 Alfa"), "{top_bar}");
        assert!(top_bar.contains("2 Beta"), "{top_bar}");
    }

    #[test]
    fn active_tab_is_a_cyan_pill_and_others_are_not() {
        let mut state = two_tab_state();
        let buffer = buffer_of(&state, 80, 24, now());
        let active = style_of_text(&buffer, "Alfa").unwrap();
        let inactive = style_of_text(&buffer, "Beta").unwrap();
        assert_eq!(active.bg, Some(Color::Cyan));
        assert_eq!(active.fg, Some(Color::Black));
        assert_ne!(inactive.bg, Some(Color::Cyan));
        press(&mut state, &[KeyInput::Tab]);
        let buffer = buffer_of(&state, 80, 24, now());
        assert_eq!(
            style_of_text(&buffer, "Beta").unwrap().bg,
            Some(Color::Cyan)
        );
        assert_ne!(
            style_of_text(&buffer, "Alfa").unwrap().bg,
            Some(Color::Cyan)
        );
    }

    #[test]
    fn help_window_in_grid_lists_grid_and_global_keys() {
        let mut state = two_tab_state();
        press(&mut state, &[KeyInput::Character('?')]);
        let text = screen(&state, 100, 30);
        for expected in [
            "Ajuda · grade de widgets",
            "Enter",
            "Em qualquer tela",
            "recarregar",
            "fecha esta ajuda",
        ] {
            assert!(text.contains(expected), "falta {expected}\n{text}");
        }
    }

    #[test]
    fn help_window_in_expanded_widget_lists_navigation_keys() {
        let mut state = two_tab_state();
        press(&mut state, &[KeyInput::Enter, KeyInput::Character('?')]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("Ajuda · widget aberto"), "{text}");
        assert!(text.contains("PgUp"), "{text}");
    }

    #[test]
    fn help_window_never_panics_on_tiny_terminals() {
        let mut state = two_tab_state();
        press(&mut state, &[KeyInput::Character('?')]);
        for (width, height) in [(0, 0), (1, 1), (10, 3), (30, 8), (60, 12)] {
            draw(&state, width, height);
        }
    }

    fn last_row(state: &AppState, width: u16, height: u16) -> String {
        draw(state, width, height).pop().unwrap()
    }

    #[test]
    fn footer_on_the_grid_lists_keys_and_help() {
        let row = last_row(&two_tab_state(), 100, 30);
        for expected in ["Enter", "abrir", "? ajuda"] {
            assert!(row.contains(expected), "falta {expected}: {row}");
        }
    }

    #[test]
    fn footer_in_an_expanded_widget_lists_navigation_keys() {
        let mut state = two_tab_state();
        press(&mut state, &[KeyInput::Enter]);
        let row = last_row(&state, 100, 30);
        for expected in ["Esc", "voltar", "? ajuda"] {
            assert!(row.contains(expected), "falta {expected}: {row}");
        }
    }

    #[test]
    fn footer_keeps_help_visible_at_minimum_width() {
        let row = last_row(&two_tab_state(), 60, 30);
        assert!(row.contains("? ajuda"), "{row}");
    }

    #[test]
    fn footer_is_hidden_below_minimum_height() {
        let state = two_tab_state();
        for height in [20, 12] {
            assert!(!screen(&state, 100, height).contains("? ajuda"));
        }
        let area = Rect::new(0, 0, 100, 20);
        assert_eq!(footer_row(area), None);
        assert_eq!(body_area(area), area);
    }

    #[test]
    fn grid_keeps_full_layout_at_twenty_one_rows() {
        let text = screen(&two_tab_state(), 100, 21);
        assert!(!text.contains("Amplie o terminal"), "{text}");
        assert!(last_row(&two_tab_state(), 100, 21).contains("? ajuda"));
    }

    #[test]
    fn notice_covers_the_footer_and_footer_returns_after_next_key() {
        let mut state = two_tab_state();
        press(&mut state, &[KeyInput::Tab, KeyInput::Enter]);
        let covered = last_row(&state, 100, 30);
        assert!(covered.contains("ainda não tem detalhe"), "{covered}");
        assert!(!covered.contains("? ajuda"), "{covered}");
        press(&mut state, &[KeyInput::Down]);
        assert!(last_row(&state, 100, 30).contains("? ajuda"));
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
        assert!(!text.contains("Kanban"), "{text}");
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
        let mut failing = ready_at(snapshot("Agenda", lines), now() - TimeDelta::minutes(45));
        failing.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let mut capped = entry("agenda", failing);
        capped.max_items = Some(3);
        let text = screen(&state_with(vec![capped]), 100, 30);
        assert!(text.contains("sem atualizar há 45 min"), "{text}");
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
    fn failure_after_success_shows_reason_and_age_in_focus_mode() {
        let old = ready_at(kanban_snapshot(), now() - TimeDelta::minutes(45));
        let mut failing = old;
        failing.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let mut state = state_with(vec![entry("kanban", failing)]);
        press(&mut state, &[KeyInput::Enter]);
        let rows = draw(&state, 100, 30);
        let failure = rows
            .iter()
            .position(|row| row.contains("⚠ falha: fora do ar"));
        let age = rows
            .iter()
            .position(|row| row.contains("sem atualizar há 45 min"));
        let old_data = rows.iter().position(|row| row.contains("Tarefa A"));
        assert!(failure.is_some(), "{rows:?}");
        assert!(failure < age && age < old_data, "{rows:?}");
        assert!(border_colors(&buffer_of(&state, 100, 30, now())).contains(&Some(Color::Red)));
    }

    #[test]
    fn home_hides_failure_reason_even_when_selected() {
        let mut failing = ready_at(
            snapshot("Kanban", vec![plain_line("inbox 3")]),
            now() - TimeDelta::minutes(45),
        );
        failing.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let state = state_with(vec![entry("kanban", failing)]);
        let text = screen(&state, 100, 30);
        assert!(!text.contains("falha:"), "{text}");
        assert!(text.contains("sem atualizar há 45 min"), "{text}");
        assert!(border_colors(&buffer_of(&state, 100, 30, now())).contains(&Some(Color::Red)));
    }

    #[test]
    fn failing_border_is_bold_only_when_focused() {
        let mut failing = ready(snapshot("Kanban", vec![plain_line("inbox 3")]));
        failing.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let focused = state_with(vec![entry("kanban", failing.clone())]);
        assert_eq!(
            border_cells(&buffer_of(&focused, 100, 30, now())),
            vec![(Some(Color::Red), Modifier::BOLD)]
        );
        let unfocused = state_with(vec![kanban_entry(), entry("kanban", failing)]);
        let cells = border_cells(&buffer_of(&unfocused, 100, 30, now()));
        assert!(
            cells.contains(&(Some(Color::Red), Modifier::empty())),
            "{cells:?}"
        );
    }

    #[test]
    fn stale_dimming_skips_failure_line() {
        let mut stale = ready_at(
            snapshot("Kanban", vec![plain_line("inbox 3")]),
            now() - TimeDelta::minutes(5),
        );
        stale.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let state = state_with(vec![entry("kanban", stale)]);
        let buffer = buffer_of(&state, 100, 30, now());
        let age = style_of_text(&buffer, "sem atualizar");
        let data = style_of_text(&buffer, "inbox 3");
        assert!(age.is_some_and(|style| !style.add_modifier.contains(Modifier::DIM)));
        assert!(data.is_some_and(|style| style.add_modifier.contains(Modifier::DIM)));
    }

    #[test]
    fn recommendation_failure_after_idle_snapshot_is_shown_in_focus_mode() {
        let mut record = ready(snapshot("IA", vec![plain_line("Sem recomendação ainda")]));
        record.apply(
            Err::<WidgetSnapshot, _>("modelo indisponível".to_string()),
            now(),
        );
        let mut state = state_with(vec![entry("ia", record)]);
        press(&mut state, &[KeyInput::Enter]);
        assert!(screen(&state, 100, 30).contains("falha: modelo indisponível"));
    }

    #[test]
    fn focus_column_needs_at_least_minimum_width() {
        let mut state = state_with(vec![kanban_entry()]);
        press(&mut state, &[KeyInput::Enter]);
        assert!(!screen(&state, 24, 30).contains("Kanban 2 itens"));
        assert!(screen(&state, 25, 30).contains("Kanban 2 itens"));
    }

    #[test]
    fn first_refresh_without_snapshot_shows_indicator_and_loading() {
        let mut refreshing = WidgetRecord::loading();
        refreshing.begin_refresh();
        let state = state_with(vec![entry("kanban", refreshing)]);
        let text = screen(&state, 100, 30);
        assert!(text.contains("[A] kanban ⟳"), "{text}");
        assert!(text.contains("carregando…"), "{text}");
    }

    #[test]
    fn max_items_counts_only_body_lines_below_failure_header() {
        let lines = (1..=5)
            .map(|number| plain_line(&format!("linha {number}")))
            .collect();
        let mut failing = ready_at(snapshot("Agenda", lines), now() - TimeDelta::minutes(45));
        failing.apply(Err::<WidgetSnapshot, _>("fora do ar".to_string()), now());
        let mut capped = entry("agenda", failing);
        capped.max_items = Some(5);
        let text = screen(&state_with(vec![capped]), 100, 30);
        for number in 1..=5 {
            assert!(text.contains(&format!("linha {number}")), "{text}");
        }
        assert!(text.contains("sem atualizar há 45 min"), "{text}");
        assert!(!text.contains("+"), "{text}");
    }

    #[test]
    fn flag_symbol_is_rendered_before_line_text() {
        let line = blocks::lines::WidgetLine {
            flag: Some(Flag::Updated),
            ..plain_line("inbox 3")
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
    fn wrapped_height_matches_the_number_of_wrapped_lines() {
        let texts = [
            "",
            "abc",
            "abcdefghij",
            "abcdefghijk",
            "日本語日本語",
            "日",
            "a\tb\nc",
            "ação com acentuação longa demais",
            "x\u{0301}y\u{0301}z",
        ];
        for text in texts {
            for width in 0..=12 {
                assert_eq!(
                    wrapped_height(text, width),
                    wrap_to_width(text, width).len(),
                    "{text:?} em largura {width}"
                );
            }
        }
    }

    fn sections_with_items(counts: &[usize]) -> Vec<DetailSection> {
        counts
            .iter()
            .enumerate()
            .map(|(section, &count)| DetailSection {
                heading: format!("secao {section}"),
                items: (0..count)
                    .map(|item| DetailItem {
                        id: None,
                        text: format!("item {item} da secao {section} {}", "palavra ".repeat(item)),
                        flag: (item % 3 == 0).then_some(Flag::Attention),
                        meta: vec![],
                    })
                    .collect(),
                blocks: vec![],
            })
            .collect()
    }

    fn texts_of(lines: &[Line]) -> Vec<String> {
        lines.iter().map(plain).collect()
    }

    #[test]
    fn section_window_is_the_same_slice_of_the_full_render() {
        let sections = sections_with_items(&[3, 0, 5, 2]);
        let full = texts_of(&section_window(
            &sections,
            4,
            20,
            0,
            usize::MAX,
            &ThemeStyles::ansi(),
        ));
        for start in 0..full.len() {
            for count in 1..6 {
                let window = texts_of(&section_window(
                    &sections,
                    4,
                    20,
                    start,
                    count,
                    &ThemeStyles::ansi(),
                ));
                let end = (start + count).min(full.len());
                assert_eq!(window, full[start..end], "início {start}, contagem {count}");
            }
        }
    }

    #[test]
    fn selected_row_points_at_the_selected_item_marker() {
        let sections = sections_with_items(&[3, 0, 5, 2]);
        let full = texts_of(&section_window(
            &sections,
            4,
            20,
            0,
            usize::MAX,
            &ThemeStyles::ansi(),
        ));
        for selected in 0..10 {
            let full = texts_of(&section_window(
                &sections,
                selected,
                20,
                0,
                usize::MAX,
                &ThemeStyles::ansi(),
            ));
            let row = selected_row(&sections, selected, 20, &ThemeStyles::ansi());
            assert!(
                full[row].starts_with(SELECTION_MARKER),
                "item {selected}: {full:?}"
            );
        }
        assert!(!full.is_empty());
    }

    #[test]
    fn selected_row_is_zero_when_the_selection_is_out_of_range() {
        let sections = sections_with_items(&[2]);
        assert_eq!(selected_row(&sections, 9, 20, &ThemeStyles::ansi()), 0);
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
        assert!(text.contains("09:00 Daily"), "{text}");
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
    fn summary_lines_render_with_and_without_flags() {
        let state = state_with(vec![entry(
            "kanban",
            ready(snapshot(
                "Kanban",
                vec![
                    plain_line("inbox 3"),
                    blocks::lines::WidgetLine {
                        flag: Some(Flag::Attention),
                        ..plain_line("atenção")
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

    const THEME_WIDTH: u16 = 100;
    const THEME_HEIGHT: u16 = 30;

    fn themed_state(name: &str) -> AppState {
        let mut state = state_with(vec![kanban_entry()]);
        let theme = crate::domain::theme::Theme::preset(name).unwrap();
        state.set_theme_styles(ThemeStyles::new(
            &theme,
            crate::infrastructure::terminal_capabilities::ColorDepth::TrueColor,
        ));
        state
    }

    fn palette_color(
        name: &str,
        pick: fn(&crate::domain::theme::Palette) -> crate::domain::theme::Rgb,
    ) -> Color {
        let crate::domain::theme::Theme::Colored { palette, .. } =
            crate::domain::theme::Theme::preset(name).unwrap()
        else {
            panic!("{name} não é colorido");
        };
        let rgb = pick(&palette);
        Color::Rgb(rgb.red, rgb.green, rgb.blue)
    }

    fn assert_every_cell_has_background(buffer: &Buffer, expected: Color, context: &str) {
        for cell in &buffer.content {
            assert_eq!(cell.bg, expected, "{context}: célula '{}'", cell.symbol());
        }
    }

    #[test]
    fn colored_theme_paints_every_cell_background() {
        for name in ["tokyo-night", "solarized-light"] {
            let state = themed_state(name);
            let expected = palette_color(name, |palette| palette.background);
            let buffer = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
            let accent = palette_color(name, |palette| palette.accent);
            for cell in &buffer.content {
                assert!(
                    cell.bg == expected || cell.bg == accent,
                    "{name}: {:?}",
                    cell.bg
                );
            }
            assert!(buffer.content.iter().any(|cell| cell.bg == expected));
        }
    }

    #[test]
    fn popup_cells_keep_the_theme_background() {
        let mut state = themed_state("tokyo-night");
        let expected = palette_color("tokyo-night", |palette| palette.background);
        let accent = palette_color("tokyo-night", |palette| palette.accent);
        press(&mut state, &[KeyInput::Character('?')]);
        let help = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
        for cell in &help.content {
            assert!(
                cell.bg == expected || cell.bg == accent,
                "ajuda: {:?}",
                cell.bg
            );
        }
        press(&mut state, &[KeyInput::Escape]);
        state.show_alert(crate::application::ports::AlertMessage {
            title: "Aviso".to_string(),
            body: "texto do aviso".to_string(),
        });
        let alert = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
        assert!(draw(&state, THEME_WIDTH, THEME_HEIGHT)
            .iter()
            .any(|row| row.contains("texto do aviso")));
        for cell in &alert.content {
            assert!(
                cell.bg == expected || cell.bg == accent,
                "alerta: {:?}",
                cell.bg
            );
        }
    }

    #[test]
    fn notice_row_keeps_the_theme_background() {
        let mut state = themed_state("dracula");
        state.show_notice("Cartão movido".to_string());
        let expected = palette_color("dracula", |palette| palette.background);
        let buffer = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
        let last_row = THEME_HEIGHT - 1;
        for column in 0..THEME_WIDTH {
            assert_eq!(buffer[(column, last_row)].bg, expected, "coluna {column}");
        }
    }

    #[test]
    fn paint_background_false_leaves_the_terminal_background() {
        let mut state = state_with(vec![kanban_entry()]);
        let theme = crate::domain::theme::Theme::preset("nord")
            .unwrap()
            .without_background();
        state.set_theme_styles(ThemeStyles::new(
            &theme,
            crate::infrastructure::terminal_capabilities::ColorDepth::TrueColor,
        ));
        let accent = palette_color("nord", |palette| palette.accent);
        let buffer = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
        for cell in &buffer.content {
            assert!(
                cell.bg == Color::Reset || cell.bg == accent,
                "{:?}",
                cell.bg
            );
        }
    }

    #[test]
    fn ansi_theme_paints_nothing_besides_the_active_tab() {
        let state = state_with(vec![kanban_entry()]);
        let buffer = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
        for cell in &buffer.content {
            assert!(
                cell.bg == Color::Reset || cell.bg == Color::Cyan,
                "{:?}",
                cell.bg
            );
        }
        assert_every_cell_has_background_except_pill(&buffer);
    }

    fn assert_every_cell_has_background_except_pill(buffer: &Buffer) {
        let painted = buffer
            .content
            .iter()
            .filter(|cell| cell.bg == Color::Cyan)
            .count();
        assert!(
            painted > 0 && painted < 20,
            "só a aba ativa tem fundo: {painted}"
        );
    }

    #[test]
    fn text_cells_use_the_theme_foreground_by_default() {
        let record = ready(snapshot("Teste", vec![plain_line("texto normal")]));
        let mut state = state_with(vec![entry("w", record)]);
        let theme = crate::domain::theme::Theme::preset("tokyo-night").unwrap();
        state.set_theme_styles(ThemeStyles::new(
            &theme,
            crate::infrastructure::terminal_capabilities::ColorDepth::TrueColor,
        ));
        let buffer = buffer_of(&state, THEME_WIDTH, THEME_HEIGHT, now());
        let style = style_of_text(&buffer, "texto normal").unwrap();
        assert_eq!(
            style.fg,
            Some(palette_color("tokyo-night", |palette| palette.foreground))
        );
        assert_eq!(
            style.bg,
            Some(palette_color("tokyo-night", |palette| palette.background))
        );
    }

    #[test]
    fn render_never_panics_on_zero_sized_frames_for_every_preset() {
        for name in crate::domain::theme::preset_names() {
            let mut state = themed_state_or_ansi(name);
            for (width, height) in [(0, 0), (1, 1), (3, 2), (0, 10), (59, 19), (100, 30)] {
                assert_eq!(
                    draw(&state, width, height).len(),
                    usize::from(height),
                    "{name}"
                );
            }
            press(&mut state, &[KeyInput::Enter, KeyInput::Character('?')]);
            for (width, height) in [(0, 0), (1, 1), (59, 19)] {
                assert_eq!(
                    draw(&state, width, height).len(),
                    usize::from(height),
                    "{name}"
                );
            }
        }
    }

    fn themed_state_or_ansi(name: &str) -> AppState {
        if name == crate::domain::theme::DEFAULT_THEME_NAME {
            state_with(vec![kanban_entry()])
        } else {
            themed_state(name)
        }
    }
}
