use std::fs;
use std::path::PathBuf;

use crate::application::recommendation_request::{CriteriaError, CriteriaSource};

pub struct FilePriorityPrompt {
    path: PathBuf,
}

impl FilePriorityPrompt {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl CriteriaSource for FilePriorityPrompt {
    fn load(&self) -> Result<String, CriteriaError> {
        fs::read_to_string(&self.path).map_err(|error| {
            CriteriaError(format!(
                "Não foi possível ler {}: {error}",
                self.path.display()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_file_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("prioridade.md");
        fs::write(&path, "critérios").unwrap();
        assert_eq!(
            FilePriorityPrompt::new(path).load(),
            Ok("critérios".to_string())
        );
    }

    #[test]
    fn missing_file_reports_the_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nao-existe.md");
        let error = FilePriorityPrompt::new(path.clone()).load().unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with(&format!("Não foi possível ler {}:", path.display())),
            "{error}"
        );
    }
}
