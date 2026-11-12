use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::application::ports::{RecommendationStore, StoreError, StoredRecommendation};
use crate::infrastructure::private_file::{read_regular_file, write_private_file_atomically};

const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
pub const MAXIMUM_RECOMMENDATION_FILE_BYTES: u64 = 1_048_576;

#[derive(Serialize, Deserialize)]
struct StoredFile {
    text: String,
    created_at: String,
}

pub struct FileRecommendationStore {
    path: PathBuf,
}

impl FileRecommendationStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

pub struct NullRecommendationStore;

impl RecommendationStore for NullRecommendationStore {
    fn load(&self) -> Option<StoredRecommendation> {
        None
    }

    fn save(&self, _recommendation: &StoredRecommendation) -> Result<(), StoreError> {
        Ok(())
    }
}

impl RecommendationStore for FileRecommendationStore {
    fn load(&self) -> Option<StoredRecommendation> {
        let content = read_regular_file(&self.path, MAXIMUM_RECOMMENDATION_FILE_BYTES)?;
        let stored: StoredFile = serde_json::from_str(&content).ok()?;
        if stored.text.trim().is_empty() {
            return None;
        }
        let created_at =
            NaiveDateTime::parse_from_str(&stored.created_at, TIMESTAMP_FORMAT).ok()?;
        Some(StoredRecommendation {
            text: stored.text,
            created_at,
        })
    }

    fn save(&self, recommendation: &StoredRecommendation) -> Result<(), StoreError> {
        let stored = StoredFile {
            text: recommendation.text.clone(),
            created_at: recommendation
                .created_at
                .format(TIMESTAMP_FORMAT)
                .to_string(),
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

    fn stored() -> StoredRecommendation {
        StoredRecommendation {
            text: "1. Fazer X\nComece por: X".to_string(),
            created_at: NaiveDate::from_ymd_opt(2026, 10, 17)
                .unwrap()
                .and_hms_opt(8, 30, 5)
                .unwrap(),
        }
    }

    #[test]
    fn store_round_trips_a_recommendation() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let store = FileRecommendationStore::new(directory.path().join("rec.json"));
        store.save(&stored()).unwrap();
        assert_eq!(store.load(), Some(stored()));
    }

    #[test]
    fn store_load_ignores_missing_empty_and_corrupt_files() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("rec.json");
        let store = FileRecommendationStore::new(path.clone());
        assert_eq!(store.load(), None);
        for content in [
            "",
            "{not json",
            r#"{"text":"x"}"#,
            r#"{"text":"x","created_at":"ontem"}"#,
            r#"{"text":"  ","created_at":"2026-10-17T08:30:05"}"#,
            "[1,2]",
        ] {
            fs::write(&path, content).unwrap();
            assert_eq!(store.load(), None, "{content}");
        }
    }

    #[test]
    fn store_save_creates_missing_directory() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("novo/sub/rec.json");
        FileRecommendationStore::new(path.clone())
            .save(&stored())
            .unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn store_save_into_unwritable_path_does_not_panic() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let blocker = directory.path().join("arquivo");
        fs::write(&blocker, "x").unwrap();
        let store = FileRecommendationStore::new(blocker.join("rec.json"));
        assert!(store.save(&stored()).is_err());
        assert_eq!(store.load(), None);
    }

    #[cfg(unix)]
    #[test]
    fn store_save_creates_a_private_file() {
        use std::os::unix::fs::PermissionsExt;
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("rec.json");
        FileRecommendationStore::new(path.clone())
            .save(&stored())
            .unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn store_save_over_a_symlink_does_not_touch_the_link_target() {
        use std::os::unix::fs::symlink;
        let directory = crate::infrastructure::test_support::private_tempdir();
        let target = directory.path().join("alvo.txt");
        fs::write(&target, "original").unwrap();
        let path = directory.path().join("rec.json");
        symlink(&target, &path).unwrap();
        FileRecommendationStore::new(path.clone())
            .save(&stored())
            .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert!(!fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn store_load_refuses_a_symlink() {
        use std::os::unix::fs::symlink;
        let directory = crate::infrastructure::test_support::private_tempdir();
        let real = directory.path().join("real.json");
        let path = directory.path().join("rec.json");
        FileRecommendationStore::new(real.clone())
            .save(&stored())
            .unwrap();
        symlink(&real, &path).unwrap();
        assert_eq!(FileRecommendationStore::new(path).load(), None);
    }

    #[test]
    fn store_load_refuses_an_oversized_file() {
        let directory = crate::infrastructure::test_support::private_tempdir();
        let path = directory.path().join("rec.json");
        let padding = "a".repeat(MAXIMUM_RECOMMENDATION_FILE_BYTES as usize);
        fs::write(
            &path,
            format!(r#"{{"text":"{padding}","created_at":"2026-10-17T08:30:05"}}"#),
        )
        .unwrap();
        assert_eq!(FileRecommendationStore::new(path).load(), None);
    }

    #[test]
    fn null_store_neither_loads_nor_saves() {
        let store = NullRecommendationStore;
        assert!(store.save(&stored()).is_ok());
        assert_eq!(store.load(), None);
    }

    #[test]
    fn default_path_ends_with_app_directory_and_file_name() {
        if let Some(path) = crate::infrastructure::app_paths::recommendation_file() {
            assert!(path.ends_with("mosaic/recommendation.json"));
        }
    }
}
