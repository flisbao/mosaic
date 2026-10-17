use crate::domain::pomodoro::{Phase, PhaseEnded};

const FOCUS_ENDED_TITLE: &str = "Foco concluído";
const BREAK_ENDED_TITLE: &str = "Descanso terminou";
const DEFAULT_REST_BODY: &str = "Hora de descansar";
const BREAK_ENDED_BODY: &str = "Volte ao foco ou troque de tarefa (p inicia)";

#[derive(Debug, Clone, PartialEq)]
pub struct AlertMessage {
    pub title: String,
    pub body: String,
}

pub trait Notifier: Send + Sync {
    fn notify(&self, message: &AlertMessage);
}

pub fn alert_for(ended: &PhaseEnded) -> AlertMessage {
    match ended.finished {
        Phase::Focus => {
            let headline = ended.suggestion.as_deref().unwrap_or(DEFAULT_REST_BODY);
            AlertMessage {
                title: FOCUS_ENDED_TITLE.to_string(),
                body: format!("{headline}\nPróximo: {} (p inicia)", phase_name(ended.next)),
            }
        }
        Phase::ShortBreak | Phase::LongBreak => AlertMessage {
            title: BREAK_ENDED_TITLE.to_string(),
            body: BREAK_ENDED_BODY.to_string(),
        },
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Focus => "foco",
        Phase::ShortBreak => "descanso curto",
        Phase::LongBreak => "descanso longo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focus_ended(next: Phase, suggestion: Option<&str>) -> PhaseEnded {
        PhaseEnded {
            finished: Phase::Focus,
            next,
            suggestion: suggestion.map(str::to_string),
        }
    }

    #[test]
    fn focus_end_shows_suggestion_and_next_phase() {
        let alert = alert_for(&focus_ended(Phase::ShortBreak, Some("Beber água")));
        assert_eq!(alert.title, "Foco concluído");
        assert_eq!(alert.body, "Beber água\nPróximo: descanso curto (p inicia)");
    }

    #[test]
    fn focus_end_without_suggestion_says_time_to_rest() {
        let alert = alert_for(&focus_ended(Phase::ShortBreak, None));
        assert!(alert.body.starts_with("Hora de descansar\n"));
    }

    #[test]
    fn long_break_is_named_in_the_next_phase_line() {
        let alert = alert_for(&focus_ended(Phase::LongBreak, Some("Levantar")));
        assert!(alert.body.ends_with("Próximo: descanso longo (p inicia)"));
    }

    #[test]
    fn break_end_asks_to_return_to_focus_or_switch_task() {
        let ended = PhaseEnded {
            finished: Phase::ShortBreak,
            next: Phase::Focus,
            suggestion: None,
        };
        let alert = alert_for(&ended);
        assert_eq!(alert.title, "Descanso terminou");
        assert_eq!(alert.body, "Volte ao foco ou troque de tarefa (p inicia)");
    }
}
