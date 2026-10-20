use chrono::NaiveDateTime;
use thiserror::Error;

use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::widget::{WidgetId, WidgetSnapshot};

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

#[derive(Debug, Clone, PartialEq)]
pub struct LlmRequest {
    pub prompt: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum LlmError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
}

pub trait LlmRunner: Send + Sync {
    fn run(&self, request: &LlmRequest) -> Result<String, LlmError>;
}

pub trait CancellableLlmRunner: LlmRunner + Cancellable {}

impl<T: LlmRunner + Cancellable> CancellableLlmRunner for T {}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredRecommendation {
    pub text: String,
    pub created_at: NaiveDateTime,
}

pub trait RecommendationStore: Send + Sync {
    fn load(&self) -> Option<StoredRecommendation>;
    fn save(&self, recommendation: &StoredRecommendation) -> Result<(), StoreError>;
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum StoreError {
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlertMessage {
    pub title: String,
    pub body: String,
}

pub trait Notifier: Send + Sync {
    fn notify(&self, message: &AlertMessage);
}

pub trait WidgetSpec {
    fn widget_id(&self) -> &WidgetId;
}

pub trait WidgetLifecycle {
    type Spec: WidgetSpec;

    fn start(&mut self, spec: &Self::Spec, generation: crate::application::reload::Generation);
    fn retire(&mut self, id: &WidgetId);
}

#[derive(Debug, Clone, Error, PartialEq)]
#[error("{0}")]
pub struct CriteriaError(pub String);

pub trait CriteriaSource {
    fn load(&self) -> Result<String, CriteriaError>;
}
