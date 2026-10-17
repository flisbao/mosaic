use std::fs;
use std::path::{Path, PathBuf};

use panel_tui::application::execute_action::{execute_confirmed, ActionNotice};
use panel_tui::application::refresh::refresh_widget;
use panel_tui::domain::widget::{WidgetId, WidgetState};
use panel_tui::infrastructure::config::WidgetSettings;
use panel_tui::infrastructure::plugin_runner::PluginRunner;
use panel_tui::presentation::app_state::{AppState, Command, Mode, WidgetEntry};
use panel_tui::presentation::key_input::KeyInput;

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
        timeout_seconds: 10,
        span: 1,
        tab: true,
    };
    PluginRunner::new(settings, root.to_path_buf())
}

fn kanban_with_one_card() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("kanban");
    fs::create_dir_all(root.join("inbox")).unwrap();
    fs::write(root.join("inbox/a.md"), "# Tarefa A\n").unwrap();
    (directory, root)
}

fn app_for(runner: &PluginRunner) -> AppState {
    AppState::new(vec![WidgetEntry {
        id: WidgetId::new("kanban"),
        state: refresh_widget(runner),
        has_tab: true,
    }])
}

fn press(app: &mut AppState, keys: &[KeyInput]) -> Option<Command> {
    let mut last = None;
    for key in keys {
        last = app.handle_key(*key);
    }
    last
}

fn choose_move_to_done(app: &mut AppState) {
    let mut keys = vec![KeyInput::Tab, KeyInput::Character('a')];
    keys.extend([KeyInput::Down; 4]);
    keys.push(KeyInput::Enter);
    press(app, &keys);
    assert!(matches!(app.mode(), Mode::Confirming(_)));
}

#[test]
fn confirmed_move_renames_the_card_and_refreshes_the_widget() {
    let (_directory, root) = kanban_with_one_card();
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
    let WidgetState::Ready(snapshot) = state else {
        panic!("esperava Ready");
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
    let (_directory, root) = kanban_with_one_card();
    let runner = kanban_runner(&root);
    let mut app = app_for(&runner);
    choose_move_to_done(&mut app);

    assert_eq!(app.handle_key(KeyInput::Character('n')), None);

    assert!(root.join("inbox/a.md").is_file());
    assert!(!root.join("done").exists());
}
