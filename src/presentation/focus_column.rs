use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::domain::widget_content::Tone;
use crate::presentation::app_state::WidgetEntry;
use crate::presentation::text_fit::{tone_style, truncate_to_width};
use crate::presentation::widget_box::{entry_title, FAILURE_COLOR, FAILURE_MARK};

const ITEM_COUNT_SUFFIX: &str = " itens";
const VALUE_SEPARATOR: &str = " ";

pub fn focus_column_lines(
    entries: &[&WidgetEntry],
    focused: Option<usize>,
    width: usize,
) -> Vec<Line<'static>> {
    entries
        .iter()
        .enumerate()
        .map(|(position, entry)| focus_row(entry, Some(position) == focused, width))
        .collect()
}

fn focus_row(entry: &WidgetEntry, focused: bool, width: usize) -> Line<'static> {
    let mark = if entry.record.is_failing() {
        FAILURE_MARK
    } else {
        ""
    };
    let text = format!(
        "{mark}{}{VALUE_SEPARATOR}{}",
        entry_title(entry),
        summary_value(entry)
    );
    Line::styled(truncate_to_width(&text, width), row_style(entry, focused))
}

fn summary_value(entry: &WidgetEntry) -> String {
    match entry
        .record
        .snapshot()
        .and_then(|snapshot| snapshot.mini.as_ref())
    {
        Some(mini) => format!("{}{VALUE_SEPARATOR}{}", mini.value, mini.text),
        None => format!("{}{ITEM_COUNT_SUFFIX}", displayed_item_count(entry)),
    }
}

fn displayed_item_count(entry: &WidgetEntry) -> usize {
    let total: usize = entry
        .record
        .snapshot()
        .and_then(|snapshot| snapshot.detail.as_ref())
        .map_or(0, |sections| {
            sections.iter().map(|section| section.items.len()).sum()
        });
    entry.max_items.map_or(total, |limit| total.min(limit))
}

fn row_style(entry: &WidgetEntry, focused: bool) -> Style {
    if focused {
        return Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
    }
    if entry.record.is_failing() {
        return Style::default().fg(FAILURE_COLOR);
    }
    let tone = entry
        .record
        .snapshot()
        .and_then(|snapshot| snapshot.mini.as_ref())
        .map_or(Tone::Normal, |mini| mini.tone);
    tone_style(tone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget::{Mini, WidgetId, WidgetRecord, WidgetSnapshot};
    use crate::domain::widget_content::Tone;
    use ratatui::style::{Color, Modifier};

    fn titled_snapshot(title: &str) -> WidgetSnapshot {
        WidgetSnapshot::titled(title)
    }

    fn with_items(mut snapshot: WidgetSnapshot, item_count: usize) -> WidgetSnapshot {
        let items = (0..item_count)
            .map(|number| crate::domain::widget::DetailItem {
                id: None,
                text: format!("item {number}"),
                flag: None,
                meta: vec![],
            })
            .collect();
        snapshot.detail = Some(vec![crate::domain::widget::DetailSection {
            blocks: vec![],
            heading: "seção".to_string(),
            items,
        }]);
        snapshot
    }

    fn ready(snapshot: WidgetSnapshot) -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(
            Ok(snapshot),
            chrono::NaiveDate::from_ymd_opt(2026, 10, 18)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        );
        record
    }

    fn failed() -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(
            Err("fora do ar".to_string()),
            chrono::NaiveDate::from_ymd_opt(2026, 10, 18)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        );
        record
    }

    fn entry(id: &str, record: WidgetRecord, max_items: Option<usize>) -> WidgetEntry {
        WidgetEntry {
            id: WidgetId::new(id),
            record,
            interval: std::time::Duration::from_secs(30),
            max_items,
        }
    }

    fn line_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn focus_column_uses_mini_or_item_count() {
        let mut agenda = titled_snapshot("Agenda");
        agenda.mini = Some(Mini {
            value: "3".to_string(),
            text: "reuniões".to_string(),
            tone: Tone::Ok,
        });
        let agenda = entry("agenda", ready(agenda), None);
        let kanban = entry(
            "kanban",
            ready(with_items(titled_snapshot("Kanban"), 2)),
            None,
        );
        let capped = entry(
            "notas",
            ready(with_items(titled_snapshot("Notas"), 5)),
            Some(1),
        );
        let entries = [&agenda, &kanban, &capped];
        let rows: Vec<String> = focus_column_lines(&entries, Some(0), 40)
            .iter()
            .map(line_text)
            .collect();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].contains("3 reuniões"), "{rows:?}");
        assert!(rows[1].contains("2 itens"), "{rows:?}");
        assert!(rows[2].contains("1 itens"), "{rows:?}");
    }

    #[test]
    fn focus_column_marks_failing_widget() {
        let failing = entry("kanban", failed(), None);
        let healthy = entry("agenda", ready(titled_snapshot("Agenda")), None);
        let entries = [&healthy, &failing];
        let lines = focus_column_lines(&entries, Some(0), 40);
        let failing_line = &lines[1];
        assert!(line_text(failing_line).starts_with('⚠'), "{failing_line:?}");
        assert_eq!(failing_line.style.fg, Some(Color::Red));
        let focused_line = &lines[0];
        assert!(focused_line.style.add_modifier.contains(Modifier::BOLD));
    }
}
