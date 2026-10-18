#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::NaiveDate;
use mosaic::application::llm::{
    execute_recommendation, LlmError, RecommendationStore, StoredRecommendation,
};
use mosaic::application::recommendation::{build_plan, LlmSelection, WidgetContext};
use mosaic::application::recommendation_view::recommendation_snapshot;
use mosaic::application::refresh::refresh_widget;
use mosaic::domain::widget::WidgetId;
use mosaic::infrastructure::config::WidgetSettings;
use mosaic::infrastructure::llm_cli::ClaudeCli;
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::recommendation_file::FileRecommendationStore;

const FORK_RACE_RETRIES: usize = 20;

fn now() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 17)
        .unwrap()
        .and_hms_opt(8, 30, 0)
        .unwrap()
}

fn kanban_runner(root: &Path) -> PluginRunner {
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/kanban.py");
    let settings = WidgetSettings {
        id: WidgetId::new("kanban"),
        command: "python3".to_string(),
        args: vec![plugin.to_string_lossy().to_string()],
        env: vec![("KANBAN_DIR".to_string(), root.to_string_lossy().to_string())],
        interval_seconds: 30,
        interval_adjusted: false,
        timeout_seconds: 10,
        span: 1,
        max_items: None,
        llm_sections: vec!["inbox".to_string()],
        llm_max_items: 20,
        llm_max_characters_per_item: 120,
    };
    PluginRunner::new(settings, root.to_path_buf())
}

fn kanban_root(directory: &Path) -> PathBuf {
    let root = directory.join("kanban");
    fs::create_dir_all(root.join("inbox")).unwrap();
    fs::create_dir_all(root.join("done")).unwrap();
    fs::write(
        root.join("inbox/a.md"),
        "---\nprioridade: alta\n---\n# Corrigir login\n",
    )
    .unwrap();
    fs::write(root.join("done/b.md"), "# Arquivar relatório\n").unwrap();
    root
}

fn fake_claude(directory: &Path, body: &str) -> String {
    let path = directory.join("fake-claude");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().to_string()
}

fn run_plan(
    directory: &Path,
    script_body: &str,
) -> (
    Result<StoredRecommendation, LlmError>,
    FileRecommendationStore,
) {
    let root = kanban_root(directory);
    let runner = kanban_runner(&root);
    let state = refresh_widget(&runner);
    let selection = LlmSelection {
        sections: vec!["inbox".to_string()],
        max_items: 20,
        max_characters_per_item: 120,
    };
    let widget_id = WidgetId::new("kanban");
    let plan = build_plan(
        "Critérios.",
        now(),
        &[WidgetContext {
            widget_id: &widget_id,
            selection: &selection,
            state: &state,
        }],
    )
    .unwrap();
    let program = fake_claude(directory, script_body);
    let cli = ClaudeCli::new(
        program,
        directory.join("neutro"),
        None,
        Duration::from_secs(10),
    );
    let store = FileRecommendationStore::new(directory.join("rec.json"));
    let mut result = execute_recommendation(&cli, &store, &plan, None, now());
    for _ in 0..FORK_RACE_RETRIES {
        match &result {
            Err(error) if error.to_string().contains("Text file busy") => {
                std::thread::sleep(Duration::from_millis(20));
                result = execute_recommendation(&cli, &store, &plan, None, now());
            }
            _ => break,
        }
    }
    (result, store)
}

#[test]
fn recommendation_is_generated_saved_and_shown_in_the_ia_box() {
    let directory = tempfile::tempdir().unwrap();
    let (result, store) = run_plan(
        directory.path(),
        "cat > \"$(dirname \"$0\")/stdin.txt\"; printf '1. Corrigir login\\nComece por: Corrigir login'",
    );

    let stored = result.unwrap();
    assert_eq!(stored.text, "1. Corrigir login\nComece por: Corrigir login");
    assert_eq!(store.load(), Some(stored.clone()));
    let snapshot = recommendation_snapshot(&stored);
    assert_eq!(snapshot.summary.title, "IA");
    assert_eq!(snapshot.summary.lines.len(), 2);
}

#[test]
fn prompt_reaches_the_cli_with_meta_and_only_selected_sections() {
    let directory = tempfile::tempdir().unwrap();
    let (result, _store) = run_plan(
        directory.path(),
        "cat > \"$(dirname \"$0\")/stdin.txt\"; printf 'ok'",
    );
    result.unwrap();

    let sent = fs::read_to_string(directory.path().join("stdin.txt")).unwrap();
    assert!(
        sent.contains("inbox/a.md | Corrigir login | prioridade=alta"),
        "{sent}"
    );
    assert!(!sent.contains("Arquivar"), "{sent}");
    assert!(sent.starts_with("Critérios."));
}

#[test]
fn cli_failure_leaves_nothing_stored() {
    let directory = tempfile::tempdir().unwrap();
    let (result, store) = run_plan(directory.path(), "echo sem credito >&2; exit 2");

    assert_eq!(
        result.unwrap_err(),
        LlmError::Failed("sem credito".to_string())
    );
    assert_eq!(store.load(), None);
}
