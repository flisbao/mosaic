use std::fs;
use std::path::PathBuf;

use crate::application::ports::{CriteriaError, CriteriaSource};
use crate::domain::language::Language;

pub struct FilePriorityPrompt {
    path: PathBuf,
    language: Language,
}

impl FilePriorityPrompt {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            language: Language::default(),
        }
    }

    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }
}

impl CriteriaSource for FilePriorityPrompt {
    fn load(&self) -> Result<String, CriteriaError> {
        fs::read_to_string(&self.path).map_err(|error| {
            let path = self.path.display();
            CriteriaError(self.language.pick(
                format!("Could not read {path}: {error}"),
                format!("Não foi possível ler {path}: {error}"),
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
        let error = FilePriorityPrompt::new(path.clone())
            .with_language(Language::BrazilianPortuguese)
            .load()
            .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with(&format!("Não foi possível ler {}:", path.display())),
            "{error}"
        );
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn an_unreadable_file_is_reported_in_the_selected_language() {
        let path = PathBuf::from("/nonexistent/prioridade.md");
        let english = FilePriorityPrompt::new(path.clone()).load().unwrap_err();
        assert!(
            english.0.starts_with("Could not read /nonexistent"),
            "{}",
            english.0
        );
        let portuguese = FilePriorityPrompt::new(path)
            .with_language(Language::BrazilianPortuguese)
            .load()
            .unwrap_err();
        assert!(
            portuguese.0.starts_with("Não foi possível ler"),
            "{}",
            portuguese.0
        );
    }
}
