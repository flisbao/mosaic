use crate::domain::widget::WidgetSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSnapshot {
    pub snapshot: WidgetSnapshot,
    pub problems: Vec<Problem>,
}
