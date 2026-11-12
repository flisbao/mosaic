use chrono::NaiveDateTime;
use thiserror::Error;

use crate::domain::action::{ActionOutcome, ConfirmedAction};
use crate::domain::release::ReleaseVersion;
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

#[derive(Debug, Clone, Error, PartialEq)]
pub enum PromptError {
    #[error("a entrada terminou antes do fim das perguntas")]
    EndOfInput,
    #[error("{0}")]
    Failed(String),
}

pub trait Prompter {
    fn ask(&mut self, question: &str) -> Result<String, PromptError>;
    fn tell(&mut self, message: &str);
}

#[derive(Debug, Error)]
pub enum PluginFetchError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
}

pub trait PluginFetcher {
    fn fetch(
        &self,
        source: &crate::domain::plugin_package::GitHubSource,
        destination: &std::path::Path,
    ) -> Result<(), PluginFetchError>;
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ReleaseError {
    #[error("{0}")]
    ToolMissing(String),
    #[error("{0}")]
    Failed(String),
    #[error("{0}")]
    NotWritable(String),
    #[error("operação cancelada")]
    Cancelled,
}

pub trait ReleaseSource: Send + Sync {
    fn latest_version(&self) -> Result<ReleaseVersion, ReleaseError>;
    fn download_text(
        &self,
        version: &ReleaseVersion,
        file_name: &str,
    ) -> Result<String, ReleaseError>;
    fn download_file(
        &self,
        version: &ReleaseVersion,
        file_name: &str,
        destination: &std::path::Path,
    ) -> Result<(), ReleaseError>;
}

pub trait CancellableReleaseSource: ReleaseSource + Cancellable {}

impl<T: ReleaseSource + Cancellable> CancellableReleaseSource for T {}

pub trait ChecksumCalculator {
    fn sha256(&self, file: &std::path::Path) -> Result<String, ReleaseError>;
}

pub trait ArchiveExtractor {
    fn extract(
        &self,
        archive: &std::path::Path,
        destination: &std::path::Path,
    ) -> Result<(), ReleaseError>;
}

pub trait ExecutableReplacer {
    fn replace(
        &self,
        new_executable: &std::path::Path,
        current_executable: &std::path::Path,
    ) -> Result<(), ReleaseError>;
}

pub trait InstallationProbe {
    fn executable_path(&self) -> Result<std::path::PathBuf, ReleaseError>;
    fn owned_by_debian_package(&self, executable: &std::path::Path) -> bool;
    fn homebrew_prefix(&self) -> Option<std::path::PathBuf>;
    fn cargo_home(&self) -> Option<std::path::PathBuf>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateCheckRecord {
    pub checked_at: NaiveDateTime,
    pub latest: ReleaseVersion,
}

pub trait UpdateCheckStore: Send + Sync {
    fn load(&self) -> Option<UpdateCheckRecord>;
    fn save(&self, record: &UpdateCheckRecord) -> Result<(), StoreError>;
}
