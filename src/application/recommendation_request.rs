use chrono::NaiveDateTime;
use thiserror::Error;

use crate::application::recommendation::{build_plan, RecommendationPlan, WidgetContext};

#[derive(Debug, Clone, Error, PartialEq)]
#[error("{0}")]
pub struct CriteriaError(pub String);

pub trait CriteriaSource {
    fn load(&self) -> Result<String, CriteriaError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendPolicy {
    pub confirm_before_send: bool,
    pub target_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecommendationStep {
    Notice(String),
    Confirm {
        text: String,
        plan: RecommendationPlan,
    },
    Send(RecommendationPlan),
}

pub fn prepare_recommendation(
    criteria: &dyn CriteriaSource,
    contexts: &[WidgetContext],
    now: NaiveDateTime,
    policy: &SendPolicy,
) -> RecommendationStep {
    let criteria_text = match criteria.load() {
        Ok(text) => text,
        Err(error) => return RecommendationStep::Notice(error.to_string()),
    };
    match build_plan(&criteria_text, now, contexts) {
        Err(error) => RecommendationStep::Notice(error.to_string()),
        Ok(plan) if policy.confirm_before_send => RecommendationStep::Confirm {
            text: confirmation_text(&plan, &policy.target_label),
            plan,
        },
        Ok(plan) => RecommendationStep::Send(plan),
    }
}

fn confirmation_text(plan: &RecommendationPlan, target_label: &str) -> String {
    format!(
        "Enviar {} itens (~{} tokens, estimativa) para {target_label}?",
        plan.item_count, plan.estimated_tokens
    )
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::application::recommendation::LlmSelection;
    use crate::domain::widget::{
        DetailItem, DetailSection, Summary, WidgetId, WidgetRecord, WidgetSnapshot,
    };
    use crate::test_support::recommendation_request_now as now;

    struct FakeCriteria(Result<String, CriteriaError>);

    impl CriteriaSource for FakeCriteria {
        fn load(&self) -> Result<String, CriteriaError> {
            self.0.clone()
        }
    }

    fn policy(confirm_before_send: bool) -> SendPolicy {
        SendPolicy {
            confirm_before_send,
            target_label: "claude/sonnet".to_string(),
        }
    }

    fn selection(sections: &[&str]) -> LlmSelection {
        LlmSelection {
            sections: sections.iter().map(|name| name.to_string()).collect(),
            max_items: 20,
            max_characters_per_item: 120,
        }
    }

    fn record_with_one_item() -> WidgetRecord {
        let mut record = WidgetRecord::loading();
        record.apply::<String>(
            Ok(WidgetSnapshot {
                mini: None,
                alerts: vec![],
                summary: Summary {
                    blocks: vec![],
                    title: "Kanban".to_string(),
                    lines: vec![],
                },
                detail: Some(vec![DetailSection {
                    heading: "Tarefas".to_string(),
                    items: vec![DetailItem {
                        id: Some("A-1".to_string()),
                        text: "Fazer algo".to_string(),
                        flag: None,
                        meta: vec![],
                    }],
                    blocks: vec![],
                }]),
                actions: vec![],
            }),
            now(),
        );
        record
    }

    #[test]
    fn criteria_read_failure_becomes_a_notice_with_the_reason() {
        let criteria = FakeCriteria(Err(CriteriaError(
            "Não foi possível ler x: nope".to_string(),
        )));
        assert_eq!(
            prepare_recommendation(&criteria, &[], now(), &policy(true)),
            RecommendationStep::Notice("Não foi possível ler x: nope".to_string())
        );
    }

    #[test]
    fn no_widget_selected_becomes_a_notice() {
        let criteria = FakeCriteria(Ok("critérios".to_string()));
        let step = prepare_recommendation(&criteria, &[], now(), &policy(true));
        let RecommendationStep::Notice(text) = step else {
            panic!("esperava Notice");
        };
        assert!(
            text.contains("nenhum widget configurado para a IA"),
            "{text}"
        );
    }

    #[test]
    fn selected_widget_without_items_becomes_a_no_items_notice() {
        let criteria = FakeCriteria(Ok("critérios".to_string()));
        let widget_id = WidgetId::new("kanban");
        let chosen = selection(&["Tarefas"]);
        let loading = WidgetRecord::loading();
        let contexts = [WidgetContext {
            widget_id: &widget_id,
            selection: &chosen,
            state: &loading,
        }];
        assert_eq!(
            prepare_recommendation(&criteria, &contexts, now(), &policy(true)),
            RecommendationStep::Notice("nenhum item nas seções selecionadas".to_string())
        );
    }

    #[test]
    fn confirm_before_send_asks_for_confirmation_with_count_and_target() {
        let criteria = FakeCriteria(Ok("critérios".to_string()));
        let widget_id = WidgetId::new("kanban");
        let chosen = selection(&["Tarefas"]);
        let record = record_with_one_item();
        let contexts = [WidgetContext {
            widget_id: &widget_id,
            selection: &chosen,
            state: &record,
        }];
        let step = prepare_recommendation(&criteria, &contexts, now(), &policy(true));
        let RecommendationStep::Confirm { text, plan } = step else {
            panic!("esperava Confirm");
        };
        assert_eq!(plan.item_count, 1);
        assert_eq!(
            text,
            format!(
                "Enviar 1 itens (~{} tokens, estimativa) para claude/sonnet?",
                plan.estimated_tokens
            )
        );
    }

    #[test]
    fn without_confirmation_the_plan_is_sent_directly() {
        let criteria = FakeCriteria(Ok("critérios".to_string()));
        let widget_id = WidgetId::new("kanban");
        let chosen = selection(&["Tarefas"]);
        let record = record_with_one_item();
        let contexts = [WidgetContext {
            widget_id: &widget_id,
            selection: &chosen,
            state: &record,
        }];
        assert!(matches!(
            prepare_recommendation(&criteria, &contexts, now(), &policy(false)),
            RecommendationStep::Send(plan) if plan.item_count == 1
        ));
    }
}
