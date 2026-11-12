#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/release_server.rs"]
mod release_server;

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use mosaic::domain::release::ReleaseTarget;
use release_server::{fake_mosaic_script, ReleaseServer};

const INSTALLED_VERSION: &str = env!("CARGO_PKG_VERSION");
const NEWER_VERSION: &str = "99.0.0";

const TEXT_FILE_BUSY_ERROR: i32 = 26;
const SPAWN_RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(25);

/// Executar um arquivo recém-gravado pode falhar com ETXTBSY se outra thread do teste fez
/// fork nesse instante; repetir resolve.
fn spawn_retrying(command: &mut Command) -> std::process::Child {
    loop {
        match command.spawn() {
            Err(error) if error.raw_os_error() == Some(TEXT_FILE_BUSY_ERROR) => {
                std::thread::sleep(SPAWN_RETRY_PAUSE);
            }
            other => return other.unwrap(),
        }
    }
}

struct Fixture {
    root: tempfile::TempDir,
    server: ReleaseServer,
}

impl Fixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
            server: ReleaseServer::start(),
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn target(&self) -> &'static str {
        ReleaseTarget::detect(std::env::consts::OS, std::env::consts::ARCH)
            .unwrap()
            .triple()
    }

    fn publish(&self, version: &str) {
        self.server.set_latest(version);
        self.server
            .publish(version, self.target(), &fake_mosaic_script(version));
    }

    fn install_copy(&self, relative: &str) -> PathBuf {
        let destination = self.path(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_mosaic"), &destination).unwrap();
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755)).unwrap();
        destination
    }

    fn run(&self, executable: &Path, arguments: &[&str], input: &str) -> Output {
        self.run_with_base(executable, arguments, input, &self.server.base())
    }

    fn run_with_base(
        &self,
        executable: &Path,
        arguments: &[&str],
        input: &str,
        base: &str,
    ) -> Output {
        let mut process = spawn_retrying(
            Command::new(executable)
                .env("MOSAIC_LANGUAGE", "pt-BR")
                .arg("update")
                .args(arguments)
                .env("MOSAIC_DOWNLOAD_BASE", base)
                .env("CARGO_HOME", self.path("cargo-home"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        );
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

fn version_of(executable: &Path) -> String {
    let output = spawn_retrying(
        Command::new(executable)
            .env("MOSAIC_LANGUAGE", "pt-BR")
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .wait_with_output()
    .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn directory_entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn check_reports_a_newer_version_without_downloading_anything() {
    let fixture = Fixture::new();
    fixture.publish(NEWER_VERSION);
    let output = fixture.run(Path::new(env!("CARGO_BIN_EXE_mosaic")), &["--check"], "");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("mosaic {NEWER_VERSION} disponível (instalada: {INSTALLED_VERSION})\n")
    );
    assert!(fixture
        .server
        .requests()
        .iter()
        .all(|path| !path.contains("/download/")));
}

#[test]
fn check_says_when_this_is_the_latest_version() {
    let fixture = Fixture::new();
    fixture.server.set_latest(INSTALLED_VERSION);
    let output = fixture.run(Path::new(env!("CARGO_BIN_EXE_mosaic")), &["--check"], "");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("mosaic {INSTALLED_VERSION} é a versão mais recente\n")
    );
}

#[test]
fn check_fails_with_exit_code_one_when_the_network_fails() {
    let fixture = Fixture::new();
    let output = fixture.run_with_base(
        Path::new(env!("CARGO_BIN_EXE_mosaic")),
        &["--check"],
        "",
        "http://127.0.0.1:1/releases",
    );
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(text(&output).starts_with("mosaic: "), "{}", text(&output));
}

#[test]
fn an_unknown_argument_is_a_usage_error() {
    let fixture = Fixture::new();
    let output = fixture.run(Path::new(env!("CARGO_BIN_EXE_mosaic")), &["--force"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).contains("uso: mosaic update"));
}

#[test]
fn update_with_yes_replaces_the_binary_and_leaves_no_temporary_files() {
    let fixture = Fixture::new();
    fixture.publish(NEWER_VERSION);
    let installed = fixture.install_copy("home/.local/bin/mosaic");
    let output = fixture.run(&installed, &["--yes"], "");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(version_of(&installed), format!("mosaic {NEWER_VERSION}"));
    assert_eq!(directory_entries(installed.parent().unwrap()), ["mosaic"]);
    assert!(text(&output).contains(&format!("mosaic atualizado para {NEWER_VERSION}")));
}

#[test]
fn update_asks_first_and_only_an_explicit_yes_replaces_the_binary() {
    for (answer, replaced) in [("n\n", false), ("\n", false), ("", false), ("s\n", true)] {
        let fixture = Fixture::new();
        fixture.publish(NEWER_VERSION);
        let installed = fixture.install_copy("bin/mosaic");
        let output = fixture.run(&installed, &[], answer);
        assert!(output.status.success(), "{}", text(&output));
        assert!(text(&output).contains("Atualizar agora? [s/N]"));
        let expected = if replaced {
            format!("mosaic {NEWER_VERSION}")
        } else {
            format!("mosaic {INSTALLED_VERSION}")
        };
        assert_eq!(version_of(&installed), expected, "resposta {answer:?}");
        if !replaced {
            assert!(text(&output).contains("Atualização cancelada; nada foi alterado."));
        }
    }
}

#[test]
fn a_wrong_checksum_leaves_the_binary_untouched_and_cleans_up() {
    let fixture = Fixture::new();
    fixture.server.set_latest(NEWER_VERSION);
    fixture.server.publish_with_checksum(
        NEWER_VERSION,
        fixture.target(),
        &fake_mosaic_script(NEWER_VERSION),
        Some(&"0".repeat(64)),
    );
    let installed = fixture.install_copy("bin/mosaic");
    let output = fixture.run(&installed, &["--yes"], "");
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(text(&output).contains("SHA-256"), "{}", text(&output));
    assert_eq!(
        version_of(&installed),
        format!("mosaic {INSTALLED_VERSION}")
    );
    assert_eq!(directory_entries(installed.parent().unwrap()), ["mosaic"]);
}

#[test]
fn already_on_the_latest_version_downloads_nothing() {
    let fixture = Fixture::new();
    fixture.publish(INSTALLED_VERSION);
    let installed = fixture.install_copy("bin/mosaic");
    let output = fixture.run(&installed, &["--yes"], "");
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("é a versão mais recente"));
    assert!(fixture
        .server
        .requests()
        .iter()
        .all(|path| !path.contains("/download/")));
}

#[test]
fn homebrew_and_cargo_installs_get_instructions_and_stay_untouched() {
    let fixture = Fixture::new();
    fixture.publish(NEWER_VERSION);
    for (relative, expected) in [
        ("opt/Cellar/mosaic/0.0.1/bin/mosaic", "brew upgrade mosaic"),
        ("projeto/target/release/mosaic", "cargo install --path ."),
        ("cargo-home/bin/mosaic", "cargo install --path ."),
    ] {
        let installed = fixture.install_copy(relative);
        let output = fixture.run(&installed, &["--yes"], "");
        assert!(output.status.success(), "{}", text(&output));
        assert!(text(&output).contains(expected), "{}", text(&output));
        assert_eq!(
            version_of(&installed),
            format!("mosaic {INSTALLED_VERSION}"),
            "{relative}"
        );
    }
    assert!(fixture
        .server
        .requests()
        .iter()
        .all(|path| !path.contains("/download/")));
}

#[test]
fn a_directory_without_write_permission_explains_the_way_out() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let fixture = Fixture::new();
    fixture.publish(NEWER_VERSION);
    let installed = fixture.install_copy("locked/mosaic");
    let directory = installed.parent().unwrap();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
    let output = fixture.run(&installed, &["--yes"], "");
    fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(text(&output).contains("não usa sudo"), "{}", text(&output));
    assert_eq!(
        version_of(&installed),
        format!("mosaic {INSTALLED_VERSION}")
    );
}

#[test]
fn updating_does_not_leak_the_environment_to_curl() {
    let fixture = Fixture::new();
    fixture.publish(NEWER_VERSION);
    let bin = fixture.path("fakebin");
    fs::create_dir_all(&bin).unwrap();
    let record = fixture.path("curl-environment");
    release_server::write_executable(
        &bin.join("curl"),
        &format!("#!/bin/sh\nenv > {}\nexit 22\n", record.display()),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_mosaic"))
        .env("MOSAIC_LANGUAGE", "pt-BR")
        .args(["update", "--check"])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("SEGREDO_DO_USUARIO", "nao-vaza")
        .env("MOSAIC_DOWNLOAD_BASE", fixture.server.base())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let environment = fs::read_to_string(record).unwrap();
    assert!(!environment.contains("SEGREDO_DO_USUARIO"));
    assert!(environment.contains("PATH="));
}
