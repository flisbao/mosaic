use serde_json::{Map, Value};

use crate::domain::blocks::{
    lines::MAXIMUM_LINES_PER_BLOCK,
    rows::{Row, Tag, MAXIMUM_TAGS_PER_ROW},
    Block,
};
use crate::infrastructure::protocol_report::{child_path, indexed_path, Reader};

use super::BlockParser;

pub(super) struct RowsParser;

impl BlockParser for RowsParser {
    fn type_name(&self) -> &'static str {
        "rows"
    }

    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Option<Block> {
        reader.accepting_no_errors(|reader| Block::Rows(parse_rows(reader, object, path)))
    }
}

fn parse_rows(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Vec<Row> {
    let rows_path = child_path(path, "rows");
    let entries = reader.required_list(object, "rows", path);
    let entries = reader.capped(entries, MAXIMUM_LINES_PER_BLOCK, &rows_path);
    let mut rows = vec![];
    for (index, entry) in entries.iter().enumerate() {
        let row_path = indexed_path(&rows_path, index);
        if let Some(row) = parse_row(reader, entry, &row_path) {
            rows.push(row);
        }
    }
    rows
}

fn parse_row(reader: &mut Reader, entry: &Value, path: &str) -> Option<Row> {
    let object = reader.object_entry(entry, path)?;
    let text = reader.required_text(object, "text", path)?;
    Some(Row {
        text,
        time: reader.optional_text(object, "time", path),
        tags: parse_tags(reader, object, path),
        tone: reader.tone(object, path),
        marker: parse_marker(reader, object, path),
        flag: reader.flag(object, path),
    })
}

fn parse_tags(reader: &mut Reader, row: &Map<String, Value>, path: &str) -> Vec<Tag> {
    let tags_path = child_path(path, "tags");
    let entries = match row.get("tags") {
        Some(Value::Array(entries)) => entries,
        Some(Value::Null) | None => return vec![],
        Some(_) => {
            reader.warning(&tags_path, "precisa ser uma lista; ignorado");
            return vec![];
        }
    };
    let entries = reader.capped(entries, MAXIMUM_TAGS_PER_ROW, &tags_path);
    let mut tags = vec![];
    for (index, entry) in entries.iter().enumerate() {
        let tag_path = indexed_path(&tags_path, index);
        let Some(tag) = entry.as_object() else {
            reader.warning(&tag_path, "precisa ser um objeto; ignorado");
            continue;
        };
        let Some(text) = reader.optional_text(tag, "text", &tag_path) else {
            reader.warning(&tag_path, "etiqueta sem texto; ignorada");
            continue;
        };
        let tone = reader.tone(tag, &tag_path);
        tags.push(Tag { text, tone });
    }
    tags
}

fn parse_marker(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Option<char> {
    let text = reader.optional_text(object, "marker", path)?;
    let mut characters = text.chars();
    match (characters.next(), characters.next()) {
        (Some(marker), None) => Some(marker),
        _ => {
            reader.warning(
                &child_path(path, "marker"),
                "precisa ter exatamente um caractere; ignorado",
            );
            None
        }
    }
}
