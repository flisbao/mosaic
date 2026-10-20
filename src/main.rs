mod check_command;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use crossterm::event::{self, Event};
use mosaic::application::alert_tracker::AlertTracker;
use mosaic::application::execute_action::execute_confirmed;
use mosaic::application::llm::{execute_recommendation, LlmRunner, RecommendationStore};
use mosaic::application::pomodoro_notifications::Notifier;
use mosaic::application::ports::{Cancellable, Clock, WidgetRunner};
use mosaic::application::recommendation::{
    build_plan, LlmSelection, RecommendationPlan, WidgetContext,
};
use mosaic::application::recommendation_view::IA_WIDGET_ID;
use mosaic::application::reload::{apply_plan, reload_widgets, WidgetGenerations};
use mosaic::application::reload_plan::{ReloadPlan, WidgetChange};
use mosaic::application::shutdown::{should_stop, ShutdownCoordinator};
use mosaic::domain::action::ConfirmedAction;
use mosaic::domain::pomodoro::PomodoroTimer;
use mosaic::domain::widget::{WidgetId, WidgetRecord};
use mosaic::infrastructure::config::{
    config_directory, config_file_path, load_config, prepare_reload, read_config_file,
    reload_notice, retain_restart_sections, AppConfig, ConfigError, Harness, LlmConfig,
    WidgetSettings,
};
use mosaic::infrastructure::default_files::install_defaults_if_missing;
use mosaic::infrastructure::llm_cli::{ClaudeCli, OpenCodeCli};
use mosaic::infrastructure::notifiers::{BellNotifier, CommandNotifier, CompositeNotifier};
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::recommendation_file::{
    default_recommendation_path, FileRecommendationStore, NullRecommendationStore,
};
use mosaic::infrastructure::shutdown::termination_requested;
use mosaic::infrastructure::system_clock::SystemClock;
use mosaic::infrastructure::widget_runtime::{RunnerFactory, WidgetRuntime, WidgetUpdate};
use mosaic::presentation::app_events::{apply_event, stored_ia_result, AppEvent, EventContext};
use mosaic::presentation::app_state::{AppState, Command};
use mosaic::presentation::key_input::from_crossterm;
use mosaic::presentation::render::render;
use mosaic::presentation::tab_layout::{
    build_tab_views, carry_over_records, startup_notice, TabLayout, TabPlacement, WidgetPlacement,
};
use thiserror::Error;

const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const LLM_DISABLED_NOTICE: &str = "IA desligada na configuração";
const APP_DIRECTORY_NAME: &str = "mosaic";
const LLM_WORKING_DIRECTORY_NAME: &str = "llm";
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
    let warnings: Vec<String> = config
        .warnings
        .iter()
        .chain(&services.startup_warnings)
        .cloned()
        .collect();
    if let Some(notice) = startup_notice(&warnings) {
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
    let working_directory = directory.to_path_buf();
    let factory: RunnerFactory = Box::new(move |settings| {
        Arc::new(PluginRunner::new(
            settings.clone(),
            working_directory.clone(),
        )) as Arc<dyn WidgetRunner>
    });
    let mut runtime = WidgetRuntime::new(
        factory,
        sender.clone(),
        widget_updated,
        Arc::clone(&shutdown),
    );
    let mut generations = WidgetGenerations::default();
    apply_plan(
        &startup_plan(&config.widgets),
        &config.widgets,
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

fn widget_updated(update: WidgetUpdate) -> AppEvent {
    AppEvent::WidgetUpdated(update.id, update.generation, update.result)
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

fn build_llm_services(config: &AppConfig, warnings: &mut Vec<String>) -> Option<LlmServices> {
    let Some(state_directory) = dirs::state_dir().or_else(dirs::data_local_dir) else {
        warnings.push(NO_STATE_DIRECTORY_LLM_WARNING.to_string());
        return None;
    };
    let working_directory = state_directory
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
    let store: Arc<dyn RecommendationStore> = match default_recommendation_path() {
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
            let context = EventContext {
                generations: &services.generations,
                notifier: &services.notifier,
                recommendation_store: services.llm.as_ref().map(|llm| llm.store.as_ref()),
            };
            apply_event(state, app_event, &context, &mut alert_tracker, clock.now());
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
            Some(Command::Execute(action)) => spawn_action(action, state, services, sender),
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

fn stored_ia_record(store: &dyn RecommendationStore, now: NaiveDateTime) -> WidgetRecord {
    let mut record = WidgetRecord::loading();
    record.apply(stored_ia_result(store), now);
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

fn spawn_action(
    action: ConfirmedAction,
    state: &mut AppState,
    services: &Services,
    sender: &Sender<AppEvent>,
) {
    let widget = action.widget().clone();
    let (Some(runner), Some(generation)) = (
        services.runtime.runner(&widget),
        services.generations.current(&widget),
    ) else {
        state.finish_action(&widget);
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
