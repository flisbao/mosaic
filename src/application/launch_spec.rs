use std::time::Duration;

use crate::application::ports::WidgetSpec;
use crate::domain::widget::WidgetId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub inherit_env: Option<Vec<String>>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetSchedule {
    pub interval: Duration,
    pub interval_adjusted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetLaunchSpec {
    pub id: WidgetId,
    pub command: ProviderCommand,
    pub schedule: WidgetSchedule,
}

impl WidgetSpec for WidgetLaunchSpec {
    fn widget_id(&self) -> &WidgetId {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> WidgetLaunchSpec {
        WidgetLaunchSpec {
            id: WidgetId::new(id),
            command: ProviderCommand {
                program: "python3".to_string(),
                args: vec!["jira.py".to_string()],
                env: vec![],
                inherit_env: None,
                timeout: Duration::from_secs(5),
            },
            schedule: WidgetSchedule {
                interval: Duration::from_secs(60),
                interval_adjusted: false,
            },
        }
    }

    #[test]
    fn widget_id_is_the_spec_id() {
        assert_eq!(spec("jira").widget_id(), &WidgetId::new("jira"));
    }

    #[test]
    fn specs_with_different_timeouts_are_different() {
        let mut other = spec("jira");
        other.command.timeout = Duration::from_secs(9);
        assert_ne!(spec("jira"), other);
    }
}
