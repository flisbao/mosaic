use std::collections::HashMap;
use std::sync::mpsc::{SendError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::application::launch_spec::WidgetLaunchSpec;
use crate::application::ports::{Cancellable, WidgetRunner};
use crate::application::refresh::refresh_widget;
use crate::application::reload::{Generation, WidgetLifecycle, WidgetUpdate};
use crate::application::shutdown::ShutdownCoordinator;
use crate::application::widget_notices::with_interval_notice;
use crate::domain::widget::WidgetId;
use crate::infrastructure::refresh_signal::{refresh_channel, RefreshSignal, RefreshWake};

const WIDGET_SHUTDOWN_KEY_PREFIX: &str = "widget:";

pub type RunnerFactory = Box<dyn Fn(&WidgetLaunchSpec) -> Arc<dyn WidgetRunner> + Send + Sync>;

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
    type Spec = WidgetLaunchSpec;

    fn start(&mut self, spec: &WidgetLaunchSpec, generation: Generation) {
        let id = spec.id.clone();
        let runner = (self.factory)(spec);
        self.shutdown.register(
            &shutdown_key(&id),
            Arc::clone(&runner) as Arc<dyn Cancellable>,
        );
        let interval = spec.schedule.interval;
        let interval_notice = spec
            .schedule
            .interval_adjusted
            .then_some(spec.schedule.interval.as_secs());
        let signal = spawn_refresher(
            id.clone(),
            generation,
            Arc::clone(&runner),
            interval,
            interval_notice,
            self.sink.clone(),
        );
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
    id: WidgetId,
    generation: Generation,
    runner: Arc<dyn WidgetRunner>,
    interval: Duration,
    interval_notice: Option<u64>,
    sink: UpdateSink<E>,
) -> RefreshSignal {
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
    use crate::application::launch_spec::{ProviderCommand, WidgetSchedule};
    use crate::application::ports::{ActionExecutor, SourceError, WidgetSource};
    use crate::application::refresh::fakes::snapshot_titled;
    use crate::application::reload::WidgetGenerations;
    use crate::domain::action::{ActionOutcome, ConfirmedAction};
    use crate::domain::widget::WidgetSnapshot;

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

    fn settings(id: &str) -> WidgetLaunchSpec {
        WidgetLaunchSpec {
            id: WidgetId::new(id),
            command: ProviderCommand {
                program: "ignorado".to_string(),
                args: vec![],
                env: vec![],
                timeout: Duration::from_secs(5),
            },
            schedule: WidgetSchedule {
                interval: Duration::from_secs(30),
                interval_adjusted: false,
            },
        }
    }

    fn generation() -> Generation {
        WidgetGenerations::default().advance(&WidgetId::new("a"))
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

        let generation = generation();
        runtime.start(&settings("a"), generation);

        let update = receiver.recv_timeout(RECEIVE_LIMIT).unwrap();
        assert_eq!(update.id.as_str(), "a");
        assert_eq!(update.generation, generation);
        assert!(update.result.is_ok());
        assert!(runtime.runner(&WidgetId::new("a")).is_some());
    }

    #[test]
    fn start_registers_the_runner_for_shutdown() {
        let fake = Arc::new(FakeRunner::default());
        let (mut runtime, _receiver, shutdown) = runtime_with(&fake);
        runtime.start(&settings("a"), generation());

        shutdown.shutdown();

        assert_eq!(fake.cancels.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retire_requests_cancel_and_forgets_the_runner() {
        let fake = Arc::new(FakeRunner::default());
        let (mut runtime, _receiver, _shutdown) = runtime_with(&fake);
        runtime.start(&settings("a"), generation());

        runtime.retire(&WidgetId::new("a"));

        assert_eq!(fake.request_cancels.load(Ordering::SeqCst), 1);
        assert!(runtime.runner(&WidgetId::new("a")).is_none());
    }
}
