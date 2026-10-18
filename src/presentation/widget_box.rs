use chrono::{NaiveDateTime, TimeDelta};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::domain::layout::{visible_item_capacity, MINIMUM_BOX_HEIGHT};
use crate::domain::widget::WidgetRecord;
use crate::presentation::app_state::WidgetEntry;
use crate::presentation::text_fit::{display_width, truncate_to_width};
use crate::presentation::widget_view::render_blocks;

const LOADING_TEXT: &str = "carregando…";
const FAILURE_PREFIX: &str = "⚠ falha: ";
pub const FAILURE_MARK: &str = "⚠ ";
pub const FAILURE_COLOR: Color = Color::Red;
const SELECTED_BORDER_COLOR: Color = Color::Cyan;
const IDLE_BORDER_COLOR: Color = Color::DarkGray;
const MINIMUM_BOX_WIDTH: u16 = 3;
pub const BORDER_COLUMNS: u16 = 2;
const REFRESH_INDICATOR: &str = " ⟳";
const OVERFLOW_PREFIX: &str = "+";
const OVERFLOW_SUFFIX: &str = " itens";
const OVERFLOW_COLOR: Color = Color::DarkGray;
const STALE_AGE_PREFIX: &str = "sem atualizar há ";
const MINUTES_PER_HOUR: i64 = 60;
const WARNING_COLOR: Color = Color::Yellow;

pub struct BoxContext<'a> {
    pub shortcut: Option<char>,
    pub selected: bool,
    pub now: NaiveDateTime,
    pub warning: Option<&'a str>,
}

pub fn render_widget_box(frame: &mut Frame, entry: &WidgetEntry, area: Rect, context: &BoxContext) {
    if area.width < MINIMUM_BOX_WIDTH || area.height < MINIMUM_BOX_HEIGHT {
        return;
    }
    let record = &entry.record;
    let inner_width = area.width.saturating_sub(BORDER_COLUMNS);
    let title = box_title(
        context.shortcut,
        &entry_title(entry),
        record.is_refreshing(),
        usize::from(inner_width),
    );
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color(record, context.selected)));
    let inner = block.inner(area);
    let width = usize::from(inner.width);
    let mut lines = warning_lines(context.warning, width);
    lines.extend(box_body_lines(entry, width, context.now, context.selected));
    let body = clip_with_overflow(lines, visible_item_capacity(area.height, entry.max_items));
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(body).style(body_style(entry, context.now)),
        inner,
    );
}

pub fn entry_title(entry: &WidgetEntry) -> String {
    match entry.record.snapshot() {
        Some(snapshot) => snapshot.summary.title.clone(),
        None => entry.id.as_str().to_string(),
    }
}

pub fn border_color(record: &WidgetRecord, selected: bool) -> Color {
    if record.is_failing() {
        FAILURE_COLOR
    } else if selected {
        SELECTED_BORDER_COLOR
    } else {
        IDLE_BORDER_COLOR
    }
}

pub fn box_body_lines(
    entry: &WidgetEntry,
    width: usize,
    now: NaiveDateTime,
    focused: bool,
) -> Vec<Line<'static>> {
    let record = &entry.record;
    match (record.snapshot(), record.last_error()) {
        (None, Some(message)) => message
            .lines()
            .map(|line| failure_line(line, width))
            .collect(),
        (None, None) => vec![Line::from(LOADING_TEXT)],
        (Some(snapshot), _) => {
            let mut lines = failure_header(record, width, now, focused);
            lines.extend(render_blocks(&snapshot.summary.display_blocks(), width));
            lines
        }
    }
}

fn failure_header(
    record: &WidgetRecord,
    width: usize,
    now: NaiveDateTime,
    focused: bool,
) -> Vec<Line<'static>> {
    let Some(message) = record.last_error() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if focused {
        let reason = format!("{FAILURE_PREFIX}{message}");
        lines.extend(reason.lines().map(|line| failure_line(line, width)));
    }
    if let Some(age) = record.age(now) {
        let text = format!("{STALE_AGE_PREFIX}{}", age_text(age));
        lines.push(failure_line(&text, width));
    }
    lines
}

pub fn age_text(age: TimeDelta) -> String {
    let minutes = age.num_minutes();
    if minutes < MINUTES_PER_HOUR {
        format!("{minutes} min")
    } else {
        format!("{} h", minutes / MINUTES_PER_HOUR)
    }
}

fn failure_line(text: &str, width: usize) -> Line<'static> {
    Line::styled(
        truncate_to_width(text, width),
        Style::default().fg(FAILURE_COLOR),
    )
}

fn warning_lines(warning: Option<&str>, width: usize) -> Vec<Line<'static>> {
    warning
        .map(|text| {
            Line::styled(
                truncate_to_width(text, width),
                Style::default().fg(WARNING_COLOR),
            )
        })
        .into_iter()
        .collect()
}

fn body_style(entry: &WidgetEntry, now: NaiveDateTime) -> Style {
    let interval = TimeDelta::from_std(entry.interval).unwrap_or(TimeDelta::MAX);
    if entry.record.is_stale(now, interval) {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default()
    }
}

pub fn box_title(shortcut: Option<char>, title: &str, refreshing: bool, width: usize) -> String {
    let prefix = shortcut
        .map(|letter| format!("[{letter}] "))
        .unwrap_or_default();
    let suffix = if refreshing { REFRESH_INDICATOR } else { "" };
    let body_width = width.saturating_sub(display_width(&prefix) + display_width(suffix));
    let body = truncate_to_width(title, body_width);
    truncate_to_width(&format!("{prefix}{body}{suffix}"), width)
}

pub fn clip_with_overflow(lines: Vec<Line<'static>>, capacity: usize) -> Vec<Line<'static>> {
    if capacity == 0 {
        return Vec::new();
    }
    if lines.len() <= capacity {
        return lines;
    }
    let kept = capacity.saturating_sub(1);
    let omitted = lines.len().saturating_sub(kept);
    let mut clipped: Vec<Line<'static>> = lines.into_iter().take(kept).collect();
    clipped.push(overflow_line(omitted));
    clipped
}

fn overflow_line(omitted: usize) -> Line<'static> {
    let text = format!("{OVERFLOW_PREFIX}{omitted}{OVERFLOW_SUFFIX}");
    Line::from(Span::styled(text, Style::default().fg(OVERFLOW_COLOR)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use ratatui::text::Line;
    use unicode_width::UnicodeWidthStr;

    fn line(text: &str) -> Line<'static> {
        Line::from(text.to_string())
    }

    fn line_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn overflow_shows_plus_n_items_on_last_line() {
        let lines: Vec<Line<'static>> = (1..=10)
            .map(|number| line(&format!("linha {number}")))
            .collect();
        let clipped: Vec<String> = clip_with_overflow(lines, 4).iter().map(line_text).collect();
        assert_eq!(clipped, vec!["linha 1", "linha 2", "linha 3", "+7 itens"]);
    }

    #[test]
    fn lines_within_capacity_are_kept() {
        let lines = vec![line("a"), line("b")];
        assert_eq!(clip_with_overflow(lines.clone(), 2), lines);
    }

    #[test]
    fn capacity_zero_renders_nothing() {
        assert!(clip_with_overflow(vec![line("a"), line("b")], 0).is_empty());
    }

    #[test]
    fn title_has_shortcut_prefix_and_truncates_by_display_width() {
        assert_eq!(box_title(Some('A'), "Kanban", false, 20), "[A] Kanban");
        assert_eq!(box_title(None, "Kanban", false, 20), "Kanban");
        assert_eq!(
            box_title(Some('B'), "日本語 📋 tarefas", false, 9),
            "[B] 日本…"
        );
        assert_eq!(
            box_title(Some('B'), "日本語 📋 tarefas", true, 9),
            "[B] 日… ⟳"
        );
    }

    #[test]
    fn title_never_exceeds_width() {
        let titles = ["Kanban", "日本語 📋 tarefas", "🍅🍅🍅 agenda longa"];
        for width in 0..=40 {
            for title in titles {
                for shortcut in [None, Some('A')] {
                    for refreshing in [false, true] {
                        let rendered = box_title(shortcut, title, refreshing, width);
                        assert!(rendered.width() <= width, "{rendered:?} em largura {width}");
                    }
                }
            }
        }
    }

    #[test]
    fn age_text_uses_minutes_then_hours() {
        assert_eq!(age_text(TimeDelta::minutes(45)), "45 min");
        assert_eq!(age_text(TimeDelta::minutes(125)), "2 h");
    }
}
