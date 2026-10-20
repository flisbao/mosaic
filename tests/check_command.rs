#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
fn check_passes_provider_flags_after_separator() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("widget.sh"),
        "[ \"$1\" = \"--timeout\" ] && [ \"$2\" = \"7\" ] && [ \"$3\" = \"--cwd\" ] && [ \"$4\" = \"provider-dir\" ] || exit 9\nprintf '{\"summary\":{\"title\":\"flags recebidas\"}}'\n",
    )
    .unwrap();
    let output = run_check(
        directory.path(),
        &[
            "sh",
            "widget.sh",
            "--",
            "--timeout",
            "7",
            "--cwd",
            "provider-dir",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("flags recebidas"));
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
fn check_kanban_output_is_clean() {
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
    assert!(!stdout(&output).contains("AVISO"));
}

#[test]
fn check_never_opens_the_terminal() {
    let directory = tempfile::tempdir().unwrap();
    write_widget(directory.path(), r#"{"summary":{"title":"Sem terminal"}}"#);
    let output = run_check(directory.path(), &["sh", "widget.sh"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(!stdout(&output).contains('\u{1b}'));
}

#[test]
fn check_cwd_flag_runs_the_provider_in_the_given_directory() {
    let outer = tempfile::tempdir().unwrap();
    let inner = outer.path().join("config");
    fs::create_dir(&inner).unwrap();
    write_widget(
        &inner,
        r#"{"summary":{"title":"Agenda","blocks":[{"type":"lines","lines":[{"text":"olá"}]}]}}"#,
    );
    let output = run_check(
        outer.path(),
        &["--cwd", inner.to_str().unwrap(), "sh", "widget.sh"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
}

#[test]
fn check_timeout_flag_limits_the_provider() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("slow.sh"), "sleep 5\n").unwrap();
    let output = run_check(directory.path(), &["--timeout", "1", "sh", "slow.sh"]);
    assert_ne!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("tempo limite de 1s"),
        "{}",
        stdout(&output)
    );
}

fn run_mosaic(working_directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .args(arguments)
        .current_dir(working_directory)
        .env("HOME", working_directory)
        .env("XDG_CONFIG_HOME", working_directory)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn check_config_flag_exits_zero_on_valid_and_one_on_invalid() {
    let directory = tempfile::tempdir().unwrap();
    let valid = directory.path().join("valid.toml");
    fs::write(
        &valid,
        "[[widgets]]\nid = \"jira\"\ncommand = \"python3\"\ntab = true\n",
    )
    .unwrap();
    let invalid = directory.path().join("invalid.toml");
    fs::write(&invalid, "[[widgets]]\nid = \"jira\"\ncommand = \n").unwrap();

    let valid_output = run_mosaic(
        directory.path(),
        &["check", "--config", valid.to_str().unwrap()],
    );
    let invalid_output = run_mosaic(
        directory.path(),
        &["check", "--config", invalid.to_str().unwrap()],
    );

    assert_eq!(
        valid_output.status.code(),
        Some(0),
        "{}",
        stderr(&valid_output)
    );
    assert!(
        stdout(&valid_output).contains("aviso"),
        "{}",
        stdout(&valid_output)
    );
    assert_eq!(invalid_output.status.code(), Some(EXIT_INVALID));
    assert!(
        stderr(&invalid_output).contains("linha 3"),
        "{}",
        stderr(&invalid_output)
    );
}

#[test]
fn check_config_with_two_paths_is_a_usage_error() {
    let directory = tempfile::tempdir().unwrap();

    let output = run_mosaic(directory.path(), &["check", "--config", "a.toml", "b.toml"]);

    assert_eq!(
        output.status.code(),
        Some(EXIT_USAGE),
        "{}",
        stderr(&output)
    );
}

#[test]
fn check_config_with_missing_file_exits_one() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("nao-existe.toml");

    let output = run_mosaic(
        directory.path(),
        &["check", "--config", missing.to_str().unwrap()],
    );

    assert_eq!(
        output.status.code(),
        Some(EXIT_INVALID),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("falha de leitura"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn check_config_without_path_reads_the_default_config_file() {
    let directory = tempfile::tempdir().unwrap();
    let config_directory = directory.path().join("mosaic");
    fs::create_dir_all(&config_directory).unwrap();
    fs::write(
        config_directory.join("config.toml"),
        "[[widgets]]\nid = \"jira\"\ncommand = \"python3\"\n",
    )
    .unwrap();

    let output = run_mosaic(directory.path(), &["check", "--config"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}
