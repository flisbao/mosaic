use chrono::NaiveDateTime;
use thiserror::Error;

use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::widget::WidgetSnapshot;

#[derive(Debug, Clone, Error, PartialEq)]
pub enum SourceError {
    #[error("{0}")]
    Failed(String),
    #[error("{0}")]
    TimedOut(String),
    #[error("{0}")]
    OutputTooLarge(String),
    #[error("processo cancelado")]
    Cancelled,
    #[error("{0}")]
    InvalidSnapshot(String),
}

pub trait Cancellable: Send + Sync {
    fn cancel(&self);
    fn request_cancel(&self);
}

pub trait WidgetSource: Cancellable {
    fn snapshot(&self) -> Result<WidgetSnapshot, SourceError>;
}

pub trait ActionExecutor: Send + Sync {
    fn execute(&self, action: &ConfirmedAction) -> Result<ActionOutcome, SourceError>;
}

pub trait WidgetRunner: WidgetSource + ActionExecutor {}

impl<T: WidgetSource + ActionExecutor> WidgetRunner for T {}

pub trait Clock: Send + Sync {
    fn now(&self) -> NaiveDateTime;
}
