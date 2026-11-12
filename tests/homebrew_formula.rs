#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const RENDER_SCRIPT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/scripts/render-homebrew-formula.sh"
);

const TARGETS: [(&str, char); 4] = [
    ("aarch64-apple-darwin", 'a'),
    ("x86_64-apple-darwin", 'b'),
    ("aarch64-unknown-linux-musl", 'c'),
    ("x86_64-unknown-linux-musl", 'd'),
];

fn checksums_text(version: &str, skip: Option<&str>) -> String {
    let mut text = String::new();
    for (target, digit) in TARGETS {
        if Some(target) == skip {
            continue;
        }
        text.push_str(&format!(
            "{}  mosaic-v{version}-{target}.tar.gz\n",
            digit.to_string().repeat(64)
        ));
    }
    text.push_str(&format!(
        "{}  mosaic-v{version}-x86_64-unknown-linux-gnu.tar.gz\n",
        "e".repeat(64)
    ));
    text
}

fn render(version: &str, checksums: &str) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("SHA256SUMS");
    fs::write(&file, checksums).unwrap();
    render_file(version, &file)
}

fn render_file(version: &str, file: &Path) -> Output {
    Command::new("sh")
        .arg(RENDER_SCRIPT)
        .arg(version)
        .arg(file)
        .output()
        .unwrap()
}

fn expected_formula(version: &str) -> String {
    let url = |target: &str| {
        format!("https://github.com/flisbao/mosaic/releases/download/v{version}/mosaic-v{version}-{target}.tar.gz")
    };
    let block = |target: &str, digit: char| {
        format!(
            "      url \"{}\"\n      sha256 \"{}\"\n",
            url(target),
            digit.to_string().repeat(64)
        )
    };
    format!(
        "class Mosaic < Formula
  desc \"Terminal dashboard built from pluggable widgets\"
  homepage \"https://github.com/flisbao/mosaic\"
  version \"{version}\"
  license \"MIT\"

  on_macos do
    on_arm do
{}    end
    on_intel do
{}    end
  end

  on_linux do
    on_arm do
{}    end
    on_intel do
{}    end
  end

  def install
    bin.install \"mosaic\"
    doc.install \"README.md\", \"PROTOCOL.md\"
    pkgshare.install \"schema\"
  end

  test do
    assert_match version.to_s, shell_output(\"#{{bin}}/mosaic --version\")
  end
end
",
        block(TARGETS[0].0, TARGETS[0].1),
        block(TARGETS[1].0, TARGETS[1].1),
        block(TARGETS[2].0, TARGETS[2].1),
        block(TARGETS[3].0, TARGETS[3].1),
    )
}

#[test]
fn renders_the_formula_from_the_checksums() {
    let output = render("0.0.11", &checksums_text("0.0.11", None));
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        expected_formula("0.0.11")
    );
}

#[test]
fn a_leading_v_in_the_version_is_accepted() {
    let output = render("v0.0.11", &checksums_text("0.0.11", None));
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        expected_formula("0.0.11")
    );
}

#[test]
fn a_missing_target_fails_naming_it_and_prints_no_formula() {
    let output = render(
        "0.0.11",
        &checksums_text("0.0.11", Some("aarch64-apple-darwin")),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("aarch64-apple-darwin"));
}

#[test]
fn checksums_for_another_version_do_not_count() {
    let output = render("0.0.12", &checksums_text("0.0.11", None));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn a_malformed_hash_or_version_and_a_missing_file_are_errors() {
    let broken = checksums_text("0.0.11", None).replacen(&"a".repeat(64), "abc", 1);
    assert_eq!(render("0.0.11", &broken).status.code(), Some(1));
    assert_eq!(
        render("0.0.11; rm", &checksums_text("0.0.11", None))
            .status
            .code(),
        Some(1)
    );
    let missing = render_file("0.0.11", Path::new("/nonexistent/SHA256SUMS"));
    assert_eq!(missing.status.code(), Some(1));
    let no_arguments = Command::new("sh").arg(RENDER_SCRIPT).output().unwrap();
    assert_eq!(no_arguments.status.code(), Some(2));
}
