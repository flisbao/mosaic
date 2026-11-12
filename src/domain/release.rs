use std::path::{Component, Path, PathBuf};

use crate::domain::plugin_package::{PluginPackageError, PluginVersion};

pub type ReleaseVersion = PluginVersion;

const EXECUTABLE_NAME: &str = "mosaic";
const TAG_PREFIX: char = 'v';
const SHA256_HEX_DIGITS: usize = 64;
const HOMEBREW_CELLAR_DIRECTORY: &str = "Cellar";
const CARGO_BUILD_DIRECTORY: &str = "target";
const CARGO_PROFILE_DIRECTORIES: [&str; 2] = ["debug", "release"];
const CARGO_BIN_DIRECTORY: &str = "bin";
const DEBIAN_ARCHITECTURE: &str = "amd64";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseTarget {
    LinuxX86_64,
    LinuxAarch64,
    MacosAarch64,
    MacosX86_64,
}

impl ReleaseTarget {
    pub fn detect(operating_system: &str, architecture: &str) -> Option<Self> {
        match (operating_system, architecture) {
            ("linux", "x86_64") => Some(Self::LinuxX86_64),
            ("linux", "aarch64") => Some(Self::LinuxAarch64),
            ("macos", "aarch64") => Some(Self::MacosAarch64),
            ("macos", "x86_64") => Some(Self::MacosX86_64),
            _ => None,
        }
    }

    pub fn triple(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "x86_64-unknown-linux-musl",
            Self::LinuxAarch64 => "aarch64-unknown-linux-musl",
            Self::MacosAarch64 => "aarch64-apple-darwin",
            Self::MacosX86_64 => "x86_64-apple-darwin",
        }
    }

    pub fn has_debian_package(self) -> bool {
        self == Self::LinuxX86_64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAssets {
    pub version: ReleaseVersion,
    pub target: ReleaseTarget,
}

impl ReleaseAssets {
    pub fn tag(&self) -> String {
        format!("{TAG_PREFIX}{}", self.version)
    }

    pub fn package_name(&self) -> String {
        format!("{EXECUTABLE_NAME}-{}-{}", self.tag(), self.target.triple())
    }

    pub fn archive_name(&self) -> String {
        format!("{}.tar.gz", self.package_name())
    }

    pub fn checksum_name(&self) -> String {
        format!("{}.sha256", self.archive_name())
    }

    pub fn executable_path_in_archive(&self) -> PathBuf {
        Path::new(&self.package_name()).join(EXECUTABLE_NAME)
    }
}

pub fn debian_package_name(version: &ReleaseVersion) -> String {
    format!("{EXECUTABLE_NAME}_{version}_{DEBIAN_ARCHITECTURE}.deb")
}

pub fn version_from_release_location(location: &str) -> Result<ReleaseVersion, PluginPackageError> {
    let last_segment = location
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    ReleaseVersion::parse(
        last_segment
            .strip_prefix(TAG_PREFIX)
            .unwrap_or(last_segment),
    )
}

pub fn expected_checksum(checksum_file: &str) -> Option<String> {
    let first_word = checksum_file.split_whitespace().next()?;
    (first_word.len() == SHA256_HEX_DIGITS
        && first_word.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then(|| first_word.to_ascii_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    UpToDate,
    Available { latest: ReleaseVersion },
}

impl UpdateStatus {
    pub fn compare(current: &ReleaseVersion, latest: ReleaseVersion) -> Self {
        if latest > *current {
            Self::Available { latest }
        } else {
            Self::UpToDate
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    DebianPackage,
    Homebrew,
    Cargo,
    SelfManaged,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallFacts {
    pub owned_by_debian_package: bool,
    pub homebrew_prefix: Option<PathBuf>,
    pub cargo_home: Option<PathBuf>,
}

impl InstallMethod {
    pub fn classify(executable: &Path, facts: &InstallFacts) -> Self {
        if is_homebrew_path(executable, facts.homebrew_prefix.as_deref()) {
            Self::Homebrew
        } else if facts.owned_by_debian_package {
            Self::DebianPackage
        } else if is_cargo_path(executable, facts.cargo_home.as_deref()) {
            Self::Cargo
        } else {
            Self::SelfManaged
        }
    }

    pub fn may_be_system_package(executable: &Path) -> bool {
        executable.starts_with("/usr") || executable.starts_with("/bin")
    }
}

fn is_homebrew_path(executable: &Path, prefix: Option<&Path>) -> bool {
    executable
        .components()
        .any(|component| component == Component::Normal(HOMEBREW_CELLAR_DIRECTORY.as_ref()))
        || prefix.is_some_and(|prefix| executable.starts_with(prefix))
}

fn is_cargo_path(executable: &Path, cargo_home: Option<&Path>) -> bool {
    if cargo_home.is_some_and(|home| executable.starts_with(home.join(CARGO_BIN_DIRECTORY))) {
        return true;
    }
    let names: Vec<_> = executable
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    names
        .iter()
        .position(|name| *name == CARGO_BUILD_DIRECTORY)
        .is_some_and(|position| {
            names[position + 1..]
                .iter()
                .any(|name| CARGO_PROFILE_DIRECTORIES.contains(name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> ReleaseVersion {
        ReleaseVersion::parse(text).unwrap()
    }

    #[test]
    fn linux_always_maps_to_the_musl_tarball_and_macos_to_darwin() {
        let triple = |system, architecture| {
            ReleaseTarget::detect(system, architecture).map(ReleaseTarget::triple)
        };
        assert_eq!(triple("linux", "x86_64"), Some("x86_64-unknown-linux-musl"));
        assert_eq!(
            triple("linux", "aarch64"),
            Some("aarch64-unknown-linux-musl")
        );
        assert_eq!(triple("macos", "aarch64"), Some("aarch64-apple-darwin"));
        assert_eq!(triple("macos", "x86_64"), Some("x86_64-apple-darwin"));
        assert_eq!(triple("windows", "x86_64"), None);
        assert_eq!(triple("linux", "riscv64"), None);
    }

    #[test]
    fn asset_names_follow_the_release_layout() {
        let assets = ReleaseAssets {
            version: version("0.0.11"),
            target: ReleaseTarget::LinuxX86_64,
        };
        assert_eq!(assets.tag(), "v0.0.11");
        assert_eq!(
            assets.archive_name(),
            "mosaic-v0.0.11-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            assets.checksum_name(),
            "mosaic-v0.0.11-x86_64-unknown-linux-musl.tar.gz.sha256"
        );
        assert_eq!(
            assets.executable_path_in_archive(),
            Path::new("mosaic-v0.0.11-x86_64-unknown-linux-musl/mosaic")
        );
        assert_eq!(
            debian_package_name(&version("0.0.11")),
            "mosaic_0.0.11_amd64.deb"
        );
    }

    #[test]
    fn only_x86_64_linux_has_a_debian_package() {
        assert!(ReleaseTarget::LinuxX86_64.has_debian_package());
        assert!(!ReleaseTarget::LinuxAarch64.has_debian_package());
        assert!(!ReleaseTarget::MacosAarch64.has_debian_package());
    }

    #[test]
    fn the_version_comes_from_the_last_segment_of_the_release_location() {
        for location in [
            "https://github.com/flisbao/mosaic/releases/tag/v0.0.11",
            "https://github.com/flisbao/mosaic/releases/tag/v0.0.11/",
            "http://127.0.0.1:1/releases/tag/v0.0.11?x=1",
            "/releases/tag/0.0.11",
        ] {
            assert_eq!(
                version_from_release_location(location),
                Ok(version("0.0.11"))
            );
        }
        for location in [
            "https://github.com/flisbao/mosaic/releases",
            "https://github.com/flisbao/mosaic/releases/tag/v0.0",
            "https://github.com/flisbao/mosaic/releases/tag/v0.0.11-beta",
            "",
        ] {
            assert!(
                version_from_release_location(location).is_err(),
                "{location}"
            );
        }
    }

    #[test]
    fn the_expected_checksum_is_the_first_word_in_lowercase() {
        let digest = "AB".repeat(32);
        assert_eq!(
            expected_checksum(&format!("{digest}  mosaic-v0.0.11.tar.gz\n")),
            Some(digest.to_lowercase())
        );
        assert_eq!(expected_checksum(""), None);
        assert_eq!(expected_checksum("abc  file"), None);
        assert_eq!(expected_checksum(&format!("{}g", "a".repeat(63))), None);
    }

    #[test]
    fn only_a_strictly_newer_version_is_available() {
        let current = version("0.0.10");
        assert_eq!(
            UpdateStatus::compare(&current, version("0.0.11")),
            UpdateStatus::Available {
                latest: version("0.0.11")
            }
        );
        assert_eq!(
            UpdateStatus::compare(&current, version("0.0.10")),
            UpdateStatus::UpToDate
        );
        assert_eq!(
            UpdateStatus::compare(&current, version("0.0.9")),
            UpdateStatus::UpToDate
        );
        assert_eq!(
            UpdateStatus::compare(&version("0.0.9"), version("0.0.10")),
            UpdateStatus::Available {
                latest: version("0.0.10")
            }
        );
    }

    fn classify(path: &str, facts: &InstallFacts) -> InstallMethod {
        InstallMethod::classify(Path::new(path), facts)
    }

    #[test]
    fn a_debian_package_is_recognised_only_when_the_package_manager_owns_the_file() {
        let owned = InstallFacts {
            owned_by_debian_package: true,
            ..InstallFacts::default()
        };
        assert_eq!(
            classify("/usr/bin/mosaic", &owned),
            InstallMethod::DebianPackage
        );
        assert_eq!(
            classify("/usr/bin/mosaic", &InstallFacts::default()),
            InstallMethod::SelfManaged
        );
        assert_eq!(
            classify("/usr/local/bin/mosaic", &InstallFacts::default()),
            InstallMethod::SelfManaged
        );
    }

    #[test]
    fn homebrew_is_recognised_by_the_cellar_or_the_prefix() {
        let facts = InstallFacts {
            homebrew_prefix: Some(PathBuf::from("/opt/homebrew")),
            ..InstallFacts::default()
        };
        assert_eq!(
            classify(
                "/opt/homebrew/Cellar/mosaic/0.0.11/bin/mosaic",
                &InstallFacts::default()
            ),
            InstallMethod::Homebrew
        );
        assert_eq!(
            classify("/opt/homebrew/bin/mosaic", &facts),
            InstallMethod::Homebrew
        );
        assert_eq!(
            classify("/opt/other/bin/mosaic", &facts),
            InstallMethod::SelfManaged
        );
    }

    #[test]
    fn cargo_installs_and_checkout_builds_are_recognised() {
        let facts = InstallFacts {
            cargo_home: Some(PathBuf::from("/home/ana/.cargo")),
            ..InstallFacts::default()
        };
        for path in [
            "/home/ana/.cargo/bin/mosaic",
            "/home/ana/projetos/mosaic/target/release/mosaic",
            "/home/ana/projetos/mosaic/target/x86_64-unknown-linux-gnu/debug/mosaic",
        ] {
            assert_eq!(classify(path, &facts), InstallMethod::Cargo, "{path}");
        }
        for path in ["/home/ana/target/mosaic", "/home/ana/.local/bin/mosaic"] {
            assert_eq!(classify(path, &facts), InstallMethod::SelfManaged, "{path}");
        }
    }

    #[test]
    fn only_paths_under_usr_or_bin_are_probed_for_a_system_package() {
        assert!(InstallMethod::may_be_system_package(Path::new(
            "/usr/bin/mosaic"
        )));
        assert!(InstallMethod::may_be_system_package(Path::new(
            "/bin/mosaic"
        )));
        assert!(!InstallMethod::may_be_system_package(Path::new(
            "/home/ana/.local/bin/mosaic"
        )));
    }
}
