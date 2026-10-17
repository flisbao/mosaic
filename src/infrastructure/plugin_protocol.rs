use serde::Deserialize;
use thiserror::Error;

use crate::domain::widget::{
    ActionScope, ActionSpec, DetailItem, DetailSection, Flag, Summary, SummaryLine, WidgetSnapshot,
};

const EMPTY_PARAMS_JSON: &str = "{}";

#[derive(Debug, Error)]
#[error("saída de plugin não é um JSON válido de snapshot: {0}")]
pub struct ProtocolError(String);

#[derive(Deserialize)]
struct RawSnapshot {
    summary: RawSummary,
    detail: Option<RawDetail>,
    #[serde(default)]
    actions: Vec<RawAction>,
}

#[derive(Deserialize)]
struct RawSummary {
    title: String,
    #[serde(default)]
    lines: Vec<RawLine>,
}

#[derive(Deserialize)]
struct RawLine {
    text: String,
    flag: Option<String>,
}

#[derive(Deserialize)]
struct RawDetail {
    #[serde(default)]
    sections: Vec<RawSection>,
}

#[derive(Deserialize)]
struct RawSection {
    heading: String,
    #[serde(default)]
    items: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    id: Option<String>,
    text: String,
    flag: Option<String>,
    meta: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Deserialize)]
struct RawAction {
    id: String,
    label: String,
    description: Option<String>,
    applies_to: Option<String>,
    params: Option<serde_json::Value>,
}

pub fn parse_snapshot(json: &str) -> Result<WidgetSnapshot, ProtocolError> {
    let raw: RawSnapshot =
        serde_json::from_str(json).map_err(|error| ProtocolError(error.to_string()))?;
    Ok(WidgetSnapshot {
        summary: Summary {
            title: raw.summary.title,
            lines: raw
                .summary
                .lines
                .into_iter()
                .map(|line| SummaryLine {
                    text: line.text,
                    flag: parse_flag(line.flag.as_deref()),
                })
                .collect(),
        },
        detail: raw.detail.map(|detail| {
            detail
                .sections
                .into_iter()
                .map(|section| DetailSection {
                    heading: section.heading,
                    items: section
                        .items
                        .into_iter()
                        .map(|item| DetailItem {
                            id: item.id,
                            text: item.text,
                            flag: parse_flag(item.flag.as_deref()),
                            meta: text_meta(item.meta),
                        })
                        .collect(),
                })
                .collect()
        }),
        actions: raw.actions.into_iter().map(convert_action).collect(),
    })
}

fn convert_action(raw: RawAction) -> ActionSpec {
    ActionSpec {
        id: raw.id,
        label: raw.label,
        description: raw.description,
        scope: match raw.applies_to.as_deref() {
            Some("item") => ActionScope::Item,
            _ => ActionScope::Widget,
        },
        params_json: raw
            .params
            .map(|params| params.to_string())
            .unwrap_or_else(|| EMPTY_PARAMS_JSON.to_string()),
    }
}

fn text_meta(meta: Option<serde_json::Map<String, serde_json::Value>>) -> Vec<(String, String)> {
    meta.into_iter()
        .flatten()
        .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string())))
        .collect()
}

fn parse_flag(flag: Option<&str>) -> Option<Flag> {
    match flag {
        Some("updated") => Some(Flag::Updated),
        Some("warning") => Some(Flag::Warning),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget::{ActionScope, Flag};

    #[test]
    fn parses_summary_only_snapshot() {
        let snapshot =
            parse_snapshot(r#"{"summary":{"title":"Kanban","lines":[{"text":"inbox 3"}]}}"#)
                .unwrap();
        assert_eq!(snapshot.summary.title, "Kanban");
        assert_eq!(snapshot.summary.lines[0].text, "inbox 3");
        assert_eq!(snapshot.summary.lines[0].flag, None);
        assert_eq!(snapshot.detail, None);
        assert!(snapshot.actions.is_empty());
    }

    #[test]
    fn parses_detail_items_with_ids_and_flags() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[{"text":"x","flag":"updated"}]},
                "detail":{"sections":[{"heading":"inbox","items":[
                    {"id":"inbox/a.md","text":"Tarefa A","flag":"warning"},
                    {"text":"sem id"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(snapshot.summary.lines[0].flag, Some(Flag::Updated));
        let sections = snapshot.detail.unwrap();
        assert_eq!(sections[0].heading, "inbox");
        assert_eq!(sections[0].items[0].id.as_deref(), Some("inbox/a.md"));
        assert_eq!(sections[0].items[0].flag, Some(Flag::Warning));
        assert_eq!(sections[0].items[1].id, None);
    }

    #[test]
    fn parses_item_action_with_params() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "actions":[{"id":"move","label":"Mover","description":"Mover para done",
                            "applies_to":"item","params":{"to":"done"}},
                           {"id":"refresh","label":"Atualizar"}]}"#,
        )
        .unwrap();
        let move_action = &snapshot.actions[0];
        assert_eq!(move_action.scope, ActionScope::Item);
        assert_eq!(move_action.description.as_deref(), Some("Mover para done"));
        assert!(move_action.params_json.contains("\"to\""));
        let refresh = &snapshot.actions[1];
        assert_eq!(refresh.scope, ActionScope::Widget);
        assert_eq!(refresh.params_json, "{}");
    }

    #[test]
    fn parses_item_meta_in_order() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "detail":{"sections":[{"heading":"h","items":[
                    {"text":"x","meta":{"prioridade":"alta","parado_ha":"2h30"}}]}]}}"#,
        )
        .unwrap();
        let item = &snapshot.detail.unwrap()[0].items[0];
        assert_eq!(
            item.meta,
            vec![
                ("prioridade".to_string(), "alta".to_string()),
                ("parado_ha".to_string(), "2h30".to_string())
            ]
        );
    }

    #[test]
    fn missing_meta_is_empty() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "detail":{"sections":[{"heading":"h","items":[{"text":"x"}]}]}}"#,
        )
        .unwrap();
        assert!(snapshot.detail.unwrap()[0].items[0].meta.is_empty());
    }

    #[test]
    fn non_text_meta_values_are_ignored() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "detail":{"sections":[{"heading":"h","items":[
                    {"text":"x","meta":{"a":"x","b":3}}]}]}}"#,
        )
        .unwrap();
        let item = &snapshot.detail.unwrap()[0].items[0];
        assert_eq!(item.meta, vec![("a".to_string(), "x".to_string())]);
    }

    #[test]
    fn unknown_flag_is_ignored() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[{"text":"x","flag":"explosion"}]}}"#,
        )
        .unwrap();
        assert_eq!(snapshot.summary.lines[0].flag, None);
    }

    #[test]
    fn truncated_json_is_an_error() {
        let error = parse_snapshot(r#"{"summary":{"title":"K""#).unwrap_err();
        assert!(error.to_string().contains("JSON"));
    }

    #[test]
    fn missing_summary_title_is_an_error() {
        assert!(parse_snapshot(r#"{"summary":{"lines":[]}}"#).is_err());
    }
}
