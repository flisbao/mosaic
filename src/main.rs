#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod check_command;
mod cli_info;
mod confirmation;
mod plugin_setup_command;
mod plugins_command;
mod remote_plugins_command;
mod skills_command;
mod tabs_command;
mod themes_command;
mod update_command;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use mosaic::application::alert_tracker::AlertTracker;
use mosaic::application::execute_action::execute_confirmed;
use mosaic::application::launch_spec::WidgetLaunchSpec;
use mosaic::application::llm::execute_recommendation;
use mosaic::application::ports::{
    Cancellable, Clock, LlmRunner, Notifier, RecommendationStore, SetupCatalog, WidgetConfigWriter,
    WidgetRunner,
};
use mosaic::application::recommendation::{LlmSelection, RecommendationPlan, WidgetContext};
use mosaic::application::recommendation_request::{
    prepare_recommendation, RecommendationStep, SendPolicy,
};
use mosaic::application::reload::{apply_plan, reload_widgets, WidgetGenerations};
use mosaic::application::reload_plan::{ReloadPlan, WidgetChange};
use mosaic::application::shutdown::{should_stop, ShutdownCoordinator};
use mosaic::application::update::{
    newer_version_for_notice, update_check_enabled, NO_UPDATE_CHECK_VARIABLE,
};
use mosaic::display_in_portuguese;
use mosaic::domain::action::ConfirmedAction;
use mosaic::domain::color_depth::ColorDepth;
use mosaic::domain::language::{Language, LocalizedMessage};
use mosaic::domain::pomodoro::PomodoroTimer;
use mosaic::domain::release::ReleaseVersion;
use mosaic::domain::widget::{WidgetId, WidgetRecord};
use mosaic::infrastructure::app_paths::{
    llm_working_directory, recommendation_file, update_check_file,
};
use mosaic::infrastructure::concurrency_gate::ConcurrencyGate;
use mosaic::infrastructure::config::{
    config_directory, load_config, prepare_reload, read_installed_config, restart_sections,
    retain_restart_sections, AppConfig, ConfigError, LlmConfig, WidgetSettings,
};
use mosaic::infrastructure::curl_release_source::{
    download_base_from_environment, CurlReleaseSource,
};
use mosaic::infrastructure::default_files::install_defaults_if_missing;
use mosaic::infrastructure::llm_cli::build_llm_runner;
use mosaic::infrastructure::notifiers::{BellNotifier, CommandNotifier, CompositeNotifier};
use mosaic::infrastructure::plugin_runner::{PluginRunner, MAXIMUM_CONCURRENT_PARSES};
use mosaic::infrastructure::priority_prompt::FilePriorityPrompt;
use mosaic::infrastructure::recommendation_file::{
    FileRecommendationStore, NullRecommendationStore,
};
use mosaic::infrastructure::setup_catalog::{FileSetupCatalog, FileWidgetConfigWriter};
use mosaic::infrastructure::shared_language::SharedLanguage;
use mosaic::infrastructure::shutdown::termination_requested;
use mosaic::infrastructure::system_clock::SystemClock;
use mosaic::infrastructure::update_check_file::FileUpdateCheckStore;
use mosaic::infrastructure::widget_runtime::{RunnerFactory, WidgetRuntime};
use mosaic::presentation::app_events::{
    apply_event, stored_recommendation_result, AppEvent, EventContext,
};
use mosaic::presentation::app_state::{AppState, Command, NewWidget};
use mosaic::presentation::key_input::{from_crossterm, KeyInput};
use mosaic::presentation::messages;
use mosaic::presentation::redraw_policy::{clock_changes_what_is_drawn, needs_time_redraw};
use mosaic::presentation::reload_notice::{added_widget_notice, reload_notice};
use mosaic::presentation::render::render;
use mosaic::presentation::tab_layout::{
    build_tab_views, carry_over_records, startup_notice, TabLayout, TabPlacement, WidgetPlacement,
};
use mosaic::presentation::theme_application::apply_theme;
use ratatui::crossterm::event::{self, Event};
use thiserror::Error;

const EVENT_WAIT_LIMIT: Duration = Duration::from_millis(250);
const EVENT_QUEUE_CAPACITY: usize = 16;
const CHECK_SUBCOMMAND: &str = "check";
const SKILLS_SUBCOMMAND: &str = "skills";
const PLUGINS_SUBCOMMAND: &str = "plugins";
const THEMES_SUBCOMMAND: &str = "themes";
const TABS_SUBCOMMAND: &str = "tabs";
const UPDATE_SUBCOMMAND: &str = "update";
const LLM_SHUTDOWN_KEY: &str = "llm";
const NOTIFIER_SHUTDOWN_KEY: &str = "notifier";
const UPDATE_CHECK_SHUTDOWN_KEY: &str = "update-check";

#[derive(Debug, Error)]
enum AppError {
    Config(#[from] ConfigError),
    Terminal(#[from] std::io::Error),
}

impl LocalizedMessage for AppError {
    fn message(&self, language: Language) -> String {
        match self {
            AppError::Config(error) => error.message(language),
            AppError::Terminal(error) => messages::terminal_failure(language, &error.to_string()),
        }
    }
}

display_in_portuguese!(AppError);

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
    language: SharedLanguage,
    color_depth: ColorDepth,
    setup_catalog: Box<dyn SetupCatalog>,
    config_writer: Box<dyn WidgetConfigWriter>,
}

struct EventChannel {
    sender: SyncSender<AppEvent>,
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
    if arguments.first().map(String::as_str) == Some(SKILLS_SUBCOMMAND) {
        return skills_command::run(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some(PLUGINS_SUBCOMMAND) {
        return plugins_command::run(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some(TABS_SUBCOMMAND) {
        return tabs_command::run(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some(THEMES_SUBCOMMAND) {
        return themes_command::run(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some(UPDATE_SUBCOMMAND) {
        return update_command::run(&arguments[1..]);
    }
    if let Some(exit_code) = cli_info::run(&arguments) {
        return exit_code;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "mosaic: {}",
                error.message(Language::detect_from_environment())
            );
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let fallback_language = Language::detect_from_environment();
    let directory = config_directory()?;
    install_defaults_if_missing(&directory, fallback_language)?;
    let config = load_config(&directory, fallback_language)?;
    let termination = termination_requested()?;
    let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    let color_depth = ColorDepth::detect(|name| std::env::var(name).ok());
    let language = SharedLanguage::new(config.language);
    let services = build_services(
        &config,
        &directory,
        &sender,
        Arc::clone(&termination),
        color_depth,
        &language,
    );
    start_update_check(&config, &services.shutdown, &sender);
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

fn start_update_check(
    config: &AppConfig,
    shutdown: &ShutdownCoordinator,
    sender: &SyncSender<AppEvent>,
) {
    let disabled_by_environment = std::env::var(NO_UPDATE_CHECK_VARIABLE).ok();
    if !update_check_enabled(config.update.check, disabled_by_environment.as_deref()) {
        return;
    }
    let (Some(path), Ok(current)) = (
        update_check_file(),
        ReleaseVersion::parse(env!("CARGO_PKG_VERSION")),
    ) else {
        return;
    };
    let source = Arc::new(CurlReleaseSource::new(download_base_from_environment()));
    shutdown.register(
        UPDATE_CHECK_SHUTDOWN_KEY,
        Arc::clone(&source) as Arc<dyn Cancellable>,
    );
    let sender = sender.clone();
    thread::spawn(move || {
        let store = FileUpdateCheckStore::new(path);
        if let Some(latest) =
            newer_version_for_notice(source.as_ref(), &store, &SystemClock, &current)
        {
            let _ = sender.send(AppEvent::UpdateAvailable(latest));
        }
    });
}

fn initial_state(
    config: &AppConfig,
    services: &Services,
    clock: &dyn Clock,
) -> (AppState, Vec<u16>) {
    let recommendation_record = services
        .llm
        .as_ref()
        .map(|llm| stored_recommendation_record(llm.store.as_ref(), clock.now(), config.language));
    let TabLayout {
        entries,
        tabs,
        spans,
    } = build_tab_views(
        &widget_placements(config),
        &tab_placements(config),
        recommendation_record,
    );
    let mut state = AppState::new(entries, tabs).with_language(config.language);
    let warnings = apply_theme(
        &mut state,
        &config.theme,
        services.color_depth,
        startup_warnings(config, services),
    );
    if let Some(notice) = startup_notice(&warnings) {
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
    sender: &SyncSender<AppEvent>,
    termination: Arc<AtomicBool>,
    color_depth: ColorDepth,
    language: &SharedLanguage,
) -> Services {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let working_directory = directory.to_path_buf();
    let parse_gate = Arc::new(ConcurrencyGate::new(MAXIMUM_CONCURRENT_PARSES));
    let runner_language = language.clone();
    let factory: RunnerFactory = Box::new(move |spec| {
        Arc::new(
            PluginRunner::new(spec.command.clone(), working_directory.clone())
                .with_parse_gate(Arc::clone(&parse_gate))
                .with_language(runner_language.clone()),
        ) as Arc<dyn WidgetRunner>
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
        build_llm_services(config, &mut startup_warnings, language)
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
        language: language.clone(),
        color_depth,
        setup_catalog: Box::new(
            FileSetupCatalog::new(directory.to_path_buf()).with_language(language.clone()),
        ),
        config_writer: Box::new(
            FileWidgetConfigWriter::new(directory.to_path_buf()).with_language(language.clone()),
        ),
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

fn build_llm_services(
    config: &AppConfig,
    warnings: &mut Vec<String>,
    language: &SharedLanguage,
) -> Option<LlmServices> {
    let Some(working_directory) = llm_working_directory() else {
        warnings.push(messages::no_state_directory_llm_warning(config.language).to_string());
        return None;
    };
    let shared = build_llm_runner(&config.llm, working_directory, language.clone());
    let runner: Arc<dyn LlmRunner> = shared.clone();
    let cancellable: Arc<dyn Cancellable> = shared;
    let store: Arc<dyn RecommendationStore> = match recommendation_file() {
        Some(path) => Arc::new(FileRecommendationStore::new(path).with_language(language.clone())),
        None => {
            warnings.push(messages::no_state_directory_store_warning(config.language).to_string());
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
        spawn_input_reader(self.events.sender.clone());
        let mut last_tick = Instant::now();
        let mut last_drawn = self.clock.now();
        let mut needs_redraw = true;
        let mut alert_tracker = AlertTracker::new();
        loop {
            if should_stop(&self.services.termination) {
                return Ok(());
            }
            let mut app_events = Vec::new();
            match self.events.receiver.recv_timeout(EVENT_WAIT_LIMIT) {
                Ok(app_event) => app_events.push(app_event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
            app_events.extend(self.events.receiver.try_iter());
            needs_redraw |= !app_events.is_empty();
            for app_event in app_events {
                match app_event {
                    AppEvent::Input(input) => {
                        if let Some(Command::Quit) = self.handle_input(input) {
                            return Ok(());
                        }
                    }
                    AppEvent::TerminalResized => {}
                    AppEvent::TerminalReadFailed(message) => {
                        return Err(std::io::Error::other(message).into());
                    }
                    other => self.apply_app_event(other, &mut alert_tracker),
                }
            }
            let elapsed = last_tick.elapsed();
            last_tick = Instant::now();
            if let Some(alert) = self.state.advance_pomodoro(elapsed) {
                self.services.notifier.notify(&alert);
                needs_redraw = true;
            }
            let now = self.clock.now();
            let ticking = clock_changes_what_is_drawn(&self.state);
            if needs_redraw || needs_time_redraw(last_drawn, now, ticking) {
                terminal.draw(|frame| render(frame, &self.state, self.spans.as_slice(), now))?;
                last_drawn = now;
                needs_redraw = false;
            }
        }
    }

    fn apply_app_event(&mut self, app_event: AppEvent, alert_tracker: &mut AlertTracker) {
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
            Command::OpenAddWidget => open_add_widget(&mut self.state, &self.services),
            Command::AppendWidget(widget) => append_widget(
                &widget,
                &mut self.state,
                &mut self.services,
                &mut self.spans,
            ),
        }
        None
    }
}

fn spawn_input_reader(sender: SyncSender<AppEvent>) {
    thread::spawn(move || {
        read_input_events(&sender, event::read);
    });
}

fn read_input_events(
    read_event_sender: &SyncSender<AppEvent>,
    mut read_event: impl FnMut() -> std::io::Result<Event>,
) {
    loop {
        let terminal_event = match read_event() {
            Ok(terminal_event) => terminal_event,
            Err(error) => {
                let _ = read_event_sender.send(AppEvent::TerminalReadFailed(error.to_string()));
                break;
            }
        };
        let app_event = match terminal_event {
            Event::Key(key) => from_crossterm(key).map(AppEvent::Input),
            Event::Resize(_, _) => Some(AppEvent::TerminalResized),
            _ => None,
        };
        if let Some(app_event) = app_event {
            if read_event_sender.send(app_event).is_err() {
                break;
            }
        }
    }
}

fn stored_recommendation_record(
    store: &dyn RecommendationStore,
    now: NaiveDateTime,
    language: Language,
) -> WidgetRecord {
    let mut record = WidgetRecord::loading();
    record.apply(stored_recommendation_result(store, language), now);
    record
}

fn reload_config(state: &mut AppState, services: &mut Services, spans: &mut Vec<u16>) {
    let fallback_language = Language::detect_from_environment();
    let outcome = reload_widgets(
        read_installed_config(&services.config_directory, fallback_language),
        |text| prepare_reload(&services.config.widgets, text, fallback_language),
        &mut services.runtime,
        &mut services.generations,
    );
    match outcome {
        Ok((next, plan)) => apply_reloaded_config(state, services, spans, next, &plan),
        Err(error) => state.show_notice(error.message(state.language())),
    }
}

fn open_add_widget(state: &mut AppState, services: &Services) {
    match services.setup_catalog.load() {
        Ok(snapshot) => state.open_add_widget(snapshot),
        Err(error) => state.show_notice(messages::cannot_add_widget(
            state.language(),
            &error.to_string(),
        )),
    }
}

fn append_widget(
    widget: &NewWidget,
    state: &mut AppState,
    services: &mut Services,
    spans: &mut Vec<u16>,
) {
    match services.config_writer.append(&widget.blocks) {
        Ok(()) => {
            state.finish_adding_widget();
            reload_config(state, services, spans);
            let reload_result = state.notice().map(str::to_string);
            state.show_notice(added_widget_notice(
                &widget.widget_id,
                reload_result.as_deref(),
                state.language(),
            ));
        }
        Err(error) => state.fail_adding_widget(error.to_string()),
    }
}

fn apply_reloaded_config(
    state: &mut AppState,
    services: &mut Services,
    spans: &mut Vec<u16>,
    next: AppConfig,
    plan: &ReloadPlan,
) {
    state.set_language(next.language);
    services.language.set(next.language);
    let warnings = apply_theme(
        state,
        &next.theme,
        services.color_depth,
        next.warnings.clone(),
    );
    let notice = reload_notice(
        &restart_sections(&services.config, &next),
        &warnings,
        next.language,
    );
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
    sender: &SyncSender<AppEvent>,
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

fn spawn_event_task(
    sender: &SyncSender<AppEvent>,
    work: impl FnOnce() -> AppEvent + Send + 'static,
) {
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
    sender: &SyncSender<AppEvent>,
) {
    let Some(llm) = &services.llm else {
        state.show_notice(messages::llm_disabled_notice(state.language()).to_string());
        return;
    };
    let criteria =
        FilePriorityPrompt::new(services.config_directory.join(&llm.config.priority_prompt))
            .with_language(state.language());
    let policy = SendPolicy {
        confirm_before_send: llm.config.confirm_before_send,
        target_label: describe_target(&llm.config),
        language: state.language(),
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
    sender: &SyncSender<AppEvent>,
) {
    let Some(llm) = &services.llm else {
        return;
    };
    state.begin_widget_refresh(&WidgetId::recommendation());
    let runner = Arc::clone(&llm.runner);
    let store = Arc::clone(&llm.store);
    let model = llm.config.model.clone();
    let now = clock.now();
    let language = state.language();
    spawn_event_task(sender, move || {
        AppEvent::RecommendationFinished(execute_recommendation(
            runner.as_ref(),
            store.as_ref(),
            &plan,
            model.as_deref(),
            now,
            language,
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

#[cfg(test)]
mod input_reader_tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn input_reader_reports_terminal_read_errors() {
        let (sender, receiver) = sync_channel(EVENT_QUEUE_CAPACITY);
        let mut reads = [Err(std::io::Error::other("read failed"))].into_iter();

        read_input_events(&sender, || reads.next().unwrap());

        assert!(matches!(
            receiver.try_recv(),
            Ok(AppEvent::TerminalReadFailed(message)) if message == "read failed"
        ));
    }
}
