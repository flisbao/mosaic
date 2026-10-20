pub mod lines;
pub mod rows;
pub mod stat;
pub mod table;

use lines::WidgetLine;
use rows::Row;
use stat::StatItem;
use table::{Cell, Column};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Lines,
    Rows,
    Stat,
    Table,
}

impl BlockKind {
    pub const ALL: [BlockKind; 4] = [
        BlockKind::Lines,
        BlockKind::Rows,
        BlockKind::Stat,
        BlockKind::Table,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            BlockKind::Lines => "lines",
            BlockKind::Rows => "rows",
            BlockKind::Stat => "stat",
            BlockKind::Table => "table",
        }
    }
}

pub const BLOCK_TYPE_NAMES: [&str; 4] = block_type_names();

const fn block_type_names() -> [&'static str; 4] {
    let mut names = [""; 4];
    let mut index = 0;
    while index < BlockKind::ALL.len() {
        names[index] = BlockKind::ALL[index].name();
        index += 1;
    }
    names
}

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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_kind_names_match_the_public_list_in_order() {
        let names: Vec<&str> = BlockKind::ALL.iter().map(|kind| kind.name()).collect();
        assert_eq!(names, BLOCK_TYPE_NAMES);
        assert_eq!(BLOCK_TYPE_NAMES, ["lines", "rows", "stat", "table"]);
    }
}
