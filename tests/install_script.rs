#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/release_server.rs"]
mod release_server;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use release_server::{fake_mosaic_script, write_executable, ReleaseServer};

const INSTALL_SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh");
const SHELL: &str = "/bin/sh";
const TEXT_FILE_BUSY_MESSAGE: &str = "Text file busy";
const TEXT_FILE_BUSY_RETRIES: u32 = 20;
const BASE_TOOLS: [&str; 13] = [
    "mktemp", "mkdir", "rm", "tar", "gzip", "install", "mv", "grep", "sed", "tr", "tail", "cut",
    "cat",
];

struct Environment {
    root: tempfile::TempDir,
    server: ReleaseServer,
    downloader: &'static str,
    checksum_tools: Vec<&'static str>,
    system: &'static str,
    machine: &'static str,
    extra_path: Option<PathBuf>,
}

impl Environment {
    fn new() -> Self {
        let server = ReleaseServer::start();
        Self {
            root: tempfile::tempdir().unwrap(),
            server,
            downloader: "curl",
            checksum_tools: vec!["sha256sum", "shasum"],
            system: "Linux",
            machine: "x86_64",
            extra_path: None,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn install_directory(&self) -> PathBuf {
        self.path("home/.local/bin")
    }

    fn toolbox(&self) -> PathBuf {
        let toolbox = self.path("toolbox");
        let _ = fs::remove_dir_all(&toolbox);
        fs::create_dir_all(&toolbox).unwrap();
        let tools = BASE_TOOLS
            .iter()
            .chain(std::iter::once(&self.downloader))
            .chain(self.checksum_tools.iter());
        for tool in tools {
            if let Some(found) = find_on_host_path(tool) {
                symlink(found, toolbox.join(tool)).unwrap();
            }
        }
        write_executable(
            &toolbox.join("uname"),
            &format!(
                "#!/bin/sh\nif [ \"$1\" = \"-s\" ]; then echo {}; else echo {}; fi\n",
                self.system, self.machine
            ),
        );
        toolbox
    }

    /// Executar um arquivo recém-gravado pode falhar com ETXTBSY se outra thread do teste fez
    /// fork nesse instante; como a instalação é idempotente, repetir resolve.
    fn run(&self, extra_environment: &[(&str, &str)]) -> Output {
        let mut output = self.run_once(extra_environment);
        for _ in 0..TEXT_FILE_BUSY_RETRIES {
            if !text(&output).contains(TEXT_FILE_BUSY_MESSAGE) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            output = self.run_once(extra_environment);
        }
        output
    }

    fn run_once(&self, extra_environment: &[(&str, &str)]) -> Output {
        let mut search_path = self.toolbox().into_os_string();
        if let Some(extra) = &self.extra_path {
            search_path.push(":");
            search_path.push(extra);
        }
        let mut command = Command::new(SHELL);
        command
            .arg(INSTALL_SCRIPT)
            .env_clear()
            .env("PATH", &search_path)
            .env("HOME", self.path("home"))
            .env("MOSAIC_DOWNLOAD_BASE", self.server.base());
        for (name, value) in extra_environment {
            command.env(name, value);
        }
        command.output().unwrap()
    }

    fn run_default(&self) -> Output {
        self.run(&[(
            "MOSAIC_INSTALL_DIR",
            self.install_directory().to_str().unwrap(),
        )])
    }
}

fn find_on_host_path(tool: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(tool))
        .find(|candidate| candidate.is_file())
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn leftovers(directory: &Path) -> Vec<String> {
    fs::read_dir(directory)
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn installs_the_latest_release_for_linux_x86_64_using_the_musl_tarball() {
    let environment = Environment::new();
    environment.server.set_latest("0.0.11");
    environment.server.publish(
        "0.0.11",
        "x86_64-unknown-linux-musl",
        &fake_mosaic_script("0.0.11"),
    );
    let output = environment.run_default();
    assert!(output.status.success(), "{}", text(&output));
    let installed = environment.install_directory().join("mosaic");
    assert!(installed.is_file());
    let version = Command::new(&installed).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&version.stdout), "mosaic 0.0.11\n");
    assert_eq!(leftovers(&environment.install_directory()), ["mosaic"]);
    let output_text = text(&output);
    assert!(output_text.contains("mosaic 0.0.11"), "{output_text}");
    assert!(output_text.contains("mosaic --help"), "{output_text}");
}

#[test]
fn the_arm_linux_and_macos_targets_are_chosen_from_uname() {
    for (system, machine, target) in [
        ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
        ("Linux", "arm64", "aarch64-unknown-linux-musl"),
        ("Darwin", "arm64", "aarch64-apple-darwin"),
        ("Darwin", "x86_64", "x86_64-apple-darwin"),
    ] {
        let mut environment = Environment::new();
        environment.system = system;
        environment.machine = machine;
        environment.server.set_latest("0.0.11");
        environment
            .server
            .publish("0.0.11", target, &fake_mosaic_script("0.0.11"));
        let output = environment.run_default();
        assert!(
            output.status.success(),
            "{system} {machine}: {}",
            text(&output)
        );
        assert!(environment.install_directory().join("mosaic").is_file());
    }
}

#[test]
fn a_pinned_version_is_installed_with_or_without_the_v_prefix() {
    for pinned in ["v0.0.10", "0.0.10"] {
        let environment = Environment::new();
        environment.server.set_latest("0.0.11");
        for version in ["0.0.10", "0.0.11"] {
            environment.server.publish(
                version,
                "x86_64-unknown-linux-musl",
                &fake_mosaic_script(version),
            );
        }
        let output = environment.run(&[
            ("MOSAIC_VERSION", pinned),
            (
                "MOSAIC_INSTALL_DIR",
                environment.install_directory().to_str().unwrap(),
            ),
        ]);
        assert!(output.status.success(), "{}", text(&output));
        let version = Command::new(environment.install_directory().join("mosaic"))
            .arg("--version")
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&version.stdout), "mosaic 0.0.10\n");
        assert!(!environment
            .server
            .requests()
            .iter()
            .any(|path| path == "/releases/latest"));
    }
}

#[test]
fn an_invalid_version_is_refused_before_any_download() {
    let invalid_versions = [
        "v1.2.3/../x",
        "v1.2.3\n../x",
        "v1.2.3\n",
        "\nv1.2.3",
        "v1.2",
        "v1.2.3.4",
        "v1..3",
        "v.1.2",
        "v1.2.",
        "vv1.2.3",
        "v",
        "v1.2.3-rc1",
    ];
    for invalid in invalid_versions {
        let environment = Environment::new();
        let output = environment.run(&[
            ("MOSAIC_VERSION", invalid),
            (
                "MOSAIC_INSTALL_DIR",
                environment.install_directory().to_str().unwrap(),
            ),
        ]);
        assert_eq!(output.status.code(), Some(1), "{invalid:?}");
        assert!(
            text(&output).contains("invalid version"),
            "{invalid:?}: {}",
            text(&output)
        );
        assert!(environment.server.requests().is_empty(), "{invalid:?}");
    }
}

#[test]
fn a_wrong_checksum_aborts_without_touching_the_installed_binary() {
    let environment = Environment::new();
    environment.server.set_latest("0.0.11");
    environment.server.publish_with_checksum(
        "0.0.11",
        "x86_64-unknown-linux-musl",
        &fake_mosaic_script("0.0.11"),
        Some(&"0".repeat(64)),
    );
    fs::create_dir_all(environment.install_directory()).unwrap();
    write_executable(
        &environment.install_directory().join("mosaic"),
        "#!/bin/sh\necho antigo\n",
    );
    let output = environment.run_default();
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output).contains("checksum"), "{}", text(&output));
    assert_eq!(
        fs::read_to_string(environment.install_directory().join("mosaic")).unwrap(),
        "#!/bin/sh\necho antigo\n"
    );
    assert_eq!(leftovers(&environment.install_directory()), ["mosaic"]);
}

#[test]
fn unsupported_systems_and_architectures_fail_clearly() {
    for (system, machine, expected) in [
        ("FreeBSD", "x86_64", "unsupported system: FreeBSD"),
        ("Linux", "riscv64", "unsupported architecture: riscv64"),
        ("Darwin", "ppc", "unsupported architecture: ppc"),
    ] {
        let mut environment = Environment::new();
        environment.system = system;
        environment.machine = machine;
        let output = environment.run_default();
        assert_eq!(output.status.code(), Some(1), "{system}");
        assert!(text(&output).contains(expected), "{}", text(&output));
        assert!(!environment.install_directory().exists());
    }
}

#[test]
fn a_missing_release_file_fails_without_installing() {
    let environment = Environment::new();
    environment.server.set_latest("0.0.11");
    let output = environment.run_default();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output).contains("could not download"),
        "{}",
        text(&output)
    );
    assert!(!environment.install_directory().join("mosaic").exists());
}

#[test]
fn wget_is_used_when_curl_is_missing() {
    if find_on_host_path("wget").is_none() {
        return;
    }
    let mut environment = Environment::new();
    environment.downloader = "wget";
    environment.server.set_latest("0.0.11");
    environment.server.publish(
        "0.0.11",
        "x86_64-unknown-linux-musl",
        &fake_mosaic_script("0.0.11"),
    );
    let output = environment.run_default();
    assert!(output.status.success(), "{}", text(&output));
    assert!(environment.install_directory().join("mosaic").is_file());
}

#[test]
fn without_a_downloader_or_a_checksum_tool_it_aborts_without_installing() {
    let mut environment = Environment::new();
    environment.downloader = "not-a-downloader";
    let output = environment.run_default();
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output).contains("curl or wget"), "{}", text(&output));

    let mut environment = Environment::new();
    environment.checksum_tools = Vec::new();
    environment.server.set_latest("0.0.11");
    environment.server.publish(
        "0.0.11",
        "x86_64-unknown-linux-musl",
        &fake_mosaic_script("0.0.11"),
    );
    let output = environment.run_default();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output).contains("sha256sum or shasum"),
        "{}",
        text(&output)
    );
    assert!(!environment.install_directory().join("mosaic").exists());
}

#[test]
fn the_path_hint_appears_only_when_the_directory_is_outside_the_path() {
    let environment = Environment::new();
    environment.server.set_latest("0.0.11");
    environment.server.publish(
        "0.0.11",
        "x86_64-unknown-linux-musl",
        &fake_mosaic_script("0.0.11"),
    );
    let output = environment.run_default();
    let expected_line = format!(
        "export PATH=\"{}:$PATH\"",
        environment.install_directory().display()
    );
    assert!(text(&output).contains(&expected_line), "{}", text(&output));

    let mut environment = environment;
    environment.extra_path = Some(environment.install_directory());
    let output = environment.run_default();
    assert!(output.status.success(), "{}", text(&output));
    assert!(!text(&output).contains("export PATH"), "{}", text(&output));
}

#[test]
fn the_default_destination_is_inside_home_and_no_sudo_is_involved() {
    let environment = Environment::new();
    environment.server.set_latest("0.0.11");
    environment.server.publish(
        "0.0.11",
        "x86_64-unknown-linux-musl",
        &fake_mosaic_script("0.0.11"),
    );
    let output = environment.run(&[]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(environment.path("home/.local/bin/mosaic").is_file());
    let script = fs::read_to_string(INSTALL_SCRIPT).unwrap();
    assert!(!script.contains("sudo "));
}
