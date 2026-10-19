use crate::domain::error::DomainError;
use crate::domain::widget::{ActionScope, ActionSpec, WidgetId};

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

    pub fn params_json(&self) -> &str {
        self.spec.params_json.trim()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutcome {
    pub message: String,
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
    fn confirmed_action_exposes_raw_params_and_item() {
        let action = propose(
            spec(ActionScope::Item, None, r#" {"to":"done"} "#),
            Some("inbox/a.md"),
        )
        .confirm();
        assert_eq!(action.params_json(), r#"{"to":"done"}"#);
        assert_eq!(action.item_id(), Some("inbox/a.md"));
    }
}
