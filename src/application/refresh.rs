use crate::application::ports::WidgetSource;
use crate::domain::widget::WidgetState;

pub fn refresh_widget(source: &dyn WidgetSource) -> WidgetState {
    match source.snapshot() {
        Ok(snapshot) => WidgetState::Ready(snapshot),
        Err(error) => WidgetState::Failed(error.to_string()),
    }
}

#[cfg(test)]
pub(crate) mod fakes {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::application::ports::{SourceError, WidgetSource};
    use crate::domain::widget::{Summary, WidgetSnapshot};

    pub struct FakeSource {
        pub result: Result<WidgetSnapshot, SourceError>,
        pub calls: AtomicUsize,
    }

    impl FakeSource {
        pub fn returning(result: Result<WidgetSnapshot, SourceError>) -> Self {
            Self {
                result,
                calls: AtomicUsize::new(0),
            }
        }

        pub fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl WidgetSource for FakeSource {
        fn snapshot(&self) -> Result<WidgetSnapshot, SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    pub fn snapshot_titled(title: &str) -> WidgetSnapshot {
        WidgetSnapshot {
            summary: Summary {
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
    use crate::application::ports::SourceError;
    use crate::domain::widget::WidgetState;

    #[test]
    fn successful_snapshot_becomes_ready() {
        let source = FakeSource::returning(Ok(snapshot_titled("Kanban")));
        assert_eq!(
            refresh_widget(&source),
            WidgetState::Ready(snapshot_titled("Kanban"))
        );
        assert_eq!(source.call_count(), 1);
    }

    #[test]
    fn source_failure_becomes_failed_with_message() {
        let source = FakeSource::returning(Err(SourceError::Failed("fora do ar".to_string())));
        assert_eq!(
            refresh_widget(&source),
            WidgetState::Failed("fora do ar".to_string())
        );
    }
}
