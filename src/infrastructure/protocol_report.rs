use serde_json::{Map, Value};

use crate::domain::blocks::Block;
use crate::domain::widget::{
    ActionScope, ActionSpec, DetailItem, DetailSection, Flag, Summary, SummaryLine, WidgetSnapshot,
};
use crate::domain::widget_content::{clean_text, clean_text_limited, Alert, AlertLevel, Tone};
use crate::infrastructure::plugin_protocol::ProtocolError;
use crate::infrastructure::protocol_blocks::find_parser;

pub use crate::domain::snapshot_report::{ParsedSnapshot, Problem, Severity};

pub const SUPPORTED_PROTOCOL_VERSION: u64 = 1;
pub const MAXIMUM_TEXT_CHARACTERS: usize = 500;
pub const MAXIMUM_ALERT_ID_CHARACTERS: usize = 100;
pub const MAXIMUM_ALERTS_PER_SNAPSHOT: usize = 100;
pub const MAXIMUM_BLOCKS_PER_WIDGET: usize = 20;
pub const MAXIMUM_LEGACY_LINES: usize = 200;
pub const MAXIMUM_SECTIONS: usize = 100;
pub const MAXIMUM_ITEMS_PER_SECTION: usize = 1000;
pub const MAXIMUM_ACTIONS: usize = 100;
pub const MAXIMUM_KEY_CHARACTERS: usize = 100;

const EMPTY_PARAMS_JSON: &str = "{}";
const UNKNOWN_FIELD_MESSAGE: &str = "campo desconhecido";
const ROOT_KEYS: [&str; 5] = ["protocol", "summary", "detail", "actions", "alerts"];
const SUMMARY_KEYS: [&str; 3] = ["title", "lines", "blocks"];
const DETAIL_KEYS: [&str; 1] = ["sections"];
const SECTION_KEYS: [&str; 3] = ["heading", "items", "blocks"];
const ITEM_KEYS: [&str; 4] = ["id", "text", "flag", "meta"];
const ACTION_KEYS: [&str; 5] = ["id", "label", "description", "applies_to", "params"];

pub fn unsupported_protocol_text(version: u64) -> String {
    format!("este widget exige o protocolo {version}; atualize o mosaic")
}

pub fn parse_snapshot_report(json: &str) -> Result<ParsedSnapshot, ProtocolError> {
    let value: Value = serde_json::from_str(json).map_err(ProtocolError::new)?;
    let Value::Object(root) = value else {
        return Err(ProtocolError::new("a raiz precisa ser um objeto"));
    };
    let mut reader = Reader::default();
    let summary_object = root
        .get("summary")
        .and_then(Value::as_object)
        .ok_or_else(|| ProtocolError::new("summary: objeto obrigatório"))?;
    let title = summary_object
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| ProtocolError::new("summary.title: texto obrigatório"))?;
    let title = reader.limited(title, "summary.title");

    if let Some(version) = reader.protocol_version(&root) {
        if version > SUPPORTED_PROTOCOL_VERSION {
            return Ok(reader.unsupported_protocol(title, version));
        }
    }
    reader.unknown_fields(&root, &ROOT_KEYS, "");
    reader.unknown_fields(summary_object, &SUMMARY_KEYS, "summary");
    let summary = Summary {
        title,
        lines: reader.legacy_lines(summary_object),
        blocks: reader.blocks(summary_object, "summary"),
    };
    let snapshot = WidgetSnapshot {
        summary,
        detail: reader.detail(&root),
        actions: reader.actions(&root),
        alerts: reader.alerts(&root),
    };
    Ok(ParsedSnapshot {
        snapshot,
        problems: reader.problems,
    })
}

pub(super) fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

fn key_path(parent: &str, key: &str) -> String {
    child_path(parent, &clean_text_limited(key, MAXIMUM_KEY_CHARACTERS))
}

pub(super) fn indexed_path(parent: &str, index: usize) -> String {
    format!("{parent}[{index}]")
}

#[derive(Default)]
pub(super) struct Reader {
    problems: Vec<Problem>,
}

impl Reader {
    fn report(&mut self, path: &str, message: &str, severity: Severity) {
        self.problems.push(Problem {
            path: path.to_string(),
            message: message.to_string(),
            severity,
        });
    }

    pub(super) fn error(&mut self, path: &str, message: &str) {
        self.report(path, message, Severity::Error);
    }

    pub(super) fn warning(&mut self, path: &str, message: &str) {
        self.report(path, message, Severity::Warning);
    }

    fn unknown_fields(&mut self, object: &Map<String, Value>, known: &[&str], path: &str) {
        for key in object.keys().filter(|key| !known.contains(&key.as_str())) {
            self.warning(&key_path(path, key), UNKNOWN_FIELD_MESSAGE);
        }
    }

    pub(super) fn limited(&mut self, text: &str, path: &str) -> String {
        let cleaned = clean_text(text);
        if cleaned.chars().count() <= MAXIMUM_TEXT_CHARACTERS {
            return cleaned;
        }
        self.warning(
            path,
            &format!("texto cortado em {MAXIMUM_TEXT_CHARACTERS} caracteres"),
        );
        clean_text_limited(&cleaned, MAXIMUM_TEXT_CHARACTERS)
    }

    pub(super) fn required_text(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<String> {
        let field_path = child_path(path, key);
        match object.get(key) {
            Some(Value::String(text)) => Some(self.limited(text, &field_path)),
            Some(_) => {
                self.error(&field_path, "precisa ser texto");
                None
            }
            None => {
                self.error(&field_path, "campo obrigatório ausente");
                None
            }
        }
    }

    pub(super) fn optional_text(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<String> {
        let field_path = child_path(path, key);
        match object.get(key) {
            Some(Value::String(text)) => Some(self.limited(text, &field_path)),
            Some(Value::Null) | None => None,
            Some(_) => {
                self.warning(&field_path, "precisa ser texto; ignorado");
                None
            }
        }
    }

    pub(super) fn flag(&mut self, object: &Map<String, Value>, path: &str) -> Option<Flag> {
        let field_path = child_path(path, "flag");
        match object.get("flag") {
            Some(Value::String(name)) => {
                let flag = parse_flag(name);
                if flag.is_none() {
                    self.warning(
                        &field_path,
                        &format!("flag desconhecida: {}", clean_text(name)),
                    );
                }
                flag
            }
            Some(Value::Null) | None => None,
            Some(_) => {
                self.warning(&field_path, "precisa ser texto; ignorado");
                None
            }
        }
    }

    fn protocol_version(&mut self, root: &Map<String, Value>) -> Option<u64> {
        let value = root.get("protocol")?;
        match value.as_u64().filter(|version| *version > 0) {
            Some(version) => Some(version),
            None => {
                self.error("protocol", "precisa ser um número inteiro positivo");
                None
            }
        }
    }

    fn unsupported_protocol(mut self, title: String, version: u64) -> ParsedSnapshot {
        self.warning("protocol", &unsupported_protocol_text(version));
        let notice = SummaryLine {
            text: unsupported_protocol_text(version),
            flag: None,
        };
        ParsedSnapshot {
            snapshot: WidgetSnapshot {
                summary: Summary {
                    title,
                    lines: vec![notice],
                    blocks: vec![],
                },
                detail: None,
                actions: vec![],
                alerts: vec![],
            },
            problems: self.problems,
        }
    }

    fn list<'a>(
        &mut self,
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<&'a Vec<Value>> {
        match object.get(key) {
            Some(Value::Array(entries)) => Some(entries),
            Some(Value::Null) | None => None,
            Some(_) => {
                self.error(&child_path(path, key), "precisa ser uma lista");
                None
            }
        }
    }

    pub(super) fn object_entry<'a>(
        &mut self,
        entry: &'a Value,
        path: &str,
    ) -> Option<&'a Map<String, Value>> {
        let object = entry.as_object();
        if object.is_none() {
            self.error(path, "precisa ser um objeto");
        }
        object
    }

    pub(super) fn error_count(&self) -> usize {
        self.problems
            .iter()
            .filter(|problem| problem.severity == Severity::Error)
            .count()
    }

    pub(super) fn capped<'a>(
        &mut self,
        entries: &'a [Value],
        limit: usize,
        path: &str,
    ) -> &'a [Value] {
        if entries.len() <= limit {
            return entries;
        }
        self.warning(path, &format!("apenas {limit} itens são lidos"));
        &entries[..limit]
    }

    pub(super) fn required_list<'a>(
        &mut self,
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> &'a [Value] {
        match object.get(key) {
            Some(Value::Array(entries)) => entries,
            Some(_) => {
                self.error(&child_path(path, key), "precisa ser uma lista");
                &[]
            }
            None => {
                self.error(&child_path(path, key), "campo obrigatório ausente");
                &[]
            }
        }
    }

    pub(super) fn tone(&mut self, object: &Map<String, Value>, path: &str) -> Tone {
        let tone_path = child_path(path, "tone");
        match object.get("tone") {
            Some(Value::String(name)) => Tone::parse(name).unwrap_or_else(|| {
                self.warning(
                    &tone_path,
                    &format!("tom desconhecido: {}", clean_text(name)),
                );
                Tone::Normal
            }),
            Some(Value::Null) | None => Tone::Normal,
            Some(_) => {
                self.warning(&tone_path, "precisa ser texto; ignorado");
                Tone::Normal
            }
        }
    }

    fn blocks(&mut self, object: &Map<String, Value>, path: &str) -> Vec<Block> {
        let blocks_path = child_path(path, "blocks");
        let Some(entries) = self.list(object, "blocks", path) else {
            return vec![];
        };
        let entries = self.capped(entries, MAXIMUM_BLOCKS_PER_WIDGET, &blocks_path);
        let mut blocks = vec![];
        for (index, entry) in entries.iter().enumerate() {
            if let Some(block) = self.block(entry, &indexed_path(&blocks_path, index)) {
                blocks.push(block);
            }
        }
        blocks
    }

    fn block(&mut self, entry: &Value, path: &str) -> Option<Block> {
        let Some(object) = entry.as_object() else {
            self.error(path, "bloco precisa ser um objeto");
            return None;
        };
        let Some(kind) = object.get("type").and_then(Value::as_str) else {
            self.error(&child_path(path, "type"), "type ausente");
            return None;
        };
        let Some(parser) = find_parser(kind) else {
            let message = format!("tipo de bloco desconhecido: {}", clean_text(kind));
            self.warning(path, &message);
            return None;
        };
        let errors_before = self.error_count();
        let block = parser.parse(self, object, path);
        (self.error_count() == errors_before).then_some(block)
    }

    fn legacy_lines(&mut self, summary: &Map<String, Value>) -> Vec<SummaryLine> {
        let Some(entries) = self.list(summary, "lines", "summary") else {
            return vec![];
        };
        let entries = self.capped(entries, MAXIMUM_LEGACY_LINES, "summary.lines");
        let mut lines = vec![];
        for (index, entry) in entries.iter().enumerate() {
            let path = indexed_path("summary.lines", index);
            let Some(object) = self.object_entry(entry, &path) else {
                continue;
            };
            let Some(text) = self.required_text(object, "text", &path) else {
                continue;
            };
            let flag = self.flag(object, &path);
            lines.push(SummaryLine { text, flag });
        }
        lines
    }

    fn detail(&mut self, root: &Map<String, Value>) -> Option<Vec<DetailSection>> {
        let detail = match root.get("detail") {
            Some(Value::Null) | None => return None,
            Some(value) => self.object_entry(value, "detail")?,
        };
        self.unknown_fields(detail, &DETAIL_KEYS, "detail");
        let entries = self.list(detail, "sections", "detail")?;
        let entries = self.capped(entries, MAXIMUM_SECTIONS, "detail.sections");
        let mut sections = vec![];
        for (index, entry) in entries.iter().enumerate() {
            let path = indexed_path("detail.sections", index);
            if let Some(section) = self.section(entry, &path) {
                sections.push(section);
            }
        }
        Some(sections)
    }

    fn section(&mut self, entry: &Value, path: &str) -> Option<DetailSection> {
        let object = self.object_entry(entry, path)?;
        self.unknown_fields(object, &SECTION_KEYS, path);
        let heading = self.required_text(object, "heading", path)?;
        let blocks = self.blocks(object, path);
        let items_path = child_path(path, "items");
        let mut items = vec![];
        let entries = self
            .list(object, "items", path)
            .map_or(&[][..], Vec::as_slice);
        let entries = self.capped(entries, MAXIMUM_ITEMS_PER_SECTION, &items_path);
        for (index, item) in entries.iter().enumerate() {
            if let Some(item) = self.item(item, &indexed_path(&items_path, index)) {
                items.push(item);
            }
        }
        Some(DetailSection {
            heading,
            items,
            blocks,
        })
    }

    fn item(&mut self, entry: &Value, path: &str) -> Option<DetailItem> {
        let object = self.object_entry(entry, path)?;
        self.unknown_fields(object, &ITEM_KEYS, path);
        let text = self.required_text(object, "text", path)?;
        let id = match object.get("id") {
            Some(Value::String(id)) => Some(id.clone()),
            Some(Value::Null) | None => None,
            Some(_) => {
                self.warning(&child_path(path, "id"), "precisa ser texto; ignorado");
                None
            }
        };
        let flag = self.flag(object, path);
        let meta = self.meta(object, path);
        Some(DetailItem {
            id,
            text,
            flag,
            meta,
        })
    }

    fn meta(&mut self, item: &Map<String, Value>, path: &str) -> Vec<(String, String)> {
        let meta_path = child_path(path, "meta");
        let meta = match item.get("meta") {
            Some(Value::Object(meta)) => meta,
            Some(Value::Null) | None => return vec![],
            Some(_) => {
                self.warning(&meta_path, "precisa ser um objeto; ignorado");
                return vec![];
            }
        };
        let mut pairs = vec![];
        for (key, value) in meta {
            match value {
                Value::String(text) => {
                    let value = self.limited(text, &key_path(&meta_path, key));
                    pairs.push((clean_text_limited(key, MAXIMUM_TEXT_CHARACTERS), value));
                }
                _ => self.warning(&key_path(&meta_path, key), "precisa ser texto; ignorado"),
            }
        }
        pairs
    }

    fn actions(&mut self, root: &Map<String, Value>) -> Vec<ActionSpec> {
        let Some(entries) = self.list(root, "actions", "") else {
            return vec![];
        };
        let entries = self.capped(entries, MAXIMUM_ACTIONS, "actions");
        let mut actions = vec![];
        for (index, entry) in entries.iter().enumerate() {
            if let Some(action) = self.action(entry, &indexed_path("actions", index)) {
                actions.push(action);
            }
        }
        actions
    }

    fn action(&mut self, entry: &Value, path: &str) -> Option<ActionSpec> {
        let object = self.object_entry(entry, path)?;
        self.unknown_fields(object, &ACTION_KEYS, path);
        let id = self.required_text(object, "id", path);
        let label = self.required_text(object, "label", path);
        let (id, label) = (id?, label?);
        Some(ActionSpec {
            id,
            label,
            description: self.optional_text(object, "description", path),
            scope: match object.get("applies_to").and_then(Value::as_str) {
                Some("item") => ActionScope::Item,
                _ => ActionScope::Widget,
            },
            params_json: object
                .get("params")
                .map(Value::to_string)
                .unwrap_or_else(|| EMPTY_PARAMS_JSON.to_string()),
        })
    }

    fn alerts(&mut self, root: &Map<String, Value>) -> Vec<Alert> {
        let Some(entries) = self.list(root, "alerts", "") else {
            return vec![];
        };
        if entries.len() > MAXIMUM_ALERTS_PER_SNAPSHOT {
            self.warning(
                "alerts",
                &format!("apenas {MAXIMUM_ALERTS_PER_SNAPSHOT} avisos são lidos por leitura"),
            );
        }
        let mut alerts = vec![];
        for (index, entry) in entries.iter().take(MAXIMUM_ALERTS_PER_SNAPSHOT).enumerate() {
            if let Some(alert) = self.alert(entry, &indexed_path("alerts", index)) {
                alerts.push(alert);
            }
        }
        alerts
    }

    fn alert(&mut self, entry: &Value, path: &str) -> Option<Alert> {
        let Some(object) = entry.as_object() else {
            self.warning(path, "precisa ser um objeto; descartado");
            return None;
        };
        let id = self.alert_id(object, path)?;
        let Some(title) = self.optional_text(object, "title", path) else {
            self.warning(&child_path(path, "title"), "título ausente; descartado");
            return None;
        };
        Some(Alert {
            id,
            title,
            body: self.optional_text(object, "body", path).unwrap_or_default(),
            level: self.alert_level(object, path),
        })
    }

    fn alert_id(&mut self, object: &Map<String, Value>, path: &str) -> Option<String> {
        let id_path = child_path(path, "id");
        let id = object.get("id").and_then(Value::as_str).map(clean_text);
        match id {
            Some(id) if !id.is_empty() && id.chars().count() <= MAXIMUM_ALERT_ID_CHARACTERS => {
                Some(id)
            }
            _ => {
                self.warning(
                    &id_path,
                    &format!(
                        "id precisa ser um texto de 1 a {MAXIMUM_ALERT_ID_CHARACTERS} caracteres; aviso descartado"
                    ),
                );
                None
            }
        }
    }

    fn alert_level(&mut self, object: &Map<String, Value>, path: &str) -> AlertLevel {
        match object.get("level").and_then(Value::as_str) {
            Some("warn") => AlertLevel::Warn,
            Some("info") | None => AlertLevel::Info,
            Some(_) => {
                self.warning(
                    &child_path(path, "level"),
                    "nível desconhecido; usando info",
                );
                AlertLevel::Info
            }
        }
    }
}

fn parse_flag(name: &str) -> Option<Flag> {
    match name {
        "updated" => Some(Flag::Updated),
        "warning" => Some(Flag::Warning),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::blocks::{
        lines::WidgetLine,
        rows::{Row, Tag},
        stat::StatItem,
        table::{Cell, Column},
        Block,
    };
    use crate::domain::widget::Flag;
    use crate::domain::widget_content::{AlertLevel, Align, Tone};

    fn report(json: &str) -> ParsedSnapshot {
        parse_snapshot_report(json).unwrap()
    }

    fn paths(parsed: &ParsedSnapshot, severity: Severity) -> Vec<String> {
        parsed
            .problems
            .iter()
            .filter(|problem| problem.severity == severity)
            .map(|problem| problem.path.clone())
            .collect()
    }

    #[test]
    fn legacy_snapshot_is_valid_without_problems() {
        let parsed = report(
            r#"{"summary":{"title":"K","lines":[{"text":"x","flag":"updated"}]},
                "detail":{"sections":[{"heading":"h","items":[
                    {"id":"a","text":"A","flag":"warning","meta":{"p":"alta"}}]}]},
                "actions":[{"id":"move","label":"Mover","applies_to":"item","params":{"to":"done"}}]}"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        assert_eq!(parsed.snapshot.summary.lines[0].flag, Some(Flag::Updated));
        let item = &parsed.snapshot.detail.unwrap()[0].items[0];
        assert_eq!(item.id.as_deref(), Some("a"));
        assert_eq!(item.meta, vec![("p".to_string(), "alta".to_string())]);
        assert_eq!(parsed.snapshot.actions[0].params_json, r#"{"to":"done"}"#);
    }

    #[test]
    fn protocol_absent_or_one_is_accepted() {
        for json in [
            r#"{"summary":{"title":"K"}}"#,
            r#"{"protocol":1,"summary":{"title":"K"}}"#,
        ] {
            assert!(report(json).problems.is_empty());
        }
    }

    #[test]
    fn protocol_two_asks_to_update() {
        let parsed = report(r#"{"protocol":2,"summary":{"title":"K","lines":[{"text":"x"}]}}"#);
        assert_eq!(parsed.snapshot.summary.lines.len(), 1);
        assert_eq!(
            parsed.snapshot.summary.lines[0].text,
            "este widget exige o protocolo 2; atualize o mosaic"
        );
        assert_eq!(parsed.snapshot.summary.title, "K");
    }

    #[test]
    fn protocol_must_be_a_positive_integer() {
        for value in [r#""1""#, "0", "-1", "1.5", "null"] {
            let parsed = report(&format!(
                r#"{{"protocol":{value},"summary":{{"title":"K"}}}}"#
            ));
            assert_eq!(paths(&parsed, Severity::Error), vec!["protocol"], "{value}");
            assert_eq!(
                parsed.problems[0].message,
                "precisa ser um número inteiro positivo"
            );
        }
    }

    #[test]
    fn unknown_fields_are_warnings_with_paths() {
        let parsed = report(
            r#"{"extra":1,"summary":{"title":"K","extra":1},
                "detail":{"zz":1,"sections":[{"heading":"h","yy":1,"items":[{"text":"x","zzz":1}]}]},
                "actions":[{"id":"a","label":"A","qq":1}]}"#,
        );
        let mut found = paths(&parsed, Severity::Warning);
        found.sort();
        let mut expected = vec![
            "extra",
            "summary.extra",
            "detail.zz",
            "detail.sections[0].yy",
            "detail.sections[0].items[0].zzz",
            "actions[0].qq",
        ];
        expected.sort();
        assert_eq!(found, expected);
        assert!(paths(&parsed, Severity::Error).is_empty());
        assert_eq!(parsed.problems[0].message, "campo desconhecido");
    }

    #[test]
    fn texts_are_cleaned() {
        let parsed = report(
            "{\"summary\":{\"title\":\"A\\u001b[0m\\n B\",\"lines\":[{\"text\":\"x\\u202ey\\u0000\"}]},\
              \"detail\":{\"sections\":[{\"heading\":\"h\\tz\",\"items\":[{\"text\":\"\\u001bT\"}]}]},\
              \"alerts\":[{\"id\":\"1\",\"title\":\"t\\u001b\",\"body\":\"b\\n\\nc\"}]}",
        );
        assert_eq!(parsed.snapshot.summary.title, "A[0m B");
        assert_eq!(parsed.snapshot.summary.lines[0].text, "xy");
        let section = &parsed.snapshot.detail.unwrap()[0];
        assert_eq!(section.heading, "h z");
        assert_eq!(section.items[0].text, "T");
        assert_eq!(parsed.snapshot.alerts[0].title, "t");
        assert_eq!(parsed.snapshot.alerts[0].body, "b c");
    }

    #[test]
    fn alerts_are_parsed_and_invalid_ones_dropped() {
        let long_id = "x".repeat(101);
        let json = format!(
            r#"{{"summary":{{"title":"K"}},"alerts":[
                {{"id":"a","title":"A","level":"warn"}},
                {{"title":"sem id"}},
                {{"id":"{long_id}","title":"longo"}},
                {{"id":"","title":"vazio"}},
                {{"id":"b","title":"B","level":"estranho"}}]}}"#
        );
        let parsed = report(&json);
        let alerts = &parsed.snapshot.alerts;
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].level, AlertLevel::Warn);
        assert_eq!(alerts[1].id, "b");
        assert_eq!(alerts[1].level, AlertLevel::Info);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec![
                "alerts[1].id",
                "alerts[2].id",
                "alerts[3].id",
                "alerts[4].level"
            ]
        );
    }

    #[test]
    fn missing_summary_title_is_a_protocol_error() {
        assert!(parse_snapshot_report(r#"{"summary":{"lines":[]}}"#).is_err());
        assert!(parse_snapshot_report(r#"{"summary":{"title":3}}"#).is_err());
        assert!(parse_snapshot_report(r#"{"detail":{}}"#).is_err());
    }

    #[test]
    fn root_that_is_not_an_object_is_a_protocol_error() {
        for json in ["[]", "3", "\"x\"", "null"] {
            assert!(parse_snapshot_report(json).is_err(), "{json}");
        }
    }

    #[test]
    fn deeply_nested_json_does_not_panic() {
        let nested = format!("{}{}", "[".repeat(500), "]".repeat(500));
        let json = format!(r#"{{"summary":{{"title":"K"}},"extra":{nested}}}"#);
        assert!(parse_snapshot_report(&json).is_err());
    }

    #[test]
    fn malformed_legacy_entries_are_errors_with_paths() {
        let parsed = report(
            r#"{"summary":{"title":"K","lines":[{"text":"ok"},{"flag":"updated"},3]},
                "detail":{"sections":[{"heading":"h","items":[{"id":"a"}]}, 7]},
                "actions":[{"id":"a"}]}"#,
        );
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec![
                "summary.lines[1].text",
                "summary.lines[2]",
                "detail.sections[0].items[0].text",
                "detail.sections[1]",
                "actions[0].label"
            ]
        );
        assert_eq!(parsed.snapshot.summary.lines.len(), 1);
    }

    fn summary_with_blocks(blocks: &str) -> ParsedSnapshot {
        report(&format!(
            r#"{{"summary":{{"title":"K","blocks":{blocks}}}}}"#
        ))
    }

    fn first_block(parsed: &ParsedSnapshot) -> &Block {
        &parsed.snapshot.summary.blocks[0]
    }

    #[test]
    fn lines_block_parses_text_tone_and_flag() {
        let parsed = summary_with_blocks(
            r#"[{"type":"lines","lines":[
                {"text":"a","tone":"ok","flag":"updated"},{"text":"b"}]}]"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let expected = Block::Lines(vec![
            WidgetLine {
                text: "a".to_string(),
                tone: Tone::Ok,
                flag: Some(Flag::Updated),
            },
            WidgetLine {
                text: "b".to_string(),
                tone: Tone::Normal,
                flag: None,
            },
        ]);
        assert_eq!(first_block(&parsed), &expected);
    }

    #[test]
    fn rows_block_parses_all_fields() {
        let parsed = summary_with_blocks(
            r#"[{"type":"rows","rows":[{"time":"09:00","text":"Daily","tone":"current",
                "marker":"▶","flag":"warning","tags":[{"text":"agora","tone":"danger"}]}]}]"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let expected = Block::Rows(vec![Row {
            time: Some("09:00".to_string()),
            text: "Daily".to_string(),
            tags: vec![Tag {
                text: "agora".to_string(),
                tone: Tone::Danger,
            }],
            tone: Tone::Current,
            marker: Some('▶'),
            flag: Some(Flag::Warning),
        }]);
        assert_eq!(first_block(&parsed), &expected);
    }

    #[test]
    fn rows_tags_are_limited_to_three_with_a_warning() {
        let parsed = summary_with_blocks(
            r#"[{"type":"rows","rows":[{"text":"x","tags":[
                {"text":"1"},{"text":"2"},{"text":"3"},{"text":"4"}]}]}]"#,
        );
        let Block::Rows(rows) = first_block(&parsed) else {
            panic!("esperava rows");
        };
        assert_eq!(rows[0].tags.len(), 3);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].rows[0].tags"]
        );
    }

    #[test]
    fn marker_must_be_a_single_character() {
        for marker in ["", "ab", "  "] {
            let parsed = summary_with_blocks(&format!(
                r#"[{{"type":"rows","rows":[{{"text":"x","marker":"{marker}"}}]}}]"#
            ));
            let Block::Rows(rows) = first_block(&parsed) else {
                panic!("esperava rows");
            };
            assert_eq!(rows[0].marker, None, "{marker:?}");
            assert_eq!(
                paths(&parsed, Severity::Warning),
                vec!["summary.blocks[0].rows[0].marker"]
            );
        }
    }

    #[test]
    fn unknown_block_type_is_a_warning_and_ignored() {
        let parsed = summary_with_blocks(r#"[{"type":"grafico"},{"type":"lines","lines":[]}]"#);
        assert_eq!(parsed.snapshot.summary.blocks.len(), 1);
        assert_eq!(paths(&parsed, Severity::Warning), vec!["summary.blocks[0]"]);
        assert_eq!(
            parsed.problems[0].message,
            "tipo de bloco desconhecido: grafico"
        );
    }

    #[test]
    fn block_that_is_not_an_object_is_an_error_with_path() {
        let parsed = summary_with_blocks(r#"[{"type":"lines","lines":[]},"texto",null,[],{}]"#);
        assert_eq!(parsed.snapshot.summary.blocks.len(), 1);
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec![
                "summary.blocks[1]",
                "summary.blocks[2]",
                "summary.blocks[3]",
                "summary.blocks[4].type"
            ]
        );
        assert_eq!(parsed.problems[0].message, "bloco precisa ser um objeto");
    }

    #[test]
    fn rows_that_is_not_a_list_discards_only_that_block() {
        let parsed = summary_with_blocks(
            r#"[{"type":"rows","rows":"x"},{"type":"lines","lines":[{"text":"fica"}]}]"#,
        );
        assert_eq!(parsed.snapshot.summary.blocks.len(), 1);
        assert!(matches!(first_block(&parsed), Block::Lines(_)));
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec!["summary.blocks[0].rows"]
        );
    }

    #[test]
    fn row_without_text_is_an_error_with_path() {
        let parsed = summary_with_blocks(
            r#"[{"type":"rows","rows":[{"text":"a"},{"text":"b"},{"time":"1"}]}]"#,
        );
        assert!(parsed.snapshot.summary.blocks.is_empty());
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec!["summary.blocks[0].rows[2].text"]
        );
    }

    #[test]
    fn unknown_tone_becomes_normal_with_a_warning() {
        let parsed =
            summary_with_blocks(r#"[{"type":"lines","lines":[{"text":"a","tone":"rosa"}]}]"#);
        let Block::Lines(lines) = first_block(&parsed) else {
            panic!("esperava lines");
        };
        assert_eq!(lines[0].tone, Tone::Normal);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].lines[0].tone"]
        );
    }

    #[test]
    fn limits_are_enforced() {
        let many_blocks = vec![r#"{"type":"lines","lines":[]}"#; 21].join(",");
        let parsed = summary_with_blocks(&format!("[{many_blocks}]"));
        assert_eq!(parsed.snapshot.summary.blocks.len(), 20);
        assert_eq!(paths(&parsed, Severity::Warning), vec!["summary.blocks"]);

        let many_lines = vec![r#"{"text":"x"}"#; 201].join(",");
        let parsed =
            summary_with_blocks(&format!(r#"[{{"type":"lines","lines":[{many_lines}]}}]"#));
        let Block::Lines(lines) = first_block(&parsed) else {
            panic!("esperava lines");
        };
        assert_eq!(lines.len(), 200);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].lines"]
        );

        let long_text = "y".repeat(600);
        let parsed = summary_with_blocks(&format!(
            r#"[{{"type":"lines","lines":[{{"text":"{long_text}"}}]}}]"#
        ));
        let Block::Lines(lines) = first_block(&parsed) else {
            panic!("esperava lines");
        };
        assert_eq!(lines[0].text.chars().count(), 500);
        assert!(lines[0].text.ends_with('…'));
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].lines[0].text"]
        );
    }

    #[test]
    fn blocks_in_detail_sections_are_parsed() {
        let parsed = report(
            r#"{"summary":{"title":"K"},"detail":{"sections":[{"heading":"h",
                "blocks":[{"type":"lines","lines":[{"text":"x"}]}]}]}}"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let section = &parsed.snapshot.detail.unwrap()[0];
        assert_eq!(section.blocks.len(), 1);
    }

    #[test]
    fn blocks_and_items_coexist_in_a_section() {
        let parsed = report(
            r#"{"summary":{"title":"K"},"detail":{"sections":[{"heading":"h",
                "blocks":[{"type":"lines","lines":[{"text":"x"}]}],
                "items":[{"text":"i"}]}]}}"#,
        );
        let section = &parsed.snapshot.detail.unwrap()[0];
        assert_eq!(section.blocks.len(), 1);
        assert_eq!(section.items.len(), 1);
    }

    #[test]
    fn blocks_take_precedence_over_summary_lines() {
        let parsed = report(
            r#"{"summary":{"title":"K","lines":[{"text":"velho"}],
                "blocks":[{"type":"lines","lines":[{"text":"novo"}]}]}}"#,
        );
        let displayed = parsed.snapshot.summary.display_blocks();
        let Block::Lines(lines) = &displayed[0] else {
            panic!("esperava lines");
        };
        assert_eq!(lines[0].text, "novo");
    }

    fn cell(text: &str) -> Cell {
        Cell {
            text: text.to_string(),
            tone: Tone::Normal,
            flag: None,
        }
    }

    #[test]
    fn stat_block_parses_items_and_hint() {
        let parsed = summary_with_blocks(
            r#"[{"type":"stat","items":[{"value":"3","label":"reuniões","tone":"warn"},
                {"value":"2h","label":"livre"}],"hint":"hoje"}]"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let expected = Block::Stat {
            items: vec![
                StatItem {
                    value: "3".to_string(),
                    label: "reuniões".to_string(),
                    tone: Tone::Warn,
                },
                StatItem {
                    value: "2h".to_string(),
                    label: "livre".to_string(),
                    tone: Tone::Normal,
                },
            ],
            hint: Some("hoje".to_string()),
        };
        assert_eq!(first_block(&parsed), &expected);
    }

    #[test]
    fn stat_items_are_limited_to_eight() {
        let items = [r#"{"value":"1","label":"x"}"#; 9].join(",");
        let parsed = summary_with_blocks(&format!(r#"[{{"type":"stat","items":[{items}]}}]"#));
        let Block::Stat { items, .. } = first_block(&parsed) else {
            panic!("esperava stat");
        };
        assert_eq!(items.len(), 8);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].items"]
        );
    }

    #[test]
    fn stat_item_without_value_is_an_error_with_path() {
        let parsed = summary_with_blocks(
            r#"[{"type":"stat","items":[{"value":"1","label":"a"},{"label":"b"}]}]"#,
        );
        assert!(parsed.snapshot.summary.blocks.is_empty());
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec!["summary.blocks[0].items[1].value"]
        );
    }

    #[test]
    fn table_block_parses_columns_alignment_and_cells() {
        let parsed = summary_with_blocks(
            r#"[{"type":"table","columns":[{"title":"Nome"},{"title":"Qtd","align":"right"}],
                "rows":[["ana","3"],["bia","10"]]}]"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let expected = Block::Table {
            columns: vec![
                Column {
                    title: "Nome".to_string(),
                    align: Align::Left,
                },
                Column {
                    title: "Qtd".to_string(),
                    align: Align::Right,
                },
            ],
            rows: vec![vec![cell("ana"), cell("3")], vec![cell("bia"), cell("10")]],
        };
        assert_eq!(first_block(&parsed), &expected);
    }

    #[test]
    fn table_cell_can_be_a_plain_string_or_an_object() {
        let parsed = summary_with_blocks(
            r#"[{"type":"table","columns":[{"title":"A"},{"title":"B"}],
                "rows":[["x",{"text":"y","tone":"danger","flag":"warning"}]]}]"#,
        );
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let Block::Table { rows, .. } = first_block(&parsed) else {
            panic!("esperava table");
        };
        assert_eq!(rows[0][0], cell("x"));
        assert_eq!(
            rows[0][1],
            Cell {
                text: "y".to_string(),
                tone: Tone::Danger,
                flag: Some(Flag::Warning),
            }
        );
    }

    #[test]
    fn table_row_with_extra_cells_is_cut_with_a_warning() {
        let parsed = summary_with_blocks(
            r#"[{"type":"table","columns":[{"title":"A"}],"rows":[["1","2","3"]]}]"#,
        );
        let Block::Table { rows, .. } = first_block(&parsed) else {
            panic!("esperava table");
        };
        assert_eq!(rows[0], vec![cell("1")]);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].rows[0]"]
        );
    }

    #[test]
    fn table_row_with_missing_cells_is_padded_with_a_warning() {
        let parsed = summary_with_blocks(
            r#"[{"type":"table","columns":[{"title":"A"},{"title":"B"}],"rows":[["1"]]}]"#,
        );
        let Block::Table { rows, .. } = first_block(&parsed) else {
            panic!("esperava table");
        };
        assert_eq!(rows[0], vec![cell("1"), cell("")]);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].rows[0]"]
        );
    }

    #[test]
    fn table_without_columns_is_an_error() {
        for columns in ["[]", "3", "null"] {
            let parsed = summary_with_blocks(&format!(
                r#"[{{"type":"table","columns":{columns},"rows":[]}}]"#
            ));
            assert!(parsed.snapshot.summary.blocks.is_empty(), "{columns}");
            assert_eq!(
                paths(&parsed, Severity::Error),
                vec!["summary.blocks[0].columns"],
                "{columns}"
            );
        }
        let parsed = summary_with_blocks(r#"[{"type":"table","rows":[]}]"#);
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec!["summary.blocks[0].columns"]
        );
    }

    #[test]
    fn table_columns_are_limited_to_ten() {
        let columns = [r#"{"title":"c"}"#; 11].join(",");
        let parsed = summary_with_blocks(&format!(
            r#"[{{"type":"table","columns":[{columns}],"rows":[]}}]"#
        ));
        let Block::Table { columns, .. } = first_block(&parsed) else {
            panic!("esperava table");
        };
        assert_eq!(columns.len(), 10);
        assert_eq!(
            paths(&parsed, Severity::Warning),
            vec!["summary.blocks[0].columns"]
        );
    }

    #[test]
    fn table_rows_that_are_not_lists_are_errors_with_path() {
        let parsed = summary_with_blocks(
            r#"[{"type":"table","columns":[{"title":"A"}],"rows":[["ok"],"texto",{"a":1}]}]"#,
        );
        assert!(parsed.snapshot.summary.blocks.is_empty());
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec!["summary.blocks[0].rows[1]", "summary.blocks[0].rows[2]"]
        );
    }

    #[test]
    fn numbers_and_nulls_in_text_fields_are_errors_not_panics() {
        let parsed = summary_with_blocks(
            r#"[{"type":"stat","items":[{"value":5,"label":"x"}]},
                {"type":"lines","lines":[{"text":null}]},
                {"type":"table","columns":[{"title":"A"}],"rows":[[null],[7]]}]"#,
        );
        assert!(parsed.snapshot.summary.blocks.is_empty());
        assert_eq!(
            paths(&parsed, Severity::Error),
            vec![
                "summary.blocks[0].items[0].value",
                "summary.blocks[1].lines[0].text",
                "summary.blocks[2].rows[0][0]",
                "summary.blocks[2].rows[1][0]"
            ]
        );
    }

    #[test]
    fn hostile_keys_are_cleaned_in_problem_paths() {
        let long_key = "k".repeat(300);
        let json = format!(
            "{{\"summary\":{{\"title\":\"t\",\"x\\u001b[31mRED\\u202e\":1,\"{long_key}\":2}},\
              \"detail\":{{\"sections\":[{{\"heading\":\"h\",\"items\":[{{\"text\":\"a\",\
              \"meta\":{{\"m\\u001b]0;pwn\\u0007\":3}}}}]}}]}}}}"
        );
        let parsed = report(&json);
        assert!(!parsed.problems.is_empty());
        for problem in &parsed.problems {
            assert!(
                !problem.path.contains(['\u{1b}', '\u{202e}', '\u{7}']),
                "{:?}",
                problem.path
            );
            assert!(problem.path.chars().count() < 200, "{:?}", problem.path);
        }
    }

    #[test]
    fn legacy_lists_are_capped_with_warnings() {
        let lines = vec![r#"{"text":"x"}"#; MAXIMUM_LEGACY_LINES + 1].join(",");
        let sections = vec![r#"{"heading":"h"}"#; MAXIMUM_SECTIONS + 1].join(",");
        let items = vec![r#"{"text":"i"}"#; MAXIMUM_ITEMS_PER_SECTION + 1].join(",");
        let actions = vec![r#"{"id":"a","label":"A"}"#; MAXIMUM_ACTIONS + 1].join(",");
        let parsed = report(&format!(
            r#"{{"summary":{{"title":"t","lines":[{lines}]}},
                "detail":{{"sections":[{sections},{{"heading":"x","items":[{items}]}}]}},
                "actions":[{actions}]}}"#
        ));
        assert_eq!(parsed.snapshot.summary.lines.len(), MAXIMUM_LEGACY_LINES);
        let sections = parsed.snapshot.detail.as_ref().unwrap();
        assert_eq!(sections.len(), MAXIMUM_SECTIONS);
        assert_eq!(parsed.snapshot.actions.len(), MAXIMUM_ACTIONS);
        let mut warned = paths(&parsed, Severity::Warning);
        warned.sort();
        assert!(warned.contains(&"summary.lines".to_string()), "{warned:?}");
        assert!(
            warned.contains(&"detail.sections".to_string()),
            "{warned:?}"
        );
        assert!(warned.contains(&"actions".to_string()), "{warned:?}");
    }
}
