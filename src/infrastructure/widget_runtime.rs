use std::collections::HashMap;
use std::sync::mpsc::{SendError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::application::ports::{Cancellable, SourceError, WidgetRunner};
use crate::application::refresh::refresh_widget;
use crate::application::reload::WidgetLifecycle;
use crate::application::shutdown::ShutdownCoordinator;
use crate::application::widget_notices::with_interval_notice;
use crate::domain::widget::{WidgetId, WidgetSnapshot};
use crate::infrastructure::config::WidgetSettings;
use crate::infrastructure::refresh_signal::{refresh_channel, RefreshSignal, RefreshWake};

const WIDGET_SHUTDOWN_KEY_PREFIX: &str = "widget:";

pub type RunnerFactory = Box<dyn Fn(&WidgetSettings) -> Arc<dyn WidgetRunner> + Send + Sync>;

pub struct WidgetUpdate {
    pub id: WidgetId,
    pub generation: u64,
    pub result: Result<WidgetSnapshot, SourceError>,
}

struct UpdateSink<E> {
    sender: Sender<E>,
    into_event: fn(WidgetUpdate) -> E,
}

impl<E> Clone for UpdateSink<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            into_event: self.into_event,
        }
    }
}

impl<E> UpdateSink<E> {
    fn deliver(&self, update: WidgetUpdate) -> Result<(), SendError<E>> {
        self.sender.send((self.into_event)(update))
    }
}

pub struct WidgetRuntime<E> {
    factory: RunnerFactory,
    sink: UpdateSink<E>,
    shutdown: Arc<ShutdownCoordinator>,
    runners: HashMap<WidgetId, Arc<dyn WidgetRunner>>,
    refresh_signals: HashMap<WidgetId, RefreshSignal>,
}

impl<E: Send + 'static> WidgetRuntime<E> {
    pub fn new(
        factory: RunnerFactory,
        sender: Sender<E>,
        into_event: fn(WidgetUpdate) -> E,
        shutdown: Arc<ShutdownCoordinator>,
    ) -> Self {
        Self {
            factory,
            sink: UpdateSink { sender, into_event },
            shutdown,
            runners: HashMap::new(),
            refresh_signals: HashMap::new(),
        }
    }

    pub fn runner(&self, id: &WidgetId) -> Option<&Arc<dyn WidgetRunner>> {
        self.runners.get(id)
    }

    pub fn trigger_all(&self) {
        for signal in self.refresh_signals.values() {
            signal.request();
        }
    }
}

fn shutdown_key(id: &WidgetId) -> String {
    format!("{WIDGET_SHUTDOWN_KEY_PREFIX}{}", id.as_str())
}

impl<E: Send + 'static> WidgetLifecycle for WidgetRuntime<E> {
    type Spec = WidgetSettings;

    fn start(&mut self, settings: &WidgetSettings, generation: u64) {
        let id = settings.id.clone();
        let runner = (self.factory)(settings);
        self.shutdown.register(
            &shutdown_key(&id),
            Arc::clone(&runner) as Arc<dyn Cancellable>,
        );
        let signal = spawn_refresher(settings, generation, Arc::clone(&runner), self.sink.clone());
        self.runners.insert(id.clone(), runner);
        self.refresh_signals.insert(id, signal);
    }

    fn retire(&mut self, id: &WidgetId) {
        self.shutdown.retire(&shutdown_key(id));
        self.runners.remove(id);
        self.refresh_signals.remove(id);
    }
}

fn spawn_refresher<E: Send + 'static>(
    settings: &WidgetSettings,
    generation: u64,
    runner: Arc<dyn WidgetRunner>,
    sink: UpdateSink<E>,
) -> RefreshSignal {
    let id = settings.id.clone();
    let interval = Duration::from_secs(settings.interval_seconds);
    let interval_notice = settings
        .interval_adjusted
        .then_some(settings.interval_seconds);
    let (signal, waiter) = refresh_channel();
    thread::spawn(move || loop {
        let refreshed = refresh_widget(runner.as_ref());
        let result = match interval_notice {
            Some(seconds) => with_interval_notice(refreshed, seconds),
            None => refreshed,
        };
        let update = WidgetUpdate {
            id: id.clone(),
            generation,
            result,
        };
        if sink.deliver(update).is_err() {
            break;
        }
        if waiter.wait(interval) == RefreshWake::Closed {
            break;
        }
    });
    signal
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::application::ports::{ActionExecutor, WidgetSource};
    use crate::application::refresh::fakes::snapshot_titled;
    use crate::domain::action::{ActionOutcome, ConfirmedAction};
    use crate::domain::widget::WidgetSnapshot;
    use crate::infrastructure::config::default_llm_selection;

    const RECEIVE_LIMIT: Duration = Duration::from_secs(2);

    #[derive(Default)]
    struct FakeRunner {
        request_cancels: AtomicUsize,
        cancels: AtomicUsize,
    }

    impl Cancellable for FakeRunner {
        fn cancel(&self) {
            self.cancels.fetch_add(1, Ordering::SeqCst);
        }

        fn request_cancel(&self) {
            self.request_cancels.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl WidgetSource for FakeRunner {
        fn snapshot(&self) -> Result<WidgetSnapshot, SourceError> {
            Ok(snapshot_titled("fake"))
        }
    }

    impl ActionExecutor for FakeRunner {
        fn execute(&self, _action: &ConfirmedAction) -> Result<ActionOutcome, SourceError> {
            Ok(ActionOutcome {
                message: String::new(),
            })
        }
    }

    fn settings(id: &str) -> WidgetSettings {
        WidgetSettings {
            id: WidgetId::new(id),
            command: "ignorado".to_string(),
            args: vec![],
            env: vec![],
            interval_seconds: 30,
            interval_adjusted: false,
            timeout_seconds: 5,
            span: 1,
            max_items: None,
            llm_selection: default_llm_selection(vec![]),
        }
    }

    fn runtime_with(
        fake: &Arc<FakeRunner>,
    ) -> (
        WidgetRuntime<WidgetUpdate>,
        mpsc::Receiver<WidgetUpdate>,
        Arc<ShutdownCoordinator>,
    ) {
        let (sender, receiver) = mpsc::channel();
        let shutdown = Arc::new(ShutdownCoordinator::new());
        let shared = Arc::clone(fake);
        let factory: RunnerFactory =
            Box::new(move |_settings| Arc::clone(&shared) as Arc<dyn WidgetRunner>);
        let runtime = WidgetRuntime::new(factory, sender, |update| update, Arc::clone(&shutdown));
        (runtime, receiver, shutdown)
    }

    #[test]
    fn start_runs_the_widget_and_reports_its_snapshot() {
        let fake = Arc::new(FakeRunner::default());
        let (mut runtime, receiver, _shutdown) = runtime_with(&fake);

        runtime.start(&settings("a"), 7);

        let update = receiver.recv_timeout(RECEIVE_LIMIT).unwrap();
        assert_eq!(update.id.as_str(), "a");
        assert_eq!(update.generation, 7);
        assert!(update.result.is_ok());
        assert!(runtime.runner(&WidgetId::new("a")).is_some());
    }

    #[test]
    fn start_registers_the_runner_for_shutdown() {
        let fake = Arc::new(FakeRunner::default());
        let (mut runtime, _receiver, shutdown) = runtime_with(&fake);
        runtime.start(&settings("a"), 1);

        shutdown.shutdown();

        assert_eq!(fake.cancels.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retire_requests_cancel_and_forgets_the_runner() {
        let fake = Arc::new(FakeRunner::default());
        let (mut runtime, _receiver, _shutdown) = runtime_with(&fake);
        runtime.start(&settings("a"), 1);

        runtime.retire(&WidgetId::new("a"));

        assert_eq!(fake.request_cancels.load(Ordering::SeqCst), 1);
        assert!(runtime.runner(&WidgetId::new("a")).is_none());
    }
}
