#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod check_command;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use mosaic::application::alert_tracker::AlertTracker;
use mosaic::application::execute_action::execute_confirmed;
use mosaic::application::launch_spec::WidgetLaunchSpec;
use mosaic::application::llm::{execute_recommendation, LlmRunner, RecommendationStore};
use mosaic::application::pomodoro_notifications::Notifier;
use mosaic::application::ports::{Cancellable, Clock, WidgetRunner};
use mosaic::application::recommendation::{LlmSelection, RecommendationPlan, WidgetContext};
use mosaic::application::recommendation_request::{
    prepare_recommendation, RecommendationStep, SendPolicy,
};
use mosaic::application::reload::{apply_plan, reload_widgets, WidgetGenerations};
use mosaic::application::reload_plan::{ReloadPlan, WidgetChange};
use mosaic::application::shutdown::{should_stop, ShutdownCoordinator};
use mosaic::domain::action::ConfirmedAction;
use mosaic::domain::pomodoro::PomodoroTimer;
use mosaic::domain::widget::{WidgetId, WidgetRecord};
use mosaic::infrastructure::app_paths::{llm_working_directory, recommendation_file};
use mosaic::infrastructure::config::{
    config_directory, config_file_path, load_config, prepare_reload, read_config_file,
    restart_sections, retain_restart_sections, AppConfig, ConfigError, LlmConfig, WidgetSettings,
};
use mosaic::infrastructure::default_files::install_defaults_if_missing;
use mosaic::infrastructure::llm_cli::build_llm_runner;
use mosaic::infrastructure::notifiers::{BellNotifier, CommandNotifier, CompositeNotifier};
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::priority_prompt::FilePriorityPrompt;
use mosaic::infrastructure::recommendation_file::{
    FileRecommendationStore, NullRecommendationStore,
};
use mosaic::infrastructure::shutdown::termination_requested;
use mosaic::infrastructure::system_clock::SystemClock;
use mosaic::infrastructure::widget_runtime::{RunnerFactory, WidgetRuntime};
use mosaic::presentation::app_events::{
    apply_event, stored_recommendation_result, AppEvent, EventContext,
};
use mosaic::presentation::app_state::{AppState, Command};
use mosaic::presentation::key_input::{from_crossterm, KeyInput};
use mosaic::presentation::reload_notice::reload_notice;
use mosaic::presentation::render::render;
use mosaic::presentation::tab_layout::{
    build_tab_views, carry_over_records, startup_notice, TabLayout, TabPlacement, WidgetPlacement,
};
use ratatui::crossterm::event::{self, Event};
use thiserror::Error;

const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const LLM_DISABLED_NOTICE: &str = "IA desligada na configuração";
const NO_STATE_DIRECTORY_LLM_WARNING: &str = "IA desligada: não há diretório de estado do usuário";
const NO_STATE_DIRECTORY_STORE_WARNING: &str =
    "recomendações não serão salvas: não há diretório de estado do usuário";
const CHECK_SUBCOMMAND: &str = "check";
const LLM_SHUTDOWN_KEY: &str = "llm";
const NOTIFIER_SHUTDOWN_KEY: &str = "notifier";

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("falha de terminal: {0}")]
    Terminal(#[from] std::io::Error),
}

struct LlmServices {
    runner: Arc<dyn LlmRunner>,
    cancellable: Arc<dyn Cancellable>,
    store: Arc<dyn RecommendationStore>,
    config: LlmConfig,
    selections: HashMap<String, LlmSelection>,
}

struct Services {
    runtime: WidgetRuntime<AppEvent>,
    generations: WidgetGenerations,
    shutdown: Arc<ShutdownCoordinator>,
    termination: Arc<AtomicBool>,
    llm: Option<LlmServices>,
    startup_warnings: Vec<String>,
    notifier: CompositeNotifier,
    config_directory: PathBuf,
    config: AppConfig,
}

struct EventChannel {
    sender: Sender<AppEvent>,
    receiver: mpsc::Receiver<AppEvent>,
}

struct App {
    state: AppState,
    spans: Vec<u16>,
    services: Services,
    events: EventChannel,
    clock: SystemClock,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some(CHECK_SUBCOMMAND) {
        return check_command::run(&arguments[1..]);
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mosaic: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let directory = config_directory()?;
    install_defaults_if_missing(&directory)?;
    let config = load_config(&directory)?;
    let termination = termination_requested()?;
    let (sender, receiver) = mpsc::channel();
    let services = build_services(&config, &directory, &sender, Arc::clone(&termination));
    let (state, spans) = initial_state(&config, &services, &SystemClock);
    let mut app = App {
        state,
        spans,
        services,
        events: EventChannel { sender, receiver },
        clock: SystemClock,
    };

    let mut terminal = ratatui::init();
    let result = app.run_event_loop(&mut terminal);
    ratatui::restore();
    app.services.shutdown.shutdown();
    result
}

fn initial_state(
    config: &AppConfig,
    services: &Services,
    clock: &dyn Clock,
) -> (AppState, Vec<u16>) {
    let recommendation_record = services
        .llm
        .as_ref()
        .map(|llm| stored_recommendation_record(llm.store.as_ref(), clock.now()));
    let TabLayout {
        entries,
        tabs,
        spans,
    } = build_tab_views(
        &widget_placements(config),
        &tab_placements(config),
        recommendation_record,
    );
    let mut state = AppState::new(entries, tabs);
    if let Some(notice) = startup_notice(&startup_warnings(config, services)) {
        state.show_notice(notice);
    }
    if config.pomodoro.enabled {
        state = state.with_pomodoro(PomodoroTimer::new(config.pomodoro.settings()));
    }
    (state, spans)
}

fn startup_warnings(config: &AppConfig, services: &Services) -> Vec<String> {
    config
        .warnings
        .iter()
        .chain(&services.startup_warnings)
        .cloned()
        .collect()
}

fn widget_placements(config: &AppConfig) -> Vec<WidgetPlacement> {
    config
        .widgets
        .iter()
        .map(|widget| WidgetPlacement {
            id: widget.id.clone(),
            interval: Duration::from_secs(widget.interval_seconds),
            max_items: widget.max_items,
            span: widget.span,
        })
        .collect()
}

fn tab_placements(config: &AppConfig) -> Vec<TabPlacement> {
    config
        .tabs
        .iter()
        .map(|tab| TabPlacement {
            name: tab.name.clone(),
            widget_ids: tab.widget_ids.clone(),
        })
        .collect()
}

fn build_services(
    config: &AppConfig,
    directory: &Path,
    sender: &Sender<AppEvent>,
    termination: Arc<AtomicBool>,
) -> Services {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let working_directory = directory.to_path_buf();
    let factory: RunnerFactory = Box::new(move |spec| {
        Arc::new(PluginRunner::new(
            spec.command.clone(),
            working_directory.clone(),
        )) as Arc<dyn WidgetRunner>
    });
    let mut runtime = WidgetRuntime::new(
        factory,
        sender.clone(),
        AppEvent::WidgetUpdated,
        Arc::clone(&shutdown),
    );
    let mut generations = WidgetGenerations::default();
    let launch_specs: Vec<WidgetLaunchSpec> = config
        .widgets
        .iter()
        .map(WidgetSettings::launch_spec)
        .collect();
    apply_plan(
        &startup_plan(&config.widgets),
        &launch_specs,
        &mut runtime,
        &mut generations,
    );
    let mut startup_warnings = Vec::new();
    let llm = if config.llm.enabled {
        build_llm_services(config, &mut startup_warnings)
    } else {
        None
    };
    if let Some(llm) = &llm {
        shutdown.register(LLM_SHUTDOWN_KEY, Arc::clone(&llm.cancellable));
    }
    let notifier = build_notifier(config, &shutdown);
    Services {
        runtime,
        generations,
        shutdown,
        termination,
        llm,
        startup_warnings,
        notifier,
        config_directory: directory.to_path_buf(),
        config: config.clone(),
    }
}

fn startup_plan(widgets: &[WidgetSettings]) -> ReloadPlan {
    ReloadPlan {
        changes: widgets
            .iter()
            .map(|widget| (widget.id.clone(), WidgetChange::Add))
            .collect(),
    }
}

fn build_llm_services(config: &AppConfig, warnings: &mut Vec<String>) -> Option<LlmServices> {
    let Some(working_directory) = llm_working_directory() else {
        warnings.push(NO_STATE_DIRECTORY_LLM_WARNING.to_string());
        return None;
    };
    let shared = build_llm_runner(&config.llm, working_directory);
    let runner: Arc<dyn LlmRunner> = shared.clone();
    let cancellable: Arc<dyn Cancellable> = shared;
    let store: Arc<dyn RecommendationStore> = match recommendation_file() {
        Some(path) => Arc::new(FileRecommendationStore::new(path)),
        None => {
            warnings.push(NO_STATE_DIRECTORY_STORE_WARNING.to_string());
            Arc::new(NullRecommendationStore)
        }
    };
    Some(LlmServices {
        runner,
        cancellable,
        store,
        config: config.llm.clone(),
        selections: llm_selections(&config.widgets),
    })
}

fn llm_selections(widgets: &[WidgetSettings]) -> HashMap<String, LlmSelection> {
    widgets
        .iter()
        .map(|widget| (widget.id.as_str().to_string(), widget.llm_selection.clone()))
        .collect()
}

fn build_notifier(config: &AppConfig, shutdown: &ShutdownCoordinator) -> CompositeNotifier {
    let mut notifiers: Vec<Box<dyn Notifier>> = Vec::new();
    if config.pomodoro.bell {
        notifiers.push(Box::new(BellNotifier::new(Box::new(std::io::stdout()))));
    }
    if let Some(command) = &config.pomodoro.notify_command {
        let notifier = Arc::new(CommandNotifier::new(command.clone()));
        shutdown.register(
            NOTIFIER_SHUTDOWN_KEY,
            Arc::clone(&notifier) as Arc<dyn Cancellable>,
        );
        notifiers.push(Box::new(notifier));
    }
    CompositeNotifier::new(notifiers)
}

impl App {
    fn run_event_loop(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<(), AppError> {
        let mut last_tick = Instant::now();
        let mut alert_tracker = AlertTracker::new();
        loop {
            if should_stop(&self.services.termination) {
                return Ok(());
            }
            self.apply_pending_events(&mut alert_tracker);
            let elapsed = last_tick.elapsed();
            last_tick = Instant::now();
            if let Some(alert) = self.state.advance_pomodoro(elapsed) {
                self.services.notifier.notify(&alert);
            }
            terminal.draw(|frame| {
                render(frame, &self.state, self.spans.as_slice(), self.clock.now())
            })?;
            if !event::poll(INPUT_POLL_INTERVAL)? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            let Some(input) = from_crossterm(key) else {
                continue;
            };
            if let Some(Command::Quit) = self.handle_input(input) {
                return Ok(());
            }
        }
    }

    fn apply_pending_events(&mut self, alert_tracker: &mut AlertTracker) {
        while let Ok(app_event) = self.events.receiver.try_recv() {
            let context = EventContext {
                generations: &self.services.generations,
                notifier: &self.services.notifier,
                recommendation_store: self.services.llm.as_ref().map(|llm| llm.store.as_ref()),
            };
            apply_event(
                &mut self.state,
                app_event,
                &context,
                alert_tracker,
                self.clock.now(),
            );
        }
    }

    fn handle_input(&mut self, input: KeyInput) -> Option<Command> {
        let command = self.state.handle_key(input)?;
        match command {
            Command::Quit => return Some(Command::Quit),
            Command::RefreshAll => self.services.runtime.trigger_all(),
            Command::ReloadConfig => {
                reload_config(&mut self.state, &mut self.services, &mut self.spans)
            }
            Command::Execute(action) => {
                spawn_action(action, &mut self.state, &self.services, &self.events.sender)
            }
            Command::RequestRecommendation => request_recommendation(
                &mut self.state,
                &self.services,
                &self.clock,
                &self.events.sender,
            ),
            Command::SendRecommendation(plan) => start_recommendation(
                &mut self.state,
                &self.services,
                plan,
                &self.clock,
                &self.events.sender,
            ),
        }
        None
    }
}

fn stored_recommendation_record(
    store: &dyn RecommendationStore,
    now: NaiveDateTime,
) -> WidgetRecord {
    let mut record = WidgetRecord::loading();
    record.apply(stored_recommendation_result(store), now);
    record
}

fn reload_config(state: &mut AppState, services: &mut Services, spans: &mut Vec<u16>) {
    let outcome = reload_widgets(
        read_config_file(&config_file_path(&services.config_directory)),
        |text| prepare_reload(&services.config.widgets, text),
        &mut services.runtime,
        &mut services.generations,
    );
    match outcome {
        Ok((next, plan)) => apply_reloaded_config(state, services, spans, next, &plan),
        Err(error) => state.show_notice(error.to_string()),
    }
}

fn apply_reloaded_config(
    state: &mut AppState,
    services: &mut Services,
    spans: &mut Vec<u16>,
    next: AppConfig,
    plan: &ReloadPlan,
) {
    let notice = reload_notice(&restart_sections(&services.config, &next), &next.warnings);
    let TabLayout {
        entries,
        tabs,
        spans: reloaded_spans,
    } = rebuilt_layout(state, &next, plan);
    state.replace_tabs(entries, tabs);
    *spans = reloaded_spans;
    services.config = retain_restart_sections(&services.config, next);
    if let Some(llm) = services.llm.as_mut() {
        llm.selections = llm_selections(&services.config.widgets);
    }
    state.show_notice(notice);
}

fn rebuilt_layout(state: &AppState, config: &AppConfig, plan: &ReloadPlan) -> TabLayout {
    let recommendation_record = state.recommendation_record().cloned();
    let mut layout = build_tab_views(
        &widget_placements(config),
        &tab_placements(config),
        recommendation_record,
    );
    carry_over_records(
        &mut layout.entries,
        state.entries(),
        &plan.kept_widget_ids(),
    );
    layout
}

fn spawn_action(
    action: ConfirmedAction,
    state: &mut AppState,
    services: &Services,
    sender: &Sender<AppEvent>,
) {
    let widget = action.widget().clone();
    let (Some(runner), Some(refresh)) = (
        services.runtime.runner(&widget),
        services.runtime.refresh_signal(&widget),
    ) else {
        state.finish_action(&widget);
        return;
    };
    let runner = Arc::clone(runner);
    let sender = sender.clone();
    thread::spawn(move || {
        let notice = execute_confirmed(runner.as_ref(), action);
        let _ = sender.send(AppEvent::ActionFinished(widget, notice));
        refresh.request();
    });
}

fn spawn_event_task(sender: &Sender<AppEvent>, work: impl FnOnce() -> AppEvent + Send + 'static) {
    let sender = sender.clone();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
}

fn widget_contexts<'a>(
    state: &'a AppState,
    selections: &'a HashMap<String, LlmSelection>,
) -> Vec<WidgetContext<'a>> {
    state
        .entries()
        .iter()
        .filter_map(|entry| {
            Some(WidgetContext {
                widget_id: &entry.id,
                selection: selections.get(entry.id.as_str())?,
                state: &entry.record,
            })
        })
        .collect()
}

fn request_recommendation(
    state: &mut AppState,
    services: &Services,
    clock: &dyn Clock,
    sender: &Sender<AppEvent>,
) {
    let Some(llm) = &services.llm else {
        state.show_notice(LLM_DISABLED_NOTICE.to_string());
        return;
    };
    let criteria =
        FilePriorityPrompt::new(services.config_directory.join(&llm.config.priority_prompt));
    let policy = SendPolicy {
        confirm_before_send: llm.config.confirm_before_send,
        target_label: describe_target(&llm.config),
    };
    let step = prepare_recommendation(
        &criteria,
        &widget_contexts(state, &llm.selections),
        clock.now(),
        &policy,
    );
    match step {
        RecommendationStep::Notice(text) => state.show_notice(text),
        RecommendationStep::Confirm { text, plan } => state.ask_send_confirmation(text, plan),
        RecommendationStep::Send(plan) => {
            start_recommendation(state, services, plan, clock, sender)
        }
    }
}

fn start_recommendation(
    state: &mut AppState,
    services: &Services,
    plan: RecommendationPlan,
    clock: &dyn Clock,
    sender: &Sender<AppEvent>,
) {
    let Some(llm) = &services.llm else {
        return;
    };
    state.begin_widget_refresh(&WidgetId::recommendation());
    let runner = Arc::clone(&llm.runner);
    let store = Arc::clone(&llm.store);
    let model = llm.config.model.clone();
    let now = clock.now();
    spawn_event_task(sender, move || {
        AppEvent::RecommendationFinished(execute_recommendation(
            runner.as_ref(),
            store.as_ref(),
            &plan,
            model.as_deref(),
            now,
        ))
    });
}

fn describe_target(config: &LlmConfig) -> String {
    let harness = config.harness.program_name();
    match &config.model {
        Some(model) => format!("{harness}/{model}"),
        None => harness.to_string(),
    }
}
