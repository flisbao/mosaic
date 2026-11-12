const LLM_SECTION_LABEL: &str = "[llm]";
const POMODORO_SECTION_LABEL: &str = "[pomodoro]/notificações";
const UPDATE_SECTION_LABEL: &str = "[update]";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestartSections {
    pub llm_changed: bool,
    pub pomodoro_changed: bool,
    pub update_changed: bool,
}

impl RestartSections {
    pub fn labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.llm_changed {
            labels.push(LLM_SECTION_LABEL);
        }
        if self.pomodoro_changed {
            labels.push(POMODORO_SECTION_LABEL);
        }
        if self.update_changed {
            labels.push(UPDATE_SECTION_LABEL);
        }
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_change_has_no_labels() {
        assert!(RestartSections::default().labels().is_empty());
    }

    #[test]
    fn labels_follow_llm_then_pomodoro_order() {
        let all = RestartSections {
            llm_changed: true,
            pomodoro_changed: true,
            update_changed: true,
        };
        assert_eq!(
            all.labels(),
            vec!["[llm]", "[pomodoro]/notificações", "[update]"]
        );
    }
}
