#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Output, Stdio};

const EXIT_USAGE: i32 = 2;

fn run_themes(arguments: &[&str], color_term: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .arg("themes")
        .args(arguments)
        .env("COLORTERM", color_term)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

#[test]
fn lists_every_preset_and_exits_zero() {
    let output = run_themes(&[], "");
    assert_eq!(output.status.code(), Some(0));
    let stdout = text(&output.stdout);
    assert!(stdout.contains("tokyo-night"), "{stdout}");
    assert!(stdout.contains("solarized-light"), "{stdout}");
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn truecolor_terminals_get_swatches() {
    let output = run_themes(&[], "truecolor");
    assert!(text(&output.stdout).contains('\x1b'));
}

#[test]
fn shows_one_theme_with_its_roles() {
    let output = run_themes(&["dracula"], "");
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("#bd93f9"));
}

#[test]
fn an_unknown_theme_exits_with_usage() {
    let output = run_themes(&["neon"], "");
    assert_eq!(output.status.code(), Some(EXIT_USAGE));
    assert!(text(&output.stderr).contains("tema desconhecido"));
}
