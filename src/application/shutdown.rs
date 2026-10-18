use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crate::application::ports::Cancellable;

pub fn should_stop(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

pub struct ShutdownCoordinator {
    cancellables: Vec<Arc<dyn Cancellable>>,
    completed: AtomicBool,
}

impl ShutdownCoordinator {
    pub fn new(cancellables: Vec<Arc<dyn Cancellable>>) -> Self {
        Self {
            cancellables,
            completed: AtomicBool::new(false),
        }
    }

    pub fn shutdown(&self) {
        if self.completed.swap(true, Ordering::SeqCst) {
            return;
        }
        thread::scope(|scope| {
            for cancellable in &self.cancellables {
                scope.spawn(move || cancellable.cancel());
            }
        });
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
    fn shutdown_cancels_every_source() {
        let first = fake_source();
        let second = fake_source();
        let coordinator = ShutdownCoordinator::new(vec![
            first.clone() as Arc<dyn Cancellable>,
            second.clone() as Arc<dyn Cancellable>,
        ]);

        coordinator.shutdown();

        assert_eq!(first.cancel_call_count(), 1);
        assert_eq!(second.cancel_call_count(), 1);
    }

    #[test]
    fn runtime_shutdown_is_idempotent() {
        let source = fake_source();
        let coordinator = ShutdownCoordinator::new(vec![source.clone() as Arc<dyn Cancellable>]);

        coordinator.shutdown();
        coordinator.shutdown();

        assert_eq!(source.cancel_call_count(), 1);
    }

    #[test]
    fn dropping_coordinator_cancels_sources_once() {
        let source = fake_source();
        {
            let coordinator =
                ShutdownCoordinator::new(vec![source.clone() as Arc<dyn Cancellable>]);
            coordinator.shutdown();
        }

        assert_eq!(source.cancel_call_count(), 1);
    }

    #[test]
    fn shutdown_cancels_slow_sources_in_parallel() {
        let sources: Vec<Arc<SlowCancellable>> = (0..SLOW_SOURCE_COUNT)
            .map(|_| Arc::new(SlowCancellable::new()))
            .collect();
        let coordinator = ShutdownCoordinator::new(
            sources
                .iter()
                .map(|source| source.clone() as Arc<dyn Cancellable>)
                .collect(),
        );

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
        let coordinator = ShutdownCoordinator::new(vec![
            widget.clone() as Arc<dyn Cancellable>,
            llm.clone() as Arc<dyn Cancellable>,
        ]);

        coordinator.shutdown();

        assert_eq!(widget.cancel_call_count(), 1);
        assert_eq!(llm.cancel_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn should_stop_reflects_the_termination_flag() {
        let flag = AtomicBool::new(false);
        assert!(!should_stop(&flag));

        flag.store(true, Ordering::SeqCst);

        assert!(should_stop(&flag));
    }
}
