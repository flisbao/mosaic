use serde_json::{Map, Value};

use crate::domain::blocks::{
    stat::{StatItem, MAXIMUM_STAT_ITEMS},
    Block,
};
use crate::infrastructure::protocol_report::{child_path, indexed_path, Reader};

use super::BlockParser;

pub(super) struct StatParser;

impl BlockParser for StatParser {
    fn type_name(&self) -> &'static str {
        "stat"
    }

    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Block {
        let items = parse_items(reader, object, path);
        Block::Stat {
            items,
            hint: reader.optional_text(object, "hint", path),
        }
    }
}

fn parse_items(reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Vec<StatItem> {
    let items_path = child_path(path, "items");
    let entries = reader.required_list(object, "items", path);
    let entries = reader.capped(entries, MAXIMUM_STAT_ITEMS, &items_path);
    let mut items = vec![];
    for (index, entry) in entries.iter().enumerate() {
        let item_path = indexed_path(&items_path, index);
        let Some(item) = reader.object_entry(entry, &item_path) else {
            continue;
        };
        let value = reader.required_text(item, "value", &item_path);
        let label = reader.required_text(item, "label", &item_path);
        let (Some(value), Some(label)) = (value, label) else {
            continue;
        };
        let tone = reader.tone(item, &item_path);
        items.push(StatItem { value, label, tone });
    }
    items
}
