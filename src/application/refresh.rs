use crate::application::ports::{SourceError, WidgetSource};
use crate::domain::widget::WidgetSnapshot;

pub fn refresh_widget(source: &dyn WidgetSource) -> Result<WidgetSnapshot, SourceError> {
    source.snapshot()
}

#[cfg(test)]
pub(crate) mod fakes {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::application::ports::{Cancellable, SourceError, WidgetSource};
    use crate::domain::widget::{Summary, WidgetSnapshot};

    pub struct FakeSource {
        pub result: Result<WidgetSnapshot, SourceError>,
        pub calls: AtomicUsize,
        pub cancel_calls: AtomicUsize,
    }

    impl FakeSource {
        pub fn returning(result: Result<WidgetSnapshot, SourceError>) -> Self {
            Self {
                result,
                calls: AtomicUsize::new(0),
                cancel_calls: AtomicUsize::new(0),
            }
        }

        pub fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        pub fn cancel_call_count(&self) -> usize {
            self.cancel_calls.load(Ordering::SeqCst)
        }
    }

    impl WidgetSource for FakeSource {
        fn snapshot(&self) -> Result<WidgetSnapshot, SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    impl Cancellable for FakeSource {
        fn cancel(&self) {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        }
        fn request_cancel(&self) {}
    }

    pub fn snapshot_titled(title: &str) -> WidgetSnapshot {
        WidgetSnapshot {
            mini: None,
            alerts: vec![],
            summary: Summary {
                blocks: vec![],
                title: title.to_string(),
                lines: vec![],
            },
            detail: None,
            actions: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::{snapshot_titled, FakeSource};
    use super::*;
    use crate::application::ports::Cancellable;

    #[test]
    fn successful_snapshot_is_returned() {
        let source = FakeSource::returning(Ok(snapshot_titled("Kanban")));
        assert_eq!(refresh_widget(&source), Ok(snapshot_titled("Kanban")));
        assert_eq!(source.call_count(), 1);
    }

    #[test]
    fn fake_source_counts_cancel_calls() {
        let source = FakeSource::returning(Ok(snapshot_titled("Kanban")));
        source.cancel();
        source.cancel();
        assert_eq!(source.cancel_call_count(), 2);
    }

    #[test]
    fn source_failure_is_returned_as_error() {
        let source = FakeSource::returning(Err(SourceError::Failed("fora do ar".to_string())));
        assert_eq!(
            refresh_widget(&source),
            Err(SourceError::Failed("fora do ar".to_string()))
        );
    }
}
