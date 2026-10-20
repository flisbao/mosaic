use std::fs;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};

use mosaic::application::execute_action::{execute_confirmed, ActionNotice};
use mosaic::application::refresh::refresh_widget;
use mosaic::domain::widget::{WidgetId, WidgetRecord};
use mosaic::infrastructure::config::{default_llm_selection, WidgetSettings};
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::presentation::app_state::{AppState, Command, Mode, TabView, WidgetEntry};
use mosaic::presentation::key_input::KeyInput;

fn kanban_plugin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/kanban.py")
}

fn kanban_runner(root: &Path) -> PluginRunner {
    let settings = WidgetSettings {
        id: WidgetId::new("kanban"),
        command: "python3".to_string(),
        args: vec![kanban_plugin_path().to_string_lossy().to_string()],
        env: vec![("KANBAN_DIR".to_string(), root.to_string_lossy().to_string())],
        interval_seconds: 30,
        interval_adjusted: false,
        timeout_seconds: 10,
        span: 1,
        max_items: None,
        llm_selection: default_llm_selection(vec![]),
    };
    PluginRunner::new(settings, root.to_path_buf())
}

fn kanban_with_one_item() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("kanban");
    fs::create_dir_all(root.join("inbox")).unwrap();
    fs::write(root.join("inbox/a.md"), "# Tarefa A\n").unwrap();
    (directory, root)
}

fn app_for(runner: &PluginRunner) -> AppState {
    let mut record = WidgetRecord::loading();
    record.apply(refresh_widget(runner), now());
    AppState::new(
        vec![WidgetEntry {
            id: WidgetId::new("kanban"),
            record,
            interval: std::time::Duration::from_secs(30),
            max_items: None,
        }],
        vec![TabView {
            name: "Principal".to_string(),
            entry_indexes: vec![0],
        }],
    )
}

fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 18)
        .and_then(|date| date.and_hms_opt(12, 0, 0))
        .expect("data válida")
}

fn press(app: &mut AppState, keys: &[KeyInput]) -> Option<Command> {
    let mut last = None;
    for key in keys {
        last = app.handle_key(*key);
    }
    last
}

fn choose_move_to_done(app: &mut AppState) {
    let mut keys = vec![KeyInput::Enter, KeyInput::Character('a')];
    keys.extend([KeyInput::Down; 4]);
    keys.push(KeyInput::Enter);
    press(app, &keys);
    assert!(matches!(app.mode(), Mode::Confirming(_)));
}

#[test]
fn confirmed_move_renames_the_item_and_refreshes_the_widget() {
    let (_directory, root) = kanban_with_one_item();
    let runner = kanban_runner(&root);
    let mut app = app_for(&runner);
    choose_move_to_done(&mut app);

    let Some(Command::Execute(confirmed)) = app.handle_key(KeyInput::Character('s')) else {
        panic!("esperava Command::Execute");
    };
    let (notice, state) = execute_confirmed(&runner, &runner, confirmed);

    assert!(matches!(notice, ActionNotice::Succeeded(_)), "{notice:?}");
    assert!(root.join("done/a.md").is_file());
    assert!(!root.join("inbox/a.md").exists());
    let Ok(snapshot) = state else {
        panic!("esperava snapshot");
    };
    let lines: Vec<String> = snapshot
        .summary
        .lines
        .iter()
        .map(|l| l.text.clone())
        .collect();
    assert!(lines.contains(&"done 1".to_string()), "{lines:?}");
    assert!(lines.contains(&"inbox 0".to_string()), "{lines:?}");
}

#[test]
fn cancel_leaves_files_untouched() {
    let (_directory, root) = kanban_with_one_item();
    let runner = kanban_runner(&root);
    let mut app = app_for(&runner);
    choose_move_to_done(&mut app);

    assert_eq!(app.handle_key(KeyInput::Character('n')), None);

    assert!(root.join("inbox/a.md").is_file());
    assert!(!root.join("done").exists());
}
