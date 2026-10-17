use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use panel_tui::application::execute_action::{execute_confirmed, ActionNotice};
use panel_tui::application::llm::{
    execute_recommendation, LlmError, LlmRunner, RecommendationStore, StoredRecommendation,
};
use panel_tui::application::pomodoro_notifications::Notifier;
use panel_tui::application::ports::Clock;
use panel_tui::application::recommendation::{
    build_plan, LlmSelection, RecommendationPlan, WidgetContext,
};
use panel_tui::application::recommendation_view::{
    idle_snapshot, recommendation_snapshot, IA_WIDGET_ID,
};
use panel_tui::application::refresh::refresh_widget;
use panel_tui::domain::action::ConfirmedAction;
use panel_tui::domain::pomodoro::PomodoroTimer;
use panel_tui::domain::widget::{WidgetId, WidgetState};
use panel_tui::infrastructure::config::{
    config_directory, config_file_path, parse_config, AppConfig, ConfigError, Harness, LlmConfig,
    WidgetSettings,
};
use panel_tui::infrastructure::default_files::install_defaults_if_missing;
use panel_tui::infrastructure::llm_cli::{ClaudeCli, OpenCodeCli};
use panel_tui::infrastructure::notifiers::{BellNotifier, CommandNotifier, CompositeNotifier};
use panel_tui::infrastructure::plugin_runner::PluginRunner;
use panel_tui::infrastructure::recommendation_file::{
    default_recommendation_path, FileRecommendationStore,
};
use panel_tui::infrastructure::system_clock::SystemClock;
use panel_tui::infrastructure::terminal_capabilities::detect_graphics_support;
use panel_tui::presentation::app_state::{AppState, Command, WidgetEntry};
use panel_tui::presentation::key_input::from_crossterm;
use panel_tui::presentation::render::render;
use thiserror::Error;

const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const ACTION_DONE_FALLBACK_MESSAGE: &str = "Ação concluída";
const RECOMMENDATION_READY_MESSAGE: &str = "Recomendação atualizada";
const LLM_DISABLED_NOTICE: &str = "IA desligada na configuração";
const APP_DIRECTORY_NAME: &str = "panel-tui";
const LLM_WORKING_DIRECTORY_NAME: &str = "llm";
const FALLBACK_STORE_FILE_NAME: &str = "panel-tui-recommendation.json";
const IA_WIDGET_SPAN: u16 = 1;

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("falha de terminal: {0}")]
    Terminal(#[from] std::io::Error),
}

enum AppEvent {
    WidgetUpdated(WidgetId, WidgetState),
    ActionFinished(WidgetId, ActionNotice, WidgetState),
    RecommendationFinished(Result<StoredRecommendation, LlmError>),
}

struct WidgetRuntime {
    runners: HashMap<String, Arc<PluginRunner>>,
    refresh_triggers: Vec<Sender<()>>,
}

struct LlmServices {
    runner: Arc<dyn LlmRunner>,
    store: Arc<dyn RecommendationStore>,
    config: LlmConfig,
    selections: HashMap<String, LlmSelection>,
}

struct Services {
    runtime: WidgetRuntime,
    llm: Option<LlmServices>,
    notifier: CompositeNotifier,
    config_directory: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("panel: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let directory = config_directory()?;
    install_defaults_if_missing(&directory)?;
    let config_text = fs::read_to_string(config_file_path(&directory))
        .map_err(|error| ConfigError::Io(error.to_string()))?;
    let config = parse_config(&config_text)?;
    let _graphics_support = detect_graphics_support(&|name| std::env::var(name).ok());

    let (sender, receiver) = mpsc::channel();
    let services = build_services(&config, &directory, &sender);
    let mut spans: Vec<u16> = config.widgets.iter().map(|widget| widget.span).collect();
    let mut entries: Vec<WidgetEntry> = config
        .widgets
        .iter()
        .map(|widget| WidgetEntry {
            id: widget.id.clone(),
            state: WidgetState::Loading,
            has_tab: widget.tab,
        })
        .collect();
    if let Some(llm) = &services.llm {
        spans.push(IA_WIDGET_SPAN);
        entries.push(WidgetEntry {
            id: WidgetId::new(IA_WIDGET_ID),
            state: stored_ia_state(llm.store.as_ref()),
            has_tab: true,
        });
    }
    let mut state = AppState::new(entries);
    if config.pomodoro.enabled {
        state = state.with_pomodoro(PomodoroTimer::new(config.pomodoro.settings()));
    }
    let clock = SystemClock;

    let mut terminal = ratatui::init();
    let result = event_loop(
        &mut terminal,
        &mut state,
        &spans,
        &clock,
        &services,
        &sender,
        &receiver,
    );
    ratatui::restore();
    result
}

fn build_services(config: &AppConfig, directory: &Path, sender: &Sender<AppEvent>) -> Services {
    Services {
        runtime: start_widgets(&config.widgets, directory, sender),
        llm: config.llm.enabled.then(|| build_llm_services(config)),
        notifier: build_notifier(config),
        config_directory: directory.to_path_buf(),
    }
}

fn build_llm_services(config: &AppConfig) -> LlmServices {
    let working_directory = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIRECTORY_NAME)
        .join(LLM_WORKING_DIRECTORY_NAME);
    let timeout = Duration::from_secs(config.llm.timeout_seconds);
    let runner: Arc<dyn LlmRunner> = match config.llm.harness {
        Harness::Claude => Arc::new(ClaudeCli::new(
            "claude".to_string(),
            working_directory,
            config.llm.max_budget_usd,
            timeout,
        )),
        Harness::OpenCode => Arc::new(OpenCodeCli::new(
            "opencode".to_string(),
            working_directory,
            timeout,
        )),
    };
    let store_path = default_recommendation_path()
        .unwrap_or_else(|| std::env::temp_dir().join(FALLBACK_STORE_FILE_NAME));
    let selections = config
        .widgets
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
        .collect();
    LlmServices {
        runner,
        store: Arc::new(FileRecommendationStore::new(store_path)),
        config: config.llm.clone(),
        selections,
    }
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

fn start_widgets(
    widgets: &[WidgetSettings],
    working_directory: &Path,
    sender: &Sender<AppEvent>,
) -> WidgetRuntime {
    let mut runners = HashMap::new();
    let mut refresh_triggers = Vec::new();
    for settings in widgets {
        let runner = Arc::new(PluginRunner::new(
            settings.clone(),
            working_directory.to_path_buf(),
        ));
        runners.insert(settings.id.as_str().to_string(), Arc::clone(&runner));
        refresh_triggers.push(spawn_refresher(
            settings.id.clone(),
            runner,
            Duration::from_secs(settings.interval_seconds),
            sender.clone(),
        ));
    }
    WidgetRuntime {
        runners,
        refresh_triggers,
    }
}

fn spawn_refresher(
    id: WidgetId,
    runner: Arc<PluginRunner>,
    interval: Duration,
    sender: Sender<AppEvent>,
) -> Sender<()> {
    let (trigger, triggered) = mpsc::channel();
    thread::spawn(move || loop {
        let state = refresh_widget(runner.as_ref());
        if sender
            .send(AppEvent::WidgetUpdated(id.clone(), state))
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
    spans: &[u16],
    clock: &dyn Clock,
    services: &Services,
    sender: &Sender<AppEvent>,
    receiver: &mpsc::Receiver<AppEvent>,
) -> Result<(), AppError> {
    let mut last_tick = Instant::now();
    loop {
        while let Ok(app_event) = receiver.try_recv() {
            apply_event(state, app_event, services);
        }
        let elapsed = last_tick.elapsed();
        last_tick = Instant::now();
        if let Some(alert) = state.advance_pomodoro(elapsed) {
            services.notifier.notify(&alert);
        }
        terminal.draw(|frame| render(frame, state, spans, clock.now()))?;
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
            Some(Command::RefreshAll) => trigger_refresh(&services.runtime),
            Some(Command::Execute(action)) => spawn_action(action, &services.runtime, sender),
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

fn apply_event(state: &mut AppState, app_event: AppEvent, services: &Services) {
    match app_event {
        AppEvent::WidgetUpdated(id, widget_state) => state.update_widget(&id, widget_state),
        AppEvent::ActionFinished(id, notice, widget_state) => {
            state.update_widget(&id, widget_state);
            state.show_notice(describe_notice(notice));
        }
        AppEvent::RecommendationFinished(result) => {
            apply_recommendation_result(state, services, result)
        }
    }
}

fn apply_recommendation_result(
    state: &mut AppState,
    services: &Services,
    result: Result<StoredRecommendation, LlmError>,
) {
    let ia = WidgetId::new(IA_WIDGET_ID);
    match result {
        Ok(stored) => {
            state.update_widget(&ia, WidgetState::Ready(recommendation_snapshot(&stored)));
            state.show_notice(RECOMMENDATION_READY_MESSAGE.to_string());
        }
        Err(LlmError::Unavailable(message)) => {
            if let Some(llm) = &services.llm {
                state.update_widget(&ia, stored_ia_state(llm.store.as_ref()));
            }
            state.show_notice(message);
        }
        Err(LlmError::Failed(message)) => state.update_widget(&ia, WidgetState::Failed(message)),
    }
}

fn stored_ia_state(store: &dyn RecommendationStore) -> WidgetState {
    WidgetState::Ready(match store.load() {
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

fn trigger_refresh(runtime: &WidgetRuntime) {
    for trigger in &runtime.refresh_triggers {
        let _ = trigger.send(());
    }
}

fn spawn_action(action: ConfirmedAction, runtime: &WidgetRuntime, sender: &Sender<AppEvent>) {
    let Some(runner) = runtime.runners.get(action.widget().as_str()) else {
        return;
    };
    let runner = Arc::clone(runner);
    let sender = sender.clone();
    thread::spawn(move || {
        let widget = action.widget().clone();
        let (notice, widget_state) = execute_confirmed(runner.as_ref(), runner.as_ref(), action);
        let _ = sender.send(AppEvent::ActionFinished(widget, notice, widget_state));
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
                    state: &entry.state,
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
    state.update_widget(&WidgetId::new(IA_WIDGET_ID), WidgetState::Loading);
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
