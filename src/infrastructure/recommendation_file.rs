use std::fs;
use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::application::llm::{RecommendationStore, StoredRecommendation};

const APP_DIRECTORY_NAME: &str = "mosaic";
const FILE_NAME: &str = "recommendation.json";
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

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

pub fn default_recommendation_path() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|directory| directory.join(APP_DIRECTORY_NAME).join(FILE_NAME))
}

impl RecommendationStore for FileRecommendationStore {
    fn load(&self) -> Option<StoredRecommendation> {
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
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, content);
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

    #[test]
    fn default_path_ends_with_app_directory_and_file_name() {
        if let Some(path) = default_recommendation_path() {
            assert!(path.ends_with("mosaic/recommendation.json"));
        }
    }
}
