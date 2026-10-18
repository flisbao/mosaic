use std::collections::HashMap;

use crate::application::reload_plan::{ReloadPlan, WidgetChange};
use crate::domain::widget::WidgetId;

pub trait WidgetLifecycle {
    fn start(&mut self, id: &WidgetId, generation: u64);
    fn retire(&mut self, id: &WidgetId);
}

#[derive(Debug, Default)]
pub struct WidgetGenerations {
    last_issued: u64,
    current: HashMap<WidgetId, u64>,
}

impl WidgetGenerations {
    pub fn advance(&mut self, id: &WidgetId) -> u64 {
        self.last_issued += 1;
        self.current.insert(id.clone(), self.last_issued);
        self.last_issued
    }

    pub fn forget(&mut self, id: &WidgetId) {
        self.current.remove(id);
    }

    pub fn accepts(&self, id: &WidgetId, generation: u64) -> bool {
        self.current(id) == Some(generation)
    }

    pub fn current(&self, id: &WidgetId) -> Option<u64> {
        self.current.get(id).copied()
    }
}

pub fn apply_plan(
    plan: &ReloadPlan,
    lifecycle: &mut dyn WidgetLifecycle,
    generations: &mut WidgetGenerations,
) {
    for (id, change) in &plan.changes {
        match change {
            WidgetChange::Keep => {}
            WidgetChange::Remove => {
                lifecycle.retire(id);
                generations.forget(id);
            }
            WidgetChange::Restart => {
                lifecycle.retire(id);
                start_widget(id, lifecycle, generations);
            }
            WidgetChange::Add => start_widget(id, lifecycle, generations),
        }
    }
}

fn start_widget(
    id: &WidgetId,
    lifecycle: &mut dyn WidgetLifecycle,
    generations: &mut WidgetGenerations,
) {
    let generation = generations.advance(id);
    lifecycle.start(id, generation);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct RecordingLifecycle {
        calls: Vec<String>,
    }

    impl WidgetLifecycle for RecordingLifecycle {
        fn start(&mut self, id: &WidgetId, generation: u64) {
            self.calls
                .push(format!("start {} {generation}", id.as_str()));
        }

        fn retire(&mut self, id: &WidgetId) {
            self.calls.push(format!("retire {}", id.as_str()));
        }
    }

    fn plan_with(changes: &[(&str, WidgetChange)]) -> ReloadPlan {
        ReloadPlan {
            changes: changes
                .iter()
                .map(|(id, change)| (WidgetId::new(*id), *change))
                .collect(),
        }
    }

    fn widget(id: &str) -> WidgetId {
        WidgetId::new(id)
    }

    #[test]
    fn keep_leaves_widget_untouched() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let plan = plan_with(&[("jira", WidgetChange::Keep)]);

        apply_plan(&plan, &mut lifecycle, &mut generations);

        assert!(lifecycle.calls.is_empty());
    }

    #[test]
    fn remove_retires_widget_and_rejects_its_results() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        generations.advance(&widget("clima"));
        let generation = generations.advance(&widget("clima"));
        let plan = plan_with(&[("clima", WidgetChange::Remove)]);

        apply_plan(&plan, &mut lifecycle, &mut generations);

        assert_eq!(lifecycle.calls, vec!["retire clima".to_string()]);
        assert!(!generations.accepts(&widget("clima"), generation));
    }

    #[test]
    fn restart_retires_old_then_starts_with_a_new_generation() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let old = generations.advance(&widget("jira"));
        let plan = plan_with(&[("jira", WidgetChange::Restart)]);

        apply_plan(&plan, &mut lifecycle, &mut generations);

        assert_eq!(lifecycle.calls.len(), 2);
        assert_eq!(lifecycle.calls[0], "retire jira");
        assert!(lifecycle.calls[1].starts_with("start jira "));
        assert!(!generations.accepts(&widget("jira"), old));
    }

    #[test]
    fn add_starts_widget_with_accepted_generation() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let plan = plan_with(&[("agenda", WidgetChange::Add)]);

        apply_plan(&plan, &mut lifecycle, &mut generations);

        assert_eq!(lifecycle.calls.len(), 1);
        assert!(lifecycle.calls[0].starts_with("start agenda "));
    }

    #[test]
    fn stale_generation_result_is_ignored() {
        let mut generations = WidgetGenerations::default();
        let stale = generations.advance(&widget("jira"));
        let current = generations.advance(&widget("jira"));

        assert!(!generations.accepts(&widget("jira"), stale));
        assert!(generations.accepts(&widget("jira"), current));
    }

    #[test]
    fn current_generation_is_the_latest_advanced_one() {
        let mut generations = WidgetGenerations::default();
        assert_eq!(generations.current(&widget("jira")), None);

        generations.advance(&widget("jira"));
        let latest = generations.advance(&widget("jira"));

        assert_eq!(generations.current(&widget("jira")), Some(latest));
    }

    #[test]
    fn generation_is_never_reused_after_remove_and_add() {
        let mut generations = WidgetGenerations::default();
        let first = generations.advance(&widget("jira"));
        generations.forget(&widget("jira"));
        let second = generations.advance(&widget("jira"));

        assert_ne!(first, second);
        assert!(!generations.accepts(&widget("jira"), first));
        assert!(generations.accepts(&widget("jira"), second));
    }
}
