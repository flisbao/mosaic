use std::fmt;

pub const LANGUAGE_VARIABLE: &str = "MOSAIC_LANGUAGE";
const LOCALE_VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];
const ENGLISH_CODE: &str = "en";
const BRAZILIAN_PORTUGUESE_CODE: &str = "pt-BR";
const PORTUGUESE_PREFIX: &str = "pt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    BrazilianPortuguese,
}

pub const ALL_LANGUAGES: [Language; 2] = [Language::English, Language::BrazilianPortuguese];

impl Language {
    pub const fn code(self) -> &'static str {
        match self {
            Language::English => ENGLISH_CODE,
            Language::BrazilianPortuguese => BRAZILIAN_PORTUGUESE_CODE,
        }
    }

    pub fn from_setting(text: &str) -> Option<Self> {
        let normalized = text.trim().replace('_', "-").to_ascii_lowercase();
        ALL_LANGUAGES
            .into_iter()
            .find(|language| language.code().to_ascii_lowercase() == normalized)
    }

    pub fn from_locale(text: &str) -> Self {
        if text
            .trim()
            .to_ascii_lowercase()
            .starts_with(PORTUGUESE_PREFIX)
        {
            Language::BrazilianPortuguese
        } else {
            Language::English
        }
    }

    pub fn detect(lookup: impl Fn(&str) -> Option<String>) -> Self {
        std::iter::once(LANGUAGE_VARIABLE)
            .chain(LOCALE_VARIABLES)
            .find_map(|name| lookup(name).filter(|value| !value.trim().is_empty()))
            .map_or(Language::English, |value| Language::from_locale(&value))
    }

    pub fn detect_from_environment() -> Self {
        Self::detect(|name| std::env::var(name).ok())
    }
}

impl fmt::Display for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

pub trait LocalizedMessage {
    fn message(&self, language: Language) -> String;
}

#[macro_export]
macro_rules! display_in_portuguese {
    ($type:ty) => {
        impl ::std::fmt::Display for $type {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter.write_str(&$crate::domain::language::LocalizedMessage::message(
                    self,
                    $crate::domain::language::Language::BrazilianPortuguese,
                ))
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        move |name| map.get(name).map(|value| value.to_string())
    }

    #[test]
    fn english_is_the_default() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(Language::detect(lookup(&[])), Language::English);
    }

    #[test]
    fn codes_round_trip_through_settings() {
        for language in ALL_LANGUAGES {
            assert_eq!(Language::from_setting(language.code()), Some(language));
        }
        assert_eq!(
            Language::from_setting("PT_br"),
            Some(Language::BrazilianPortuguese)
        );
        assert_eq!(Language::from_setting(" en "), Some(Language::English));
        assert_eq!(Language::from_setting("fr"), None);
        assert_eq!(Language::from_setting(""), None);
    }

    #[test]
    fn explicit_variable_wins_over_the_locale() {
        let environment = lookup(&[("MOSAIC_LANGUAGE", "en"), ("LANG", "pt_BR.UTF-8")]);
        assert_eq!(Language::detect(environment), Language::English);
        let environment = lookup(&[("MOSAIC_LANGUAGE", "pt-BR"), ("LANG", "en_US.UTF-8")]);
        assert_eq!(Language::detect(environment), Language::BrazilianPortuguese);
    }

    #[test]
    fn locale_variables_follow_the_posix_order() {
        let environment = lookup(&[("LC_ALL", "C"), ("LC_MESSAGES", "pt_BR"), ("LANG", "pt_BR")]);
        assert_eq!(Language::detect(environment), Language::English);
        let environment = lookup(&[("LC_MESSAGES", "pt_BR.UTF-8"), ("LANG", "en_US")]);
        assert_eq!(Language::detect(environment), Language::BrazilianPortuguese);
        let environment = lookup(&[("LANG", "pt_PT.UTF-8")]);
        assert_eq!(Language::detect(environment), Language::BrazilianPortuguese);
    }

    #[test]
    fn empty_variables_are_skipped() {
        let environment = lookup(&[("MOSAIC_LANGUAGE", ""), ("LC_ALL", "  "), ("LANG", "pt_BR")]);
        assert_eq!(Language::detect(environment), Language::BrazilianPortuguese);
    }

    #[test]
    fn any_other_value_means_english() {
        let environment = lookup(&[("MOSAIC_LANGUAGE", "fr_FR"), ("LANG", "pt_BR")]);
        assert_eq!(Language::detect(environment), Language::English);
    }
}
