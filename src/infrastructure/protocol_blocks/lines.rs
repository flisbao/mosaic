use serde_json::{Map, Value};

use crate::domain::blocks::{
    lines::{WidgetLine, MAXIMUM_LINES_PER_BLOCK},
    Block,
};
use crate::infrastructure::protocol_report::{child_path, indexed_path, Reader};

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
    let entries = reader.capped(entries, MAXIMUM_LINES_PER_BLOCK, &lines_path);
    let mut lines = vec![];
    for (index, entry) in entries.iter().enumerate() {
        let line_path = indexed_path(&lines_path, index);
        let Some(line) = reader.object_entry(entry, &line_path) else {
            continue;
        };
        let Some(text) = reader.required_text(line, "text", &line_path) else {
            continue;
        };
        let tone = reader.tone(line, &line_path);
        let flag = reader.flag(line, &line_path);
        lines.push(WidgetLine { text, tone, flag });
    }
    lines
}
