use crate::domain::widget_content::Tone;

pub const MAXIMUM_STAT_ITEMS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct StatItem {
    pub value: String,
    pub label: String,
    pub tone: Tone,
}
