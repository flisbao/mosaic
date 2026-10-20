use thiserror::Error;

use crate::domain::snapshot_report::Problem;
use crate::domain::widget::{Flag, WidgetSnapshot};
use crate::domain::widget_content::Tone;
use crate::infrastructure::protocol_report::{parse_snapshot_report, Severity};

#[derive(Debug, Error)]
#[error("saída de plugin não é um JSON válido de snapshot: {0}")]
pub struct ProtocolError(String);

pub const MAXIMUM_ERROR_NOTICES: usize = 3;

impl ProtocolError {
    pub(crate) fn new(message: impl ToString) -> Self {
        Self(message.to_string())
    }
}

pub fn parse_snapshot(json: &str) -> Result<WidgetSnapshot, ProtocolError> {
    let parsed = parse_snapshot_report(json)?;
    let mut snapshot = parsed.snapshot;
    for notice in error_notices(&parsed.problems).into_iter().rev() {
        snapshot
            .summary
            .put_notice_first(notice, Tone::Warn, Some(Flag::Attention));
    }
    Ok(snapshot)
}

fn error_notices(problems: &[Problem]) -> Vec<String> {
    let errors: Vec<&Problem> = problems
        .iter()
        .filter(|problem| problem.severity == Severity::Error)
        .collect();
    if errors.len() <= MAXIMUM_ERROR_NOTICES {
        return errors.into_iter().map(problem_notice).collect();
    }
    let shown = MAXIMUM_ERROR_NOTICES - 1;
    let mut notices: Vec<String> = errors[..shown]
        .iter()
        .map(|problem| problem_notice(problem))
        .collect();
    notices.push(format!("… e mais {} erros", errors.len() - shown));
    notices
}

const INVALID_BLOCK_LABEL: &str = "bloco inválido";
const INVALID_MINI_LABEL: &str = "mini inválido";
const MINI_PATH_ROOT: &str = "mini";

fn problem_notice(problem: &Problem) -> String {
    format!(
        "{}: {}: {}",
        notice_label(&problem.path),
        problem.path,
        problem.message
    )
}

fn notice_label(path: &str) -> &'static str {
    if path.split(['.', '[']).next() == Some(MINI_PATH_ROOT) {
        INVALID_MINI_LABEL
    } else {
        INVALID_BLOCK_LABEL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget::{ActionScope, Flag};

    #[test]
    fn parses_summary_only_snapshot() {
        let snapshot =
            parse_snapshot(r#"{"summary":{"title":"Kanban","lines":[{"text":"inbox 3"}]}}"#)
                .unwrap();
        assert_eq!(snapshot.summary.title, "Kanban");
        assert_eq!(snapshot.summary.lines[0].text, "inbox 3");
        assert_eq!(snapshot.summary.lines[0].flag, None);
        assert_eq!(snapshot.detail, None);
        assert!(snapshot.actions.is_empty());
    }

    #[test]
    fn parses_detail_items_with_ids_and_flags() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[{"text":"x","flag":"updated"}]},
                "detail":{"sections":[{"heading":"inbox","items":[
                    {"id":"inbox/a.md","text":"Tarefa A","flag":"attention"},
                    {"text":"sem id"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(snapshot.summary.lines[0].flag, Some(Flag::Updated));
        let sections = snapshot.detail.unwrap();
        assert_eq!(sections[0].heading, "inbox");
        assert_eq!(sections[0].items[0].id.as_deref(), Some("inbox/a.md"));
        assert_eq!(sections[0].items[0].flag, Some(Flag::Attention));
        assert_eq!(sections[0].items[1].id, None);
    }

    #[test]
    fn parses_item_action_with_params() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "actions":[{"id":"move","label":"Mover","description":"Mover para done",
                            "applies_to":"item","params":{"to":"done"}},
                           {"id":"refresh","label":"Atualizar"}]}"#,
        )
        .unwrap();
        let move_action = &snapshot.actions[0];
        assert_eq!(move_action.scope, ActionScope::Item);
        assert_eq!(move_action.description.as_deref(), Some("Mover para done"));
        assert!(move_action.params_json.contains("\"to\""));
        let refresh = &snapshot.actions[1];
        assert_eq!(refresh.scope, ActionScope::Widget);
        assert_eq!(refresh.params_json, "{}");
    }

    #[test]
    fn parses_item_meta_in_order() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "detail":{"sections":[{"heading":"h","items":[
                    {"text":"x","meta":{"prioridade":"alta","parado_ha":"2h30"}}]}]}}"#,
        )
        .unwrap();
        let item = &snapshot.detail.unwrap()[0].items[0];
        assert_eq!(
            item.meta,
            vec![
                ("prioridade".to_string(), "alta".to_string()),
                ("parado_ha".to_string(), "2h30".to_string())
            ]
        );
    }

    #[test]
    fn missing_meta_is_empty() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "detail":{"sections":[{"heading":"h","items":[{"text":"x"}]}]}}"#,
        )
        .unwrap();
        assert!(snapshot.detail.unwrap()[0].items[0].meta.is_empty());
    }

    #[test]
    fn non_text_meta_values_are_ignored() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[]},
                "detail":{"sections":[{"heading":"h","items":[
                    {"text":"x","meta":{"a":"x","b":3}}]}]}}"#,
        )
        .unwrap();
        let item = &snapshot.detail.unwrap()[0].items[0];
        assert_eq!(item.meta, vec![("a".to_string(), "x".to_string())]);
    }

    #[test]
    fn unknown_flag_is_ignored() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[{"text":"x","flag":"explosion"}]}}"#,
        )
        .unwrap();
        assert_eq!(snapshot.summary.lines[0].flag, None);
    }

    #[test]
    fn truncated_json_is_an_error() {
        let error = parse_snapshot(r#"{"summary":{"title":"K""#).unwrap_err();
        assert!(error.to_string().contains("JSON"));
    }

    #[test]
    fn missing_summary_title_is_an_error() {
        assert!(parse_snapshot(r#"{"summary":{"lines":[]}}"#).is_err());
    }

    #[test]
    fn errors_become_warning_lines_at_the_top_of_the_summary() {
        let snapshot =
            parse_snapshot(r#"{"protocol":"x","summary":{"title":"K","lines":[{"text":"ok"}]}}"#)
                .unwrap();
        let first = &snapshot.summary.lines[0];
        assert_eq!(
            first.text,
            "bloco inválido: protocol: precisa ser um número inteiro positivo"
        );
        assert_eq!(first.flag, Some(Flag::Attention));
        assert_eq!(snapshot.summary.lines[1].text, "ok");
    }

    #[test]
    fn mini_errors_are_labelled_as_mini_not_as_block() {
        let snapshot = parse_snapshot(
            r#"{"summary":{"title":"K","lines":[{"text":"ok"}]},"mini":{"value":" ","text":"t"}}"#,
        )
        .unwrap();
        let first = &snapshot.summary.lines[0];
        assert_eq!(
            first.text,
            "mini inválido: mini.value: precisa ter conteúdo"
        );
        assert_eq!(first.flag, Some(Flag::Attention));
    }

    #[test]
    fn protocol_above_supported_shows_only_the_update_notice() {
        let snapshot = parse_snapshot(r#"{"protocol":2,"summary":{"title":"K"}}"#).unwrap();
        assert_eq!(snapshot.summary.lines.len(), 1);
        assert!(snapshot.summary.lines[0].text.contains("protocolo 2"));
    }

    #[test]
    fn many_errors_are_summarised_in_a_bounded_number_of_notices() {
        let broken = vec!["1"; 5000].join(",");
        let json = format!(
            r#"{{"summary":{{"title":"K","lines":[{broken}],
                "blocks":[{{"type":"lines","lines":[{{"text":"ok"}}]}}]}}}}"#
        );
        let started = std::time::Instant::now();
        let snapshot = parse_snapshot(&json).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(snapshot.summary.blocks.len() <= 1 + MAXIMUM_ERROR_NOTICES);
        assert!(snapshot.summary.lines.len() <= MAXIMUM_ERROR_NOTICES);
    }
}
