use crate::domain::card::{Alert, Block, CardLine, Tone};

pub const FLAG_NAMES: [&str; 2] = ["updated", "warning"];

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
            .map(|line| CardLine {
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
pub struct WidgetSnapshot {
    pub summary: Summary,
    pub detail: Option<Vec<DetailSection>>,
    pub actions: Vec<ActionSpec>,
    pub alerts: Vec<Alert>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WidgetState {
    Loading,
    Ready(WidgetSnapshot),
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::card::{Block, CardLine, Tone};

    fn card_line(text: &str) -> CardLine {
        CardLine {
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
    fn display_blocks_prefers_blocks_over_lines() {
        let blocks = vec![Block::Lines(vec![card_line("novo")])];
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
        let expected = Block::Lines(vec![CardLine {
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

    #[test]
    fn widget_id_exposes_inner_text() {
        assert_eq!(WidgetId::new("kanban").as_str(), "kanban");
    }
}
