use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("a ação '{action_id}' exige um item selecionado")]
    ItemActionWithoutItem { action_id: String },
}
