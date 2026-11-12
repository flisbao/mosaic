use std::time::Duration;

use thiserror::Error;

use crate::application::pomodoro_notifications::alert_for;
use crate::application::ports::AlertMessage;
use crate::display_in_portuguese;
use crate::domain::language::{Language, LocalizedMessage};
use crate::domain::pomodoro::PomodoroTimer;
use crate::presentation::messages;

#[derive(Debug, Error, PartialEq)]
pub(super) struct PomodoroDisabled;

impl LocalizedMessage for PomodoroDisabled {
    fn message(&self, language: Language) -> String {
        messages::pomodoro_disabled_notice(language).to_string()
    }
}

display_in_portuguese!(PomodoroDisabled);

#[derive(Default)]
pub(super) struct PomodoroControl {
    timer: Option<PomodoroTimer>,
}

impl PomodoroControl {
    pub(super) fn enabled(timer: PomodoroTimer) -> Self {
        Self { timer: Some(timer) }
    }

    pub(super) fn timer(&self) -> Option<&PomodoroTimer> {
        self.timer.as_ref()
    }

    pub(super) fn toggle(&mut self) -> Result<(), PomodoroDisabled> {
        self.timer.as_mut().ok_or(PomodoroDisabled)?.toggle();
        Ok(())
    }

    pub(super) fn reset(&mut self) -> Result<(), PomodoroDisabled> {
        self.timer.as_mut().ok_or(PomodoroDisabled)?.reset();
        Ok(())
    }

    pub(super) fn advance(
        &mut self,
        elapsed: Duration,
        language: Language,
    ) -> Option<AlertMessage> {
        let ended = self.timer.as_mut()?.advance(elapsed)?;
        Some(alert_for(&ended, language))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::domain::language::Language;
    use crate::domain::pomodoro::{PomodoroSettings, PomodoroTimer};

    fn enabled_control() -> PomodoroControl {
        PomodoroControl::enabled(PomodoroTimer::new(PomodoroSettings {
            focus: Duration::from_secs(25 * 60),
            short_break: Duration::from_secs(5 * 60),
            long_break: Duration::from_secs(15 * 60),
            cycles_before_long_break: 4,
            suggestions: vec!["Beber água".to_string()],
        }))
    }

    #[test]
    fn disabled_control_rejects_toggle_and_reset() {
        let mut control = PomodoroControl::default();

        assert_eq!(control.toggle(), Err(PomodoroDisabled));
        assert_eq!(control.reset(), Err(PomodoroDisabled));
        assert!(control.timer().is_none());
    }

    #[test]
    fn disabled_control_never_produces_alerts() {
        let mut control = PomodoroControl::default();
        assert_eq!(
            control.advance(Duration::from_secs(60 * 60), Language::BrazilianPortuguese),
            None
        );
    }

    #[test]
    fn enabled_control_returns_the_alert_when_the_focus_phase_ends() {
        let mut control = enabled_control();
        control.toggle().unwrap();

        assert_eq!(
            control.advance(Duration::from_secs(60), Language::BrazilianPortuguese),
            None
        );
        let alert = control
            .advance(Duration::from_secs(25 * 60), Language::BrazilianPortuguese)
            .expect("aviso de fim de foco");

        assert_eq!(alert.title, "Foco concluído");
    }

    #[test]
    fn the_disabled_error_carries_the_user_facing_notice() {
        assert_eq!(
            PomodoroDisabled.to_string(),
            "Pomodoro desligado na configuração"
        );
    }
}
