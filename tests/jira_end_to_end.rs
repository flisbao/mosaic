#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use panel_tui::application::ports::{ActionExecutor, SourceError, WidgetSource};
use panel_tui::domain::action::ProposedAction;
use panel_tui::domain::widget::{ActionScope, ActionSpec, WidgetId, WidgetSnapshot};
use panel_tui::infrastructure::config::WidgetSettings;
use panel_tui::infrastructure::plugin_runner::PluginRunner;

const TOKEN: &str = "TOKEN-E2E-XYZ";
const EMAIL: &str = "eu@empresa.com";

struct FakeJira {
    child: Child,
    port: u16,
}

impl FakeJira {
    fn start() -> FakeJira {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/tests/fake_jira_server.py");
        let mut child = Command::new("python3")
            .arg(script)
            .env("FAKE_JIRA_EMAIL", EMAIL)
            .env("FAKE_JIRA_TOKEN", TOKEN)
            .stdout(Stdio::piped())
            .spawn()
            .expect("python3 precisa estar instalado");
        let stdout = child.stdout.take().unwrap();
        let mut first_line = String::new();
        BufReader::new(stdout).read_line(&mut first_line).unwrap();
        let port = first_line
            .trim()
            .parse()
            .expect("a primeira linha deve ser a porta");
        FakeJira { child, port }
    }
}

impl Drop for FakeJira {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn jira_runner(server: &FakeJira, state_dir: &Path, mode: &str, token: &str) -> PluginRunner {
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/jira.py");
    let environment = [
        ("JIRA_SITE", "teste.atlassian.net".to_string()),
        ("JIRA_EMAIL", EMAIL.to_string()),
        ("JIRA_TOKEN_ENV", "E2E_TOKEN".to_string()),
        ("E2E_TOKEN", token.to_string()),
        ("JIRA_MODE", mode.to_string()),
        ("JIRA_PROJECT", "VHPJ".to_string()),
        ("JIRA_BASE_URL", format!("http://127.0.0.1:{}", server.port)),
        ("JIRA_STATE_DIR", state_dir.to_string_lossy().to_string()),
    ];
    let settings = WidgetSettings {
        id: WidgetId::new("jira"),
        command: "python3".to_string(),
        args: vec![plugin.to_string_lossy().to_string()],
        env: environment
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect(),
        interval_seconds: 120,
        interval_adjusted: false,
        timeout_seconds: 30,
        span: 1,
        tab: true,
        llm_sections: vec!["Sem dono".to_string()],
        llm_max_items: 20,
        llm_max_characters_per_item: 120,
    };
    PluginRunner::new(settings, state_dir.to_path_buf())
}

fn summary_texts(snapshot: &WidgetSnapshot) -> Vec<String> {
    snapshot
        .summary
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

fn all_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(all_files(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn manager_snapshot_through_the_plugin_runner() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), "manager", TOKEN);

    let snapshot = runner.snapshot().unwrap();

    let texts = summary_texts(&snapshot);
    assert!(texts.contains(&"sem dono: 1".to_string()), "{texts:?}");
    assert!(snapshot.summary.title.contains("Jira"));
    let sections = snapshot.detail.unwrap();
    assert_eq!(sections[0].heading, "Sem dono");
    assert_eq!(sections[0].items[0].id.as_deref(), Some("VHPJ-1"));
    assert!(sections[0].items[0]
        .meta
        .contains(&("responsavel".to_string(), "ninguem".to_string())));
    assert_eq!(snapshot.actions.len(), 2);
}

#[test]
fn personal_snapshot_through_the_plugin_runner() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), "personal", TOKEN);

    let snapshot = runner.snapshot().unwrap();

    let texts = summary_texts(&snapshot);
    assert!(texts.contains(&"em andamento: 1".to_string()), "{texts:?}");
    assert!(
        texts.contains(&"concluídos em 7 dias: 1".to_string()),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"tempo mediano: 24h".to_string()),
        "{texts:?}"
    );
}

#[test]
fn wrong_token_shows_the_auth_message_in_the_error() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), "manager", "token-errado");

    let error = runner.snapshot().unwrap_err();

    let SourceError::Failed(message) = error;
    assert!(message.contains("credenciais inválidas"), "{message}");
    assert!(!message.contains("token-errado"));
}

#[test]
fn mark_seen_action_through_the_plugin_runner() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), "manager", TOKEN);
    let before = summary_texts(&runner.snapshot().unwrap());
    assert!(before.contains(&"atualizados: 4".to_string()), "{before:?}");

    let spec = ActionSpec {
        id: "mark_seen".to_string(),
        label: "Marcar como visto".to_string(),
        description: None,
        scope: ActionScope::Widget,
        params_json: "{}".to_string(),
    };
    let action = ProposedAction::new(WidgetId::new("jira"), spec, None)
        .unwrap()
        .confirm();
    let outcome = runner.execute(&action).unwrap();

    assert_eq!(outcome.message, "Marcado como visto");
    let after = summary_texts(&runner.snapshot().unwrap());
    assert!(after.contains(&"atualizados: 0".to_string()), "{after:?}");
}

#[test]
fn token_never_appears_in_runner_output_or_state_files() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), "manager", TOKEN);
    let snapshot = runner.snapshot().unwrap();
    assert!(!format!("{snapshot:?}").contains(TOKEN));
    let files = all_files(state.path());
    assert!(!files.is_empty());
    for file in files {
        let content = fs::read_to_string(&file).unwrap();
        assert!(!content.contains(TOKEN), "{}", file.display());
    }
}
