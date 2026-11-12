use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

use crate::application::ports::{ReleaseError, ReleaseSource};
use crate::domain::language::Language;
use crate::domain::release::{version_from_release_location, ReleaseVersion};
use crate::infrastructure::external_tool::{external_command, release_error};
use crate::infrastructure::process::{run_process, CancelFlag, HasCancelFlag, ProcessLimits};

pub const DEFAULT_DOWNLOAD_BASE: &str = "https://github.com/flisbao/mosaic/releases";
pub const DOWNLOAD_BASE_VARIABLE: &str = "MOSAIC_DOWNLOAD_BASE";
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(15);
const ARCHIVE_TIMEOUT: Duration = Duration::from_secs(300);
const MAXIMUM_REDIRECTIONS: &str = "5";
const MAXIMUM_TEXT_BYTES: usize = 4_096;
const MAXIMUM_ARCHIVE_BYTES: usize = 104_857_600;
const NO_OUTPUT_BYTES: usize = 1_048_576;
const CURL_PROGRAM: &str = "curl";

pub fn download_base_from_environment() -> Option<String> {
    std::env::var(DOWNLOAD_BASE_VARIABLE)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub struct CurlReleaseSource {
    program: OsString,
    base: String,
    restrict_to_https: bool,
    cancel: CancelFlag,
    language: Language,
}

impl CurlReleaseSource {
    pub fn new(download_base: Option<String>) -> Self {
        let restrict_to_https = download_base.is_none();
        Self {
            program: OsString::from(CURL_PROGRAM),
            base: download_base
                .unwrap_or_else(|| DEFAULT_DOWNLOAD_BASE.to_string())
                .trim_end_matches('/')
                .to_string(),
            restrict_to_https,
            cancel: CancelFlag::default(),
            language: Language::default(),
        }
    }

    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    #[cfg(test)]
    fn with_program(mut self, program: &OsStr) -> Self {
        self.program = program.to_os_string();
        self
    }

    fn release_url(&self, version: &ReleaseVersion, file_name: &str) -> String {
        format!("{}/download/v{version}/{file_name}", self.base)
    }

    fn curl_arguments(&self, timeout: Duration, url: &str) -> Vec<OsString> {
        let mut arguments: Vec<OsString> =
            ["-q", "--fail", "--silent", "--show-error", "--location"]
                .into_iter()
                .map(OsString::from)
                .collect();
        arguments.extend(
            ["--max-redirs", MAXIMUM_REDIRECTIONS, "--max-time"]
                .into_iter()
                .map(OsString::from),
        );
        arguments.push(timeout.as_secs().to_string().into());
        if self.restrict_to_https {
            arguments.extend(
                ["--proto", "=https", "--proto-redir", "=https", "--tlsv1.2"]
                    .into_iter()
                    .map(OsString::from),
            );
        }
        arguments.push(OsString::from("--"));
        arguments.push(url.into());
        arguments
    }

    fn run(
        &self,
        extra_arguments: &[&OsStr],
        timeout: Duration,
        url: &str,
        maximum_output_bytes: usize,
    ) -> Result<String, ReleaseError> {
        let mut command = external_command(&self.program);
        let mut arguments = self.curl_arguments(timeout, url);
        let separator = arguments.len() - 2;
        for (offset, extra) in extra_arguments.iter().enumerate() {
            arguments.insert(separator + offset, (*extra).to_os_string());
        }
        command.args(arguments);
        let limits = ProcessLimits {
            timeout,
            maximum_output_bytes,
        };
        run_process(command, None, &limits, &self.cancel)
            .map_err(|error| release_error(CURL_PROGRAM, error, self.language))
    }
}

impl HasCancelFlag for CurlReleaseSource {
    fn cancel_flag(&self) -> &CancelFlag {
        &self.cancel
    }
}

impl ReleaseSource for CurlReleaseSource {
    fn latest_version(&self) -> Result<ReleaseVersion, ReleaseError> {
        let url = format!("{}/latest", self.base);
        let location = self.run(
            &[
                OsStr::new("--output"),
                OsStr::new("/dev/null"),
                OsStr::new("--write-out"),
                OsStr::new("%{url_effective}"),
            ],
            LOOKUP_TIMEOUT,
            &url,
            MAXIMUM_TEXT_BYTES,
        )?;
        version_from_release_location(location.trim()).map_err(|_| {
            ReleaseError::Failed(
                self.language
                    .pick(
                        "could not find the latest version",
                        "não consegui descobrir a versão mais recente",
                    )
                    .to_string(),
            )
        })
    }

    fn download_text(
        &self,
        version: &ReleaseVersion,
        file_name: &str,
    ) -> Result<String, ReleaseError> {
        self.run(
            &[],
            LOOKUP_TIMEOUT,
            &self.release_url(version, file_name),
            MAXIMUM_TEXT_BYTES,
        )
    }

    fn download_file(
        &self,
        version: &ReleaseVersion,
        file_name: &str,
        destination: &Path,
    ) -> Result<(), ReleaseError> {
        self.run(
            &[
                OsStr::new("--output"),
                destination.as_os_str(),
                OsStr::new("--max-filesize"),
                OsStr::new(&MAXIMUM_ARCHIVE_BYTES.to_string()),
            ],
            ARCHIVE_TIMEOUT,
            &self.release_url(version, file_name),
            NO_OUTPUT_BYTES,
        )
        .map(|_| ())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::application::ports::Cancellable;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn version() -> ReleaseVersion {
        ReleaseVersion::parse("0.0.11").unwrap()
    }

    fn fake_curl(directory: &Path, script: &str) -> std::path::PathBuf {
        let program = directory.join("curl");
        fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    const TEXT_FILE_BUSY: &str = "Text file busy";

    fn retry_while_busy<T>(
        mut attempt: impl FnMut() -> Result<T, ReleaseError>,
    ) -> Result<T, ReleaseError> {
        for _ in 0..20 {
            match attempt() {
                Err(ReleaseError::Failed(message)) if message.contains(TEXT_FILE_BUSY) => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                other => return other,
            }
        }
        attempt()
    }

    fn arguments_of(source: &CurlReleaseSource, url: &str) -> Vec<String> {
        source
            .curl_arguments(Duration::from_secs(15), url)
            .into_iter()
            .map(|argument| argument.to_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_default_source_is_restricted_to_https_without_shell_or_curlrc() {
        let source = CurlReleaseSource::new(None);
        let arguments = arguments_of(&source, "https://x/y");
        assert_eq!(arguments[0], "-q");
        for expected in [
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "--tlsv1.2",
        ] {
            assert!(arguments.contains(&expected.to_string()), "{expected}");
        }
        assert_eq!(arguments[arguments.len() - 2], "--");
        assert_eq!(arguments.last().unwrap(), "https://x/y");
        assert_eq!(source.base(), DEFAULT_DOWNLOAD_BASE);
    }

    #[test]
    fn an_explicit_base_drops_the_protocol_restriction_and_trailing_slash() {
        let source = CurlReleaseSource::new(Some("http://127.0.0.1:1/releases/".into()));
        assert_eq!(source.base(), "http://127.0.0.1:1/releases");
        assert!(!arguments_of(&source, "u").contains(&"--proto".to_string()));
    }

    #[test]
    fn the_latest_version_is_read_from_the_effective_url() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_curl(
            directory.path(),
            "printf 'http://127.0.0.1:1/releases/tag/v0.0.12'",
        );
        let source =
            CurlReleaseSource::new(Some("http://x".into())).with_program(program.as_os_str());
        assert_eq!(
            retry_while_busy(|| source.latest_version()),
            Ok(ReleaseVersion::parse("0.0.12").unwrap())
        );
    }

    #[test]
    fn a_location_without_a_version_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_curl(directory.path(), "printf 'http://x/releases/latest'");
        let source =
            CurlReleaseSource::new(Some("http://x".into())).with_program(program.as_os_str());
        assert!(matches!(
            retry_while_busy(|| source.latest_version()),
            Err(ReleaseError::Failed(_))
        ));
    }

    #[test]
    fn the_arguments_of_a_download_name_the_url_the_output_and_the_size_limit() {
        let directory = tempfile::tempdir().unwrap();
        let record = directory.path().join("arguments");
        let program = fake_curl(
            directory.path(),
            &format!("printf '%s\\n' \"$@\" > {}", record.display()),
        );
        let source =
            CurlReleaseSource::new(Some("http://x/r".into())).with_program(program.as_os_str());
        let destination = directory.path().join("archive.tar.gz");
        retry_while_busy(|| source.download_file(&version(), "archive.tar.gz", &destination))
            .unwrap();
        let recorded = fs::read_to_string(record).unwrap();
        let lines: Vec<_> = recorded.lines().collect();
        assert!(lines.contains(&"http://x/r/download/v0.0.11/archive.tar.gz"));
        let output = lines.iter().position(|line| *line == "--output").unwrap();
        assert_eq!(lines[output + 1], destination.to_str().unwrap());
        assert!(lines.contains(&"--max-filesize"));
        assert!(lines.contains(&"104857600"));
    }

    #[test]
    fn failures_missing_curl_and_cancellation_are_distinct_errors() {
        let directory = tempfile::tempdir().unwrap();
        let failing = fake_curl(directory.path(), "echo 'HTTP 404' >&2; exit 22");
        let source =
            CurlReleaseSource::new(Some("http://x".into())).with_program(failing.as_os_str());
        match retry_while_busy(|| source.download_text(&version(), "a")) {
            Err(ReleaseError::Failed(message)) => assert!(message.contains("404"), "{message}"),
            other => panic!("{other:?}"),
        }
        let source = CurlReleaseSource::new(Some("http://x".into()))
            .with_program(directory.path().join("missing").as_os_str());
        assert!(matches!(
            source.download_text(&version(), "a"),
            Err(ReleaseError::ToolMissing(_))
        ));
        let source =
            CurlReleaseSource::new(Some("http://x".into())).with_program(failing.as_os_str());
        source.cancel();
        assert_eq!(
            source.download_text(&version(), "a"),
            Err(ReleaseError::Cancelled)
        );
    }

    #[test]
    fn a_slow_curl_is_killed_when_the_source_is_cancelled() {
        let directory = tempfile::tempdir().unwrap();
        let program = fake_curl(directory.path(), "sleep 30");
        let source = std::sync::Arc::new(
            CurlReleaseSource::new(Some("http://x".into())).with_program(program.as_os_str()),
        );
        let worker = {
            let source = std::sync::Arc::clone(&source);
            std::thread::spawn(move || retry_while_busy(|| source.download_text(&version(), "a")))
        };
        std::thread::sleep(Duration::from_millis(200));
        source.cancel();
        assert_eq!(worker.join().unwrap(), Err(ReleaseError::Cancelled));
    }
}
