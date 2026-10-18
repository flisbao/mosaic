mod lines;
mod rows;
mod stat;
mod table;

use serde_json::{Map, Value};

use crate::domain::blocks::Block;
use crate::infrastructure::protocol_report::Reader;

use lines::LinesParser;
use rows::RowsParser;
use stat::StatParser;
use table::TableParser;

pub(super) trait BlockParser: Sync {
    fn type_name(&self) -> &'static str;
    fn parse(&self, reader: &mut Reader, object: &Map<String, Value>, path: &str) -> Block;
}

const PARSERS: [&dyn BlockParser; 4] = [&LinesParser, &RowsParser, &StatParser, &TableParser];

pub(super) fn find_parser(type_name: &str) -> Option<&'static dyn BlockParser> {
    PARSERS
        .iter()
        .copied()
        .find(|parser| parser.type_name() == type_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::blocks::BLOCK_TYPE_NAMES;

    #[test]
    fn parser_registry_matches_block_type_names() {
        let names: Vec<&str> = PARSERS.iter().map(|parser| parser.type_name()).collect();
        assert_eq!(names, BLOCK_TYPE_NAMES);
    }
}
