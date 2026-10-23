#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;
const SKILL_NAME: &str = "mosaic-plugin-jira";

fn run_skills(home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .arg("skills")
        .args(arguments)
        .env("HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn package_with_one_skill() -> tempfile::TempDir {
    let source = tempfile::tempdir().unwrap();
    let skill = source.path().join(SKILL_NAME);
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
    source
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

#[test]
fn install_links_the_skills_into_the_claude_directory_of_the_home() {
    let home = tempfile::tempdir().unwrap();
    let source = package_with_one_skill();
    let output = run_skills(
        home.path(),
        &["install", "--from", source.path().to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains(SKILL_NAME));
    let link = home.path().join(".claude/skills").join(SKILL_NAME);
    assert!(link.join("SKILL.md").is_file());
}

#[test]
fn install_honours_the_to_flag() {
    let home = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let source = package_with_one_skill();
    let output = run_skills(
        home.path(),
        &[
            "install",
            "--from",
            source.path().to_str().unwrap(),
            "--to",
            target.path().to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(target.path().join(SKILL_NAME).exists());
}

#[test]
fn install_twice_succeeds_and_says_it_was_already_installed() {
    let home = tempfile::tempdir().unwrap();
    let source = package_with_one_skill();
    let arguments = ["install", "--from", source.path().to_str().unwrap()];
    run_skills(home.path(), &arguments);
    let output = run_skills(home.path(), &arguments);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("já instalada"));
}

#[test]
fn install_reports_a_conflict_without_overwriting_and_fails() {
    let home = tempfile::tempdir().unwrap();
    let own = home.path().join(".claude/skills").join(SKILL_NAME);
    fs::create_dir_all(&own).unwrap();
    fs::write(own.join("SKILL.md"), "minha").unwrap();
    let source = package_with_one_skill();
    let output = run_skills(
        home.path(),
        &["install", "--from", source.path().to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(EXIT_FAILURE));
    assert!(text(&output.stderr).contains(SKILL_NAME));
    assert_eq!(fs::read_to_string(own.join("SKILL.md")).unwrap(), "minha");
}

#[test]
fn install_fails_when_the_skills_directory_does_not_exist() {
    let home = tempfile::tempdir().unwrap();
    let output = run_skills(
        home.path(),
        &[
            "install",
            "--from",
            home.path().join("nada").to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(EXIT_FAILURE));
    assert!(text(&output.stderr).contains("não encontrada"));
}

#[test]
fn unknown_arguments_print_the_usage() {
    let home = tempfile::tempdir().unwrap();
    for arguments in [
        &[][..],
        &["remove"],
        &["install", "--bogus"],
        &["install", "--from"],
    ] {
        let output = run_skills(home.path(), arguments);
        assert_eq!(output.status.code(), Some(EXIT_USAGE), "{arguments:?}");
        assert!(text(&output.stderr).contains("uso: mosaic skills install"));
    }
}
