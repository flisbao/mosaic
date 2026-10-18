use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use crossterm::event::{self, Event};
use mosaic::application::alert_tracker::AlertTracker;
use mosaic::application::check_report::build_check_report;
use mosaic::application::execute_action::{execute_confirmed, ActionNotice};
use mosaic::application::llm::{
    execute_recommendation, LlmError, LlmRunner, RecommendationStore, StoredRecommendation,
};
use mosaic::application::pomodoro_notifications::{AlertMessage, Notifier};
use mosaic::application::ports::{Cancellable, Clock, SourceError};
use mosaic::application::recommendation::{
    build_plan, LlmSelection, RecommendationPlan, WidgetContext,
};
use mosaic::application::recommendation_view::{
    idle_snapshot, recommendation_snapshot, IA_WIDGET_ID,
};
use mosaic::application::refresh::refresh_widget;
use mosaic::application::reload::{
    apply_if_current, apply_plan, reload_widgets, WidgetGenerations, WidgetLifecycle,
};
use mosaic::application::reload_plan::{ReloadPlan, WidgetChange};
use mosaic::application::shutdown::{should_stop, ShutdownCoordinator};
use mosaic::application::widget_notices::with_interval_notice;
use mosaic::domain::action::ConfirmedAction;
use mosaic::domain::pomodoro::PomodoroTimer;
use mosaic::domain::widget::WidgetSnapshot;
use mosaic::domain::widget::{WidgetId, WidgetRecord};
use mosaic::domain::widget_content::AlertLevel;
use mosaic::infrastructure::config::{
    config_directory, config_file_path, load_config, parse_config, prepare_reload,
    read_config_file, reload_notice, retain_restart_sections, AppConfig, ConfigError, Harness,
    LlmConfig, WidgetSettings, DEFAULT_INTERVAL_SECONDS, DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM,
    DEFAULT_LLM_MAX_ITEMS, DEFAULT_SPAN,
};
use mosaic::infrastructure::default_files::install_defaults_if_missing;
use mosaic::infrastructure::llm_cli::{ClaudeCli, OpenCodeCli};
use mosaic::infrastructure::notifiers::{BellNotifier, CommandNotifier, CompositeNotifier};
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::protocol_report::parse_snapshot_report;
use mosaic::infrastructure::recommendation_file::{
    default_recommendation_path, FileRecommendationStore,
};
use mosaic::infrastructure::shutdown::termination_requested;
use mosaic::infrastructure::system_clock::SystemClock;
use mosaic::infrastructure::terminal_capabilities::detect_graphics_support;
use mosaic::presentation::app_state::{AppState, Command};
use mosaic::presentation::key_input::from_crossterm;
use mosaic::presentation::render::render;
use mosaic::presentation::tab_layout::{
    build_tab_views, carry_over_records, startup_notice, TabLayout, TabPlacement, WidgetPlacement,
};
use mosaic::presentation::widget_view::{lines_to_plain_text, render_blocks};
use thiserror::Error;

const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const ACTION_DONE_FALLBACK_MESSAGE: &str = "Ação concluída";
const RECOMMENDATION_READY_MESSAGE: &str = "Recomendação atualizada";
const LLM_DISABLED_NOTICE: &str = "IA desligada na configuração";
const APP_DIRECTORY_NAME: &str = "mosaic";
const LLM_WORKING_DIRECTORY_NAME: &str = "llm";
const FALLBACK_STORE_FILE_NAME: &str = "mosaic-recommendation.json";
const CHECK_SUBCOMMAND: &str = "check";
const CHECK_USAGE: &str = "uso: mosaic check <comando> [args...] [--env NOME=valor]...";
const CHECK_ENV_FLAG: &str = "--env";
const CHECK_WIDGET_ID: &str = "check";
const CHECK_TIMEOUT_SECONDS: u64 = 10;
const CHECK_PREVIEW_WIDTH: usize = 60;
const EXIT_CODE_USAGE: u8 = 2;
const EXIT_CODE_FAILURE: u8 = 1;
const CHECK_CONFIG_FLAG: &str = "--config";
const CHECK_CONFIG_USAGE: &str = "uso: mosaic check --config [caminho]";
const LLM_SHUTDOWN_KEY: &str = "llm";
const WIDGET_SHUTDOWN_KEY_PREFIX: &str = "widget:";

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("falha de terminal: {0}")]
    Terminal(#[from] std::io::Error),
}

enum AppEvent {
    WidgetUpdated(WidgetId, u64, Result<WidgetSnapshot, SourceError>),
    ActionFinished(
        WidgetId,
        u64,
        ActionNotice,
        Result<WidgetSnapshot, SourceError>,
    ),
    RecommendationFinished(Result<StoredRecommendation, LlmError>),
}

struct WidgetRuntime {
    working_directory: PathBuf,
    sender: Sender<AppEvent>,
    shutdown: Arc<ShutdownCoordinator>,
    runners: HashMap<WidgetId, Arc<PluginRunner>>,
    refresh_triggers: HashMap<WidgetId, Sender<()>>,
}

impl WidgetRuntime {
    fn new(
        working_directory: PathBuf,
        sender: Sender<AppEvent>,
        shutdown: Arc<ShutdownCoordinator>,
    ) -> Self {
        Self {
            working_directory,
            sender,
            shutdown,
            runners: HashMap::new(),
            refresh_triggers: HashMap::new(),
        }
    }

    fn runner(&self, id: &WidgetId) -> Option<&Arc<PluginRunner>> {
        self.runners.get(id)
    }

    fn trigger_all(&self) {
        for trigger in self.refresh_triggers.values() {
            let _ = trigger.send(());
        }
    }
}

fn shutdown_key(id: &WidgetId) -> String {
    format!("{WIDGET_SHUTDOWN_KEY_PREFIX}{}", id.as_str())
}

impl WidgetLifecycle for WidgetRuntime {
    type Spec = WidgetSettings;

    fn start(&mut self, settings: &WidgetSettings, generation: u64) {
        let id = settings.id.clone();
        let interval = Duration::from_secs(settings.interval_seconds);
        let interval_notice = settings
            .interval_adjusted
            .then_some(settings.interval_seconds);
        let runner = Arc::new(PluginRunner::new(
            settings.clone(),
            self.working_directory.clone(),
        ));
        self.shutdown.register(
            &shutdown_key(&id),
            Arc::clone(&runner) as Arc<dyn Cancellable>,
        );
        let trigger = spawn_refresher(
            id.clone(),
            generation,
            Arc::clone(&runner),
            interval,
            interval_notice,
            self.sender.clone(),
        );
        self.runners.insert(id.clone(), runner);
        self.refresh_triggers.insert(id, trigger);
    }

    fn retire(&mut self, id: &WidgetId) {
        self.shutdown.retire(&shutdown_key(id));
        self.runners.remove(id);
        self.refresh_triggers.remove(id);
    }
}

struct LlmServices {
    runner: Arc<dyn LlmRunner>,
    cancellable: Arc<dyn Cancellable>,
    store: Arc<dyn RecommendationStore>,
    config: LlmConfig,
    selections: HashMap<String, LlmSelection>,
}

struct Services {
    runtime: WidgetRuntime,
    generations: WidgetGenerations,
    shutdown: Arc<ShutdownCoordinator>,
    termination: Arc<AtomicBool>,
    llm: Option<LlmServices>,
    notifier: CompositeNotifier,
    config_directory: PathBuf,
    config: AppConfig,
}

struct CheckInvocation {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

fn parse_check_arguments(arguments: &[String]) -> Result<CheckInvocation, String> {
    let mut command = None;
    let mut args = Vec::new();
    let mut env = Vec::new();
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        if argument != CHECK_ENV_FLAG {
            match command {
                None => command = Some(argument.clone()),
                Some(_) => args.push(argument.clone()),
            }
            continue;
        }
        let assignment = remaining
            .next()
            .ok_or_else(|| format!("{CHECK_ENV_FLAG} precisa de NOME=valor\n{CHECK_USAGE}"))?;
        match assignment.split_once('=') {
            Some((name, value)) if !name.is_empty() => {
                env.push((name.to_string(), value.to_string()))
            }
            _ => return Err(format!("variável inválida: {assignment}\n{CHECK_USAGE}")),
        }
    }
    let command = command.ok_or_else(|| CHECK_USAGE.to_string())?;
    Ok(CheckInvocation { command, args, env })
}

fn check_preview(snapshot: &WidgetSnapshot) -> Vec<String> {
    let blocks = snapshot.summary.display_blocks();
    lines_to_plain_text(&render_blocks(&blocks, CHECK_PREVIEW_WIDTH))
}

fn run_check(arguments: &[String]) -> ExitCode {
    if arguments.first().map(String::as_str) == Some(CHECK_CONFIG_FLAG) {
        return run_check_config(&arguments[1..]);
    }
    let invocation = match parse_check_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    let settings = WidgetSettings {
        id: WidgetId::new(CHECK_WIDGET_ID),
        command: invocation.command,
        args: invocation.args,
        env: invocation.env,
        interval_seconds: DEFAULT_INTERVAL_SECONDS,
        interval_adjusted: false,
        timeout_seconds: CHECK_TIMEOUT_SECONDS,
        span: DEFAULT_SPAN,
        max_items: None,
        llm_sections: vec![],
        llm_max_items: DEFAULT_LLM_MAX_ITEMS,
        llm_max_characters_per_item: DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM,
    };
    let working_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let runner = PluginRunner::new(settings, working_directory);
    let parse = |json: &str| parse_snapshot_report(json).map_err(|error| error.to_string());
    let outcome = build_check_report(runner.raw_snapshot_output(), &parse, &check_preview);
    println!("{}", outcome.text);
    ExitCode::from(u8::try_from(outcome.exit_code).unwrap_or(EXIT_CODE_FAILURE))
}

fn run_check_config(arguments: &[String]) -> ExitCode {
    let path = match config_path_argument(arguments) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    match read_config_file(&path).and_then(|text| parse_config(&text)) {
        Ok(config) => {
            for warning in &config.warnings {
                println!("aviso: {warning}");
            }
            println!("configuração válida: {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("mosaic: {error}");
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn config_path_argument(arguments: &[String]) -> Result<PathBuf, String> {
    match arguments {
        [] => config_directory()
            .map(|directory| config_file_path(&directory))
            .map_err(|error| error.to_string()),
        [path] => Ok(PathBuf::from(path)),
        _ => Err(CHECK_CONFIG_USAGE.to_string()),
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some(CHECK_SUBCOMMAND) {
        return run_check(&arguments[1..]);
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
    let _graphics_support = detect_graphics_support(&|name| std::env::var(name).ok());

    let termination = termination_requested()?;
    let (sender, receiver) = mpsc::channel();
    let mut services = build_services(&config, &directory, &sender, Arc::clone(&termination));
    let clock = SystemClock;
    let ia_record = services
        .llm
        .as_ref()
        .map(|llm| stored_ia_record(llm.store.as_ref(), clock.now()));
    let TabLayout {
        entries,
        tabs,
        mut spans,
    } = build_tab_views(
        &widget_placements(&config),
        &tab_placements(&config),
        ia_record,
    );
    let mut state = AppState::new(entries, tabs);
    if let Some(notice) = startup_notice(&config.warnings) {
        state.show_notice(notice);
    }
    if config.pomodoro.enabled {
        state = state.with_pomodoro(PomodoroTimer::new(config.pomodoro.settings()));
    }

    let mut terminal = ratatui::init();
    let result = event_loop(
        &mut terminal,
        &mut state,
        &mut spans,
        &clock,
        &mut services,
        &sender,
        &receiver,
    );
    ratatui::restore();
    services.shutdown.shutdown();
    result
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
    let mut runtime = WidgetRuntime::new(
        directory.to_path_buf(),
        sender.clone(),
        Arc::clone(&shutdown),
    );
    let mut generations = WidgetGenerations::default();
    apply_plan(
        &startup_plan(&config.widgets),
        &config.widgets,
        &mut runtime,
        &mut generations,
    );
    let llm = config.llm.enabled.then(|| build_llm_services(config));
    if let Some(llm) = &llm {
        shutdown.register(LLM_SHUTDOWN_KEY, Arc::clone(&llm.cancellable));
    }
    Services {
        runtime,
        generations,
        shutdown,
        termination,
        llm,
        notifier: build_notifier(config),
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

fn share_llm_runner<T: LlmRunner + Cancellable + 'static>(
    runner: T,
) -> (Arc<dyn LlmRunner>, Arc<dyn Cancellable>) {
    let shared = Arc::new(runner);
    (
        Arc::clone(&shared) as Arc<dyn LlmRunner>,
        shared as Arc<dyn Cancellable>,
    )
}

fn build_llm_services(config: &AppConfig) -> LlmServices {
    let working_directory = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIRECTORY_NAME)
        .join(LLM_WORKING_DIRECTORY_NAME);
    let timeout = Duration::from_secs(config.llm.timeout_seconds);
    let (runner, cancellable) = match config.llm.harness {
        Harness::Claude => share_llm_runner(ClaudeCli::new(
            "claude".to_string(),
            working_directory,
            config.llm.max_budget_usd,
            timeout,
        )),
        Harness::OpenCode => share_llm_runner(OpenCodeCli::new(
            "opencode".to_string(),
            working_directory,
            timeout,
        )),
    };
    let store_path = default_recommendation_path()
        .unwrap_or_else(|| std::env::temp_dir().join(FALLBACK_STORE_FILE_NAME));
    LlmServices {
        runner,
        cancellable,
        store: Arc::new(FileRecommendationStore::new(store_path)),
        config: config.llm.clone(),
        selections: llm_selections(&config.widgets),
    }
}

fn llm_selections(widgets: &[WidgetSettings]) -> HashMap<String, LlmSelection> {
    widgets
        .iter()
        .map(|widget| {
            (
                widget.id.as_str().to_string(),
                LlmSelection {
                    sections: widget.llm_sections.clone(),
                    max_items: widget.llm_max_items,
                    max_characters_per_item: widget.llm_max_characters_per_item,
                },
            )
        })
        .collect()
}

fn build_notifier(config: &AppConfig) -> CompositeNotifier {
    let mut notifiers: Vec<Box<dyn Notifier>> = Vec::new();
    if config.pomodoro.bell {
        notifiers.push(Box::new(BellNotifier::new(Box::new(std::io::stdout()))));
    }
    if let Some(command) = &config.pomodoro.notify_command {
        notifiers.push(Box::new(CommandNotifier::new(command.clone())));
    }
    CompositeNotifier::new(notifiers)
}

fn spawn_refresher(
    id: WidgetId,
    generation: u64,
    runner: Arc<PluginRunner>,
    interval: Duration,
    interval_notice: Option<u64>,
    sender: Sender<AppEvent>,
) -> Sender<()> {
    let (trigger, triggered) = mpsc::channel();
    thread::spawn(move || loop {
        let refreshed = refresh_widget(runner.as_ref());
        let state = match interval_notice {
            Some(seconds) => with_interval_notice(refreshed, seconds),
            None => refreshed,
        };
        if sender
            .send(AppEvent::WidgetUpdated(id.clone(), generation, state))
            .is_err()
        {
            break;
        }
        if let Err(RecvTimeoutError::Disconnected) = triggered.recv_timeout(interval) {
            break;
        }
    });
    trigger
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut AppState,
    spans: &mut Vec<u16>,
    clock: &dyn Clock,
    services: &mut Services,
    sender: &Sender<AppEvent>,
    receiver: &mpsc::Receiver<AppEvent>,
) -> Result<(), AppError> {
    let mut last_tick = Instant::now();
    let mut alert_tracker = AlertTracker::new();
    loop {
        if should_stop(&services.termination) {
            return Ok(());
        }
        while let Ok(app_event) = receiver.try_recv() {
            apply_event(state, app_event, services, &mut alert_tracker, clock.now());
        }
        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();
        if let Some(alert) = state.advance_pomodoro(elapsed) {
            services.notifier.notify(&alert);
        }
        terminal.draw(|frame| render(frame, state, spans.as_slice(), clock.now()))?;
        if !event::poll(INPUT_POLL_INTERVAL)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        let Some(input) = from_crossterm(key) else {
            continue;
        };
        match state.handle_key(input) {
            Some(Command::Quit) => return Ok(()),
            Some(Command::RefreshAll) => services.runtime.trigger_all(),
            Some(Command::ReloadConfig) => reload_config(state, services, spans),
            Some(Command::Execute(action)) => spawn_action(action, services, sender),
            Some(Command::RequestRecommendation) => {
                request_recommendation(state, services, clock, sender)
            }
            Some(Command::SendRecommendation(plan)) => {
                start_recommendation(state, services, plan, clock, sender)
            }
            None => {}
        }
    }
}

fn announce_widget_alerts(
    state: &mut AppState,
    services: &Services,
    tracker: &mut AlertTracker,
    id: &WidgetId,
    result: &Result<WidgetSnapshot, SourceError>,
) {
    let Ok(snapshot) = result else {
        return;
    };
    for alert in tracker.fresh_alerts(id, &snapshot.alerts) {
        let message = AlertMessage {
            title: alert.title,
            body: alert.body,
        };
        if alert.level == AlertLevel::Warn {
            services.notifier.notify(&message);
        }
        state.show_alert(message);
    }
}

fn apply_event(
    state: &mut AppState,
    app_event: AppEvent,
    services: &Services,
    tracker: &mut AlertTracker,
    now: NaiveDateTime,
) {
    match app_event {
        AppEvent::WidgetUpdated(id, generation, result) => {
            apply_if_current(&services.generations, &id, generation, result, |result| {
                announce_widget_alerts(state, services, tracker, &id, &result);
                state.update_widget(&id, result, now);
            });
        }
        AppEvent::ActionFinished(id, generation, notice, result) => {
            apply_if_current(&services.generations, &id, generation, result, |result| {
                announce_widget_alerts(state, services, tracker, &id, &result);
                state.update_widget(&id, result, now);
            });
            state.show_notice(describe_notice(notice));
        }
        AppEvent::RecommendationFinished(result) => {
            apply_recommendation_result(state, services, result, now)
        }
    }
}

fn apply_recommendation_result(
    state: &mut AppState,
    services: &Services,
    result: Result<StoredRecommendation, LlmError>,
    now: NaiveDateTime,
) {
    let ia = WidgetId::new(IA_WIDGET_ID);
    match result {
        Ok(stored) => {
            state.update_widget(&ia, Ok(recommendation_snapshot(&stored)), now);
            state.show_notice(RECOMMENDATION_READY_MESSAGE.to_string());
        }
        Err(LlmError::Unavailable(message)) => {
            if let Some(llm) = &services.llm {
                state.update_widget(&ia, stored_ia_result(llm.store.as_ref()), now);
            }
            state.show_notice(message);
        }
        Err(LlmError::Failed(message)) => {
            state.update_widget(&ia, Err(SourceError::Failed(message)), now)
        }
    }
}

fn stored_ia_record(store: &dyn RecommendationStore, now: NaiveDateTime) -> WidgetRecord {
    let mut record = WidgetRecord::loading();
    record.apply(stored_ia_result(store), now);
    record
}

fn stored_ia_result(store: &dyn RecommendationStore) -> Result<WidgetSnapshot, SourceError> {
    Ok(match store.load() {
        Some(stored) => recommendation_snapshot(&stored),
        None => idle_snapshot(),
    })
}

fn describe_notice(notice: ActionNotice) -> String {
    match notice {
        ActionNotice::Succeeded(message) if message.is_empty() => {
            ACTION_DONE_FALLBACK_MESSAGE.to_string()
        }
        ActionNotice::Succeeded(message) => message,
        ActionNotice::Failed(message) => format!("Falha: {message}"),
    }
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
    let notice = reload_notice(&services.config, &next);
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
    let ia_record = state
        .entries()
        .iter()
        .find(|entry| entry.id.as_str() == IA_WIDGET_ID)
        .map(|entry| entry.record.clone());
    let mut layout = build_tab_views(
        &widget_placements(config),
        &tab_placements(config),
        ia_record,
    );
    carry_over_records(
        &mut layout.entries,
        state.entries(),
        &plan.kept_widget_ids(),
    );
    layout
}

fn spawn_action(action: ConfirmedAction, services: &Services, sender: &Sender<AppEvent>) {
    let widget = action.widget().clone();
    let (Some(runner), Some(generation)) = (
        services.runtime.runner(&widget),
        services.generations.current(&widget),
    ) else {
        return;
    };
    let runner = Arc::clone(runner);
    let sender = sender.clone();
    thread::spawn(move || {
        let (notice, result) = execute_confirmed(runner.as_ref(), runner.as_ref(), action);
        let _ = sender.send(AppEvent::ActionFinished(widget, generation, notice, result));
    });
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
    let prompt_path = services.config_directory.join(&llm.config.priority_prompt);
    let criteria = match fs::read_to_string(&prompt_path) {
        Ok(criteria) => criteria,
        Err(error) => {
            state.show_notice(format!(
                "Não foi possível ler {}: {error}",
                prompt_path.display()
            ));
            return;
        }
    };
    let plan = {
        let contexts: Vec<WidgetContext> = state
            .entries()
            .iter()
            .filter_map(|entry| {
                Some(WidgetContext {
                    widget_id: &entry.id,
                    selection: llm.selections.get(entry.id.as_str())?,
                    state: &entry.record,
                })
            })
            .collect();
        build_plan(&criteria, clock.now(), &contexts)
    };
    match plan {
        Err(error) => state.show_notice(error.to_string()),
        Ok(plan) if llm.config.confirm_before_send => {
            let text = format!(
                "Enviar {} itens (~{} tokens, estimativa) para {}?",
                plan.item_count,
                plan.estimated_tokens,
                describe_target(&llm.config)
            );
            state.ask_send_confirmation(text, plan);
        }
        Ok(plan) => start_recommendation(state, services, plan, clock, sender),
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
    state.begin_widget_refresh(&WidgetId::new(IA_WIDGET_ID));
    let runner = Arc::clone(&llm.runner);
    let store = Arc::clone(&llm.store);
    let model = llm.config.model.clone();
    let now = clock.now();
    let sender = sender.clone();
    thread::spawn(move || {
        let result = execute_recommendation(
            runner.as_ref(),
            store.as_ref(),
            &plan,
            model.as_deref(),
            now,
        );
        let _ = sender.send(AppEvent::RecommendationFinished(result));
    });
}

fn describe_target(config: &LlmConfig) -> String {
    let harness = match config.harness {
        Harness::Claude => "claude",
        Harness::OpenCode => "opencode",
    };
    match &config.model {
        Some(model) => format!("{harness}/{model}"),
        None => harness.to_string(),
    }
}
