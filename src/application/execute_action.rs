use crate::application::ports::ActionExecutor;
use crate::domain::action::ConfirmedAction;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionNotice {
    Succeeded(String),
    Failed(String),
}

pub fn execute_confirmed(executor: &dyn ActionExecutor, action: ConfirmedAction) -> ActionNotice {
    match executor.execute(&action) {
        Ok(outcome) => ActionNotice::Succeeded(outcome.message),
        Err(error) => ActionNotice::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::SourceError;
    use crate::domain::action::{ActionOutcome, ConfirmedAction, ProposedAction};
    use crate::domain::widget::{ActionScope, ActionSpec, WidgetId};

    struct FakeExecutor(Result<ActionOutcome, SourceError>);

    impl ActionExecutor for FakeExecutor {
        fn execute(&self, _action: &ConfirmedAction) -> Result<ActionOutcome, SourceError> {
            self.0.clone()
        }
    }

    fn confirmed() -> ConfirmedAction {
        let spec = ActionSpec {
            id: "move".to_string(),
            label: "Mover".to_string(),
            description: None,
            scope: ActionScope::Widget,
            params_json: "{}".to_string(),
        };
        ProposedAction::new(WidgetId::new("kanban"), spec, None)
            .unwrap()
            .confirm()
    }

    #[test]
    fn success_returns_the_message() {
        let executor = FakeExecutor(Ok(ActionOutcome {
            message: "movido".to_string(),
        }));

        let notice = execute_confirmed(&executor, confirmed());

        assert_eq!(notice, ActionNotice::Succeeded("movido".to_string()));
    }

    #[test]
    fn failure_returns_an_error_notice() {
        let executor = FakeExecutor(Err(SourceError::Failed("destino existe".to_string())));

        let notice = execute_confirmed(&executor, confirmed());

        assert_eq!(notice, ActionNotice::Failed("destino existe".to_string()));
    }
}
