use chrono::NaiveDateTime;
use thiserror::Error;

use crate::domain::widget::{DetailItem, WidgetId, WidgetRecord};

pub const CHARACTERS_PER_TOKEN: usize = 4;
pub const META_VALUE_MAXIMUM_CHARACTERS: usize = 40;

const ELLIPSIS: char = '…';
const MISSING_ID_PLACEHOLDER: &str = "-";

#[derive(Debug, Clone, PartialEq)]
pub struct LlmSelection {
    pub sections: Vec<String>,
    pub max_items: usize,
    pub max_characters_per_item: usize,
}

pub struct WidgetContext<'a> {
    pub widget_id: &'a WidgetId,
    pub selection: &'a LlmSelection,
    pub state: &'a WidgetRecord,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecommendationPlan {
    pub prompt: String,
    pub item_count: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Error, PartialEq)]
pub enum RecommendationError {
    #[error("nenhum widget configurado para a IA; defina `llm_sections`")]
    NoWidgetSelected,
    #[error("nenhum item nas seções selecionadas")]
    NoItems,
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARACTERS_PER_TOKEN)
}

pub fn format_item_line(item: &DetailItem, max_characters: usize) -> String {
    let id = item
        .id
        .as_deref()
        .map(collapse_whitespace)
        .unwrap_or_else(|| MISSING_ID_PLACEHOLDER.to_string());
    let text = truncate_characters(&collapse_whitespace(&item.text), max_characters);
    let meta: Vec<String> = item
        .meta
        .iter()
        .map(|(key, value)| {
            let value =
                truncate_characters(&collapse_whitespace(value), META_VALUE_MAXIMUM_CHARACTERS);
            format!("{}={value}", collapse_whitespace(key))
        })
        .collect();
    if meta.is_empty() {
        format!("{id} | {text}")
    } else {
        format!("{id} | {text} | {}", meta.join(" "))
    }
}

pub fn build_plan(
    criteria: &str,
    now: NaiveDateTime,
    contexts: &[WidgetContext],
) -> Result<RecommendationPlan, RecommendationError> {
    let selected: Vec<&WidgetContext> = contexts
        .iter()
        .filter(|context| !context.selection.sections.is_empty())
        .collect();
    if selected.is_empty() {
        return Err(RecommendationError::NoWidgetSelected);
    }
    let mut groups = Vec::new();
    let mut item_count = 0;
    for context in selected {
        let lines = selected_lines(context);
        if lines.is_empty() {
            continue;
        }
        item_count += lines.len();
        groups.push(format!(
            "## {}\n{}",
            context.widget_id.as_str(),
            lines.join("\n")
        ));
    }
    if item_count == 0 {
        return Err(RecommendationError::NoItems);
    }
    let prompt = format!(
        "{}\n\nAgora: {}\n\n{}\n",
        criteria.trim_end(),
        now.format("%Y-%m-%d %H:%M"),
        groups.join("\n\n")
    );
    Ok(RecommendationPlan {
        estimated_tokens: estimate_tokens(&prompt),
        prompt,
        item_count,
    })
}

fn selected_lines(context: &WidgetContext) -> Vec<String> {
    let Some(snapshot) = context.state.snapshot() else {
        return Vec::new();
    };
    let Some(sections) = &snapshot.detail else {
        return Vec::new();
    };
    sections
        .iter()
        .filter(|section| context.selection.sections.contains(&section.heading))
        .flat_map(|section| section.items.iter())
        .take(context.selection.max_items)
        .map(|item| format_item_line(item, context.selection.max_characters_per_item))
        .collect()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_characters(text: &str, maximum: usize) -> String {
    if text.chars().count() <= maximum {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(maximum.saturating_sub(1)).collect();
    if maximum > 0 {
        truncated.push(ELLIPSIS);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget::{DetailSection, Summary, WidgetId, WidgetRecord, WidgetSnapshot};
    use chrono::NaiveDate;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 17)
            .unwrap()
            .and_hms_opt(8, 30, 0)
            .unwrap()
    }

    fn item(id: Option<&str>, text: &str, meta: &[(&str, &str)]) -> DetailItem {
        DetailItem {
            id: id.map(str::to_string),
            text: text.to_string(),
            flag: None,
            meta: meta
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    fn state(sections: Vec<(&str, Vec<DetailItem>)>) -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(
            Ok(WidgetSnapshot {
                mini: None,
                alerts: vec![],
                summary: Summary {
                    blocks: vec![],
                    title: "Kanban".to_string(),
                    lines: vec![],
                },
                detail: Some(
                    sections
                        .into_iter()
                        .map(|(heading, items)| DetailSection {
                            blocks: vec![],
                            heading: heading.to_string(),
                            items,
                        })
                        .collect(),
                ),
                actions: vec![],
            }),
            now(),
        );
        record
    }

    fn selection(sections: &[&str], max_items: usize) -> LlmSelection {
        LlmSelection {
            sections: sections.iter().map(|s| s.to_string()).collect(),
            max_items,
            max_characters_per_item: 120,
        }
    }

    fn plan_for(
        widget: &str,
        selection: &LlmSelection,
        state: &WidgetRecord,
    ) -> Result<RecommendationPlan, RecommendationError> {
        let id = WidgetId::new(widget);
        build_plan(
            "Critérios.",
            now(),
            &[WidgetContext {
                widget_id: &id,
                selection,
                state,
            }],
        )
    }

    #[test]
    fn estimate_tokens_rounds_up() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("ááááá"), 2);
    }

    #[test]
    fn item_line_has_id_text_and_meta() {
        let line = format_item_line(
            &item(
                Some("inbox/a.md"),
                "Corrigir login",
                &[("prioridade", "alta"), ("jira", "P-1")],
            ),
            120,
        );
        assert_eq!(
            line,
            "inbox/a.md | Corrigir login | prioridade=alta jira=P-1"
        );
    }

    #[test]
    fn item_line_without_id_uses_dash_and_without_meta_omits_third_field() {
        assert_eq!(format_item_line(&item(None, "Algo", &[]), 120), "- | Algo");
    }

    #[test]
    fn item_text_whitespace_is_collapsed_to_one_line() {
        let line = format_item_line(&item(Some("a"), "linha1\n\nlinha2\t\tfim  ", &[]), 120);
        assert_eq!(line, "a | linha1 linha2 fim");
    }

    #[test]
    fn item_text_is_truncated_with_ellipsis() {
        let line = format_item_line(&item(Some("a"), &"x".repeat(500), &[]), 10);
        assert_eq!(line, format!("a | {}…", "x".repeat(9)));
    }

    #[test]
    fn meta_values_are_collapsed_and_truncated_to_forty() {
        let value = format!("{}\n{}", "y".repeat(30), "z".repeat(30));
        let line = format_item_line(&item(Some("a"), "t", &[("nota", &value)]), 120);
        let meta_value = line.rsplit("nota=").next().unwrap();
        assert_eq!(meta_value.chars().count(), META_VALUE_MAXIMUM_CHARACTERS);
        assert!(meta_value.ends_with('…'));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn plan_includes_only_selected_sections() {
        let widget = state(vec![
            ("inbox", vec![item(Some("i1"), "Dentro", &[])]),
            ("done", vec![item(Some("d1"), "Fora", &[])]),
        ]);
        let plan = plan_for("kanban", &selection(&["inbox"], 20), &widget).unwrap();
        assert!(plan.prompt.contains("i1 | Dentro"));
        assert!(!plan.prompt.contains("Fora"));
        assert_eq!(plan.item_count, 1);
    }

    #[test]
    fn plan_respects_max_items() {
        let widget = state(vec![(
            "inbox",
            vec![
                item(Some("1"), "um", &[]),
                item(Some("2"), "dois", &[]),
                item(Some("3"), "tres", &[]),
            ],
        )]);
        let plan = plan_for("kanban", &selection(&["inbox"], 2), &widget).unwrap();
        assert_eq!(plan.item_count, 2);
        assert!(!plan.prompt.contains("tres"));
    }

    #[test]
    fn plan_groups_items_by_widget_with_criteria_and_time_first() {
        let widget = state(vec![("inbox", vec![item(Some("1"), "um", &[])])]);
        let plan = plan_for("kanban", &selection(&["inbox"], 20), &widget).unwrap();
        let criteria = plan.prompt.find("Critérios.").unwrap();
        let time = plan.prompt.find("Agora: 2026-10-17 08:30").unwrap();
        let group = plan.prompt.find("## kanban").unwrap();
        assert!(criteria < time && time < group);
    }

    #[test]
    fn plan_counts_items_and_tokens_of_the_prompt() {
        let widget = state(vec![("inbox", vec![item(Some("1"), "um", &[])])]);
        let plan = plan_for("kanban", &selection(&["inbox"], 20), &widget).unwrap();
        assert_eq!(plan.estimated_tokens, estimate_tokens(&plan.prompt));
        assert!(plan.estimated_tokens > 0);
    }

    #[test]
    fn widget_without_sections_is_skipped() {
        let widget = state(vec![("inbox", vec![item(Some("1"), "um", &[])])]);
        let result = plan_for("kanban", &selection(&[], 20), &widget);
        assert_eq!(result.unwrap_err(), RecommendationError::NoWidgetSelected);
    }

    #[test]
    fn no_selected_widget_is_an_error() {
        assert_eq!(
            build_plan("c", now(), &[]).unwrap_err(),
            RecommendationError::NoWidgetSelected
        );
    }

    #[test]
    fn selected_sections_without_items_is_an_error() {
        let widget = state(vec![("inbox", vec![])]);
        let result = plan_for("kanban", &selection(&["inbox"], 20), &widget);
        assert_eq!(result.unwrap_err(), RecommendationError::NoItems);
    }

    #[test]
    fn loading_and_failed_widgets_are_skipped() {
        let mut failing = WidgetRecord::loading();
        failing.apply::<String>(Err("x".to_string()), now());
        for widget in [WidgetRecord::loading(), failing] {
            let result = plan_for("kanban", &selection(&["inbox"], 20), &widget);
            assert_eq!(result.unwrap_err(), RecommendationError::NoItems);
        }
    }
}
