#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;

fn run_plugins(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .arg("plugins")
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn write_plugin(source: &Path, name: &str) {
    let folder = source.join(name);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(format!("{name}.py")), "print('x')").unwrap();
}

#[test]
fn install_copies_the_plugins_and_reports_them() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_plugin(source.path(), "jira");
    write_plugin(source.path(), "github");
    let output = run_plugins(&[
        "install",
        "--from",
        source.path().to_str().unwrap(),
        "--to",
        target.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert!(stdout.contains("instalado: jira"), "{stdout}");
    assert!(stdout.contains("2 plugins em"), "{stdout}");
    assert!(target.path().join("jira/jira.py").is_file());
}

#[test]
fn a_second_install_keeps_existing_plugins_unless_forced() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_plugin(source.path(), "jira");
    let from = source.path().to_str().unwrap();
    let to = target.path().to_str().unwrap();
    run_plugins(&["install", "--from", from, "--to", to]);
    let again = run_plugins(&["install", "--from", from, "--to", to]);
    assert!(text(&again.stdout).contains("já instalado: jira"));
    let forced = run_plugins(&["install", "--from", from, "--to", to, "--force"]);
    assert!(text(&forced.stdout).contains("substituído: jira"));
}

#[test]
fn an_unknown_plugin_exits_with_failure() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_plugin(source.path(), "jira");
    let output = run_plugins(&[
        "install",
        "--from",
        source.path().to_str().unwrap(),
        "--to",
        target.path().to_str().unwrap(),
        "nope",
    ]);
    assert_eq!(output.status.code(), Some(EXIT_FAILURE));
    assert!(text(&output.stderr).contains("plugin não encontrado"));
}

#[test]
fn incorrect_usage_exits_with_two() {
    assert_eq!(run_plugins(&[]).status.code(), Some(EXIT_USAGE));
    assert_eq!(
        run_plugins(&["install", "--x"]).status.code(),
        Some(EXIT_USAGE)
    );
}

#[test]
fn list_prints_one_plugin_per_line() {
    let source = tempfile::tempdir().unwrap();
    write_plugin(source.path(), "jira");
    write_plugin(source.path(), "github");
    let output = run_plugins(&["list", "--from", source.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "github\njira\n");
}

#[test]
fn an_invalid_plugin_name_is_a_usage_error() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_plugin(source.path(), "jira");
    for bad in ["../x", "Jira", "jira.py"] {
        let output = run_plugins(&[
            "install",
            "--from",
            source.path().to_str().unwrap(),
            "--to",
            target.path().to_str().unwrap(),
            bad,
        ]);
        assert_eq!(output.status.code(), Some(EXIT_USAGE), "{bad}");
        assert!(
            text(&output.stderr).contains("nome de plugin inválido"),
            "{bad}"
        );
    }
    assert!(!target.path().join("jira").exists());
}
