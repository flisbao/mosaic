use thiserror::Error;

use crate::display_in_portuguese;
use crate::domain::language::{Language, LocalizedMessage};

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    ItemActionWithoutItem { action_id: String },
}

impl LocalizedMessage for DomainError {
    fn message(&self, language: Language) -> String {
        match (self, language) {
            (Self::ItemActionWithoutItem { action_id }, Language::English) => {
                format!("action '{action_id}' requires a selected item")
            }
            (Self::ItemActionWithoutItem { action_id }, Language::BrazilianPortuguese) => {
                format!("a ação '{action_id}' exige um item selecionado")
            }
        }
    }
}

display_in_portuguese!(DomainError);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_assertions::assert_translated;

    #[test]
    fn the_message_is_translated() {
        let error = DomainError::ItemActionWithoutItem {
            action_id: "open".to_string(),
        };
        assert_translated("item action", |language| error.message(language));
        assert_eq!(
            error.message(Language::English),
            "action 'open' requires a selected item"
        );
    }

    #[test]
    fn display_keeps_the_portuguese_text() {
        let error = DomainError::ItemActionWithoutItem {
            action_id: "open".to_string(),
        };
        assert_eq!(error.to_string(), "a ação 'open' exige um item selecionado");
    }
}
