use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::application::ports::WidgetSource;

pub struct ShutdownCoordinator {
    sources: Vec<Arc<dyn WidgetSource>>,
    completed: AtomicBool,
}

impl ShutdownCoordinator {
    pub fn new(sources: Vec<Arc<dyn WidgetSource>>) -> Self {
        Self {
            sources,
            completed: AtomicBool::new(false),
        }
    }

    pub fn shutdown(&self) {
        if self.completed.swap(true, Ordering::SeqCst) {
            return;
        }
        for source in &self.sources {
            source.cancel();
        }
    }
}

impl Drop for ShutdownCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ShutdownCoordinator;
    use crate::application::ports::WidgetSource;
    use crate::application::refresh::fakes::{snapshot_titled, FakeSource};

    fn fake_source() -> Arc<FakeSource> {
        Arc::new(FakeSource::returning(Ok(snapshot_titled("provider"))))
    }

    #[test]
    fn shutdown_cancels_every_source() {
        let first = fake_source();
        let second = fake_source();
        let coordinator = ShutdownCoordinator::new(vec![
            first.clone() as Arc<dyn WidgetSource>,
            second.clone() as Arc<dyn WidgetSource>,
        ]);

        coordinator.shutdown();

        assert_eq!(first.cancel_call_count(), 1);
        assert_eq!(second.cancel_call_count(), 1);
    }

    #[test]
    fn runtime_shutdown_is_idempotent() {
        let source = fake_source();
        let coordinator = ShutdownCoordinator::new(vec![source.clone() as Arc<dyn WidgetSource>]);

        coordinator.shutdown();
        coordinator.shutdown();

        assert_eq!(source.cancel_call_count(), 1);
    }

    #[test]
    fn dropping_coordinator_cancels_sources_once() {
        let source = fake_source();
        {
            let coordinator =
                ShutdownCoordinator::new(vec![source.clone() as Arc<dyn WidgetSource>]);
            coordinator.shutdown();
        }

        assert_eq!(source.cancel_call_count(), 1);
    }
}
