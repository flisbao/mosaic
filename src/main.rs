use std::collections::HashMap;
use std::fs;
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event};
use panel_tui::application::execute_action::{execute_confirmed, ActionNotice};
use panel_tui::application::ports::Clock;
use panel_tui::application::refresh::refresh_widget;
use panel_tui::domain::action::ConfirmedAction;
use panel_tui::domain::widget::{WidgetId, WidgetState};
use panel_tui::infrastructure::config::{
    config_directory, config_file_path, parse_config, ConfigError, WidgetSettings,
};
use panel_tui::infrastructure::default_files::install_defaults_if_missing;
use panel_tui::infrastructure::plugin_runner::PluginRunner;
use panel_tui::infrastructure::system_clock::SystemClock;
use panel_tui::infrastructure::terminal_capabilities::detect_graphics_support;
use panel_tui::presentation::app_state::{AppState, Command, WidgetEntry};
use panel_tui::presentation::key_input::from_crossterm;
use panel_tui::presentation::render::render;
use thiserror::Error;

const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const ACTION_DONE_FALLBACK_MESSAGE: &str = "Ação concluída";

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
}

struct WidgetRuntime {
    runners: HashMap<String, Arc<PluginRunner>>,
    refresh_triggers: Vec<Sender<()>>,
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
    let runtime = start_widgets(&config.widgets, &directory, &sender);
    let spans: Vec<u16> = config.widgets.iter().map(|widget| widget.span).collect();
    let mut state = AppState::new(
        config
            .widgets
            .iter()
            .map(|widget| WidgetEntry {
                id: widget.id.clone(),
                state: WidgetState::Loading,
                has_tab: widget.tab,
            })
            .collect(),
    );
    let clock = SystemClock;

    let mut terminal = ratatui::init();
    let result = event_loop(
        &mut terminal,
        &mut state,
        &spans,
        &clock,
        &runtime,
        &sender,
        &receiver,
    );
    ratatui::restore();
    result
}

fn start_widgets(
    widgets: &[WidgetSettings],
    working_directory: &std::path::Path,
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
    runtime: &WidgetRuntime,
    sender: &Sender<AppEvent>,
    receiver: &mpsc::Receiver<AppEvent>,
) -> Result<(), AppError> {
    loop {
        while let Ok(app_event) = receiver.try_recv() {
            apply_event(state, app_event);
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
            Some(Command::RefreshAll) => trigger_refresh(runtime),
            Some(Command::Execute(action)) => spawn_action(action, runtime, sender),
            None => {}
        }
    }
}

fn apply_event(state: &mut AppState, app_event: AppEvent) {
    match app_event {
        AppEvent::WidgetUpdated(id, widget_state) => state.update_widget(&id, widget_state),
        AppEvent::ActionFinished(id, notice, widget_state) => {
            state.update_widget(&id, widget_state);
            state.show_notice(describe_notice(notice));
        }
    }
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
