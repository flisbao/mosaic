use chrono::NaiveDateTime;
use thiserror::Error;

use crate::application::recommendation::RecommendationPlan;

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
pub struct RecommendationOutcome {
    pub recommendation: StoredRecommendation,
    pub save_warning: Option<String>,
}

const EMPTY_ANSWER_MESSAGE: &str = "a IA devolveu uma resposta vazia";

pub fn execute_recommendation(
    runner: &dyn LlmRunner,
    store: &dyn RecommendationStore,
    plan: &RecommendationPlan,
    model: Option<&str>,
    now: NaiveDateTime,
) -> Result<RecommendationOutcome, LlmError> {
    let answer = runner.run(&LlmRequest {
        prompt: plan.prompt.clone(),
        model: model.map(str::to_string),
    })?;
    let text = answer.trim();
    if text.is_empty() {
        return Err(LlmError::Failed(EMPTY_ANSWER_MESSAGE.to_string()));
    }
    let recommendation = StoredRecommendation {
        text: text.to_string(),
        created_at: now,
    };
    let save_warning = store
        .save(&recommendation)
        .err()
        .map(|error| error.to_string());
    Ok(RecommendationOutcome {
        recommendation,
        save_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::sync::Mutex;

    struct FakeRunner {
        answer: Result<String, LlmError>,
        requests: Mutex<Vec<LlmRequest>>,
    }

    impl FakeRunner {
        fn answering(answer: Result<String, LlmError>) -> Self {
            Self {
                answer,
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl LlmRunner for FakeRunner {
        fn run(&self, request: &LlmRequest) -> Result<String, LlmError> {
            self.requests.lock().unwrap().push(request.clone());
            self.answer.clone()
        }
    }

    #[derive(Default)]
    struct FakeStore {
        saved: Mutex<Option<StoredRecommendation>>,
    }

    impl RecommendationStore for FakeStore {
        fn load(&self) -> Option<StoredRecommendation> {
            self.saved.lock().unwrap().clone()
        }

        fn save(&self, recommendation: &StoredRecommendation) -> Result<(), StoreError> {
            *self.saved.lock().unwrap() = Some(recommendation.clone());
            Ok(())
        }
    }

    struct FailingStore;

    impl RecommendationStore for FailingStore {
        fn load(&self) -> Option<StoredRecommendation> {
            None
        }

        fn save(&self, _recommendation: &StoredRecommendation) -> Result<(), StoreError> {
            Err(StoreError::Failed("disco cheio".to_string()))
        }
    }

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 17)
            .unwrap()
            .and_hms_opt(8, 30, 0)
            .unwrap()
    }

    fn plan() -> RecommendationPlan {
        RecommendationPlan {
            prompt: "PROMPT".to_string(),
            item_count: 1,
            estimated_tokens: 2,
        }
    }

    #[test]
    fn success_saves_and_returns_the_recommendation() {
        let runner = FakeRunner::answering(Ok("1. Fazer X\n".to_string()));
        let store = FakeStore::default();

        let result = execute_recommendation(&runner, &store, &plan(), None, now()).unwrap();

        assert_eq!(result.recommendation.text, "1. Fazer X");
        assert_eq!(result.recommendation.created_at, now());
        assert_eq!(result.save_warning, None);
        assert_eq!(store.load(), Some(result.recommendation));
    }

    #[test]
    fn save_failure_keeps_the_recommendation_and_exposes_a_warning() {
        let runner = FakeRunner::answering(Ok("1. Fazer X".to_string()));
        let outcome = execute_recommendation(&runner, &FailingStore, &plan(), None, now()).unwrap();
        assert_eq!(outcome.recommendation.text, "1. Fazer X");
        assert_eq!(outcome.save_warning.as_deref(), Some("disco cheio"));
    }

    #[test]
    fn runner_receives_prompt_and_model() {
        let runner = FakeRunner::answering(Ok("ok".to_string()));
        execute_recommendation(
            &runner,
            &FakeStore::default(),
            &plan(),
            Some("haiku"),
            now(),
        )
        .unwrap();
        let requests = runner.requests.lock().unwrap();
        assert_eq!(
            requests[0],
            LlmRequest {
                prompt: "PROMPT".to_string(),
                model: Some("haiku".to_string())
            }
        );
    }

    #[test]
    fn empty_answer_is_an_error_and_is_not_saved() {
        let runner = FakeRunner::answering(Ok("  \n ".to_string()));
        let store = FakeStore::default();
        let error = execute_recommendation(&runner, &store, &plan(), None, now()).unwrap_err();
        assert_eq!(
            error,
            LlmError::Failed("a IA devolveu uma resposta vazia".to_string())
        );
        assert_eq!(store.load(), None);
    }

    #[test]
    fn runner_error_is_propagated_and_nothing_saved() {
        let runner = FakeRunner::answering(Err(LlmError::Unavailable("sem claude".to_string())));
        let store = FakeStore::default();
        let error = execute_recommendation(&runner, &store, &plan(), None, now()).unwrap_err();
        assert_eq!(error, LlmError::Unavailable("sem claude".to_string()));
        assert_eq!(store.load(), None);
    }
}
