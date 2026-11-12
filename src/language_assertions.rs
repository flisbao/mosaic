use crate::domain::language::{Language, ALL_LANGUAGES};

pub fn assert_translated(label: &str, message: impl Fn(Language) -> String) {
    let texts: Vec<String> = ALL_LANGUAGES.into_iter().map(&message).collect();
    for (language, text) in ALL_LANGUAGES.iter().zip(&texts) {
        assert!(!text.trim().is_empty(), "{label}: empty in {language}");
    }
    assert_ne!(
        texts[0], texts[1],
        "{label}: identical in both languages: {}",
        texts[0]
    );
}
