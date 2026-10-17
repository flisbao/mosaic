use crate::application::ports::{ActionExecutor, WidgetSource};
use crate::application::refresh::refresh_widget;
use crate::domain::action::ConfirmedAction;
use crate::domain::widget::WidgetState;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionNotice {
    Succeeded(String),
    Failed(String),
}

pub fn execute_confirmed(
    executor: &dyn ActionExecutor,
    source: &dyn WidgetSource,
    action: ConfirmedAction,
) -> (ActionNotice, WidgetState) {
    let notice = match executor.execute(&action) {
        Ok(outcome) => ActionNotice::Succeeded(outcome.message),
        Err(error) => ActionNotice::Failed(error.to_string()),
    };
    (notice, refresh_widget(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::SourceError;
    use crate::application::refresh::fakes::{snapshot_titled, FakeSource};
    use crate::domain::action::{ActionOutcome, ConfirmedAction, ProposedAction};
    use crate::domain::widget::{ActionScope, ActionSpec, WidgetId, WidgetState};

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
    fn success_returns_message_and_refreshed_state() {
        let executor = FakeExecutor(Ok(ActionOutcome {
            message: "movido".to_string(),
        }));
        let source = FakeSource::returning(Ok(snapshot_titled("Kanban")));

        let (notice, state) = execute_confirmed(&executor, &source, confirmed());

        assert_eq!(notice, ActionNotice::Succeeded("movido".to_string()));
        assert_eq!(state, WidgetState::Ready(snapshot_titled("Kanban")));
        assert_eq!(source.call_count(), 1);
    }

    #[test]
    fn failure_returns_error_notice_and_still_refreshes() {
        let executor = FakeExecutor(Err(SourceError::Failed("destino existe".to_string())));
        let source = FakeSource::returning(Ok(snapshot_titled("Kanban")));

        let (notice, state) = execute_confirmed(&executor, &source, confirmed());

        assert_eq!(notice, ActionNotice::Failed("destino existe".to_string()));
        assert_eq!(state, WidgetState::Ready(snapshot_titled("Kanban")));
        assert_eq!(source.call_count(), 1);
    }
}
