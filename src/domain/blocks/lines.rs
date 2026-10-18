use crate::domain::widget::Flag;
use crate::domain::widget_content::Tone;

pub const MAXIMUM_LINES_PER_BLOCK: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetLine {
    pub text: String,
    pub tone: Tone,
    pub flag: Option<Flag>,
}
