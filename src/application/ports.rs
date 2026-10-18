use chrono::NaiveDateTime;
use thiserror::Error;

use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::widget::WidgetSnapshot;

#[derive(Debug, Clone, Error, PartialEq)]
pub enum SourceError {
    #[error("{0}")]
    Failed(String),
}

pub trait Cancellable: Send + Sync {
    fn cancel(&self);
}

pub trait WidgetSource: Cancellable {
    fn snapshot(&self) -> Result<WidgetSnapshot, SourceError>;
}

pub trait ActionExecutor: Send + Sync {
    fn execute(&self, action: &ConfirmedAction) -> Result<ActionOutcome, SourceError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> NaiveDateTime;
}
