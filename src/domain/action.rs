use crate::domain::error::DomainError;
use crate::domain::widget::{ActionScope, ActionSpec, WidgetId};

const EMPTY_PARAMS: &str = "{}";

#[derive(Debug, Clone, PartialEq)]
pub struct ProposedAction {
    widget: WidgetId,
    spec: ActionSpec,
    item_id: Option<String>,
}

impl ProposedAction {
    pub fn new(
        widget: WidgetId,
        spec: ActionSpec,
        item_id: Option<String>,
    ) -> Result<Self, DomainError> {
        if spec.scope == ActionScope::Item && item_id.is_none() {
            return Err(DomainError::ItemActionWithoutItem { action_id: spec.id });
        }
        Ok(Self {
            widget,
            spec,
            item_id,
        })
    }

    pub fn item_id(&self) -> Option<&str> {
        self.item_id.as_deref()
    }

    pub fn describe(&self) -> String {
        match &self.spec.description {
            Some(description) => description.clone(),
            None => format!("{} {}", self.spec.label, self.spec.params_json),
        }
    }

    pub fn confirm(self) -> ConfirmedAction {
        ConfirmedAction {
            widget: self.widget,
            spec: self.spec,
            item_id: self.item_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedAction {
    widget: WidgetId,
    spec: ActionSpec,
    item_id: Option<String>,
}

impl ConfirmedAction {
    pub fn widget(&self) -> &WidgetId {
        &self.widget
    }

    pub fn action_id(&self) -> &str {
        &self.spec.id
    }

    pub fn item_id(&self) -> Option<&str> {
        self.item_id.as_deref()
    }

    pub fn params_json_with_item(&self) -> String {
        let params = self.spec.params_json.trim();
        let Some(item_id) = &self.item_id else {
            return params.to_string();
        };
        let item_field = format!("\"item_id\":\"{}\"", escape_json_text(item_id));
        let params = if params.is_empty() {
            EMPTY_PARAMS
        } else {
            params
        };
        let body = params
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .map(str::trim)
            .unwrap_or("");
        if body.is_empty() {
            format!("{{{item_field}}}")
        } else {
            format!("{{{body},{item_field}}}")
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutcome {
    pub message: String,
}

fn escape_json_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", control as u32))
            }
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::DomainError;
    use crate::domain::widget::{ActionScope, ActionSpec, WidgetId};

    fn spec(scope: ActionScope, description: Option<&str>, params: &str) -> ActionSpec {
        ActionSpec {
            id: "move".to_string(),
            label: "Mover para done".to_string(),
            description: description.map(str::to_string),
            scope,
            params_json: params.to_string(),
        }
    }

    fn propose(spec: ActionSpec, item: Option<&str>) -> ProposedAction {
        ProposedAction::new(WidgetId::new("kanban"), spec, item.map(str::to_string)).unwrap()
    }

    #[test]
    fn proposed_action_exposes_item_id() {
        let action = propose(spec(ActionScope::Item, None, "{}"), Some("inbox/a.md"));
        assert_eq!(action.item_id(), Some("inbox/a.md"));
    }

    #[test]
    fn item_action_without_item_is_rejected() {
        let result = ProposedAction::new(
            WidgetId::new("kanban"),
            spec(ActionScope::Item, None, "{}"),
            None,
        );
        assert_eq!(
            result.unwrap_err(),
            DomainError::ItemActionWithoutItem {
                action_id: "move".to_string()
            }
        );
    }

    #[test]
    fn describe_prefers_description_over_label() {
        let action = propose(
            spec(ActionScope::Widget, Some("Mover 'a.md' para done"), "{}"),
            None,
        );
        assert_eq!(action.describe(), "Mover 'a.md' para done");
    }

    #[test]
    fn describe_falls_back_to_label_and_raw_params() {
        let action = propose(spec(ActionScope::Widget, None, r#"{"to":"done"}"#), None);
        assert_eq!(action.describe(), r#"Mover para done {"to":"done"}"#);
    }

    #[test]
    fn params_json_with_item_injects_item_id() {
        let action = propose(
            spec(ActionScope::Item, None, r#"{"to":"done"}"#),
            Some("inbox/a.md"),
        );
        let json = action.confirm().params_json_with_item();
        assert!(json.contains(r#""to":"done""#));
        assert!(json.contains(r#""item_id":"inbox/a.md""#));
    }

    #[test]
    fn params_json_with_item_handles_empty_params() {
        let action = propose(spec(ActionScope::Item, None, ""), Some("a.md"));
        assert_eq!(
            action.confirm().params_json_with_item(),
            r#"{"item_id":"a.md"}"#
        );
    }

    #[test]
    fn params_json_with_item_escapes_quotes_and_backslashes() {
        let action = propose(spec(ActionScope::Item, None, "{}"), Some(r#"a"b\c"#));
        assert_eq!(
            action.confirm().params_json_with_item(),
            r#"{"item_id":"a\"b\\c"}"#
        );
    }

    #[test]
    fn params_json_without_item_is_unchanged() {
        let action = propose(spec(ActionScope::Widget, None, r#"{"x":1}"#), None);
        assert_eq!(action.confirm().params_json_with_item(), r#"{"x":1}"#);
    }
}
