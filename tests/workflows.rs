#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

fn workflow(name: &str) -> String {
    fs::read_to_string(format!(
        "{}/.github/workflows/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn the_release_builds_the_portable_linux_targets_and_the_macos_ones() {
    let release = workflow("release.yml");
    for expected in [
        "target: x86_64-unknown-linux-gnu",
        "target: x86_64-unknown-linux-musl",
        "target: aarch64-unknown-linux-musl",
        "target: aarch64-apple-darwin",
        "target: x86_64-apple-darwin",
        "os: ubuntu-24.04-arm",
        "musl-tools",
    ] {
        assert!(release.contains(expected), "{expected}");
    }
}

#[test]
fn the_release_publishes_a_single_checksums_file_and_the_homebrew_formula() {
    let release = workflow("release.yml");
    for expected in [
        "SHA256SUMS",
        "scripts/render-homebrew-formula.sh",
        "dist/mosaic.rb",
    ] {
        assert!(release.contains(expected), "{expected}");
    }
    let checksums = release.find("SHA256SUMS").unwrap();
    let publication = release.find("gh release create").unwrap();
    assert!(checksums < publication);
}

#[test]
fn the_release_keeps_the_tarball_names_and_the_deb_only_for_gnu() {
    let release = workflow("release.yml");
    assert!(release.contains("name=\"mosaic-${GITHUB_REF_NAME}-${{ matrix.target }}\""));
    assert!(release.contains("if: matrix.target == 'x86_64-unknown-linux-gnu'"));
}

#[test]
fn the_ci_runs_the_tests_on_macos_and_shellcheck_on_linux() {
    let ci = workflow("ci.yml");
    assert!(ci.contains("runs-on: macos-15"));
    assert!(ci.contains("shellcheck install.sh scripts/*.sh"));
    let macos = &ci[ci.find("runs-on: macos-15").unwrap()..];
    assert!(macos.contains("cargo test"));
    assert!(!macos.contains("cargo fmt"));
    assert!(!macos.contains("cargo clippy"));
}
