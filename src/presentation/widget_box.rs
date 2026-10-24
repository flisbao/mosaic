use chrono::{NaiveDateTime, TimeDelta};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::domain::widget::WidgetRecord;
use crate::presentation::app_state::WidgetEntry;
use crate::presentation::grid_layout::{visible_item_capacity, MINIMUM_BOX_HEIGHT};
use crate::presentation::text_fit::{display_width, truncate_to_width};
use crate::presentation::theme_styles::ThemeStyles;
use crate::presentation::widget_view::render_blocks_limited;

const LOADING_TEXT: &str = "carregando…";
const FAILURE_PREFIX: &str = "⚠ falha: ";
pub const FAILURE_MARK: &str = "⚠ ";
const MINIMUM_BOX_WIDTH: u16 = 3;
const BORDER_COLUMNS: u16 = 2;
const REFRESH_INDICATOR: &str = " ⟳";
const OVERFLOW_PREFIX: &str = "+";
const OVERFLOW_SUFFIX: &str = " itens";
const STALE_AGE_PREFIX: &str = "sem atualizar há ";
const MINUTES_PER_HOUR: i64 = 60;
const HOURS_PER_DAY: i64 = 24;

pub struct BoxContext<'a> {
    pub shortcut: Option<char>,
    pub emphasis: Emphasis,
    pub now: NaiveDateTime,
    pub warning: Option<&'a str>,
    pub styles: &'a ThemeStyles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDetail {
    Hidden,
    Shown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Focused,
    Normal,
}

pub fn render_widget_box(frame: &mut Frame, entry: &WidgetEntry, area: Rect, context: &BoxContext) {
    if area.width < MINIMUM_BOX_WIDTH || area.height < MINIMUM_BOX_HEIGHT {
        return;
    }
    let block = box_block(
        entry,
        context.shortcut,
        context.emphasis,
        area.width,
        context.styles,
    );
    let inner = block.inner(area);
    let width = usize::from(inner.width);
    let mut header = warning_lines(context.warning, width, context.styles);
    header.extend(box_header(
        entry,
        width,
        context.now,
        FailureDetail::Hidden,
        context.styles,
    ));
    let header_rows = u16::try_from(header.len()).unwrap_or(u16::MAX);
    let capacity = visible_item_capacity(area.height.saturating_sub(header_rows), entry.max_items);
    let blocks = box_blocks(entry, width, capacity, context.styles);
    let mut lines = header;
    lines.extend(dim_if_stale(blocks, entry, context.now));
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn box_block(
    entry: &WidgetEntry,
    shortcut: Option<char>,
    emphasis: Emphasis,
    outer_width: u16,
    styles: &ThemeStyles,
) -> Block<'static> {
    let record = &entry.record;
    let title_width = usize::from(outer_width.saturating_sub(BORDER_COLUMNS));
    let title = box_title(
        shortcut,
        &entry_title(entry),
        record.is_refreshing(),
        title_width,
    );
    let weight = match emphasis {
        Emphasis::Focused => Modifier::BOLD,
        Emphasis::Normal => Modifier::empty(),
    };
    Block::bordered()
        .title(Line::styled(title, Style::default().add_modifier(weight)))
        .border_style(border_style(record, emphasis, styles).add_modifier(weight))
}

pub fn entry_title(entry: &WidgetEntry) -> String {
    match entry.record.snapshot() {
        Some(snapshot) => snapshot.summary.title.clone(),
        None => entry.id.as_str().to_string(),
    }
}

pub fn border_style(record: &WidgetRecord, emphasis: Emphasis, styles: &ThemeStyles) -> Style {
    if record.is_failing() {
        styles.danger()
    } else if emphasis == Emphasis::Focused {
        styles.accent()
    } else {
        styles.muted()
    }
}

pub fn box_header(
    entry: &WidgetEntry,
    width: usize,
    now: NaiveDateTime,
    detail: FailureDetail,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let record = &entry.record;
    match record.snapshot() {
        None => no_snapshot_lines(record, width, styles),
        Some(_) => failure_header(record, width, now, detail, styles),
    }
}

fn box_blocks(
    entry: &WidgetEntry,
    width: usize,
    capacity: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let Some(snapshot) = entry.record.snapshot() else {
        return Vec::new();
    };
    let limited = render_blocks_limited(&snapshot.summary.blocks, width, capacity, styles);
    clip_limited(limited.lines, limited.total, capacity, styles)
}

fn no_snapshot_lines(
    record: &WidgetRecord,
    width: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    match record.last_error() {
        Some(message) => message
            .lines()
            .map(|line| failure_line(line, width, styles))
            .collect(),
        None => vec![Line::from(LOADING_TEXT)],
    }
}

pub(crate) fn failure_header(
    record: &WidgetRecord,
    width: usize,
    now: NaiveDateTime,
    detail: FailureDetail,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let Some(message) = record.last_error() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if detail == FailureDetail::Shown {
        let reason = format!("{FAILURE_PREFIX}{message}");
        lines.extend(reason.lines().map(|line| failure_line(line, width, styles)));
    }
    if let Some(age) = record.age(now) {
        let text = format!("{STALE_AGE_PREFIX}{}", age_text(age));
        lines.push(failure_line(&text, width, styles));
    }
    lines
}

pub fn age_text(age: TimeDelta) -> String {
    let minutes = age.num_minutes();
    let hours = minutes / MINUTES_PER_HOUR;
    let days = hours / HOURS_PER_DAY;
    if minutes < MINUTES_PER_HOUR {
        format!("{minutes} min")
    } else if hours < HOURS_PER_DAY {
        format!("{hours} h")
    } else if days == 1 {
        "1 dia".to_string()
    } else {
        format!("{days} dias")
    }
}

pub fn dim_if_stale(
    lines: Vec<Line<'static>>,
    entry: &WidgetEntry,
    now: NaiveDateTime,
) -> Vec<Line<'static>> {
    let interval = TimeDelta::from_std(entry.interval).unwrap_or(TimeDelta::MAX);
    if !entry.record.is_stale(now, interval) {
        return lines;
    }
    let dim = Style::default().add_modifier(Modifier::DIM);
    lines
        .into_iter()
        .map(|line| line.patch_style(dim))
        .collect()
}

fn failure_line(text: &str, width: usize, styles: &ThemeStyles) -> Line<'static> {
    Line::styled(truncate_to_width(text, width), styles.danger())
}

fn warning_lines(warning: Option<&str>, width: usize, styles: &ThemeStyles) -> Vec<Line<'static>> {
    warning
        .map(|text| Line::styled(truncate_to_width(text, width), styles.warn()))
        .into_iter()
        .collect()
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

pub fn clip_with_overflow(
    lines: Vec<Line<'static>>,
    capacity: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let total = lines.len();
    clip_limited(lines, total, capacity, styles)
}

fn clip_limited(
    lines: Vec<Line<'static>>,
    total: usize,
    capacity: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    if capacity == 0 {
        return Vec::new();
    }
    if total <= capacity {
        return lines;
    }
    let kept = capacity.saturating_sub(1);
    let omitted = total.saturating_sub(kept);
    let mut clipped: Vec<Line<'static>> = lines.into_iter().take(kept).collect();
    clipped.push(overflow_line(omitted, styles));
    clipped
}

fn overflow_line(omitted: usize, styles: &ThemeStyles) -> Line<'static> {
    let text = format!("{OVERFLOW_PREFIX}{omitted}{OVERFLOW_SUFFIX}");
    Line::from(Span::styled(text, styles.muted()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::theme_styles::ThemeStyles;
    use crate::test_support::widget_box_now as now;
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
    fn failure_header_shows_the_reason_only_when_asked() {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(Err("quebrou".to_string()), now());

        let hidden = failure_header(
            &record,
            40,
            now(),
            FailureDetail::Hidden,
            &ThemeStyles::ansi(),
        );
        let shown = failure_header(
            &record,
            40,
            now(),
            FailureDetail::Shown,
            &ThemeStyles::ansi(),
        );

        assert!(hidden.is_empty());
        assert_eq!(shown.len(), 1);
        assert!(shown[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
            .contains("quebrou"));
    }

    #[test]
    fn border_style_depends_on_failure_then_emphasis() {
        let styles = ThemeStyles::ansi();
        let loading = WidgetRecord::loading();
        let mut failing = WidgetRecord::loading();
        failing.apply::<String>(Err("x".to_string()), now());

        assert_eq!(
            border_style(&failing, Emphasis::Focused, &styles),
            styles.danger()
        );
        assert_eq!(
            border_style(&loading, Emphasis::Focused, &styles),
            styles.accent()
        );
        assert_eq!(
            border_style(&loading, Emphasis::Normal, &styles),
            styles.muted()
        );
    }

    #[test]
    fn overflow_shows_plus_n_items_on_last_line() {
        let lines: Vec<Line<'static>> = (1..=10)
            .map(|number| line(&format!("linha {number}")))
            .collect();
        let clipped: Vec<String> = clip_with_overflow(lines, 4, &ThemeStyles::ansi())
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(clipped, vec!["linha 1", "linha 2", "linha 3", "+7 itens"]);
    }

    #[test]
    fn overflow_uses_the_total_when_only_a_prefix_was_rendered() {
        let prefix: Vec<Line<'static>> = (1..=4)
            .map(|number| line(&format!("linha {number}")))
            .collect();
        let clipped: Vec<String> = clip_limited(prefix, 10, 4, &ThemeStyles::ansi())
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(clipped, vec!["linha 1", "linha 2", "linha 3", "+7 itens"]);
    }

    #[test]
    fn lines_within_capacity_are_kept() {
        let lines = vec![line("a"), line("b")];
        assert_eq!(
            clip_with_overflow(lines.clone(), 2, &ThemeStyles::ansi()),
            lines
        );
    }

    #[test]
    fn capacity_zero_renders_nothing() {
        assert!(clip_with_overflow(vec![line("a"), line("b")], 0, &ThemeStyles::ansi()).is_empty());
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
    fn overflow_counts_exactly_the_omitted_lines() {
        let lines: Vec<Line<'static>> = (1..=5)
            .map(|number| line(&format!("linha {number}")))
            .collect();
        let clipped: Vec<String> = clip_with_overflow(lines, 4, &ThemeStyles::ansi())
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(clipped, vec!["linha 1", "linha 2", "linha 3", "+2 itens"]);
    }

    #[test]
    fn age_text_uses_minutes_hours_then_days() {
        assert_eq!(age_text(TimeDelta::minutes(0)), "0 min");
        assert_eq!(age_text(TimeDelta::minutes(59)), "59 min");
        assert_eq!(age_text(TimeDelta::minutes(60)), "1 h");
        assert_eq!(age_text(TimeDelta::minutes(125)), "2 h");
        assert_eq!(age_text(TimeDelta::hours(24)), "1 dia");
        assert_eq!(age_text(TimeDelta::hours(48)), "2 dias");
    }
}
