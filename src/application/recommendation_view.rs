use crate::application::ports::StoredRecommendation;
use crate::domain::widget::{DetailItem, DetailSection, Summary, SummaryLine, WidgetSnapshot};
use crate::domain::widget_content::clean_text_limited;

pub const SUMMARY_LINE_LIMIT: usize = 8;
const RECOMMENDATION_LINE_MAXIMUM_CHARACTERS: usize = 300;

const RECOMMENDATION_TITLE: &str = "IA";
const IDLE_LINE: &str = "Pressione i para pedir uma recomendação";
const IDLE_HEADING: &str = "Recomendação";

pub fn idle_snapshot() -> WidgetSnapshot {
    snapshot(
        vec![IDLE_LINE.to_string()],
        IDLE_HEADING.to_string(),
        vec![IDLE_LINE.to_string()],
    )
}

pub fn recommendation_snapshot(stored: &StoredRecommendation) -> WidgetSnapshot {
    let lines: Vec<String> = stored
        .text
        .lines()
        .map(|line| clean_text_limited(line, RECOMMENDATION_LINE_MAXIMUM_CHARACTERS))
        .filter(|line| !line.is_empty())
        .collect();
    let summary_lines = lines.iter().take(SUMMARY_LINE_LIMIT).cloned().collect();
    let heading = format!(
        "Recomendação de {}",
        stored.created_at.format("%d/%m %H:%M")
    );
    snapshot(summary_lines, heading, lines)
}

fn snapshot(
    summary_lines: Vec<String>,
    heading: String,
    detail_lines: Vec<String>,
) -> WidgetSnapshot {
    WidgetSnapshot {
        mini: None,
        alerts: vec![],
        summary: Summary {
            blocks: vec![],
            title: RECOMMENDATION_TITLE.to_string(),
            lines: summary_lines
                .into_iter()
                .map(|text| SummaryLine { text, flag: None })
                .collect(),
        },
        detail: Some(vec![DetailSection {
            blocks: vec![],
            heading,
            items: detail_lines
                .into_iter()
                .map(|text| DetailItem {
                    id: None,
                    text,
                    flag: None,
                    meta: vec![],
                })
                .collect(),
        }]),
        actions: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn stored(text: &str) -> StoredRecommendation {
        StoredRecommendation {
            text: text.to_string(),
            created_at: NaiveDate::from_ymd_opt(2026, 10, 17)
                .unwrap()
                .and_hms_opt(8, 30, 0)
                .unwrap(),
        }
    }

    #[test]
    fn idle_snapshot_invites_the_user_to_press_i() {
        let snapshot = idle_snapshot();
        assert_eq!(snapshot.summary.title, "IA");
        assert!(snapshot.summary.lines[0].text.contains("Pressione i"));
        assert!(snapshot.detail.is_some());
        assert!(snapshot.actions.is_empty());
    }

    #[test]
    fn recommendation_snapshot_limits_summary_lines_and_keeps_all_detail_lines() {
        let text = (1..=12)
            .map(|number| format!("linha {number}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snapshot = recommendation_snapshot(&stored(&text));
        assert_eq!(snapshot.summary.lines.len(), SUMMARY_LINE_LIMIT);
        let sections = snapshot.detail.unwrap();
        assert_eq!(sections[0].heading, "Recomendação de 17/10 08:30");
        assert_eq!(sections[0].items.len(), 12);
        assert!(sections[0].items.iter().all(|item| item.id.is_none()));
    }

    #[test]
    fn recommendation_snapshot_removes_escape_and_bidi_characters() {
        let snapshot = recommendation_snapshot(&stored("1. A\u{202e}B\u{1b}[31m\n\u{200b}\n2. C"));
        let lines: Vec<&str> = snapshot
            .summary
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(lines, vec!["1. AB[31m", "2. C"]);
    }

    #[test]
    fn recommendation_snapshot_truncates_very_long_lines() {
        let long = "x".repeat(RECOMMENDATION_LINE_MAXIMUM_CHARACTERS + 100);
        let snapshot = recommendation_snapshot(&stored(&long));
        assert_eq!(
            snapshot.summary.lines[0].text.chars().count(),
            RECOMMENDATION_LINE_MAXIMUM_CHARACTERS
        );
    }

    #[test]
    fn recommendation_snapshot_skips_blank_lines() {
        let snapshot = recommendation_snapshot(&stored("a\n\n   \nb\n"));
        let lines: Vec<&str> = snapshot
            .summary
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(lines, vec!["a", "b"]);
    }
}
