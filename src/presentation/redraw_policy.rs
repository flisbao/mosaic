use chrono::{NaiveDateTime, Timelike};

use crate::domain::pomodoro::PomodoroStatus;
use crate::presentation::app_state::AppState;

pub fn clock_changes_what_is_drawn(state: &AppState) -> bool {
    state
        .pomodoro()
        .is_some_and(|timer| timer.status() == PomodoroStatus::Running)
}

pub fn needs_time_redraw(last_drawn: NaiveDateTime, now: NaiveDateTime, ticking: bool) -> bool {
    visible_instant(last_drawn, ticking) != visible_instant(now, ticking)
}

fn visible_instant(instant: NaiveDateTime, ticking: bool) -> NaiveDateTime {
    let second = if ticking { instant.second() } else { 0 };
    instant
        .with_second(second)
        .and_then(|rounded| rounded.with_nanosecond(0))
        .unwrap_or(instant)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::domain::pomodoro::{PomodoroSettings, PomodoroTimer};
    use crate::presentation::app_state::AppState;
    use chrono::{NaiveDate, NaiveDateTime};

    use super::*;

    fn at(hour: u32, minute: u32, second: u32, millisecond: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 19)
            .and_then(|date| date.and_hms_milli_opt(hour, minute, second, millisecond))
            .expect("horário válido")
    }

    #[test]
    fn without_a_running_timer_only_a_new_minute_needs_a_redraw() {
        assert!(!needs_time_redraw(at(9, 0, 0, 0), at(9, 0, 59, 999), false));
        assert!(needs_time_redraw(at(9, 0, 59, 999), at(9, 1, 0, 0), false));
    }

    #[test]
    fn with_a_running_timer_a_new_second_needs_a_redraw() {
        assert!(!needs_time_redraw(at(9, 0, 5, 0), at(9, 0, 5, 999), true));
        assert!(needs_time_redraw(at(9, 0, 5, 999), at(9, 0, 6, 0), true));
    }

    #[test]
    fn the_same_minute_on_another_day_needs_a_redraw() {
        let next_day = at(9, 0, 0, 0) + chrono::TimeDelta::days(1);
        assert!(needs_time_redraw(at(9, 0, 0, 0), next_day, false));
    }

    #[test]
    fn clock_ticks_only_while_the_pomodoro_is_running() {
        let settings = PomodoroSettings {
            focus: Duration::from_secs(60),
            short_break: Duration::from_secs(30),
            long_break: Duration::from_secs(120),
            cycles_before_long_break: 4,
            suggestions: vec![],
        };
        let stopped =
            AppState::new(vec![], vec![]).with_pomodoro(PomodoroTimer::new(settings.clone()));
        let mut timer = PomodoroTimer::new(settings);
        timer.toggle();
        let running = AppState::new(vec![], vec![]).with_pomodoro(timer);
        assert!(!clock_changes_what_is_drawn(&stopped));
        assert!(clock_changes_what_is_drawn(&running));
    }
}
