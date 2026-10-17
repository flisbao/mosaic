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
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailItem {
    pub id: Option<String>,
    pub text: String,
    pub flag: Option<Flag>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailSection {
    pub heading: String,
    pub items: Vec<DetailItem>,
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
