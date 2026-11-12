use std::path::{Path, PathBuf};

use chrono::Duration;
use thiserror::Error;

use crate::display_in_portuguese;
use crate::domain::language::{Language, LocalizedMessage};

use crate::application::ports::{
    ArchiveExtractor, ChecksumCalculator, Clock, ExecutableReplacer, InstallationProbe,
    ReleaseError, ReleaseSource, UpdateCheckRecord, UpdateCheckStore,
};
use crate::domain::release::{
    expected_checksum, InstallFacts, InstallMethod, ReleaseAssets, ReleaseTarget, ReleaseVersion,
    UpdateStatus,
};

pub const UPDATE_CHECK_INTERVAL_HOURS: i64 = 24;
pub const NO_UPDATE_CHECK_VARIABLE: &str = "MOSAIC_NO_UPDATE_CHECK";
const DISABLING_VALUES: [&str; 3] = ["1", "true", "yes"];
const EXTRACTION_DIRECTORY: &str = "extracted";

#[derive(Debug, Clone, Error, PartialEq)]
pub enum UpdateError {
    Release(#[from] ReleaseError),
    UnsupportedPlatform {
        operating_system: String,
        architecture: String,
    },
    MissingChecksum,
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
}

impl LocalizedMessage for UpdateError {
    fn message(&self, language: Language) -> String {
        match self {
            Self::Release(error) => release_error_text(error, language),
            Self::UnsupportedPlatform {
                operating_system,
                architecture,
            } => language.pick(
                format!("there is no published binary for this system ({operating_system}/{architecture})"),
                format!("não há binário publicado para este sistema ({operating_system}/{architecture})"),
            ),
            Self::MissingChecksum => language
                .pick(
                    "the published checksum file does not have a valid SHA-256",
                    "o arquivo de checksum publicado não tem um SHA-256 válido",
                )
                .to_string(),
            Self::ChecksumMismatch { expected, actual } => language.pick(
                format!("the SHA-256 of the download does not match (expected {expected}, got {actual}); nothing was changed"),
                format!("o SHA-256 do download não confere (esperado {expected}, obtido {actual}); nada foi alterado"),
            ),
        }
    }
}

display_in_portuguese!(UpdateError);

pub fn release_error_text(error: &ReleaseError, language: Language) -> String {
    error.message(language)
}

pub fn check_latest(
    source: &dyn ReleaseSource,
    current: &ReleaseVersion,
) -> Result<UpdateStatus, ReleaseError> {
    Ok(UpdateStatus::compare(current, source.latest_version()?))
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdatePlan {
    UpToDate,
    Guidance {
        method: InstallMethod,
        latest: ReleaseVersion,
    },
    Replace {
        latest: ReleaseVersion,
        executable: PathBuf,
        target: ReleaseTarget,
    },
}

pub struct PlatformIdentity<'a> {
    pub operating_system: &'a str,
    pub architecture: &'a str,
}

pub fn plan_update(
    source: &dyn ReleaseSource,
    probe: &dyn InstallationProbe,
    current: &ReleaseVersion,
    platform: &PlatformIdentity<'_>,
) -> Result<UpdatePlan, UpdateError> {
    let UpdateStatus::Available { latest } = check_latest(source, current)? else {
        return Ok(UpdatePlan::UpToDate);
    };
    let executable = probe.executable_path()?;
    let facts = InstallFacts {
        owned_by_debian_package: InstallMethod::may_be_system_package(&executable)
            && probe.owned_by_debian_package(&executable),
        homebrew_prefix: probe.homebrew_prefix(),
        cargo_home: probe.cargo_home(),
    };
    match InstallMethod::classify(&executable, &facts) {
        InstallMethod::SelfManaged => {
            let target = ReleaseTarget::detect(platform.operating_system, platform.architecture)
                .ok_or_else(|| UpdateError::UnsupportedPlatform {
                    operating_system: platform.operating_system.to_string(),
                    architecture: platform.architecture.to_string(),
                })?;
            Ok(UpdatePlan::Replace {
                latest,
                executable,
                target,
            })
        }
        method => Ok(UpdatePlan::Guidance { method, latest }),
    }
}

pub struct UpdateTools<'a> {
    pub source: &'a dyn ReleaseSource,
    pub checksums: &'a dyn ChecksumCalculator,
    pub extractor: &'a dyn ArchiveExtractor,
    pub replacer: &'a dyn ExecutableReplacer,
}

pub fn install_update(
    tools: &UpdateTools<'_>,
    assets: &ReleaseAssets,
    current_executable: &Path,
    staging: &Path,
) -> Result<(), UpdateError> {
    let archive_name = assets.archive_name();
    let archive = staging.join(&archive_name);
    let checksum_text = tools
        .source
        .download_text(&assets.version, &assets.checksum_name())?;
    let expected = expected_checksum(&checksum_text).ok_or(UpdateError::MissingChecksum)?;
    tools
        .source
        .download_file(&assets.version, &archive_name, &archive)?;
    let actual = tools.checksums.sha256(&archive)?.to_ascii_lowercase();
    if actual != expected {
        return Err(UpdateError::ChecksumMismatch { expected, actual });
    }
    let extracted = staging.join(EXTRACTION_DIRECTORY);
    tools.extractor.extract(&archive, &extracted)?;
    tools.replacer.replace(
        &extracted.join(assets.executable_path_in_archive()),
        current_executable,
    )?;
    Ok(())
}

pub fn update_check_enabled(configured: bool, environment_value: Option<&str>) -> bool {
    let disabled_by_environment = environment_value.is_some_and(|value| {
        DISABLING_VALUES.contains(&value.trim().to_ascii_lowercase().as_str())
    });
    configured && !disabled_by_environment
}

pub fn newer_version_for_notice(
    source: &dyn ReleaseSource,
    store: &dyn UpdateCheckStore,
    clock: &dyn Clock,
    current: &ReleaseVersion,
) -> Option<ReleaseVersion> {
    let now = clock.now();
    let record = store.load();
    let fresh = record.as_ref().filter(|record| {
        let age = now - record.checked_at;
        age >= Duration::zero() && age < Duration::hours(UPDATE_CHECK_INTERVAL_HOURS)
    });
    let latest = match fresh {
        Some(record) => record.latest.clone(),
        None => {
            let known = record.map(|record| record.latest);
            let queried = source.latest_version().ok();
            let latest = queried.clone().or(known).unwrap_or_else(|| current.clone());
            let _ = store.save(&UpdateCheckRecord {
                checked_at: now,
                latest: latest.clone(),
            });
            latest
        }
    };
    match UpdateStatus::compare(current, latest) {
        UpdateStatus::Available { latest } => Some(latest),
        UpdateStatus::UpToDate => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::StoreError;
    use crate::test_support::fixed_now;
    use chrono::NaiveDateTime;
    use std::cell::RefCell;
    use std::sync::Mutex;

    fn version(text: &str) -> ReleaseVersion {
        ReleaseVersion::parse(text).unwrap()
    }

    struct FakeSource {
        latest: Result<ReleaseVersion, ReleaseError>,
        checksum_text: String,
        queries: Mutex<u32>,
        downloads: Mutex<Vec<String>>,
    }

    impl FakeSource {
        fn new(latest: &str) -> Self {
            Self {
                latest: Ok(version(latest)),
                checksum_text: format!("{}  archive\n", "a".repeat(64)),
                queries: Mutex::new(0),
                downloads: Mutex::new(Vec::new()),
            }
        }

        fn queries(&self) -> u32 {
            *self.queries.lock().unwrap()
        }
    }

    impl ReleaseSource for FakeSource {
        fn latest_version(&self) -> Result<ReleaseVersion, ReleaseError> {
            *self.queries.lock().unwrap() += 1;
            self.latest.clone()
        }

        fn download_text(&self, _: &ReleaseVersion, name: &str) -> Result<String, ReleaseError> {
            self.downloads.lock().unwrap().push(name.to_string());
            Ok(self.checksum_text.clone())
        }

        fn download_file(
            &self,
            _: &ReleaseVersion,
            name: &str,
            _: &Path,
        ) -> Result<(), ReleaseError> {
            self.downloads.lock().unwrap().push(name.to_string());
            Ok(())
        }
    }

    struct FakeProbe {
        executable: PathBuf,
        debian_owned: bool,
        probed: RefCell<bool>,
    }

    impl FakeProbe {
        fn at(path: &str) -> Self {
            Self {
                executable: PathBuf::from(path),
                debian_owned: false,
                probed: RefCell::new(false),
            }
        }
    }

    impl InstallationProbe for FakeProbe {
        fn executable_path(&self) -> Result<PathBuf, ReleaseError> {
            Ok(self.executable.clone())
        }
        fn owned_by_debian_package(&self, _: &Path) -> bool {
            *self.probed.borrow_mut() = true;
            self.debian_owned
        }
        fn homebrew_prefix(&self) -> Option<PathBuf> {
            None
        }
        fn cargo_home(&self) -> Option<PathBuf> {
            Some(PathBuf::from("/home/ana/.cargo"))
        }
    }

    const LINUX: PlatformIdentity<'static> = PlatformIdentity {
        operating_system: "linux",
        architecture: "x86_64",
    };

    #[test]
    fn check_reports_available_and_up_to_date_and_propagates_errors() {
        let current = version("0.0.10");
        assert_eq!(
            check_latest(&FakeSource::new("0.0.11"), &current),
            Ok(UpdateStatus::Available {
                latest: version("0.0.11")
            })
        );
        assert_eq!(
            check_latest(&FakeSource::new("0.0.10"), &current),
            Ok(UpdateStatus::UpToDate)
        );
        let mut failing = FakeSource::new("0.0.11");
        failing.latest = Err(ReleaseError::Failed("sem rede".into()));
        assert_eq!(
            check_latest(&failing, &current),
            Err(ReleaseError::Failed("sem rede".into()))
        );
    }

    #[test]
    fn plan_is_up_to_date_without_probing_the_installation() {
        let probe = FakeProbe::at("/usr/bin/mosaic");
        let plan = plan_update(
            &FakeSource::new("0.0.10"),
            &probe,
            &version("0.0.10"),
            &LINUX,
        );
        assert_eq!(plan, Ok(UpdatePlan::UpToDate));
        assert!(!*probe.probed.borrow());
    }

    #[test]
    fn plan_gives_guidance_for_package_managed_and_cargo_installs() {
        let mut debian = FakeProbe::at("/usr/bin/mosaic");
        debian.debian_owned = true;
        for (probe, method) in [
            (debian, InstallMethod::DebianPackage),
            (
                FakeProbe::at("/opt/homebrew/Cellar/mosaic/0.0.10/bin/mosaic"),
                InstallMethod::Homebrew,
            ),
            (
                FakeProbe::at("/home/ana/.cargo/bin/mosaic"),
                InstallMethod::Cargo,
            ),
        ] {
            let plan = plan_update(
                &FakeSource::new("0.0.11"),
                &probe,
                &version("0.0.10"),
                &LINUX,
            );
            assert_eq!(
                plan,
                Ok(UpdatePlan::Guidance {
                    method,
                    latest: version("0.0.11")
                })
            );
        }
    }

    #[test]
    fn dpkg_is_only_asked_about_files_under_usr() {
        let home_install = FakeProbe::at("/home/ana/.local/bin/mosaic");
        let _ = plan_update(
            &FakeSource::new("0.0.11"),
            &home_install,
            &version("0.0.10"),
            &LINUX,
        );
        assert!(!*home_install.probed.borrow());
        let system_install = FakeProbe::at("/usr/local/bin/mosaic");
        let _ = plan_update(
            &FakeSource::new("0.0.11"),
            &system_install,
            &version("0.0.10"),
            &LINUX,
        );
        assert!(*system_install.probed.borrow());
    }

    #[test]
    fn plan_replaces_a_self_managed_binary_with_the_target_of_the_platform() {
        let probe = FakeProbe::at("/home/ana/.local/bin/mosaic");
        let plan = plan_update(
            &FakeSource::new("0.0.11"),
            &probe,
            &version("0.0.10"),
            &LINUX,
        );
        assert_eq!(
            plan,
            Ok(UpdatePlan::Replace {
                latest: version("0.0.11"),
                executable: PathBuf::from("/home/ana/.local/bin/mosaic"),
                target: ReleaseTarget::LinuxX86_64
            })
        );
        let unsupported = PlatformIdentity {
            operating_system: "freebsd",
            architecture: "x86_64",
        };
        assert_eq!(
            plan_update(
                &FakeSource::new("0.0.11"),
                &probe,
                &version("0.0.10"),
                &unsupported
            ),
            Err(UpdateError::UnsupportedPlatform {
                operating_system: "freebsd".into(),
                architecture: "x86_64".into()
            })
        );
    }

    struct FakeChecksums(String);
    impl ChecksumCalculator for FakeChecksums {
        fn sha256(&self, _: &Path) -> Result<String, ReleaseError> {
            Ok(self.0.clone())
        }
    }

    #[derive(Default)]
    struct FakeExtractor {
        calls: Mutex<Vec<(PathBuf, PathBuf)>>,
        failure: Option<ReleaseError>,
    }
    impl ArchiveExtractor for FakeExtractor {
        fn extract(&self, archive: &Path, destination: &Path) -> Result<(), ReleaseError> {
            self.calls
                .lock()
                .unwrap()
                .push((archive.into(), destination.into()));
            self.failure.clone().map_or(Ok(()), Err)
        }
    }

    #[derive(Default)]
    struct FakeReplacer {
        calls: Mutex<Vec<(PathBuf, PathBuf)>>,
        failure: Option<ReleaseError>,
    }
    impl ExecutableReplacer for FakeReplacer {
        fn replace(&self, new: &Path, current: &Path) -> Result<(), ReleaseError> {
            self.calls
                .lock()
                .unwrap()
                .push((new.into(), current.into()));
            self.failure.clone().map_or(Ok(()), Err)
        }
    }

    fn assets() -> ReleaseAssets {
        ReleaseAssets {
            version: version("0.0.11"),
            target: ReleaseTarget::LinuxX86_64,
        }
    }

    #[test]
    fn a_verified_download_is_extracted_and_replaces_the_executable() {
        let source = FakeSource::new("0.0.11");
        let checksums = FakeChecksums("A".repeat(64));
        let extractor = FakeExtractor::default();
        let replacer = FakeReplacer::default();
        let tools = UpdateTools {
            source: &source,
            checksums: &checksums,
            extractor: &extractor,
            replacer: &replacer,
        };
        install_update(
            &tools,
            &assets(),
            Path::new("/bin-dir/mosaic"),
            Path::new("/stage"),
        )
        .unwrap();
        assert_eq!(
            *source.downloads.lock().unwrap(),
            [
                "mosaic-v0.0.11-x86_64-unknown-linux-musl.tar.gz.sha256",
                "mosaic-v0.0.11-x86_64-unknown-linux-musl.tar.gz"
            ]
        );
        assert_eq!(
            *extractor.calls.lock().unwrap(),
            [(
                PathBuf::from("/stage/mosaic-v0.0.11-x86_64-unknown-linux-musl.tar.gz"),
                PathBuf::from("/stage/extracted")
            )]
        );
        assert_eq!(
            *replacer.calls.lock().unwrap(),
            [(
                PathBuf::from("/stage/extracted/mosaic-v0.0.11-x86_64-unknown-linux-musl/mosaic"),
                PathBuf::from("/bin-dir/mosaic")
            )]
        );
    }

    #[test]
    fn a_checksum_mismatch_extracts_nothing_and_replaces_nothing() {
        let source = FakeSource::new("0.0.11");
        let checksums = FakeChecksums("b".repeat(64));
        let extractor = FakeExtractor::default();
        let replacer = FakeReplacer::default();
        let tools = UpdateTools {
            source: &source,
            checksums: &checksums,
            extractor: &extractor,
            replacer: &replacer,
        };
        let error =
            install_update(&tools, &assets(), Path::new("/x/mosaic"), Path::new("/s")).unwrap_err();
        assert!(matches!(error, UpdateError::ChecksumMismatch { .. }));
        assert!(extractor.calls.lock().unwrap().is_empty());
        assert!(replacer.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn an_unreadable_checksum_file_stops_before_downloading_the_archive() {
        let mut source = FakeSource::new("0.0.11");
        source.checksum_text = "lixo".into();
        let checksums = FakeChecksums("a".repeat(64));
        let extractor = FakeExtractor::default();
        let replacer = FakeReplacer::default();
        let tools = UpdateTools {
            source: &source,
            checksums: &checksums,
            extractor: &extractor,
            replacer: &replacer,
        };
        assert_eq!(
            install_update(&tools, &assets(), Path::new("/x/mosaic"), Path::new("/s")),
            Err(UpdateError::MissingChecksum)
        );
        assert_eq!(source.downloads.lock().unwrap().len(), 1);
    }

    #[test]
    fn extraction_and_replacement_failures_are_reported() {
        let source = FakeSource::new("0.0.11");
        let checksums = FakeChecksums("a".repeat(64));
        let failing_extractor = FakeExtractor {
            failure: Some(ReleaseError::Failed("tar quebrou".into())),
            ..FakeExtractor::default()
        };
        let replacer = FakeReplacer::default();
        let tools = UpdateTools {
            source: &source,
            checksums: &checksums,
            extractor: &failing_extractor,
            replacer: &replacer,
        };
        assert_eq!(
            install_update(&tools, &assets(), Path::new("/x/mosaic"), Path::new("/s")),
            Err(UpdateError::Release(ReleaseError::Failed(
                "tar quebrou".into()
            )))
        );
        assert!(replacer.calls.lock().unwrap().is_empty());

        let denied = FakeReplacer {
            failure: Some(ReleaseError::NotWritable("sem permissão".into())),
            ..FakeReplacer::default()
        };
        let extractor = FakeExtractor::default();
        let tools = UpdateTools {
            source: &source,
            checksums: &checksums,
            extractor: &extractor,
            replacer: &denied,
        };
        assert_eq!(
            install_update(&tools, &assets(), Path::new("/x/mosaic"), Path::new("/s")),
            Err(UpdateError::Release(ReleaseError::NotWritable(
                "sem permissão".into()
            )))
        );
    }

    #[derive(Default)]
    struct FakeStore {
        record: Mutex<Option<UpdateCheckRecord>>,
    }
    impl UpdateCheckStore for FakeStore {
        fn load(&self) -> Option<UpdateCheckRecord> {
            self.record.lock().unwrap().clone()
        }
        fn save(&self, record: &UpdateCheckRecord) -> Result<(), StoreError> {
            *self.record.lock().unwrap() = Some(record.clone());
            Ok(())
        }
    }

    struct FixedClock(NaiveDateTime);
    impl Clock for FixedClock {
        fn now(&self) -> NaiveDateTime {
            self.0
        }
    }

    fn record(hours_ago: i64, latest: &str) -> UpdateCheckRecord {
        UpdateCheckRecord {
            checked_at: fixed_now() - Duration::hours(hours_ago),
            latest: version(latest),
        }
    }

    #[test]
    fn without_a_record_it_queries_saves_and_reports_a_newer_version() {
        let source = FakeSource::new("0.0.11");
        let store = FakeStore::default();
        let notice = newer_version_for_notice(
            &source,
            &store,
            &FixedClock(fixed_now()),
            &version("0.0.10"),
        );
        assert_eq!(notice, Some(version("0.0.11")));
        assert_eq!(source.queries(), 1);
        assert_eq!(store.load(), Some(record(0, "0.0.11")));
    }

    #[test]
    fn a_record_younger_than_24_hours_is_used_without_a_request() {
        let source = FakeSource::new("0.0.99");
        let store = FakeStore::default();
        store.save(&record(23, "0.0.11")).unwrap();
        let notice = newer_version_for_notice(
            &source,
            &store,
            &FixedClock(fixed_now()),
            &version("0.0.10"),
        );
        assert_eq!(notice, Some(version("0.0.11")));
        assert_eq!(source.queries(), 0);
    }

    #[test]
    fn a_record_of_24_hours_or_more_or_from_the_future_is_refreshed() {
        for stale in [record(24, "0.0.11"), record(-1, "0.0.11")] {
            let source = FakeSource::new("0.0.12");
            let store = FakeStore::default();
            store.save(&stale).unwrap();
            let notice = newer_version_for_notice(
                &source,
                &store,
                &FixedClock(fixed_now()),
                &version("0.0.10"),
            );
            assert_eq!(notice, Some(version("0.0.12")));
            assert_eq!(source.queries(), 1);
        }
    }

    #[test]
    fn a_failed_query_is_silent_and_still_counts_as_the_attempt_of_the_day() {
        let mut source = FakeSource::new("0.0.11");
        source.latest = Err(ReleaseError::Failed("sem rede".into()));
        let store = FakeStore::default();
        let clock = FixedClock(fixed_now());
        assert_eq!(
            newer_version_for_notice(&source, &store, &clock, &version("0.0.10")),
            None
        );
        assert_eq!(store.load(), Some(record(0, "0.0.10")));
        assert_eq!(
            newer_version_for_notice(&source, &store, &clock, &version("0.0.10")),
            None
        );
        assert_eq!(source.queries(), 1);
    }

    #[test]
    fn a_failed_refresh_keeps_the_last_known_newer_version() {
        let mut source = FakeSource::new("0.0.11");
        source.latest = Err(ReleaseError::Failed("sem rede".into()));
        let store = FakeStore::default();
        store.save(&record(30, "0.0.11")).unwrap();
        let notice = newer_version_for_notice(
            &source,
            &store,
            &FixedClock(fixed_now()),
            &version("0.0.10"),
        );
        assert_eq!(notice, Some(version("0.0.11")));
    }

    #[test]
    fn the_check_is_on_unless_the_configuration_or_the_environment_turns_it_off() {
        assert!(update_check_enabled(true, None));
        assert!(update_check_enabled(true, Some("")));
        assert!(update_check_enabled(true, Some("0")));
        assert!(!update_check_enabled(false, None));
        for value in ["1", "true", "TRUE", " yes "] {
            assert!(!update_check_enabled(true, Some(value)), "{value}");
        }
    }

    #[test]
    fn the_same_or_an_older_version_gives_no_notice() {
        for latest in ["0.0.10", "0.0.9"] {
            let source = FakeSource::new(latest);
            let notice = newer_version_for_notice(
                &source,
                &FakeStore::default(),
                &FixedClock(fixed_now()),
                &version("0.0.10"),
            );
            assert_eq!(notice, None);
        }
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;
    use crate::language_assertions::assert_translated;

    #[test]
    fn every_update_error_is_translated() {
        let errors = [
            UpdateError::UnsupportedPlatform {
                operating_system: "linux".to_string(),
                architecture: "riscv".to_string(),
            },
            UpdateError::MissingChecksum,
            UpdateError::ChecksumMismatch {
                expected: "a".to_string(),
                actual: "b".to_string(),
            },
            UpdateError::Release(ReleaseError::Cancelled),
        ];
        for error in errors {
            assert_translated("update error", |language| error.message(language));
        }
    }
}
