use serde_json::{Map, Value};

use crate::domain::blocks::{
    stat::{StatItem, MAXIMUM_STAT_ITEMS},
    Block, BlockKind,
};
use crate::infrastructure::protocol_report::{child_path, Reader};

use super::BlockParser;

pub(super) struct StatParser;

impl BlockParser for StatParser {
    fn kind(&self) -> BlockKind {
        BlockKind::Stat
    }

    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Option<Block> {
        reader.accepting_no_errors(|reader| {
            let items = parse_items(reader, object, path);
            Block::Stat {
                items,
                hint: reader.optional_text(object, "hint", path),
            }
        })
    }
}

fn parse_items(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Vec<StatItem> {
    let items_path = child_path(path, "items");
    let entries = reader.required_list(object, "items", path);
    reader.parse_each(entries, MAXIMUM_STAT_ITEMS, &items_path, parse_item)
}

fn parse_item(reader: &mut Reader, entry: &Value, path: &str) -> Option<StatItem> {
    let item = reader.object_entry(entry, path)?;
    let value = reader.required_text(item, "value", path);
    let label = reader.required_text(item, "label", path);
    let (value, label) = (value?, label?);
    let tone = reader.tone(item, path);
    Some(StatItem { value, label, tone })
}
