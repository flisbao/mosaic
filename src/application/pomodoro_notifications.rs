use crate::application::ports::AlertMessage;
use crate::domain::language::Language;
use crate::domain::pomodoro::{Phase, PhaseEnded};

pub fn alert_for(ended: &PhaseEnded, language: Language) -> AlertMessage {
    match ended.finished {
        Phase::Focus => {
            let headline = ended
                .suggestion
                .as_deref()
                .unwrap_or_else(|| default_rest_body(language));
            let next = phase_label(ended.next, language);
            AlertMessage {
                title: focus_ended_title(language).to_string(),
                body: match language {
                    Language::English => format!("{headline}\nNext: {next} (p starts)"),
                    Language::BrazilianPortuguese => {
                        format!("{headline}\nPróximo: {next} (p inicia)")
                    }
                },
            }
        }
        Phase::ShortBreak | Phase::LongBreak => AlertMessage {
            title: break_ended_title(language).to_string(),
            body: break_ended_body(language).to_string(),
        },
    }
}

fn focus_ended_title(language: Language) -> &'static str {
    match language {
        Language::English => "Focus finished",
        Language::BrazilianPortuguese => "Foco concluído",
    }
}

fn break_ended_title(language: Language) -> &'static str {
    match language {
        Language::English => "Break is over",
        Language::BrazilianPortuguese => "Descanso terminou",
    }
}

fn default_rest_body(language: Language) -> &'static str {
    match language {
        Language::English => "Time to rest",
        Language::BrazilianPortuguese => "Hora de descansar",
    }
}

fn break_ended_body(language: Language) -> &'static str {
    match language {
        Language::English => "Back to focus or switch tasks (p starts)",
        Language::BrazilianPortuguese => "Volte ao foco ou troque de tarefa (p inicia)",
    }
}

pub fn phase_label(phase: Phase, language: Language) -> &'static str {
    match (phase, language) {
        (Phase::Focus, Language::English) => "focus",
        (Phase::ShortBreak, Language::English) => "short break",
        (Phase::LongBreak, Language::English) => "long break",
        (Phase::Focus, Language::BrazilianPortuguese) => "foco",
        (Phase::ShortBreak, Language::BrazilianPortuguese) => "descanso curto",
        (Phase::LongBreak, Language::BrazilianPortuguese) => "descanso longo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_labels_are_the_user_facing_names() {
        assert_eq!(
            phase_label(Phase::Focus, Language::BrazilianPortuguese),
            "foco"
        );
        assert_eq!(
            phase_label(Phase::ShortBreak, Language::BrazilianPortuguese),
            "descanso curto"
        );
        assert_eq!(
            phase_label(Phase::LongBreak, Language::BrazilianPortuguese),
            "descanso longo"
        );
    }

    fn focus_ended(next: Phase, suggestion: Option<&str>) -> PhaseEnded {
        PhaseEnded {
            finished: Phase::Focus,
            next,
            suggestion: suggestion.map(str::to_string),
        }
    }

    #[test]
    fn focus_end_shows_suggestion_and_next_phase() {
        let alert = alert_for(
            &focus_ended(Phase::ShortBreak, Some("Beber água")),
            Language::BrazilianPortuguese,
        );
        assert_eq!(alert.title, "Foco concluído");
        assert_eq!(alert.body, "Beber água\nPróximo: descanso curto (p inicia)");
    }

    #[test]
    fn focus_end_without_suggestion_says_time_to_rest() {
        let alert = alert_for(
            &focus_ended(Phase::ShortBreak, None),
            Language::BrazilianPortuguese,
        );
        assert!(alert.body.starts_with("Hora de descansar\n"));
    }

    #[test]
    fn long_break_is_named_in_the_next_phase_line() {
        let alert = alert_for(
            &focus_ended(Phase::LongBreak, Some("Levantar")),
            Language::BrazilianPortuguese,
        );
        assert!(alert.body.ends_with("Próximo: descanso longo (p inicia)"));
    }

    #[test]
    fn break_end_asks_to_return_to_focus_or_switch_task() {
        let ended = PhaseEnded {
            finished: Phase::ShortBreak,
            next: Phase::Focus,
            suggestion: None,
        };
        let alert = alert_for(&ended, Language::BrazilianPortuguese);
        assert_eq!(alert.title, "Descanso terminou");
        assert_eq!(alert.body, "Volte ao foco ou troque de tarefa (p inicia)");
    }
}
