use serde_json::{Map, Value};

use crate::domain::blocks::{
    lines::MAXIMUM_LINES_PER_BLOCK,
    table::{Cell, Column, MAXIMUM_TABLE_COLUMNS},
    Block,
};
use crate::domain::widget_content::{Align, Tone};
use crate::infrastructure::protocol_report::{child_path, indexed_path, Reader};

use super::BlockParser;

pub(super) struct TableParser;

impl BlockParser for TableParser {
    fn type_name(&self) -> &'static str {
        "table"
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
        reader.error(&columns_path, "precisa ter ao menos uma coluna");
    }
    let entries = reader.capped(entries, MAXIMUM_TABLE_COLUMNS, &columns_path);
    let mut columns = vec![];
    for (index, entry) in entries.iter().enumerate() {
        let column_path = indexed_path(&columns_path, index);
        let Some(column) = reader.object_entry(entry, &column_path) else {
            continue;
        };
        let Some(title) = reader.required_text(column, "title", &column_path) else {
            continue;
        };
        let align = parse_align(reader, column, &column_path);
        columns.push(Column { title, align });
    }
    columns
}

fn parse_align(reader: &mut Reader, column: &Map<String, Value>, path: &str) -> Align {
    match column.get("align").and_then(Value::as_str) {
        Some("right") => Align::Right,
        Some("left") | None => Align::Left,
        Some(_) => {
            reader.warning(
                &child_path(path, "align"),
                "alinhamento desconhecido; usando left",
            );
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
    let entries = reader.capped(entries, MAXIMUM_LINES_PER_BLOCK, &rows_path);
    let mut rows = vec![];
    for (index, entry) in entries.iter().enumerate() {
        let row_path = indexed_path(&rows_path, index);
        if let Some(cells) = parse_row(reader, entry, width, &row_path) {
            rows.push(cells);
        }
    }
    rows
}

fn parse_row(reader: &mut Reader, entry: &Value, width: usize, path: &str) -> Option<Vec<Cell>> {
    let Some(entries) = entry.as_array() else {
        reader.error(path, "precisa ser uma lista de células");
        return None;
    };
    if entries.len() != width {
        reader.warning(
            path,
            &format!(
                "a linha tem {} células e a tabela {width} colunas",
                entries.len()
            ),
        );
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
            reader.error(path, "célula precisa ser texto ou objeto");
            None
        }
    }
}
