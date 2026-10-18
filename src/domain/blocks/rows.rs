use crate::domain::widget::Flag;
use crate::domain::widget_content::Tone;

pub const MAXIMUM_TAGS_PER_ROW: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub time: Option<String>,
    pub text: String,
    pub tags: Vec<Tag>,
    pub tone: Tone,
    pub marker: Option<char>,
    pub flag: Option<Flag>,
}
