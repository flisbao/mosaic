use serde_json::{Map, Value};

use crate::domain::blocks::{
    lines::{WidgetLine, MAXIMUM_LINES_PER_BLOCK},
    Block,
};
use crate::infrastructure::protocol_report::{child_path, Reader};

use super::BlockParser;

pub(super) struct LinesParser;

impl BlockParser for LinesParser {
    fn type_name(&self) -> &'static str {
        "lines"
    }

    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Option<Block> {
        reader.accepting_no_errors(|reader| Block::Lines(parse_lines(reader, object, path)))
    }
}

fn parse_lines(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Vec<WidgetLine> {
    let lines_path = child_path(path, "lines");
    let entries = reader.required_list(object, "lines", path);
    reader.parse_each(entries, MAXIMUM_LINES_PER_BLOCK, &lines_path, parse_line)
}

fn parse_line(reader: &mut Reader, entry: &Value, path: &str) -> Option<WidgetLine> {
    let line = reader.object_entry(entry, path)?;
    let text = reader.required_text(line, "text", path)?;
    let tone = reader.tone(line, path);
    let flag = reader.flag(line, path);
    Some(WidgetLine { text, tone, flag })
}
