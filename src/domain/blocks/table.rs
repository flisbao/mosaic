use crate::domain::widget::Flag;
use crate::domain::widget_content::{Align, Tone};

pub const MAXIMUM_TABLE_COLUMNS: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub title: String,
    pub align: Align,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
    pub flag: Option<Flag>,
}
