use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::application::ports::{StoreError, UpdateCheckRecord, UpdateCheckStore};
use crate::domain::release::ReleaseVersion;
use crate::infrastructure::private_file::{read_regular_file, write_private_file_atomically};

const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
const MAXIMUM_UPDATE_CHECK_FILE_BYTES: u64 = 4_096;

#[derive(Serialize, Deserialize)]
struct StoredFile {
    checked_at: String,
    latest: String,
}

pub struct FileUpdateCheckStore {
    path: PathBuf,
}

impl FileUpdateCheckStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl UpdateCheckStore for FileUpdateCheckStore {
    fn load(&self) -> Option<UpdateCheckRecord> {
        let content = read_regular_file(&self.path, MAXIMUM_UPDATE_CHECK_FILE_BYTES)?;
        let stored: StoredFile = serde_json::from_str(&content).ok()?;
        Some(UpdateCheckRecord {
            checked_at: NaiveDateTime::parse_from_str(&stored.checked_at, TIMESTAMP_FORMAT).ok()?,
            latest: ReleaseVersion::parse(&stored.latest).ok()?,
        })
    }

    fn save(&self, record: &UpdateCheckRecord) -> Result<(), StoreError> {
        let stored = StoredFile {
            checked_at: record.checked_at.format(TIMESTAMP_FORMAT).to_string(),
            latest: record.latest.to_string(),
        };
        let content = serde_json::to_string(&stored)
            .map_err(|error| StoreError::Failed(error.to_string()))?;
        write_private_file_atomically(&self.path, &content)
            .map_err(|error| StoreError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::NaiveDate;

    use super::*;

    fn record() -> UpdateCheckRecord {
        UpdateCheckRecord {
            checked_at: NaiveDate::from_ymd_opt(2026, 11, 12)
                .unwrap()
                .and_hms_opt(9, 30, 5)
                .unwrap(),
            latest: ReleaseVersion::parse("0.0.11").unwrap(),
        }
    }

    #[test]
    fn the_record_round_trips() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let store = FileUpdateCheckStore::new(directory.path().join("update-check.json"));
        assert_eq!(store.load(), None);
        store.save(&record()).unwrap();
        assert_eq!(store.load(), Some(record()));
    }

    #[test]
    fn missing_corrupt_or_invalid_files_are_ignored() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("update-check.json");
        let store = FileUpdateCheckStore::new(path.clone());
        for content in [
            "",
            "{not json",
            r#"{"checked_at":"2026-11-12T09:30:05"}"#,
            r#"{"checked_at":"ontem","latest":"0.0.11"}"#,
            r#"{"checked_at":"2026-11-12T09:30:05","latest":"0.0"}"#,
            r#"{"checked_at":"2026-11-12T09:30:05","latest":"0.0.11; rm -rf"}"#,
        ] {
            fs::write(&path, content).unwrap();
            assert_eq!(store.load(), None, "{content}");
        }
    }

    #[test]
    fn an_oversized_file_is_refused() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("update-check.json");
        fs::write(&path, " ".repeat(10_000)).unwrap();
        assert_eq!(FileUpdateCheckStore::new(path).load(), None);
    }

    #[test]
    fn saving_creates_missing_directories_and_reports_unwritable_paths() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let nested = directory.path().join("novo/estado/update-check.json");
        FileUpdateCheckStore::new(nested.clone())
            .save(&record())
            .unwrap();
        assert!(nested.is_file());
        let blocker = directory.path().join("arquivo");
        fs::write(&blocker, "x").unwrap();
        assert!(FileUpdateCheckStore::new(blocker.join("update-check.json"))
            .save(&record())
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private_and_a_symlink_is_not_followed() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("update-check.json");
        FileUpdateCheckStore::new(path.clone())
            .save(&record())
            .unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link = directory.path().join("link.json");
        symlink(&path, &link).unwrap();
        assert_eq!(FileUpdateCheckStore::new(link).load(), None);
    }
}
