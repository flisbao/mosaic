use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::application::ports::{ExecutableReplacer, InstallationProbe, ReleaseError};
use crate::domain::language::Language;
use crate::infrastructure::external_tool::external_command;
use crate::infrastructure::process::{run_process, CancelFlag, ProcessLimits};

const DPKG_PROGRAM: &str = "dpkg";
const DPKG_TIMEOUT: Duration = Duration::from_secs(10);
const MAXIMUM_DPKG_OUTPUT_BYTES: usize = 65_536;
const CARGO_HOME_VARIABLE: &str = "CARGO_HOME";
const DEFAULT_CARGO_DIRECTORY: &str = ".cargo";
const EXECUTABLE_MODE: u32 = 0o755;

#[derive(Default)]
pub struct SystemInstallationProbe {
    language: Language,
}

impl SystemInstallationProbe {
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }
}

impl InstallationProbe for SystemInstallationProbe {
    fn executable_path(&self) -> Result<PathBuf, ReleaseError> {
        std::env::current_exe()
            .and_then(std::fs::canonicalize)
            .map_err(|error| {
                ReleaseError::Failed(self.language.pick(
                    format!("could not find where mosaic is installed: {error}"),
                    format!("não descobri onde o mosaic está instalado: {error}"),
                ))
            })
    }

    fn owned_by_debian_package(&self, executable: &Path) -> bool {
        let mut command = external_command(OsStr::new(DPKG_PROGRAM));
        command.arg("-S").arg("--").arg(executable);
        let limits = ProcessLimits {
            timeout: DPKG_TIMEOUT,
            maximum_output_bytes: MAXIMUM_DPKG_OUTPUT_BYTES,
        };
        run_process(command, None, &limits, &CancelFlag::default()).is_ok()
    }

    fn cargo_home(&self) -> Option<PathBuf> {
        cargo_home_from(std::env::var_os(CARGO_HOME_VARIABLE), dirs::home_dir())
    }
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn cargo_home_from(variable: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    non_empty_path(variable).or_else(|| home.map(|home| home.join(DEFAULT_CARGO_DIRECTORY)))
}

#[derive(Default)]
pub struct AtomicExecutableReplacer {
    language: Language,
}

impl AtomicExecutableReplacer {
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }
}

impl ExecutableReplacer for AtomicExecutableReplacer {
    fn replace(
        &self,
        new_executable: &Path,
        current_executable: &Path,
    ) -> Result<(), ReleaseError> {
        if !new_executable.is_file() {
            return Err(ReleaseError::Failed(self.language.pick(
                format!("the package does not contain {}", new_executable.display()),
                format!("o pacote não contém {}", new_executable.display()),
            )));
        }
        make_executable(new_executable, self.language)?;
        std::fs::rename(new_executable, current_executable).map_err(|error| {
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                ReleaseError::NotWritable(self.language.pick(
                    format!("no permission to write to {}", current_executable.display()),
                    format!(
                        "sem permissão para escrever em {}",
                        current_executable.display()
                    ),
                ))
            } else {
                ReleaseError::Failed(self.language.pick(
                    format!(
                        "could not replace {}: {error}",
                        current_executable.display()
                    ),
                    format!(
                        "não consegui trocar {}: {error}",
                        current_executable.display()
                    ),
                ))
            }
        })
    }
}

#[cfg(unix)]
fn make_executable(path: &Path, language: Language) -> Result<(), ReleaseError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(EXECUTABLE_MODE)).map_err(
        |error| {
            ReleaseError::Failed(language.pick(
                format!("could not mark as executable: {error}"),
                format!("não consegui marcar como executável: {error}"),
            ))
        },
    )
}

#[cfg(not(unix))]
fn make_executable(_path: &Path, _language: Language) -> Result<(), ReleaseError> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_executable_path_is_canonical_and_absolute() {
        let path = SystemInstallationProbe::default()
            .executable_path()
            .unwrap();
        assert!(path.is_absolute());
        assert_eq!(path, fs::canonicalize(&path).unwrap());
    }

    #[test]
    fn a_temporary_file_is_not_owned_by_a_debian_package() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(!SystemInstallationProbe::default().owned_by_debian_package(file.path()));
    }

    #[test]
    fn cargo_home_prefers_the_variable_then_the_home_directory() {
        assert_eq!(
            cargo_home_from(Some("/c".into()), Some("/home/ana".into())),
            Some(PathBuf::from("/c"))
        );
        assert_eq!(
            cargo_home_from(Some("".into()), Some("/home/ana".into())),
            Some(PathBuf::from("/home/ana/.cargo"))
        );
        assert_eq!(cargo_home_from(None, None), None);
    }

    #[test]
    fn the_replacement_swaps_the_file_atomically_and_keeps_it_executable() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("mosaic");
        fs::write(&current, "antigo").unwrap();
        let staged = directory.path().join(".stage-mosaic");
        fs::write(&staged, "novo").unwrap();
        AtomicExecutableReplacer::default()
            .replace(&staged, &current)
            .unwrap();
        assert_eq!(fs::read_to_string(&current).unwrap(), "novo");
        assert_eq!(
            fs::metadata(&current).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(!staged.exists());
    }

    #[test]
    fn a_missing_new_executable_leaves_the_current_one_alone() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("mosaic");
        fs::write(&current, "antigo").unwrap();
        let error = AtomicExecutableReplacer::default()
            .replace(&directory.path().join("nao-existe"), &current)
            .unwrap_err();
        assert!(matches!(error, ReleaseError::Failed(_)));
        assert_eq!(fs::read_to_string(&current).unwrap(), "antigo");
    }

    #[test]
    fn a_read_only_directory_is_reported_as_not_writable() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let staging = tempfile::tempdir().unwrap();
        let staged = staging.path().join("novo");
        fs::write(&staged, "novo").unwrap();
        let locked = tempfile::tempdir().unwrap();
        let current = locked.path().join("mosaic");
        fs::write(&current, "antigo").unwrap();
        fs::set_permissions(locked.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let result = AtomicExecutableReplacer::default().replace(&staged, &current);
        fs::set_permissions(locked.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(result, Err(ReleaseError::NotWritable(_))));
        assert_eq!(fs::read_to_string(&current).unwrap(), "antigo");
    }
}
