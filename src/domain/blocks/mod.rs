pub mod lines;
pub mod rows;
pub mod stat;
pub mod table;

use lines::WidgetLine;
use rows::Row;
use stat::StatItem;
use table::{Cell, Column};

pub const BLOCK_TYPE_NAMES: [&str; 4] = ["lines", "rows", "stat", "table"];

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Lines(Vec<WidgetLine>),
    Rows(Vec<Row>),
    Stat {
        items: Vec<StatItem>,
        hint: Option<String>,
    },
    Table {
        columns: Vec<Column>,
        rows: Vec<Vec<Cell>>,
    },
}
