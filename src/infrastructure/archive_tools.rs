use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use crate::application::ports::{ArchiveExtractor, ChecksumCalculator, ReleaseError};
use crate::domain::language::Language;
use crate::domain::release::expected_checksum;
use crate::infrastructure::external_tool::{external_command, release_error};
use crate::infrastructure::process::{run_process, CancelFlag, ProcessError, ProcessLimits};

const CHECKSUM_TIMEOUT: Duration = Duration::from_secs(60);
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(120);
const MAXIMUM_TOOL_OUTPUT_BYTES: usize = 65_536;
const SHA256SUM_PROGRAM: &str = "sha256sum";
const SHASUM_PROGRAM: &str = "shasum";
const SHASUM_ALGORITHM_ARGUMENTS: [&str; 2] = ["-a", "256"];
const TAR_PROGRAM: &str = "tar";

#[derive(Default)]
pub struct ShaSumChecksum {
    language: Language,
}

impl ShaSumChecksum {
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    fn run(program: &str, arguments: &[&str], file: &Path) -> Result<String, ProcessError> {
        let mut command = external_command(OsStr::new(program));
        command.args(arguments).arg("--").arg(file);
        let limits = ProcessLimits {
            timeout: CHECKSUM_TIMEOUT,
            maximum_output_bytes: MAXIMUM_TOOL_OUTPUT_BYTES,
        };
        run_process(command, None, &limits, &CancelFlag::default())
    }
}

impl ChecksumCalculator for ShaSumChecksum {
    fn sha256(&self, file: &Path) -> Result<String, ReleaseError> {
        let output = match Self::run(SHA256SUM_PROGRAM, &[], file) {
            Err(error) if error.program_not_found() => {
                Self::run(SHASUM_PROGRAM, &SHASUM_ALGORITHM_ARGUMENTS, file)
                    .map_err(|error| release_error(SHASUM_PROGRAM, error, self.language))?
            }
            other => {
                other.map_err(|error| release_error(SHA256SUM_PROGRAM, error, self.language))?
            }
        };
        expected_checksum(&output).ok_or_else(|| {
            ReleaseError::Failed(
                self.language
                    .pick(
                        "could not compute the SHA-256",
                        "não consegui calcular o SHA-256",
                    )
                    .to_string(),
            )
        })
    }
}

#[derive(Default)]
pub struct TarExtractor {
    language: Language,
}

impl TarExtractor {
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }
}

impl ArchiveExtractor for TarExtractor {
    fn extract(&self, archive: &Path, destination: &Path) -> Result<(), ReleaseError> {
        std::fs::create_dir_all(destination).map_err(|error| {
            ReleaseError::Failed(self.language.pick(
                format!("could not create the folder: {error}"),
                format!("não consegui criar a pasta: {error}"),
            ))
        })?;
        let mut command = external_command(OsStr::new(TAR_PROGRAM));
        command.arg("-xzf").arg(archive).arg("-C").arg(destination);
        let limits = ProcessLimits {
            timeout: EXTRACTION_TIMEOUT,
            maximum_output_bytes: MAXIMUM_TOOL_OUTPUT_BYTES,
        };
        run_process(command, None, &limits, &CancelFlag::default())
            .map(|_| ())
            .map_err(|error| release_error(TAR_PROGRAM, error, self.language))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn the_checksum_of_a_known_file_is_computed_by_an_external_tool() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("abc");
        fs::write(&file, "abc").unwrap();
        assert_eq!(
            ShaSumChecksum::default().sha256(&file).as_deref(),
            Ok(ABC_SHA256)
        );
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_checksum() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            ShaSumChecksum::default().sha256(&directory.path().join("missing")),
            Err(ReleaseError::Failed(_))
        ));
    }

    #[test]
    fn a_tarball_is_extracted_into_the_destination() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source/pacote");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("mosaic"), "binario").unwrap();
        let archive = directory.path().join("pacote.tar.gz");
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(directory.path().join("source"))
            .arg("pacote")
            .status()
            .unwrap();
        assert!(status.success());
        let destination = directory.path().join("out");
        TarExtractor::default()
            .extract(&archive, &destination)
            .unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("pacote/mosaic")).unwrap(),
            "binario"
        );
    }

    #[test]
    fn a_corrupt_archive_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("ruim.tar.gz");
        fs::write(&archive, "não é um tarball").unwrap();
        assert!(matches!(
            TarExtractor::default().extract(&archive, &directory.path().join("out")),
            Err(ReleaseError::Failed(_))
        ));
    }
}
