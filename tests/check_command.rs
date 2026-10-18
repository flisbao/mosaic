#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const EXIT_INVALID: i32 = 1;
const EXIT_USAGE: i32 = 2;

fn run_check(working_directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .arg("check")
        .args(arguments)
        .current_dir(working_directory)
        .env("HOME", working_directory)
        .env("XDG_CONFIG_HOME", working_directory)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn write_widget(directory: &Path, snapshot_json: &str) {
    let script = format!("cat <<'JSON'\n{snapshot_json}\nJSON\n");
    fs::write(directory.join("widget.sh"), script).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn check_valid_widget_exits_zero_and_prints_the_preview() {
    let directory = tempfile::tempdir().unwrap();
    write_widget(
        directory.path(),
        r#"{"summary":{"title":"Agenda","blocks":[{"type":"lines","lines":[{"text":"olá mundo"}]}]}}"#,
    );
    let output = run_check(directory.path(), &["sh", "widget.sh"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(text.contains("Agenda"), "{text}");
    assert!(text.contains("olá mundo"), "{text}");
}

#[test]
fn check_shows_the_mini_field() {
    let directory = tempfile::tempdir().unwrap();
    write_widget(
        directory.path(),
        r#"{"summary":{"title":"Tarefas"},"mini":{"value":"12","text":"tarefas"}}"#,
    );
    let output = run_check(directory.path(), &["sh", "widget.sh"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("mini: 12 tarefas"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn check_with_errors_exits_one_with_paths() {
    let directory = tempfile::tempdir().unwrap();
    write_widget(
        directory.path(),
        r#"{"summary":{"title":"A","blocks":[{"type":"rows","rows":[{"time":"9"}]}]}}"#,
    );
    let output = run_check(directory.path(), &["sh", "widget.sh"]);
    assert_eq!(output.status.code(), Some(EXIT_INVALID));
    assert!(stdout(&output).contains("ERRO summary.blocks[0].rows[0].text"));
}

#[test]
fn check_passes_env_variables() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("widget.sh"),
        "printf '{\"summary\":{\"title\":\"%s\"}}' \"$MEU_VALOR\"",
    )
    .unwrap();
    let output = run_check(
        directory.path(),
        &["sh", "widget.sh", "--env", "MEU_VALOR=valor-de-teste"],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("valor-de-teste"));
}

#[test]
fn check_without_command_exits_two() {
    let directory = tempfile::tempdir().unwrap();
    let output = run_check(directory.path(), &[]);
    assert_eq!(output.status.code(), Some(EXIT_USAGE));
    assert!(String::from_utf8_lossy(&output.stderr).contains("uso:"));
}

#[test]
fn check_malformed_env_exits_two() {
    let directory = tempfile::tempdir().unwrap();
    for arguments in [
        vec!["sh", "--env", "SEMIGUAL"],
        vec!["sh", "--env", "=valor"],
        vec!["sh", "--env"],
    ] {
        let output = run_check(directory.path(), &arguments);
        assert_eq!(output.status.code(), Some(EXIT_USAGE), "{arguments:?}");
    }
}

#[test]
fn check_legacy_kanban_output_is_valid() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let kanban = tempfile::tempdir().unwrap();
    fs::create_dir_all(kanban.path().join("inbox")).unwrap();
    fs::write(kanban.path().join("inbox/a.md"), "# Tarefa A\n").unwrap();
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/kanban.py");
    let environment = format!("KANBAN_DIR={}", kanban.path().display());
    let home = tempfile::tempdir().unwrap();
    let output = run_check(
        home.path(),
        &["python3", &plugin.to_string_lossy(), "--env", &environment],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(!stdout(&output).contains("ERRO"));
}

#[test]
fn check_never_opens_the_terminal() {
    let directory = tempfile::tempdir().unwrap();
    write_widget(directory.path(), r#"{"summary":{"title":"Sem terminal"}}"#);
    let output = run_check(directory.path(), &["sh", "widget.sh"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(!stdout(&output).contains('\u{1b}'));
}
