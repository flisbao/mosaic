use serde_json::{Map, Value};

use crate::domain::card::{clean_text, clean_text_limited, Alert, AlertLevel};
use crate::domain::widget::{
    ActionScope, ActionSpec, DetailItem, DetailSection, Flag, Summary, SummaryLine, WidgetSnapshot,
};
use crate::infrastructure::plugin_protocol::ProtocolError;

pub const SUPPORTED_PROTOCOL_VERSION: u64 = 1;
pub const MAXIMUM_TEXT_CHARACTERS: usize = 500;
pub const MAXIMUM_ALERT_ID_CHARACTERS: usize = 100;
pub const MAXIMUM_ALERTS_PER_SNAPSHOT: usize = 100;

const EMPTY_PARAMS_JSON: &str = "{}";
const UNKNOWN_FIELD_MESSAGE: &str = "campo desconhecido";
const ROOT_KEYS: [&str; 5] = ["protocol", "summary", "detail", "actions", "alerts"];
const SUMMARY_KEYS: [&str; 3] = ["title", "lines", "blocks"];
const DETAIL_KEYS: [&str; 1] = ["sections"];
const SECTION_KEYS: [&str; 3] = ["heading", "items", "blocks"];
const ITEM_KEYS: [&str; 4] = ["id", "text", "flag", "meta"];
const ACTION_KEYS: [&str; 5] = ["id", "label", "description", "applies_to", "params"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSnapshot {
    pub snapshot: WidgetSnapshot,
    pub problems: Vec<Problem>,
}

pub fn unsupported_protocol_text(version: u64) -> String {
    format!("este widget exige o protocolo {version}; atualize o panel-tui")
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
        blocks: vec![],
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

fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

fn indexed_path(parent: &str, index: usize) -> String {
    format!("{parent}[{index}]")
}

#[derive(Default)]
struct Reader {
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

    fn error(&mut self, path: &str, message: &str) {
        self.report(path, message, Severity::Error);
    }

    fn warning(&mut self, path: &str, message: &str) {
        self.report(path, message, Severity::Warning);
    }

    fn unknown_fields(&mut self, object: &Map<String, Value>, known: &[&str], path: &str) {
        for key in object.keys().filter(|key| !known.contains(&key.as_str())) {
            self.warning(&child_path(path, key), UNKNOWN_FIELD_MESSAGE);
        }
    }

    fn limited(&mut self, text: &str, path: &str) -> String {
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

    fn required_text(
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

    fn optional_text(
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

    fn flag(&mut self, object: &Map<String, Value>, path: &str) -> Option<Flag> {
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

    fn object_entry<'a>(&mut self, entry: &'a Value, path: &str) -> Option<&'a Map<String, Value>> {
        let object = entry.as_object();
        if object.is_none() {
            self.error(path, "precisa ser um objeto");
        }
        object
    }

    fn legacy_lines(&mut self, summary: &Map<String, Value>) -> Vec<SummaryLine> {
        let Some(entries) = self.list(summary, "lines", "summary") else {
            return vec![];
        };
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
        let items_path = child_path(path, "items");
        let mut items = vec![];
        for (index, item) in self
            .list(object, "items", path)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(item) = self.item(item, &indexed_path(&items_path, index)) {
                items.push(item);
            }
        }
        Some(DetailSection {
            heading,
            items,
            blocks: vec![],
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
                    let value = self.limited(text, &child_path(&meta_path, key));
                    pairs.push((clean_text(key), value));
                }
                _ => self.warning(&child_path(&meta_path, key), "precisa ser texto; ignorado"),
            }
        }
        pairs
    }

    fn actions(&mut self, root: &Map<String, Value>) -> Vec<ActionSpec> {
        let Some(entries) = self.list(root, "actions", "") else {
            return vec![];
        };
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
    use crate::domain::card::AlertLevel;
    use crate::domain::widget::Flag;

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
            "este widget exige o protocolo 2; atualize o panel-tui"
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
}
