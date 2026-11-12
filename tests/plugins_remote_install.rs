#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/remote_plugins.rs"]
mod fixtures;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repository = root.path().join("repository");
        fixtures::write_package(&repository, "github", "0.0.4", "0.0.0");
        fixtures::write_fake_git(&root.path().join("bin"), &repository);
        Self { root }
    }
    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.root.path().join(relative)
    }
    fn run(&self, arguments: &[&str], input: &str) -> Output {
        let mut process = Command::new(env!("CARGO_BIN_EXE_mosaic"))
            .env("MOSAIC_LANGUAGE", "pt-BR")
            .args(["plugins", "install", "--to"])
            .arg(self.path("target"))
            .arg("--skills-dir")
            .arg(self.path("skills"))
            .args(arguments)
            .env("PATH", self.path("bin"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        process
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        process.wait_with_output().unwrap()
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn remote_install_prints_source_version_and_setup_hint_and_links_skill() {
    let fixture = Fixture::new();
    let output = fixture.run(&["owner/repo", "--yes"], "");
    assert!(output.status.success(), "{}", text(&output));
    for expected in [
        "owner/repo",
        "0.0.4",
        "0.0.0",
        "versão do plugin github: 0.0.4",
        "mosaic plugins setup github",
    ] {
        assert!(text(&output).contains(expected), "{}", text(&output));
    }
    assert!(fixture.path("target/github/github.py").is_file());
    assert!(fixture.path("skills/mosaic-plugin-github").is_symlink());
}

#[test]
fn remote_install_decline_enter_and_eof_write_nothing() {
    for input in ["n\n", "\n", ""] {
        let fixture = Fixture::new();
        let output = fixture.run(&["owner/repo"], input);
        assert!(output.status.success(), "{}", text(&output));
        assert!(text(&output).contains("Instalação cancelada; nenhum plugin foi alterado."));
        assert!(!fixture.path("target/github").exists());
        assert!(!fixture.path("skills").exists());
    }
}

#[test]
fn force_does_not_imply_yes_or_skip_minimum_version_and_preserves_config_and_state() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.path("target/github")).unwrap();
    fs::write(fixture.path("target/github/github.py"), "edited").unwrap();
    fs::write(fixture.path("mosaic.toml"), "config").unwrap();
    fs::write(fixture.path("state.json"), "state").unwrap();
    let output = fixture.run(&["owner/repo", "--force"], "\n");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        fs::read_to_string(fixture.path("target/github/github.py")).unwrap(),
        "edited"
    );
    fixtures::write_package(&fixture.path("repository"), "github", "0.0.4", "99.0.0");
    let output = fixture.run(&["owner/repo", "--force", "--yes"], "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(fixture.path("target/github/github.py")).unwrap(),
        "edited"
    );
    fixtures::write_package(&fixture.path("repository"), "github", "0.0.4", "0.0.0");
    assert!(fixture
        .run(&["owner/repo", "--force", "--yes", "--no-skill"], "")
        .status
        .success());
    assert_eq!(
        fs::read_to_string(fixture.path("mosaic.toml")).unwrap(),
        "config"
    );
    assert_eq!(
        fs::read_to_string(fixture.path("state.json")).unwrap(),
        "state"
    );
    assert!(!fixture.path("skills").exists());
}

#[test]
fn invalid_candidate_and_duplicate_ids_in_batch_prevent_every_install() {
    for duplicate in [false, true] {
        let fixture = Fixture::new();
        fixtures::write_package(
            &fixture.path("repository/plugins/github"),
            "github",
            "0.0.4",
            "0.0.0",
        );
        fixtures::write_package(
            &fixture.path("repository/plugins/gitlab"),
            if duplicate { "github" } else { "gitlab" },
            "0.0.4",
            if duplicate { "0.0.0" } else { "99.0.0" },
        );
        let output = fixture.run(&["github", "gitlab", "--yes"], "");
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        assert!(!fixture.path("target").exists());
        assert!(!fixture.path("skills").exists());
    }
}

#[test]
fn nested_source_installs_manifest_identity_and_preserves_conflicting_skill() {
    let fixture = Fixture::new();
    fixtures::write_package(
        &fixture.path("repository/addons/review"),
        "github",
        "0.0.4",
        "0.0.0",
    );
    fs::create_dir_all(fixture.path("skills/mosaic-plugin-github")).unwrap();
    fs::write(
        fixture.path("skills/mosaic-plugin-github/SKILL.md"),
        "user skill",
    )
    .unwrap();
    let output = fixture.run(&["owner/repo/addons/review", "--yes"], "");
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.path("target/github/github.py").exists());
    assert!(!fixture.path("target/review").exists());
    assert_eq!(
        fs::read_to_string(fixture.path("skills/mosaic-plugin-github/SKILL.md")).unwrap(),
        "user skill"
    );
    assert!(text(&output).contains("aviso:"));
}

#[test]
fn failed_download_reports_failure_without_partial_install() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path("bin/git"),
        "#!/bin/sh\necho 'network unavailable' >&2\nexit 1\n",
    )
    .unwrap();
    let output = fixture.run(&["owner/repo", "--yes"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output).contains("network unavailable"));
    assert!(!fixture.path("target").exists());
}

#[test]
fn legacy_local_install_remains_noninteractive_and_offline() {
    let fixture = Fixture::new();
    let local = fixture.path("local/demo");
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("demo.py"), "legacy").unwrap();
    fs::remove_file(fixture.path("bin/git")).unwrap();
    let output = fixture.run(
        &[
            "--from",
            fixture.path("local").to_str().unwrap(),
            "--no-skill",
        ],
        "",
    );
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.path("target/demo/demo.py").is_file());
}

#[test]
fn remote_usage_errors_and_offline_catalog_do_not_download() {
    let fixture = Fixture::new();
    for args in [
        vec![],
        vec!["owner/repo", "jira"],
        vec!["--from", "/tmp", "owner/repo"],
        vec!["--to", "--yes", "jira"],
        vec!["owner/repo/../escape"],
    ] {
        assert_eq!(fixture.run(&args, "").status.code(), Some(2), "{args:?}");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .env("MOSAIC_LANGUAGE", "pt-BR")
        .args(["plugins", "list"])
        .env("PATH", Path::new("/nonexistent"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "azure-devops\nazure-pipelines\ncalendar\ngithub\ngitlab\njira\n"
    );
}

#[test]
fn malformed_remote_metadata_is_safe_before_confirmation() {
    for (field, original) in [
        ("version", "0.0.4"),
        ("min_mosaic_version", "0.0.0"),
        ("plugin", "github"),
    ] {
        let fixture = Fixture::new();
        let manifest = fixture.path("repository/setup.toml");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            text.replace(
                &format!("{field} = \"{original}\""),
                &format!(r#"{field} = "\u001B[2J\u001B[H\u000Aforged""#),
            ),
        )
        .unwrap();
        let output = fixture.run(&["owner/repo"], "");
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
        assert_eq!(stderr.lines().count(), 1, "{stderr:?}");
        assert!(output.stdout.is_empty());
        assert!(!fixture.path("target").exists());
    }
    let fixture = Fixture::new();
    let manifest = fixture.path("repository/setup.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        text.replace(
            "version = \"0.0.4\"",
            &format!("version = \"{}\"", "x".repeat(5000)),
        ),
    )
    .unwrap();
    let output = fixture.run(&["owner/repo"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).chars().count() <= 510);
}

#[test]
fn installing_javascript_then_setup_uses_node() {
    let fixture = Fixture::new();
    let manifest = fixture.path("repository/setup.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("github.py", "github.js"),
    )
    .unwrap();
    fs::rename(
        fixture.path("repository/github.py"),
        fixture.path("repository/github.js"),
    )
    .unwrap();
    fs::write(
        fixture.path("repository/github.js"),
        "console.log('fixture');",
    )
    .unwrap();
    let output = fixture.run(
        &[
            "owner/repo",
            "--yes",
            "--no-skill",
            "--to",
            fixture.path("configuration/plugins").to_str().unwrap(),
        ],
        "",
    );
    assert!(output.status.success(), "{}", text(&output));
    let mut child = Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .env("MOSAIC_LANGUAGE", "pt-BR")
        .args(["plugins", "setup", "github", "--print", "--config-dir"])
        .arg(fixture.path("configuration"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\n\n\n\n\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(
        text(&output).contains("command = \"node\""),
        "{}",
        text(&output)
    );
    assert!(text(&output).contains("plugins/github/github.js"));
}

#[test]
fn relative_destination_creates_a_working_skill_link_and_is_repeatable() {
    let fixture = Fixture::new();
    for expected in ["skill instalada", "skill já instalada"] {
        let output = Command::new(env!("CARGO_BIN_EXE_mosaic"))
            .env("MOSAIC_LANGUAGE", "pt-BR")
            .current_dir(fixture.root.path())
            .env("PATH", fixture.path("bin"))
            .args([
                "plugins",
                "install",
                "owner/repo",
                "--yes",
                "--to",
                "plugins",
                "--skills-dir",
                "skills",
            ])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output));
        assert!(fixture
            .path("skills/mosaic-plugin-github/SKILL.md")
            .is_file());
        assert!(text(&output).contains(expected), "{}", text(&output));
    }
}
