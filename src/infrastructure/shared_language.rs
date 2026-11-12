use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::domain::language::{Language, ALL_LANGUAGES};

#[derive(Debug, Clone)]
pub struct SharedLanguage(Arc<AtomicU8>);

impl SharedLanguage {
    pub fn new(language: Language) -> Self {
        Self(Arc::new(AtomicU8::new(index_of(language))))
    }

    pub fn get(&self) -> Language {
        ALL_LANGUAGES
            .get(usize::from(self.0.load(Ordering::Relaxed)))
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&self, language: Language) {
        self.0.store(index_of(language), Ordering::Relaxed);
    }
}

impl Default for SharedLanguage {
    fn default() -> Self {
        Self::new(Language::default())
    }
}

fn index_of(language: Language) -> u8 {
    ALL_LANGUAGES
        .iter()
        .position(|candidate| *candidate == language)
        .and_then(|position| u8::try_from(position).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_see_the_new_language() {
        let shared = SharedLanguage::default();
        let clone = shared.clone();
        assert_eq!(clone.get(), Language::English);
        shared.set(Language::BrazilianPortuguese);
        assert_eq!(clone.get(), Language::BrazilianPortuguese);
    }
}
