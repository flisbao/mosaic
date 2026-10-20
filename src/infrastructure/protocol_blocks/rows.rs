use serde_json::{Map, Value};

use crate::domain::blocks::{
    lines::MAXIMUM_LINES_PER_BLOCK,
    rows::{Row, Tag, MAXIMUM_TAGS_PER_ROW},
    Block, BlockKind,
};
use crate::infrastructure::protocol_report::{child_path, Reader};

use super::BlockParser;

pub(super) struct RowsParser;

impl BlockParser for RowsParser {
    fn kind(&self) -> BlockKind {
        BlockKind::Rows
    }

    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Option<Block> {
        reader.accepting_no_errors(|reader| Block::Rows(parse_rows(reader, object, path)))
    }
}

fn parse_rows(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Vec<Row> {
    let rows_path = child_path(path, "rows");
    let entries = reader.required_list(object, "rows", path);
    reader.parse_each(entries, MAXIMUM_LINES_PER_BLOCK, &rows_path, parse_row)
}

fn parse_row(reader: &mut Reader, entry: &Value, path: &str) -> Option<Row> {
    let object = reader.object_entry(entry, path)?;
    let text = reader.required_text(object, "text", path)?;
    Some(Row {
        text,
        time: reader.optional_text(object, "time", path),
        tags: parse_tags(reader, object, path),
        tone: reader.tone(object, path),
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
    reader.parse_each(entries, MAXIMUM_TAGS_PER_ROW, &tags_path, parse_tag)
}

fn parse_tag(reader: &mut Reader, entry: &Value, path: &str) -> Option<Tag> {
    let Some(tag) = entry.as_object() else {
        reader.warning(path, "precisa ser um objeto; ignorado");
        return None;
    };
    let Some(text) = reader.optional_text(tag, "text", path) else {
        reader.warning(path, "etiqueta sem texto; ignorada");
        return None;
    };
    let tone = reader.tone(tag, path);
    Some(Tag { text, tone })
}
