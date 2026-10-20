use crate::application::launch_spec::WidgetLaunchSpec;
use crate::domain::widget::WidgetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetChange {
    Keep,
    Restart,
    Add,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadPlan {
    pub changes: Vec<(WidgetId, WidgetChange)>,
}

pub fn plan_reload(current: &[WidgetLaunchSpec], next: &[WidgetLaunchSpec]) -> ReloadPlan {
    let updated = next
        .iter()
        .map(|widget| (widget.id.clone(), change_for(current, widget)));
    let removed = current
        .iter()
        .filter(|widget| !next.iter().any(|candidate| candidate.id == widget.id))
        .map(|widget| (widget.id.clone(), WidgetChange::Remove));
    ReloadPlan {
        changes: updated.chain(removed).collect(),
    }
}

impl ReloadPlan {
    pub fn kept_widget_ids(&self) -> Vec<WidgetId> {
        self.changes
            .iter()
            .filter(|(_, change)| *change == WidgetChange::Keep)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

fn change_for(current: &[WidgetLaunchSpec], next: &WidgetLaunchSpec) -> WidgetChange {
    match current.iter().find(|widget| widget.id == next.id) {
        None => WidgetChange::Add,
        Some(previous) if previous == next => WidgetChange::Keep,
        Some(_) => WidgetChange::Restart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::application::launch_spec::{ProviderCommand, WidgetSchedule};

    const COMMAND: &str = "python3";
    const INTERVAL_SECONDS: u64 = 60;
    const CHANGED_INTERVAL_SECONDS: u64 = 120;
    const TIMEOUT_SECONDS: u64 = 5;
    const CHANGED_TIMEOUT_SECONDS: u64 = 9;

    type Mutation = fn(&mut WidgetLaunchSpec);

    fn identity(id: &str) -> WidgetLaunchSpec {
        WidgetLaunchSpec {
            id: WidgetId::new(id),
            command: ProviderCommand {
                program: COMMAND.to_string(),
                args: vec!["jira.py".to_string(), "my-tasks".to_string()],
                env: vec![("TOKEN".to_string(), "abc".to_string())],
                timeout: Duration::from_secs(TIMEOUT_SECONDS),
            },
            schedule: WidgetSchedule {
                interval: Duration::from_secs(INTERVAL_SECONDS),
                interval_adjusted: false,
            },
        }
    }

    fn change_of(plan: &ReloadPlan, id: &str) -> Option<WidgetChange> {
        plan.changes
            .iter()
            .find(|(widget_id, _)| widget_id.as_str() == id)
            .map(|(_, change)| *change)
    }

    #[test]
    fn identical_settings_keep() {
        let plan = plan_reload(&[identity("jira")], &[identity("jira")]);

        assert_eq!(change_of(&plan, "jira"), Some(WidgetChange::Keep));
    }

    #[test]
    fn changed_command_args_env_interval_or_timeout_restart() {
        let cases: Vec<(&str, Mutation)> = vec![
            ("command", |widget| {
                widget.command.program = "sh".to_string()
            }),
            ("args", |widget| {
                widget.command.args.push("--all".to_string())
            }),
            ("env", |widget| widget.command.env.clear()),
            ("interval", |widget| {
                widget.schedule.interval = Duration::from_secs(CHANGED_INTERVAL_SECONDS)
            }),
            ("interval_adjusted", |widget| {
                widget.schedule.interval_adjusted = true
            }),
            ("timeout", |widget| {
                widget.command.timeout = Duration::from_secs(CHANGED_TIMEOUT_SECONDS)
            }),
        ];
        for (field, change) in cases {
            let mut changed = identity("jira");
            change(&mut changed);

            let plan = plan_reload(&[identity("jira")], &[changed]);

            assert_eq!(
                change_of(&plan, "jira"),
                Some(WidgetChange::Restart),
                "campo {field} deveria reiniciar"
            );
        }
    }

    #[test]
    fn kept_widget_ids_lists_only_keep_changes() {
        let plan = ReloadPlan {
            changes: vec![
                (WidgetId::new("jira"), WidgetChange::Keep),
                (WidgetId::new("clima"), WidgetChange::Restart),
            ],
        };

        assert_eq!(plan.kept_widget_ids(), vec![WidgetId::new("jira")]);
    }

    #[test]
    fn new_id_is_added_and_missing_id_is_removed() {
        let current = [identity("jira"), identity("clima")];
        let next = [identity("jira"), identity("agenda")];

        let plan = plan_reload(&current, &next);

        assert_eq!(change_of(&plan, "jira"), Some(WidgetChange::Keep));
        assert_eq!(change_of(&plan, "agenda"), Some(WidgetChange::Add));
        assert_eq!(change_of(&plan, "clima"), Some(WidgetChange::Remove));
    }
}
