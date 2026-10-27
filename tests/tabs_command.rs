#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;
const WIDGET: &str = "[[widgets]]\nid = \"jira-a\"\ncommand = \"x\"\n";

fn tabs(config_directory: &Path, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .arg("tabs")
        .args(arguments)
        .arg("--config-dir")
        .arg(config_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn config(content: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("mosaic.toml"), content).unwrap();
    directory
}

fn read(directory: &Path) -> String {
    fs::read_to_string(directory.join("mosaic.toml")).unwrap()
}

#[test]
fn list_without_declared_tabs_explains_the_single_tab() {
    let directory = config(WIDGET);
    let output = tabs(directory.path(), &["list"], "");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("Nenhuma aba declarada: os 1 widgets"));
}

#[test]
fn list_shows_each_tab_with_its_widgets() {
    let directory = config(&format!(
        "[[tabs]]\nname = \"Work\"\n[[tabs]]\nname = \"Home\"\n{WIDGET}\n[[widgets]]\nid = \"b\"\ncommand = \"x\"\ntab = \"Home\"\n"
    ));
    let output = tabs(directory.path(), &["list"], "");
    let stdout = text(&output.stdout);
    assert!(stdout.contains("Work (1 widgets): jira-a"), "{stdout}");
    assert!(stdout.contains("Home (1 widgets): b"), "{stdout}");
}

#[test]
fn add_declares_a_new_tab_after_the_existing_ones() {
    let directory = config("[[tabs]]\nname = \"Work\"\n");
    let output = tabs(directory.path(), &["add", "Estudos"], "");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let content = read(directory.path());
    assert!(
        content.contains("[[tabs]]\nname = \"Work\"\n\n[[tabs]]\nname = \"Estudos\""),
        "{content}"
    );
    assert!(text(&output.stdout).contains("Aba \"Estudos\" criada"));
}

#[test]
fn the_first_tab_warns_about_existing_widgets_and_waits_for_confirmation() {
    let directory = config(WIDGET);
    let declined = tabs(directory.path(), &["add", "Trabalho"], "n\n");
    assert_eq!(declined.status.code(), Some(0));
    assert!(text(&declined.stdout).contains("passam a ficar nela"));
    assert_eq!(read(directory.path()), WIDGET);
    let accepted = tabs(directory.path(), &["add", "Trabalho"], "s\n");
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "{}",
        text(&accepted.stderr)
    );
    assert!(read(directory.path()).contains("name = \"Trabalho\""));
}

#[test]
fn a_duplicate_or_blank_name_is_refused() {
    let directory = config("[[tabs]]\nname = \"Work\"\n");
    let duplicate = tabs(directory.path(), &["add", "Work"], "");
    assert_eq!(duplicate.status.code(), Some(EXIT_FAILURE));
    assert!(text(&duplicate.stderr).contains("já existe uma aba chamada \"Work\""));
    let blank = tabs(directory.path(), &["add", "  "], "");
    assert_eq!(blank.status.code(), Some(EXIT_FAILURE));
    assert_eq!(read(directory.path()), "[[tabs]]\nname = \"Work\"\n");
}

#[test]
fn bad_usage_exits_with_two() {
    let directory = config("");
    let output = tabs(directory.path(), &["add"], "");
    assert_eq!(output.status.code(), Some(EXIT_USAGE));
    assert!(text(&output.stderr).contains("uso: mosaic tabs"));
}
