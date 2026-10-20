use std::collections::HashSet;

use crate::domain::widget::WidgetId;

#[derive(Debug, Default)]
pub(super) struct PendingActions {
    widgets: HashSet<WidgetId>,
}

impl PendingActions {
    pub(super) fn try_begin(&mut self, widget: &WidgetId) -> bool {
        self.widgets.insert(widget.clone())
    }

    pub(super) fn finish(&mut self, widget: &WidgetId) {
        self.widgets.remove(widget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_widget_can_have_only_one_pending_action() {
        let mut pending = PendingActions::default();
        let widget = WidgetId::new("jira");
        assert!(pending.try_begin(&widget));
        assert!(!pending.try_begin(&widget));
    }

    #[test]
    fn finishing_frees_the_widget_and_other_widgets_are_independent() {
        let mut pending = PendingActions::default();
        let jira = WidgetId::new("jira");
        let kanban = WidgetId::new("kanban");
        assert!(pending.try_begin(&jira));
        assert!(pending.try_begin(&kanban));
        pending.finish(&jira);
        assert!(pending.try_begin(&jira));
        assert!(!pending.try_begin(&kanban));
    }
}
