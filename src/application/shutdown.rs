use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use crate::application::ports::Cancellable;

pub fn should_stop(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

pub struct ShutdownCoordinator {
    sources: Mutex<Vec<(String, Arc<dyn Cancellable>)>>,
    completed: AtomicBool,
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            sources: Mutex::new(Vec::new()),
            completed: AtomicBool::new(false),
        }
    }

    pub fn register(&self, key: &str, cancellable: Arc<dyn Cancellable>) {
        let mut sources = self.lock_sources();
        sources.retain(|(existing, _)| existing != key);
        sources.push((key.to_string(), cancellable));
    }

    pub fn retire(&self, key: &str) {
        let retired = {
            let mut sources = self.lock_sources();
            sources
                .iter()
                .position(|(existing, _)| existing == key)
                .map(|position| sources.remove(position))
        };
        if let Some((_, cancellable)) = retired {
            cancellable.cancel();
        }
    }

    pub fn shutdown(&self) {
        if self.completed.swap(true, Ordering::SeqCst) {
            return;
        }
        let current: Vec<Arc<dyn Cancellable>> = self
            .lock_sources()
            .iter()
            .map(|(_, cancellable)| Arc::clone(cancellable))
            .collect();
        thread::scope(|scope| {
            for cancellable in &current {
                scope.spawn(move || cancellable.cancel());
            }
        });
    }

    fn lock_sources(&self) -> MutexGuard<'_, Vec<(String, Arc<dyn Cancellable>)>> {
        self.sources.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for ShutdownCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{should_stop, ShutdownCoordinator};
    use crate::application::llm::{LlmError, LlmRequest, LlmRunner};
    use crate::application::ports::Cancellable;
    use crate::application::refresh::fakes::snapshot_titled;
    use crate::application::refresh::fakes::FakeSource;

    const SLOW_CANCEL_DURATION: Duration = Duration::from_millis(100);
    const SLOW_SOURCE_COUNT: usize = 8;
    const PARALLEL_SHUTDOWN_BUDGET: Duration = Duration::from_millis(400);

    struct SlowCancellable {
        cancel_calls: AtomicUsize,
    }

    impl SlowCancellable {
        fn new() -> Self {
            Self {
                cancel_calls: AtomicUsize::new(0),
            }
        }
    }

    impl Cancellable for SlowCancellable {
        fn cancel(&self) {
            thread::sleep(SLOW_CANCEL_DURATION);
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn fake_source() -> Arc<FakeSource> {
        Arc::new(FakeSource::returning(Ok(snapshot_titled("provider"))))
    }

    #[test]
    fn shutdown_cancels_every_registered_source() {
        let first = fake_source();
        let second = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("first", first.clone());
        coordinator.register("second", second.clone());

        coordinator.shutdown();

        assert_eq!(first.cancel_call_count(), 1);
        assert_eq!(second.cancel_call_count(), 1);
    }

    #[test]
    fn runtime_shutdown_is_idempotent() {
        let source = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("source", source.clone());

        coordinator.shutdown();
        coordinator.shutdown();

        assert_eq!(source.cancel_call_count(), 1);
    }

    #[test]
    fn dropping_coordinator_cancels_sources_once() {
        let source = fake_source();
        {
            let coordinator = ShutdownCoordinator::new();
            coordinator.register("source", source.clone());
            coordinator.shutdown();
        }

        assert_eq!(source.cancel_call_count(), 1);
    }

    #[test]
    fn shutdown_cancels_slow_sources_in_parallel() {
        let sources: Vec<Arc<SlowCancellable>> = (0..SLOW_SOURCE_COUNT)
            .map(|_| Arc::new(SlowCancellable::new()))
            .collect();
        let coordinator = ShutdownCoordinator::new();
        for (index, source) in sources.iter().enumerate() {
            coordinator.register(&index.to_string(), source.clone());
        }

        let started = Instant::now();
        coordinator.shutdown();
        let elapsed = started.elapsed();

        assert!(
            elapsed < PARALLEL_SHUTDOWN_BUDGET,
            "shutdown levou {elapsed:?}, esperado paralelo"
        );
        for source in &sources {
            assert_eq!(source.cancel_calls.load(Ordering::SeqCst), 1);
        }
    }

    struct FakeLlmRunner {
        cancel_calls: AtomicUsize,
    }

    impl LlmRunner for FakeLlmRunner {
        fn run(&self, _request: &LlmRequest) -> Result<String, LlmError> {
            Ok(String::new())
        }
    }

    impl Cancellable for FakeLlmRunner {
        fn cancel(&self) {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn shutdown_cancels_llm_runner_alongside_widget_sources() {
        let llm = Arc::new(FakeLlmRunner {
            cancel_calls: AtomicUsize::new(0),
        });
        let widget = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("widget", widget.clone());
        coordinator.register("llm", llm.clone());

        coordinator.shutdown();

        assert_eq!(widget.cancel_call_count(), 1);
        assert_eq!(llm.cancel_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retire_cancels_only_the_named_source_once() {
        let kept = fake_source();
        let removed = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("kept", kept.clone());
        coordinator.register("removed", removed.clone());

        coordinator.retire("removed");

        assert_eq!(removed.cancel_call_count(), 1);
        assert_eq!(kept.cancel_call_count(), 0);
    }

    #[test]
    fn retired_source_is_not_cancelled_again_by_shutdown() {
        let removed = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("removed", removed.clone());

        coordinator.retire("removed");
        coordinator.shutdown();

        assert_eq!(removed.cancel_call_count(), 1);
    }

    #[test]
    fn shutdown_cancels_the_source_registered_last_under_a_key() {
        let replaced = fake_source();
        let replacement = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("jira", replaced.clone());
        coordinator.register("jira", replacement.clone());

        coordinator.shutdown();

        assert_eq!(replaced.cancel_call_count(), 0);
        assert_eq!(replacement.cancel_call_count(), 1);
    }

    #[test]
    fn retiring_an_unknown_key_does_nothing() {
        let source = fake_source();
        let coordinator = ShutdownCoordinator::new();
        coordinator.register("source", source.clone());

        coordinator.retire("missing");

        assert_eq!(source.cancel_call_count(), 0);
    }

    #[test]
    fn should_stop_reflects_the_termination_flag() {
        let flag = AtomicBool::new(false);
        assert!(!should_stop(&flag));

        flag.store(true, Ordering::SeqCst);

        assert!(should_stop(&flag));
    }
}
