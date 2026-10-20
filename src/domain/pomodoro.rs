use std::time::Duration;

pub const SECONDS_PER_MINUTE: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PomodoroStatus {
    Stopped,
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PomodoroSettings {
    pub focus: Duration,
    pub short_break: Duration,
    pub long_break: Duration,
    pub cycles_before_long_break: u32,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhaseEnded {
    pub finished: Phase,
    pub next: Phase,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PomodoroTimer {
    settings: PomodoroSettings,
    phase: Phase,
    status: PomodoroStatus,
    elapsed: Duration,
    completed_focus_count: u32,
    suggestion_index: usize,
}

impl PomodoroTimer {
    pub fn new(settings: PomodoroSettings) -> Self {
        Self {
            settings,
            phase: Phase::Focus,
            status: PomodoroStatus::Stopped,
            elapsed: Duration::ZERO,
            completed_focus_count: 0,
            suggestion_index: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn status(&self) -> PomodoroStatus {
        self.status
    }

    pub fn remaining(&self) -> Duration {
        self.phase_length(self.phase).saturating_sub(self.elapsed)
    }

    pub fn toggle(&mut self) {
        self.status = match self.status {
            PomodoroStatus::Running => PomodoroStatus::Paused,
            PomodoroStatus::Stopped | PomodoroStatus::Paused => PomodoroStatus::Running,
        };
    }

    pub fn reset(&mut self) {
        self.phase = Phase::Focus;
        self.status = PomodoroStatus::Stopped;
        self.elapsed = Duration::ZERO;
        self.completed_focus_count = 0;
        self.suggestion_index = 0;
    }

    pub fn advance(&mut self, elapsed: Duration) -> Option<PhaseEnded> {
        if self.status != PomodoroStatus::Running {
            return None;
        }
        self.elapsed = self.elapsed.saturating_add(elapsed);
        if self.elapsed < self.phase_length(self.phase) {
            return None;
        }
        Some(self.finish_phase())
    }

    fn finish_phase(&mut self) -> PhaseEnded {
        let finished = self.phase;
        let (next, suggestion) = match finished {
            Phase::Focus => (self.next_break(), self.take_suggestion()),
            Phase::ShortBreak | Phase::LongBreak => (Phase::Focus, None),
        };
        self.phase = next;
        self.status = PomodoroStatus::Stopped;
        self.elapsed = Duration::ZERO;
        PhaseEnded {
            finished,
            next,
            suggestion,
        }
    }

    fn next_break(&mut self) -> Phase {
        self.completed_focus_count += 1;
        if self
            .completed_focus_count
            .is_multiple_of(self.settings.cycles_before_long_break.max(1))
        {
            Phase::LongBreak
        } else {
            Phase::ShortBreak
        }
    }

    fn take_suggestion(&mut self) -> Option<String> {
        if self.settings.suggestions.is_empty() {
            return None;
        }
        let suggestion = self.settings.suggestions[self.suggestion_index].clone();
        self.suggestion_index = (self.suggestion_index + 1) % self.settings.suggestions.len();
        Some(suggestion)
    }

    fn phase_length(&self, phase: Phase) -> Duration {
        match phase {
            Phase::Focus => self.settings.focus,
            Phase::ShortBreak => self.settings.short_break,
            Phase::LongBreak => self.settings.long_break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    fn settings() -> PomodoroSettings {
        PomodoroSettings {
            focus: 25 * MINUTE,
            short_break: 5 * MINUTE,
            long_break: 15 * MINUTE,
            cycles_before_long_break: 4,
            suggestions: vec!["a".to_string(), "b".to_string()],
        }
    }

    fn running_timer() -> PomodoroTimer {
        let mut timer = PomodoroTimer::new(settings());
        timer.toggle();
        timer
    }

    fn finish_focus(timer: &mut PomodoroTimer) -> PhaseEnded {
        timer.toggle();
        timer.advance(25 * MINUTE).expect("foco deveria terminar")
    }

    fn finish_break(timer: &mut PomodoroTimer) {
        timer.toggle();
        let length = match timer.phase() {
            Phase::LongBreak => 15 * MINUTE,
            _ => 5 * MINUTE,
        };
        timer.advance(length).expect("descanso deveria terminar");
    }

    #[test]
    fn new_timer_is_stopped_in_focus_with_full_remaining() {
        let timer = PomodoroTimer::new(settings());
        assert_eq!(timer.phase(), Phase::Focus);
        assert_eq!(timer.status(), PomodoroStatus::Stopped);
        assert_eq!(timer.remaining(), 25 * MINUTE);
    }

    #[test]
    fn advance_does_not_count_while_stopped_or_paused() {
        let mut timer = PomodoroTimer::new(settings());
        assert_eq!(timer.advance(30 * MINUTE), None);
        assert_eq!(timer.remaining(), 25 * MINUTE);
        timer.toggle();
        timer.toggle();
        assert_eq!(timer.status(), PomodoroStatus::Paused);
        assert_eq!(timer.advance(30 * MINUTE), None);
        assert_eq!(timer.remaining(), 25 * MINUTE);
    }

    #[test]
    fn toggle_cycles_stopped_running_paused_running() {
        let mut timer = PomodoroTimer::new(settings());
        let mut seen = Vec::new();
        for _ in 0..3 {
            timer.toggle();
            seen.push(timer.status());
        }
        assert_eq!(
            seen,
            vec![
                PomodoroStatus::Running,
                PomodoroStatus::Paused,
                PomodoroStatus::Running
            ]
        );
    }

    #[test]
    fn focus_end_reports_short_break_and_first_suggestion() {
        let mut timer = running_timer();
        let ended = timer.advance(25 * MINUTE);
        assert_eq!(
            ended,
            Some(PhaseEnded {
                finished: Phase::Focus,
                next: Phase::ShortBreak,
                suggestion: Some("a".to_string())
            })
        );
        assert_eq!(timer.status(), PomodoroStatus::Stopped);
        assert_eq!(timer.phase(), Phase::ShortBreak);
        assert_eq!(timer.remaining(), 5 * MINUTE);
    }

    #[test]
    fn fourth_focus_leads_to_long_break() {
        let mut timer = PomodoroTimer::new(settings());
        for _ in 0..3 {
            let ended = finish_focus(&mut timer);
            assert_eq!(ended.next, Phase::ShortBreak);
            finish_break(&mut timer);
        }
        let ended = finish_focus(&mut timer);
        assert_eq!(ended.next, Phase::LongBreak);
        assert_eq!(timer.remaining(), 15 * MINUTE);
    }

    #[test]
    fn break_end_returns_to_focus_without_suggestion() {
        let mut timer = PomodoroTimer::new(settings());
        finish_focus(&mut timer);
        timer.toggle();
        let ended = timer.advance(5 * MINUTE).unwrap();
        assert_eq!(
            ended,
            PhaseEnded {
                finished: Phase::ShortBreak,
                next: Phase::Focus,
                suggestion: None
            }
        );
        assert_eq!(timer.phase(), Phase::Focus);
    }

    #[test]
    fn suggestions_rotate_circularly() {
        let mut timer = PomodoroTimer::new(settings());
        let mut suggestions = Vec::new();
        for _ in 0..3 {
            suggestions.push(finish_focus(&mut timer).suggestion.unwrap());
            finish_break(&mut timer);
        }
        assert_eq!(suggestions, vec!["a", "b", "a"]);
    }

    #[test]
    fn empty_suggestions_yield_none() {
        let mut empty = settings();
        empty.suggestions.clear();
        let mut timer = PomodoroTimer::new(empty);
        assert_eq!(finish_focus(&mut timer).suggestion, None);
    }

    #[test]
    fn huge_advance_ends_only_one_phase_and_discards_the_rest() {
        let mut timer = running_timer();
        let ended = timer.advance(60 * MINUTE);
        assert!(ended.is_some());
        assert_eq!(timer.status(), PomodoroStatus::Stopped);
        assert_eq!(timer.remaining(), 5 * MINUTE);
        assert_eq!(timer.advance(60 * MINUTE), None);
    }

    #[test]
    fn reset_returns_to_initial_state() {
        let mut timer = running_timer();
        timer.advance(25 * MINUTE);
        timer.reset();
        assert_eq!(timer.phase(), Phase::Focus);
        assert_eq!(timer.status(), PomodoroStatus::Stopped);
        assert_eq!(timer.remaining(), 25 * MINUTE);
        let ended = finish_focus(&mut timer);
        assert_eq!(ended.suggestion.as_deref(), Some("a"));
    }

    #[test]
    fn remaining_decreases_while_running() {
        let mut timer = running_timer();
        timer.advance(10 * MINUTE);
        assert_eq!(timer.remaining(), 15 * MINUTE);
    }
}
