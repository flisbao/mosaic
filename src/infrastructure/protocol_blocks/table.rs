use serde_json::{Map, Value};

use crate::domain::blocks::{
    lines::MAXIMUM_LINES_PER_BLOCK,
    table::{Cell, Column, MAXIMUM_TABLE_COLUMNS},
    Block, BlockKind,
};
use crate::domain::widget_content::{Align, Tone};
use crate::infrastructure::protocol_messages as messages;
use crate::infrastructure::protocol_report::{child_path, indexed_path, Reader};

use super::BlockParser;

pub(super) struct TableParser;

impl BlockParser for TableParser {
    fn kind(&self) -> BlockKind {
        BlockKind::Table
    }

    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Option<Block> {
        reader.accepting_no_errors(|reader| {
            let columns = parse_columns(reader, object, path);
            let rows = parse_rows(reader, object, path, columns.len());
            Block::Table { columns, rows }
        })
    }
}

fn parse_columns(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Vec<Column> {
    let columns_path = child_path(path, "columns");
    let entries = reader.required_list(object, "columns", path);
    if entries.is_empty() && object.get("columns").is_some_and(Value::is_array) {
        let message = messages::table_needs_a_column(reader.language());
        reader.error(&columns_path, &message);
    }
    reader.parse_each(entries, MAXIMUM_TABLE_COLUMNS, &columns_path, parse_column)
}

fn parse_column(reader: &mut Reader, entry: &Value, path: &str) -> Option<Column> {
    let column = reader.object_entry(entry, path)?;
    let title = reader.required_text(column, "title", path)?;
    let align = parse_align(reader, column, path);
    Some(Column { title, align })
}

fn parse_align(reader: &mut Reader, column: &Map<String, Value>, path: &str) -> Align {
    match column.get("align").and_then(Value::as_str) {
        Some("right") => Align::Right,
        Some("left") | None => Align::Left,
        Some(_) => {
            let message = messages::unknown_alignment(reader.language());
            reader.warning(&child_path(path, "align"), &message);
            Align::Left
        }
    }
}

fn parse_rows(
    reader: &mut Reader,
    object: &Map<String, Value>,
    path: &str,
    width: usize,
) -> Vec<Vec<Cell>> {
    let rows_path = child_path(path, "rows");
    let entries = reader.required_list(object, "rows", path);
    reader.parse_each(
        entries,
        MAXIMUM_LINES_PER_BLOCK,
        &rows_path,
        |reader, entry, row_path| parse_row(reader, entry, width, row_path),
    )
}

fn parse_row(reader: &mut Reader, entry: &Value, width: usize, path: &str) -> Option<Vec<Cell>> {
    let Some(entries) = entry.as_array() else {
        let message = messages::row_is_not_a_list_of_cells(reader.language());
        reader.error(path, &message);
        return None;
    };
    if entries.len() != width {
        let message = messages::row_width_mismatch(reader.language(), entries.len(), width);
        reader.warning(path, &message);
    }
    let mut cells = vec![];
    for (index, cell) in entries.iter().take(width).enumerate() {
        cells.extend(parse_cell(reader, cell, &indexed_path(path, index)));
    }
    cells.resize_with(width, || Cell {
        text: String::new(),
        tone: Tone::Normal,
        flag: None,
    });
    Some(cells)
}

fn parse_cell(reader: &mut Reader, entry: &Value, path: &str) -> Option<Cell> {
    match entry {
        Value::String(text) => Some(Cell {
            text: reader.limited(text, path),
            tone: Tone::Normal,
            flag: None,
        }),
        Value::Object(object) => {
            let text = reader.required_text(object, "text", path)?;
            Some(Cell {
                text,
                tone: reader.tone(object, path),
                flag: reader.flag(object, path),
            })
        }
        _ => {
            let message = messages::cell_not_text_or_object(reader.language());
            reader.error(path, &message);
            None
        }
    }
}
