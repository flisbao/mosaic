use chrono::NaiveDateTime;

use crate::application::alert_tracker::AlertTracker;
use crate::application::execute_action::ActionNotice;
use crate::application::llm::{LlmError, RecommendationOutcome, RecommendationStore};
use crate::application::pomodoro_notifications::{AlertMessage, Notifier};
use crate::application::ports::SourceError;
use crate::application::recommendation_view::{idle_snapshot, recommendation_snapshot};
use crate::application::reload::{apply_if_current, WidgetGenerations, WidgetUpdate};
use crate::domain::widget::{WidgetId, WidgetSnapshot};
use crate::domain::widget_content::AlertLevel;
use crate::presentation::app_state::AppState;

const ACTION_DONE_FALLBACK_MESSAGE: &str = "Ação concluída";
const RECOMMENDATION_READY_MESSAGE: &str = "Recomendação atualizada";

pub enum AppEvent {
    WidgetUpdated(WidgetUpdate),
    ActionFinished(WidgetId, ActionNotice),
    RecommendationFinished(Result<RecommendationOutcome, LlmError>),
}

pub struct EventContext<'a> {
    pub generations: &'a WidgetGenerations,
    pub notifier: &'a dyn Notifier,
    pub recommendation_store: Option<&'a dyn RecommendationStore>,
}

fn announce_widget_alerts(
    state: &mut AppState,
    context: &EventContext,
    tracker: &mut AlertTracker,
    id: &WidgetId,
    result: &Result<WidgetSnapshot, SourceError>,
) {
    let Ok(snapshot) = result else {
        return;
    };
    for alert in tracker.fresh_alerts(id, &snapshot.alerts) {
        let message = AlertMessage {
            title: alert.title,
            body: alert.body,
        };
        if alert.level == AlertLevel::Warn {
            context.notifier.notify(&message);
        }
        state.show_alert(message);
    }
}

pub fn apply_event(
    state: &mut AppState,
    app_event: AppEvent,
    context: &EventContext,
    tracker: &mut AlertTracker,
    now: NaiveDateTime,
) {
    match app_event {
        AppEvent::WidgetUpdated(update) => {
            apply_widget_update(state, context, tracker, update, now)
        }
        AppEvent::ActionFinished(id, notice) => {
            state.finish_action(&id);
            state.show_notice(describe_notice(notice));
        }
        AppEvent::RecommendationFinished(result) => {
            apply_recommendation_result(state, context, result, now)
        }
    }
}

fn apply_widget_update(
    state: &mut AppState,
    context: &EventContext,
    tracker: &mut AlertTracker,
    update: WidgetUpdate,
    now: NaiveDateTime,
) {
    let WidgetUpdate {
        id,
        generation,
        result,
    } = update;
    apply_if_current(context.generations, &id, generation, result, |result| {
        announce_widget_alerts(state, context, tracker, &id, &result);
        state.update_widget(&id, result, now);
    });
}

fn apply_recommendation_result(
    state: &mut AppState,
    context: &EventContext,
    result: Result<RecommendationOutcome, LlmError>,
    now: NaiveDateTime,
) {
    let recommendation_widget = WidgetId::recommendation();
    match result {
        Ok(outcome) => {
            state.update_widget(
                &recommendation_widget,
                Ok(recommendation_snapshot(&outcome.recommendation)),
                now,
            );
            state.show_notice(match outcome.save_warning {
                Some(reason) => {
                    format!("{RECOMMENDATION_READY_MESSAGE}, mas não foi possível salvar: {reason}")
                }
                None => RECOMMENDATION_READY_MESSAGE.to_string(),
            });
        }
        Err(LlmError::Unavailable(message)) => {
            if let Some(store) = context.recommendation_store {
                state.update_widget(
                    &recommendation_widget,
                    stored_recommendation_result(store),
                    now,
                );
            }
            state.show_notice(message);
        }
        Err(LlmError::Failed(message)) => state.update_widget(
            &recommendation_widget,
            Err(SourceError::Failed(message)),
            now,
        ),
    }
}

pub fn stored_recommendation_result(
    store: &dyn RecommendationStore,
) -> Result<WidgetSnapshot, SourceError> {
    Ok(match store.load() {
        Some(stored) => recommendation_snapshot(&stored),
        None => idle_snapshot(),
    })
}

fn describe_notice(notice: ActionNotice) -> String {
    match notice {
        ActionNotice::Succeeded(message) if message.is_empty() => {
            ACTION_DONE_FALLBACK_MESSAGE.to_string()
        }
        ActionNotice::Succeeded(message) => message,
        ActionNotice::Failed(message) => format!("Falha: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_notice_falls_back_when_the_message_is_empty() {
        assert_eq!(
            describe_notice(ActionNotice::Succeeded(String::new())),
            ACTION_DONE_FALLBACK_MESSAGE
        );
    }

    #[test]
    fn describe_notice_keeps_the_success_message() {
        assert_eq!(
            describe_notice(ActionNotice::Succeeded("movido".to_string())),
            "movido"
        );
    }

    #[test]
    fn describe_notice_prefixes_failures() {
        assert_eq!(
            describe_notice(ActionNotice::Failed("quebrou".to_string())),
            "Falha: quebrou"
        );
    }
}
