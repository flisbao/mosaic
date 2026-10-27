#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Output, Stdio};

const EXIT_USAGE: i32 = 2;
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn run_mosaic(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", "/nonexistent-mosaic-test")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

#[test]
fn every_version_spelling_prints_the_version_and_exits_zero() {
    for spelling in ["--version", "-V", "version"] {
        let output = run_mosaic(&[spelling]);
        assert_eq!(output.status.code(), Some(0), "{spelling}");
        assert_eq!(
            text(&output.stdout),
            format!("mosaic {VERSION}\n"),
            "{spelling}"
        );
    }
}

#[test]
fn every_help_spelling_explains_what_to_do_and_never_opens_the_panel() {
    for spelling in ["--help", "-h", "help"] {
        let output = run_mosaic(&[spelling]);
        assert_eq!(output.status.code(), Some(0), "{spelling}");
        let stdout = text(&output.stdout);
        assert!(stdout.contains(&format!("mosaic {VERSION}")), "{stdout}");
        for expected in [
            "mosaic plugins install",
            "mosaic plugins setup",
            "mosaic themes",
            "mosaic tabs",
            "mosaic check",
            "mosaic.toml",
            "--version",
        ] {
            assert!(
                stdout.contains(expected),
                "{spelling}: falta {expected}\n{stdout}"
            );
        }
        assert!(!stdout.contains('\x1b'), "{spelling}");
        assert!(!std::path::Path::new("/nonexistent-mosaic-test").exists());
    }
}

#[test]
fn an_unknown_argument_is_a_usage_error_instead_of_opening_the_panel() {
    for argument in ["--ajuda", "foo", "-x"] {
        let output = run_mosaic(&[argument]);
        assert_eq!(output.status.code(), Some(EXIT_USAGE), "{argument}");
        let stderr = text(&output.stderr);
        assert!(stderr.contains(argument), "{stderr}");
        assert!(stderr.contains("mosaic --help"), "{stderr}");
        assert!(output.stdout.is_empty());
    }
}
