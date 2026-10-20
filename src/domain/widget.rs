use std::fmt::Display;

use chrono::{NaiveDateTime, TimeDelta};

use crate::domain::blocks::{lines::WidgetLine, Block};
use crate::domain::widget_content::{Alert, Tone};

pub const FLAG_NAMES: [&str; 2] = ["updated", "warning"];
pub const MAXIMUM_MINI_VALUE_CHARACTERS: usize = 8;
pub const MAXIMUM_MINI_TEXT_CHARACTERS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WidgetId(String);

impl WidgetId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Updated,
    Warning,
}

impl Flag {
    pub fn symbol(self) -> char {
        match self {
            Flag::Updated => '●',
            Flag::Warning => '⚠',
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryLine {
    pub text: String,
    pub flag: Option<Flag>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub title: String,
    pub lines: Vec<SummaryLine>,
    pub blocks: Vec<Block>,
}

impl Summary {
    pub fn put_notice_first(&mut self, text: String, tone: Tone, flag: Option<Flag>) {
        if self.blocks.is_empty() {
            self.lines.insert(0, SummaryLine { text, flag });
        } else {
            let line = WidgetLine { text, tone, flag };
            self.blocks.insert(0, Block::Lines(vec![line]));
        }
    }

    pub fn display_blocks(&self) -> Vec<Block> {
        if !self.blocks.is_empty() {
            return self.blocks.clone();
        }
        if self.lines.is_empty() {
            return vec![];
        }
        let lines = self
            .lines
            .iter()
            .map(|line| WidgetLine {
                text: line.text.clone(),
                tone: Tone::Normal,
                flag: line.flag,
            })
            .collect();
        vec![Block::Lines(lines)]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailItem {
    pub id: Option<String>,
    pub text: String,
    pub flag: Option<Flag>,
    pub meta: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailSection {
    pub heading: String,
    pub items: Vec<DetailItem>,
    pub blocks: Vec<Block>,
}

impl DetailSection {
    pub fn display_blocks(&self) -> Vec<Block> {
        self.blocks.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionScope {
    Widget,
    Item,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionSpec {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub scope: ActionScope,
    pub params_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mini {
    pub value: String,
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSnapshot {
    pub summary: Summary,
    pub detail: Option<Vec<DetailSection>>,
    pub actions: Vec<ActionSpec>,
    pub alerts: Vec<Alert>,
    pub mini: Option<Mini>,
}

impl WidgetSnapshot {
    pub fn detail_items(&self) -> impl Iterator<Item = &DetailItem> {
        self.detail
            .iter()
            .flatten()
            .flat_map(|section| section.items.iter())
    }

    pub fn items_in_sections<'a>(
        &'a self,
        headings: &'a [String],
    ) -> impl Iterator<Item = &'a DetailItem> {
        self.detail
            .iter()
            .flatten()
            .filter(move |section| headings.contains(&section.heading))
            .flat_map(|section| section.items.iter())
    }
}

pub const STALE_AFTER_INTERVALS: u32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetRecord {
    snapshot: Option<WidgetSnapshot>,
    last_success: Option<NaiveDateTime>,
    last_error: Option<String>,
    refreshing: bool,
}

impl WidgetRecord {
    pub fn loading() -> Self {
        Self {
            snapshot: None,
            last_success: None,
            last_error: None,
            refreshing: false,
        }
    }

    pub fn apply<E: Display>(&mut self, result: Result<WidgetSnapshot, E>, now: NaiveDateTime) {
        self.refreshing = false;
        match result {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.last_success = Some(now);
                self.last_error = None;
            }
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub fn begin_refresh(&mut self) {
        self.refreshing = true;
    }

    pub fn snapshot(&self) -> Option<&WidgetSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn usable_snapshot(&self) -> Option<&WidgetSnapshot> {
        if self.is_failing() {
            None
        } else {
            self.snapshot()
        }
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn is_failing(&self) -> bool {
        self.last_error.is_some()
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing
    }

    pub fn age(&self, now: NaiveDateTime) -> Option<TimeDelta> {
        self.last_success
            .map(|at| (now - at).max(TimeDelta::zero()))
    }

    pub fn is_stale(&self, now: NaiveDateTime, interval: TimeDelta) -> bool {
        let Some(age) = self.age(now) else {
            return false;
        };
        let limit_milliseconds = interval
            .num_milliseconds()
            .saturating_mul(i64::from(STALE_AFTER_INTERVALS));
        age.num_milliseconds() > limit_milliseconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::blocks::{lines::WidgetLine, Block};
    use crate::domain::widget_content::Tone;
    use chrono::{NaiveDate, NaiveDateTime, TimeDelta};

    fn widget_line(text: &str) -> WidgetLine {
        WidgetLine {
            text: text.to_string(),
            tone: Tone::Ok,
            flag: None,
        }
    }

    fn summary(lines: Vec<SummaryLine>, blocks: Vec<Block>) -> Summary {
        Summary {
            title: "T".to_string(),
            lines,
            blocks,
        }
    }

    #[test]
    fn notice_goes_to_lines_when_there_are_no_blocks() {
        let mut target = summary(vec![], vec![]);
        target.put_notice_first("aviso".to_string(), Tone::Muted, None);
        assert_eq!(target.lines[0].text, "aviso");
        assert!(target.blocks.is_empty());
    }

    #[test]
    fn notice_becomes_the_first_block_when_blocks_exist() {
        let mut target = summary(vec![], vec![Block::Lines(vec![widget_line("x")])]);
        target.put_notice_first("aviso".to_string(), Tone::Warn, Some(Flag::Warning));
        let expected = Block::Lines(vec![WidgetLine {
            text: "aviso".to_string(),
            tone: Tone::Warn,
            flag: Some(Flag::Warning),
        }]);
        assert_eq!(target.blocks[0], expected);
        assert!(target.lines.is_empty());
    }

    #[test]
    fn display_blocks_prefers_blocks_over_lines() {
        let blocks = vec![Block::Lines(vec![widget_line("novo")])];
        let legacy = vec![SummaryLine {
            text: "velho".to_string(),
            flag: None,
        }];
        assert_eq!(summary(legacy, blocks.clone()).display_blocks(), blocks);
    }

    #[test]
    fn display_blocks_converts_legacy_lines() {
        let legacy = vec![SummaryLine {
            text: "velho".to_string(),
            flag: Some(Flag::Warning),
        }];
        let expected = Block::Lines(vec![WidgetLine {
            text: "velho".to_string(),
            tone: Tone::Normal,
            flag: Some(Flag::Warning),
        }]);
        assert_eq!(summary(legacy, vec![]).display_blocks(), vec![expected]);
    }

    #[test]
    fn display_blocks_is_empty_without_content() {
        assert!(summary(vec![], vec![]).display_blocks().is_empty());
        let section = DetailSection {
            heading: "h".to_string(),
            items: vec![],
            blocks: vec![],
        };
        assert!(section.display_blocks().is_empty());
    }

    #[test]
    fn flag_symbols_match_spec() {
        assert_eq!(Flag::Updated.symbol(), '●');
        assert_eq!(Flag::Warning.symbol(), '⚠');
    }

    fn base_time() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 18)
            .and_then(|date| date.and_hms_opt(12, 0, 0))
            .expect("data válida")
    }

    fn seconds_after(base: NaiveDateTime, seconds: i64) -> NaiveDateTime {
        base + TimeDelta::seconds(seconds)
    }

    fn succeed(title: &str) -> Result<WidgetSnapshot, String> {
        Ok(snapshot(title))
    }

    fn fail(message: &str) -> Result<WidgetSnapshot, String> {
        Err(message.to_string())
    }

    fn snapshot(title: &str) -> WidgetSnapshot {
        WidgetSnapshot {
            summary: Summary {
                title: title.to_string(),
                lines: vec![],
                blocks: vec![],
            },
            detail: None,
            actions: vec![],
            alerts: vec![],
            mini: None,
        }
    }

    #[test]
    fn success_stores_snapshot_and_clears_error() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(fail("falha"), now);
        record.apply(succeed("Kanban"), seconds_after(now, 1));
        assert_eq!(record.snapshot(), Some(&snapshot("Kanban")));
        assert_eq!(record.last_error(), None);
        assert!(!record.is_failing());
    }

    #[test]
    fn failure_after_success_keeps_last_good_snapshot_and_records_error() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(succeed("Kanban"), now);
        record.apply(fail("fora do ar"), seconds_after(now, 1));
        assert_eq!(record.snapshot(), Some(&snapshot("Kanban")));
        assert_eq!(record.last_error(), Some("fora do ar"));
        assert!(record.is_failing());
    }

    #[test]
    fn first_load_failure_has_no_snapshot_and_has_error() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(fail("fora do ar"), now);
        assert_eq!(record.snapshot(), None);
        assert_eq!(record.last_error(), Some("fora do ar"));
        assert_eq!(record.age(seconds_after(now, 10)), None);
    }

    #[test]
    fn age_is_none_before_first_success() {
        let record = WidgetRecord::loading();
        assert_eq!(record.age(base_time()), None);
    }

    #[test]
    fn age_saturates_to_zero_when_clock_goes_backwards() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(succeed("Kanban"), now);
        assert_eq!(record.age(seconds_after(now, -10)), Some(TimeDelta::zero()));
    }

    #[test]
    fn is_stale_only_after_five_intervals_even_without_failure() {
        let now = base_time();
        let interval = TimeDelta::seconds(30);
        let mut record = WidgetRecord::loading();
        record.apply(succeed("Kanban"), now);
        let limit = seconds_after(now, 150);
        assert!(!record.is_stale(limit, interval));
        assert!(record.is_stale(limit + TimeDelta::milliseconds(1), interval));
        assert!(!record.is_failing());
    }

    #[test]
    fn age_keeps_counting_from_last_success_after_failure() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(succeed("Kanban"), now);
        record.apply(fail("fora do ar"), seconds_after(now, 1));
        assert_eq!(
            record.age(seconds_after(now, 5)),
            Some(TimeDelta::seconds(5))
        );
    }

    #[test]
    fn failed_apply_clears_refreshing_flag() {
        let mut record = WidgetRecord::loading();
        record.begin_refresh();
        record.apply(fail("fora do ar"), base_time());
        assert!(!record.is_refreshing());
    }

    #[test]
    fn zero_interval_makes_any_positive_age_stale() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(succeed("Kanban"), now);
        assert!(!record.is_stale(now, TimeDelta::zero()));
        assert!(record.is_stale(seconds_after(now, 1), TimeDelta::zero()));
    }

    #[test]
    fn begin_refresh_sets_flag_and_apply_clears_it() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.begin_refresh();
        assert!(record.is_refreshing());
        record.apply(succeed("Kanban"), now);
        assert!(!record.is_refreshing());
    }

    #[test]
    fn persistent_failure_keeps_failing_until_a_success() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        for second in 0..3 {
            record.apply(fail("falha"), seconds_after(now, second));
        }
        assert!(record.is_failing());
        record.apply(succeed("Kanban"), seconds_after(now, 3));
        assert!(!record.is_failing());
    }

    #[test]
    fn huge_interval_does_not_overflow_the_staleness_limit() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        record.apply(succeed("Kanban"), now);
        assert!(!record.is_stale(seconds_after(now, 3600), TimeDelta::MAX));
    }

    #[test]
    fn widget_id_exposes_inner_text() {
        assert_eq!(WidgetId::new("kanban").as_str(), "kanban");
    }

    fn detail_snapshot() -> WidgetSnapshot {
        let item = |id: &str| DetailItem {
            id: Some(id.to_string()),
            text: id.to_string(),
            flag: None,
            meta: vec![],
        };
        WidgetSnapshot {
            detail: Some(vec![
                DetailSection {
                    heading: "inbox".to_string(),
                    items: vec![item("a"), item("b")],
                    blocks: vec![],
                },
                DetailSection {
                    heading: "done".to_string(),
                    items: vec![item("c")],
                    blocks: vec![],
                },
            ]),
            ..snapshot("T")
        }
    }

    #[test]
    fn usable_snapshot_is_none_while_loading_or_failing_and_some_when_loaded() {
        let now = base_time();
        let mut record = WidgetRecord::loading();
        assert_eq!(record.usable_snapshot(), None);

        record.apply(succeed("Kanban"), now);
        assert_eq!(record.usable_snapshot(), Some(&snapshot("Kanban")));

        record.apply(fail("falha"), seconds_after(now, 1));
        assert_eq!(record.usable_snapshot(), None);
        assert!(record.snapshot().is_some());
    }

    #[test]
    fn detail_items_flattens_every_section_in_order() {
        let ids: Vec<String> = detail_snapshot()
            .detail_items()
            .map(|item| item.text.clone())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
        assert_eq!(snapshot("T").detail_items().count(), 0);
    }

    #[test]
    fn items_in_sections_keeps_only_the_named_headings() {
        let headings = vec!["done".to_string()];
        let snapshot = detail_snapshot();
        let ids: Vec<&str> = snapshot
            .items_in_sections(&headings)
            .map(|item| item.text.as_str())
            .collect();
        assert_eq!(ids, vec!["c"]);
    }
}
