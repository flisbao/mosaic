use chrono::NaiveDateTime;

use crate::application::alert_tracker::AlertTracker;
use crate::application::execute_action::ActionNotice;
use crate::application::llm::RecommendationOutcome;
use crate::application::ports::{
    AlertMessage, LlmError, Notifier, RecommendationStore, SourceError,
};
use crate::application::recommendation_view::{idle_snapshot, recommendation_snapshot};
use crate::application::reload::{apply_if_current, WidgetGenerations, WidgetUpdate};
use crate::domain::language::Language;
use crate::domain::release::ReleaseVersion;
use crate::domain::widget::{WidgetId, WidgetSnapshot};
use crate::domain::widget_content::AlertLevel;
use crate::presentation::app_state::AppState;
use crate::presentation::key_input::KeyInput;
use crate::presentation::messages;

const NOTICE_SEPARATOR: &str = " | ";

pub enum AppEvent {
    WidgetUpdated(WidgetUpdate),
    ActionFinished(WidgetId, ActionNotice),
    RecommendationFinished(Result<RecommendationOutcome, LlmError>),
    UpdateAvailable(ReleaseVersion),
    Input(KeyInput),
    TerminalResized,
    TerminalReadFailed(String),
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
            state.show_notice(describe_notice(notice, state.language()));
        }
        AppEvent::RecommendationFinished(result) => {
            apply_recommendation_result(state, context, result, now)
        }
        AppEvent::UpdateAvailable(version) => announce_update(state, &version),
        AppEvent::Input(_) | AppEvent::TerminalResized | AppEvent::TerminalReadFailed(_) => {}
    }
}

pub fn update_notice(version: &ReleaseVersion, language: Language) -> String {
    messages::update_available_notice(language, &version.to_string())
}

fn announce_update(state: &mut AppState, version: &ReleaseVersion) {
    let notice = update_notice(version, state.language());
    let combined = match state.notice() {
        Some(existing) => format!("{existing}{NOTICE_SEPARATOR}{notice}"),
        None => notice,
    };
    state.show_notice(combined);
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
                Ok(recommendation_snapshot(
                    &outcome.recommendation,
                    state.language(),
                )),
                now,
            );
            let language = state.language();
            state.show_notice(match outcome.save_warning {
                Some(reason) => messages::recommendation_ready_but_unsaved(language, &reason),
                None => messages::recommendation_ready(language).to_string(),
            });
        }
        Err(LlmError::Unavailable(message)) => {
            if let Some(store) = context.recommendation_store {
                state.update_widget(
                    &recommendation_widget,
                    stored_recommendation_result(store, state.language()),
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
    language: Language,
) -> Result<WidgetSnapshot, SourceError> {
    Ok(match store.load() {
        Some(stored) => recommendation_snapshot(&stored, language),
        None => idle_snapshot(language),
    })
}

fn describe_notice(notice: ActionNotice, language: Language) -> String {
    match notice {
        ActionNotice::Succeeded(message) if message.is_empty() => {
            messages::action_done_fallback(language).to_string()
        }
        ActionNotice::Succeeded(message) => message,
        ActionNotice::Failed(message) => messages::action_failed(language, &message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::app_state::TabView;
    use crate::test_support::fixed_now;

    struct SilentNotifier;
    impl Notifier for SilentNotifier {
        fn notify(&self, _: &AlertMessage) {}
    }

    fn empty_state() -> AppState {
        AppState::new(
            Vec::new(),
            vec![TabView {
                name: "Principal".to_string(),
                entry_indexes: Vec::new(),
            }],
        )
        .with_language(Language::BrazilianPortuguese)
    }

    fn announce(state: &mut AppState, version: &str) {
        let generations = WidgetGenerations::default();
        let context = EventContext {
            generations: &generations,
            notifier: &SilentNotifier,
            recommendation_store: None,
        };
        apply_event(
            state,
            AppEvent::UpdateAvailable(ReleaseVersion::parse(version).unwrap()),
            &context,
            &mut AlertTracker::new(),
            fixed_now(),
        );
    }

    #[test]
    fn a_new_version_shows_the_update_notice() {
        let mut state = empty_state();
        announce(&mut state, "0.0.11");
        assert_eq!(
            state.notice(),
            Some("versão 0.0.11 disponível; rode mosaic update")
        );
    }

    #[test]
    fn a_new_version_keeps_the_notice_already_on_screen() {
        let mut state = empty_state();
        state.show_notice("chave desconhecida 'x' (ignorada)".to_string());
        announce(&mut state, "0.0.11");
        assert_eq!(
            state.notice(),
            Some(
                "chave desconhecida 'x' (ignorada) | versão 0.0.11 disponível; rode mosaic update"
            )
        );
    }

    #[test]
    fn describe_notice_falls_back_when_the_message_is_empty() {
        assert_eq!(
            describe_notice(
                ActionNotice::Succeeded(String::new()),
                Language::BrazilianPortuguese
            ),
            "Ação concluída"
        );
    }

    #[test]
    fn describe_notice_keeps_the_success_message() {
        assert_eq!(
            describe_notice(
                ActionNotice::Succeeded("movido".to_string()),
                Language::BrazilianPortuguese
            ),
            "movido"
        );
    }

    #[test]
    fn describe_notice_prefixes_failures() {
        assert_eq!(
            describe_notice(
                ActionNotice::Failed("quebrou".to_string()),
                Language::BrazilianPortuguese
            ),
            "Falha: quebrou"
        );
    }
}
