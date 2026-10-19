use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::application::llm::{RecommendationStore, StoredRecommendation};
use crate::infrastructure::private_directory::prepare_private_directory;

const APP_DIRECTORY_NAME: &str = "mosaic";
const FILE_NAME: &str = "recommendation.json";
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
const RECOMMENDATION_FILE_MODE: u32 = 0o600;
pub const MAXIMUM_RECOMMENDATION_FILE_BYTES: u64 = 1_048_576;
const TEMPORARY_FILE_SUFFIX: &str = ".tmp";

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

    fn save(&self, _recommendation: &StoredRecommendation) {}
}

#[cfg(unix)]
fn open_private_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(RECOMMENDATION_FILE_MODE)
        .open(path)
}

#[cfg(not(unix))]
fn open_private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

pub fn default_recommendation_path() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|directory| directory.join(APP_DIRECTORY_NAME).join(FILE_NAME))
}

impl RecommendationStore for FileRecommendationStore {
    fn load(&self) -> Option<StoredRecommendation> {
        let metadata = fs::symlink_metadata(&self.path).ok()?;
        if !metadata.file_type().is_file() || metadata.len() > MAXIMUM_RECOMMENDATION_FILE_BYTES {
            return None;
        }
        let content = fs::read_to_string(&self.path).ok()?;
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

    fn save(&self, recommendation: &StoredRecommendation) {
        let stored = StoredFile {
            text: recommendation.text.clone(),
            created_at: recommendation
                .created_at
                .format(TIMESTAMP_FORMAT)
                .to_string(),
        };
        let Ok(content) = serde_json::to_string(&stored) else {
            return;
        };
        let Some(parent) = self.path.parent() else {
            return;
        };
        if prepare_private_directory(parent).is_err() {
            return;
        }
        let mut temporary_name = self.path.as_os_str().to_os_string();
        temporary_name.push(TEMPORARY_FILE_SUFFIX);
        let temporary = PathBuf::from(temporary_name);
        let _ = fs::remove_file(&temporary);
        let Ok(mut file) = open_private_file(&temporary) else {
            return;
        };
        let written = file
            .write_all(content.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        if written.is_err() || fs::rename(&temporary, &self.path).is_err() {
            let _ = fs::remove_file(&temporary);
        }
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
        let directory = tempfile::tempdir().unwrap();
        let store = FileRecommendationStore::new(directory.path().join("rec.json"));
        store.save(&stored());
        assert_eq!(store.load(), Some(stored()));
    }

    #[test]
    fn store_load_ignores_missing_empty_and_corrupt_files() {
        let directory = tempfile::tempdir().unwrap();
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
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("novo/sub/rec.json");
        FileRecommendationStore::new(path.clone()).save(&stored());
        assert!(path.is_file());
    }

    #[test]
    fn store_save_into_unwritable_path_does_not_panic() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("arquivo");
        fs::write(&blocker, "x").unwrap();
        let store = FileRecommendationStore::new(blocker.join("rec.json"));
        store.save(&stored());
        assert_eq!(store.load(), None);
    }

    #[cfg(unix)]
    #[test]
    fn store_save_creates_a_private_file() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rec.json");
        FileRecommendationStore::new(path.clone()).save(&stored());
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn store_save_over_a_symlink_does_not_touch_the_link_target() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("alvo.txt");
        fs::write(&target, "original").unwrap();
        let path = directory.path().join("rec.json");
        symlink(&target, &path).unwrap();
        FileRecommendationStore::new(path.clone()).save(&stored());
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
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real.json");
        let path = directory.path().join("rec.json");
        FileRecommendationStore::new(real.clone()).save(&stored());
        symlink(&real, &path).unwrap();
        assert_eq!(FileRecommendationStore::new(path).load(), None);
    }

    #[test]
    fn store_load_refuses_an_oversized_file() {
        let directory = tempfile::tempdir().unwrap();
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
        store.save(&stored());
        assert_eq!(store.load(), None);
    }

    #[test]
    fn default_path_ends_with_app_directory_and_file_name() {
        if let Some(path) = default_recommendation_path() {
            assert!(path.ends_with("mosaic/recommendation.json"));
        }
    }
}
