#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use mosaic::application::launch_spec::ProviderCommand;
use mosaic::application::ports::WidgetSource;
use mosaic::domain::blocks::Block;
use mosaic::infrastructure::plugin_runner::PluginRunner;

mod common;

const TOKEN: &str = "TOKEN-E2E-XYZ";
const EMAIL: &str = "eu@empresa.com";
const SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const MY_TASKS_JQL: &str = "assignee = currentUser() AND statusCategory != Done";
const ANY_ISSUE_JQL: &str = "project = VHPJ";
const FAKE_ISSUE_COUNT: usize = 6;
const OFFLINE_PREFIX: &str = "dados de";
const OFFLINE_SUFFIX: &str = "(offline)";

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

fn jira_runner(
    server: &FakeJira,
    state_dir: &Path,
    token: &str,
    jql_and_options: &[&str],
) -> PluginRunner {
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/jira.py");
    let environment = [
        ("JIRA_SITE", "teste.atlassian.net".to_string()),
        ("JIRA_EMAIL", EMAIL.to_string()),
        ("JIRA_TOKEN_ENV", "E2E_TOKEN".to_string()),
        ("E2E_TOKEN", token.to_string()),
        ("JIRA_BASE_URL", format!("http://127.0.0.1:{}", server.port)),
        ("JIRA_STATE_DIR", state_dir.to_string_lossy().to_string()),
    ];
    let mut args = vec![plugin.to_string_lossy().to_string()];
    args.extend(jql_and_options.iter().map(|argument| argument.to_string()));
    let command = ProviderCommand {
        program: "python3".to_string(),
        args,
        env: environment
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect(),
        inherit_env: None,
        timeout: SNAPSHOT_TIMEOUT,
    };
    PluginRunner::new(command, state_dir.to_path_buf())
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
fn my_tasks_snapshot_through_the_plugin_runner() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(
        &server,
        state.path(),
        TOKEN,
        &["--jql", MY_TASKS_JQL, "--title", "Minhas tarefas"],
    );

    let snapshot = runner.snapshot().unwrap();

    assert_eq!(snapshot.summary.title, "Minhas tarefas");
    let texts = common::summary_line_texts(&snapshot);
    assert!(
        texts.iter().any(|text| text.contains("VHPJ-5")),
        "{texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("VHPJ-2")),
        "{texts:?}"
    );
}

#[test]
fn group_by_assignee_builds_one_detail_section_per_person() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(
        &server,
        state.path(),
        TOKEN,
        &["--jql", ANY_ISSUE_JQL, "--group-by", "assignee"],
    );

    let snapshot = runner.snapshot().unwrap();

    let sections = snapshot.detail.unwrap();
    let headings: Vec<&str> = sections
        .iter()
        .map(|section| section.heading.as_str())
        .collect();
    for expected in ["Ana Souza", "Bruno Lima", "Eu", "ninguem"] {
        assert!(headings.contains(&expected), "{headings:?}");
    }
    let unassigned = sections
        .iter()
        .find(|section| section.heading == "ninguem")
        .unwrap();
    let ticket = unassigned
        .items
        .iter()
        .find(|item| item.id.as_deref() == Some("VHPJ-1"))
        .unwrap();
    assert!(ticket
        .meta
        .contains(&("responsavel".to_string(), "ninguem".to_string())));
}

#[test]
fn stat_view_shows_the_count() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(
        &server,
        state.path(),
        TOKEN,
        &["--jql", ANY_ISSUE_JQL, "--view", "stat"],
    );

    let snapshot = runner.snapshot().unwrap();

    let values: Vec<&str> = snapshot
        .summary
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Stat { items, .. } => Some(items),
            _ => None,
        })
        .flatten()
        .map(|item| item.value.as_str())
        .collect();
    assert_eq!(values, [FAKE_ISSUE_COUNT.to_string()]);
}

#[test]
fn wrong_token_shows_the_auth_message_without_leaking_it() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(
        &server,
        state.path(),
        "token-errado",
        &["--jql", ANY_ISSUE_JQL],
    );

    let error = runner.snapshot().unwrap_err();

    let message = error.to_string();
    assert!(message.contains("credenciais inválidas"), "{message}");
    assert!(!message.contains("token-errado"));
}

#[test]
fn invalid_jql_shows_the_jira_message() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), TOKEN, &["--jql", "FAIL400"]);

    let error = runner.snapshot().unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("JQL inválido: Erro de sintaxe no JQL"),
        "{message}"
    );
}

#[test]
fn network_failure_serves_the_offline_cache() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), TOKEN, &["--jql", ANY_ISSUE_JQL]);
    runner.snapshot().unwrap();
    drop(server);

    let snapshot = runner.snapshot().unwrap();

    let texts = common::summary_line_texts(&snapshot);
    let notice = texts.first().unwrap();
    assert!(notice.starts_with(OFFLINE_PREFIX), "{texts:?}");
    assert!(notice.ends_with(OFFLINE_SUFFIX), "{texts:?}");
}

#[test]
fn token_never_appears_in_runner_output_or_state_files() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), TOKEN, &["--jql", ANY_ISSUE_JQL]);
    let snapshot = runner.snapshot().unwrap();
    assert!(!format!("{snapshot:?}").contains(TOKEN));
    let files = all_files(state.path());
    assert!(!files.is_empty());
    for file in files {
        let content = fs::read_to_string(&file).unwrap();
        assert!(!content.contains(TOKEN), "{}", file.display());
    }
}

#[test]
fn snapshot_declares_no_actions() {
    let server = FakeJira::start();
    let state = tempfile::tempdir().unwrap();
    let runner = jira_runner(&server, state.path(), TOKEN, &["--jql", ANY_ISSUE_JQL]);

    let snapshot = runner.snapshot().unwrap();

    assert!(snapshot.actions.is_empty());
}
