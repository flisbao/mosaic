#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use chrono::{NaiveDate, NaiveDateTime};
use mosaic::domain::blocks::Block;
use mosaic::domain::widget::WidgetSnapshot;

pub fn fixed_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 18)
        .and_then(|date| date.and_hms_opt(12, 0, 0))
        .expect("data válida")
}

pub fn morning_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 17)
        .and_then(|date| date.and_hms_opt(8, 30, 0))
        .expect("data válida")
}

pub fn summary_line_texts(snapshot: &WidgetSnapshot) -> Vec<String> {
    snapshot
        .summary
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Lines(lines) => Some(lines),
            _ => None,
        })
        .flatten()
        .map(|line| line.text.clone())
        .collect()
}
