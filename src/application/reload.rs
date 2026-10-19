use std::collections::HashMap;

use crate::application::reload_plan::{ReloadPlan, WidgetChange};
use crate::domain::widget::WidgetId;

pub trait WidgetSpec {
    fn widget_id(&self) -> &WidgetId;
}

pub trait WidgetLifecycle {
    type Spec: WidgetSpec;

    fn start(&mut self, spec: &Self::Spec, generation: u64);
    fn retire(&mut self, id: &WidgetId);
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedWidgets<S, C> {
    pub config: C,
    pub plan: ReloadPlan,
    pub specs: Vec<S>,
}

#[derive(Debug, Default)]
pub struct WidgetGenerations {
    last_issued: u64,
    current: HashMap<WidgetId, u64>,
}

impl WidgetGenerations {
    pub fn advance(&mut self, id: &WidgetId) -> u64 {
        self.last_issued = self.last_issued.saturating_add(1);
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

pub fn apply_if_current<T>(
    generations: &WidgetGenerations,
    id: &WidgetId,
    generation: u64,
    result: T,
    apply: impl FnOnce(T),
) -> bool {
    if !generations.accepts(id, generation) {
        return false;
    }
    apply(result);
    true
}

pub fn apply_plan<L: WidgetLifecycle>(
    plan: &ReloadPlan,
    specs: &[L::Spec],
    lifecycle: &mut L,
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
                start_widget(id, specs, lifecycle, generations);
            }
            WidgetChange::Add => start_widget(id, specs, lifecycle, generations),
        }
    }
}

fn start_widget<L: WidgetLifecycle>(
    id: &WidgetId,
    specs: &[L::Spec],
    lifecycle: &mut L,
    generations: &mut WidgetGenerations,
) {
    let Some(spec) = specs.iter().find(|spec| spec.widget_id() == id) else {
        return;
    };
    let generation = generations.advance(id);
    lifecycle.start(spec, generation);
}

pub fn reload_widgets<L, C, E>(
    text: Result<String, E>,
    prepare: impl FnOnce(&str) -> Result<PreparedWidgets<L::Spec, C>, E>,
    lifecycle: &mut L,
    generations: &mut WidgetGenerations,
) -> Result<(C, ReloadPlan), E>
where
    L: WidgetLifecycle,
{
    let text = text?;
    let prepared = prepare(&text)?;
    apply_plan(&prepared.plan, &prepared.specs, lifecycle, generations);
    Ok((prepared.config, prepared.plan))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::application::ports::SourceError;
    use crate::application::refresh::fakes::snapshot_titled;
    use crate::domain::widget::{WidgetRecord, WidgetSnapshot};

    #[derive(Debug, Clone, PartialEq)]
    struct FakeSpec {
        id: WidgetId,
    }

    impl WidgetSpec for FakeSpec {
        fn widget_id(&self) -> &WidgetId {
            &self.id
        }
    }

    #[derive(Debug, Default)]
    struct RecordingLifecycle {
        calls: Vec<String>,
    }

    impl WidgetLifecycle for RecordingLifecycle {
        type Spec = FakeSpec;

        fn start(&mut self, spec: &FakeSpec, generation: u64) {
            self.calls
                .push(format!("start {} {generation}", spec.id.as_str()));
        }

        fn retire(&mut self, id: &WidgetId) {
            self.calls.push(format!("retire {}", id.as_str()));
        }
    }

    fn widget(id: &str) -> WidgetId {
        WidgetId::new(id)
    }

    fn spec(id: &str) -> FakeSpec {
        FakeSpec { id: widget(id) }
    }

    fn plan_with(changes: &[(&str, WidgetChange)]) -> ReloadPlan {
        ReloadPlan {
            changes: changes
                .iter()
                .map(|(id, change)| (widget(id), *change))
                .collect(),
        }
    }

    fn now() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 18)
            .and_then(|date| date.and_hms_opt(12, 0, 0))
            .unwrap()
    }

    fn title_of(record: &WidgetRecord) -> Option<String> {
        record
            .snapshot()
            .map(|snapshot: &WidgetSnapshot| snapshot.summary.title.clone())
    }

    #[test]
    fn keep_leaves_widget_untouched() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let plan = plan_with(&[("jira", WidgetChange::Keep)]);

        apply_plan(&plan, &[spec("jira")], &mut lifecycle, &mut generations);

        assert!(lifecycle.calls.is_empty());
    }

    #[test]
    fn remove_retires_widget_and_rejects_its_results() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let generation = generations.advance(&widget("clima"));
        let plan = plan_with(&[("clima", WidgetChange::Remove)]);

        apply_plan(&plan, &[], &mut lifecycle, &mut generations);

        assert_eq!(lifecycle.calls, vec!["retire clima".to_string()]);
        assert!(!generations.accepts(&widget("clima"), generation));
    }

    #[test]
    fn restart_retires_old_then_starts_with_a_new_generation() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let old = generations.advance(&widget("jira"));
        let plan = plan_with(&[("jira", WidgetChange::Restart)]);

        apply_plan(&plan, &[spec("jira")], &mut lifecycle, &mut generations);

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

        apply_plan(&plan, &[spec("agenda")], &mut lifecycle, &mut generations);

        assert_eq!(lifecycle.calls.len(), 1);
        assert!(lifecycle.calls[0].starts_with("start agenda "));
        assert!(generations.current(&widget("agenda")).is_some());
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

    #[test]
    fn stale_result_after_restart_does_not_change_the_record() {
        let mut generations = WidgetGenerations::default();
        let mut records: HashMap<WidgetId, WidgetRecord> = HashMap::new();
        records.insert(widget("jira"), WidgetRecord::loading());
        let old = generations.advance(&widget("jira"));
        generations.advance(&widget("jira"));
        let mut write = |result: Result<WidgetSnapshot, SourceError>| {
            records
                .get_mut(&widget("jira"))
                .unwrap()
                .apply(result, now());
        };

        let applied_stale = apply_if_current(
            &generations,
            &widget("jira"),
            old,
            Ok(snapshot_titled("velho")),
            &mut write,
        );

        assert!(!applied_stale);
        assert_eq!(title_of(&records[&widget("jira")]), None);
    }

    #[test]
    fn current_generation_result_updates_the_record() {
        let mut generations = WidgetGenerations::default();
        let mut records: HashMap<WidgetId, WidgetRecord> = HashMap::new();
        records.insert(widget("jira"), WidgetRecord::loading());
        generations.advance(&widget("jira"));
        let current = generations.advance(&widget("jira"));
        let mut write = |result: Result<WidgetSnapshot, SourceError>| {
            records
                .get_mut(&widget("jira"))
                .unwrap()
                .apply(result, now());
        };

        let applied = apply_if_current(
            &generations,
            &widget("jira"),
            current,
            Ok(snapshot_titled("novo")),
            &mut write,
        );

        assert!(applied);
        assert_eq!(
            title_of(&records[&widget("jira")]),
            Some("novo".to_string())
        );
    }

    fn prepared(
        plan: ReloadPlan,
        specs: Vec<FakeSpec>,
    ) -> Result<PreparedWidgets<FakeSpec, String>, String> {
        Ok(PreparedWidgets {
            config: "nova".to_string(),
            plan,
            specs,
        })
    }

    #[test]
    fn unreadable_config_makes_no_lifecycle_calls_and_no_preparation() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        let mut prepare_called = false;

        let result = reload_widgets(
            Err::<String, String>("ilegível".to_string()),
            |_| {
                prepare_called = true;
                prepared(ReloadPlan { changes: vec![] }, vec![])
            },
            &mut lifecycle,
            &mut generations,
        );

        assert_eq!(result.err(), Some("ilegível".to_string()));
        assert!(!prepare_called);
        assert!(lifecycle.calls.is_empty());
    }

    #[test]
    fn invalid_config_makes_no_lifecycle_calls_and_keeps_widgets_generations() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        generations.advance(&widget("jira"));

        let result = reload_widgets(
            Ok("texto".to_string()),
            |_| Err::<PreparedWidgets<FakeSpec, String>, String>("linha 3".to_string()),
            &mut lifecycle,
            &mut generations,
        );

        assert_eq!(result.err(), Some("linha 3".to_string()));
        assert!(lifecycle.calls.is_empty());
        assert!(generations.current(&widget("jira")).is_some());
    }

    #[test]
    fn valid_reload_retires_before_starting_and_returns_the_config() {
        let mut lifecycle = RecordingLifecycle::default();
        let mut generations = WidgetGenerations::default();
        generations.advance(&widget("jira"));
        let plan = plan_with(&[
            ("jira", WidgetChange::Restart),
            ("clima", WidgetChange::Keep),
        ]);

        let result = reload_widgets(
            Ok("texto".to_string()),
            |_| prepared(plan.clone(), vec![spec("jira"), spec("clima")]),
            &mut lifecycle,
            &mut generations,
        );

        assert_eq!(lifecycle.calls.len(), 2, "{:?}", lifecycle.calls);
        assert_eq!(lifecycle.calls[0], "retire jira");
        assert!(lifecycle.calls[1].starts_with("start jira "));
        let (config, returned_plan) = result.unwrap();
        assert_eq!(config, "nova");
        assert_eq!(returned_plan, plan);
    }
}
