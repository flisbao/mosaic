#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;
const MANIFEST: &str = r#"
plugin = "demo"
script = "plugins/demo/demo.py"
default_id = "demo"
interval_seconds = 120
timeout_seconds = 30

[[steps]]
kind = "variable"
name = "DEMO_SITE"
prompt = "Site do demo"
required = true

[[steps]]
kind = "secret"
prompt = "Token do demo"
environment_key = "DEMO_TOKEN_ENV"
file_key = "DEMO_TOKEN_FILE"
required = true

[[notes]]
text = "exporte o token antes de abrir o mosaic"
"#;
const EXISTING_CONFIG: &str = "# meu painel\n[[widgets]]\nid = \"outro\"\ncommand = \"x\"\n";

fn setup(config_directory: &Path, extra: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .args(["plugins", "setup", "demo", "--config-dir"])
        .arg(config_directory)
        .args(extra)
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

fn config_with_demo_plugin() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let plugin = directory.path().join("plugins/demo");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("setup.toml"), MANIFEST).unwrap();
    fs::write(directory.path().join("mosaic.toml"), EXISTING_CONFIG).unwrap();
    directory
}

const ANSWERS: &str = "demo-um\nempresa.exemplo\n1\nDEMO_TOKEN_EMPRESA\n\n\ns\n";

#[test]
fn the_interview_appends_the_widget_and_keeps_the_rest_of_the_file() {
    let directory = config_with_demo_plugin();
    let output = setup(directory.path(), &[], ANSWERS);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let config = fs::read_to_string(directory.path().join("mosaic.toml")).unwrap();
    assert!(config.starts_with(EXISTING_CONFIG), "{config}");
    assert!(config.contains("id = \"demo-um\""), "{config}");
    assert!(
        config.contains("DEMO_SITE = \"empresa.exemplo\""),
        "{config}"
    );
    assert!(
        config.contains("DEMO_TOKEN_ENV = \"DEMO_TOKEN_EMPRESA\""),
        "{config}"
    );
    assert!(
        config.contains("inherit_env = [\"DEMO_TOKEN_EMPRESA\"]"),
        "{config}"
    );
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("exporte o token antes de abrir o mosaic"),
        "{stdout}"
    );
    assert!(stdout.contains("gravado em"), "{stdout}");
}

#[test]
fn declining_the_confirmation_writes_nothing() {
    let directory = config_with_demo_plugin();
    let answers = ANSWERS.replace("\ns\n", "\nn\n");
    let output = setup(directory.path(), &[], &answers);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let config = fs::read_to_string(directory.path().join("mosaic.toml")).unwrap();
    assert_eq!(config, EXISTING_CONFIG);
    assert!(text(&output.stdout).contains("Nada foi gravado"));
}

#[test]
fn print_shows_the_block_without_asking_or_writing() {
    let directory = config_with_demo_plugin();
    let answers = "demo-um\nempresa.exemplo\n1\nDEMO_TOKEN_EMPRESA\n\n\n";
    let output = setup(directory.path(), &["--print"], answers);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("[[widgets]]"));
    let config = fs::read_to_string(directory.path().join("mosaic.toml")).unwrap();
    assert_eq!(config, EXISTING_CONFIG);
}

#[test]
fn an_id_already_in_the_config_is_asked_again() {
    let directory = config_with_demo_plugin();
    let answers = ANSWERS.replacen("demo-um", "outro\ndemo-um", 1);
    let output = setup(directory.path(), &[], &answers);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("Já existe um widget com o id \"outro\""));
}

#[test]
fn input_that_ends_early_cancels_without_writing() {
    let directory = config_with_demo_plugin();
    let output = setup(directory.path(), &[], "demo-um\nempresa.exemplo\n");
    assert_eq!(output.status.code(), Some(EXIT_FAILURE));
    assert!(text(&output.stderr).contains("configuração cancelada"));
    let config = fs::read_to_string(directory.path().join("mosaic.toml")).unwrap();
    assert_eq!(config, EXISTING_CONFIG);
}

#[test]
fn a_plugin_that_is_not_installed_points_to_the_install_command() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("mosaic.toml"), EXISTING_CONFIG).unwrap();
    let output = setup(directory.path(), &[], "");
    assert_eq!(output.status.code(), Some(EXIT_FAILURE));
    assert!(text(&output.stderr).contains("mosaic plugins install demo"));
}

#[test]
fn a_missing_name_is_a_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .args(["plugins", "setup"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(EXIT_USAGE));
    assert!(text(&output.stderr).contains("uso: mosaic plugins"));
}
