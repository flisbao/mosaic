use thiserror::Error;

use crate::application::ports::SourceError;
use crate::domain::snapshot_report::{ParsedSnapshot, Problem, Severity};
use crate::domain::widget::{Mini, WidgetSnapshot};

const EXIT_CODE_OK: i32 = 0;
const EXIT_CODE_INVALID: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub text: String,
    pub exit_code: i32,
}

#[derive(Debug, Error, PartialEq)]
#[error("{0}")]
pub struct SnapshotParseFailure(pub String);

pub fn build_check_report(
    raw: Result<String, SourceError>,
    parse: &dyn Fn(&str) -> Result<ParsedSnapshot, SnapshotParseFailure>,
    preview: &dyn Fn(&WidgetSnapshot) -> Vec<String>,
) -> CheckOutcome {
    let parsed = raw
        .map_err(|error| error.to_string())
        .and_then(|output| parse(&output).map_err(|error| error.to_string()));
    match parsed {
        Ok(parsed) => report_parsed(parsed, preview),
        Err(message) => CheckOutcome {
            text: format!("ERRO {message}"),
            exit_code: EXIT_CODE_INVALID,
        },
    }
}

fn report_parsed(
    parsed: ParsedSnapshot,
    preview: &dyn Fn(&WidgetSnapshot) -> Vec<String>,
) -> CheckOutcome {
    let has_errors = parsed
        .problems
        .iter()
        .any(|problem| problem.severity == Severity::Error);
    let mut problems = parsed.problems;
    problems.sort_by_key(|problem| problem.severity != Severity::Error);
    let mut lines: Vec<String> = problems.iter().map(problem_line).collect();
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(parsed.snapshot.summary.title.clone());
    lines.extend(parsed.snapshot.mini.as_ref().map(mini_line));
    lines.extend(preview(&parsed.snapshot));
    CheckOutcome {
        text: lines.join("\n"),
        exit_code: if has_errors {
            EXIT_CODE_INVALID
        } else {
            EXIT_CODE_OK
        },
    }
}

fn mini_line(mini: &Mini) -> String {
    format!("mini: {} {}", mini.value, mini.text)
}

fn problem_line(problem: &Problem) -> String {
    let label = match problem.severity {
        Severity::Error => "ERRO",
        Severity::Warning => "AVISO",
    };
    format!("{label} {}: {}", problem.path, problem.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::snapshot_report::{Problem, Severity};
    use crate::domain::widget::{Mini, WidgetSnapshot};
    use crate::domain::widget_content::Tone;

    fn snapshot() -> WidgetSnapshot {
        WidgetSnapshot::titled("Agenda")
    }

    fn problem(path: &str, message: &str, severity: Severity) -> Problem {
        Problem {
            path: path.to_string(),
            message: message.to_string(),
            severity,
        }
    }

    fn report(problems: Vec<Problem>) -> CheckOutcome {
        let parse = move |_: &str| {
            Ok(ParsedSnapshot {
                snapshot: snapshot(),
                problems: problems.clone(),
            })
        };
        let preview = |_: &WidgetSnapshot| vec!["linha da prévia".to_string()];
        build_check_report(Ok("{}".to_string()), &parse, &preview)
    }

    #[test]
    fn valid_snapshot_has_exit_code_zero_and_shows_the_preview() {
        let outcome = report(vec![]);
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.text, "Agenda\nlinha da prévia");
    }

    #[test]
    fn warnings_do_not_change_the_exit_code() {
        let outcome = report(vec![problem("x", "campo desconhecido", Severity::Warning)]);
        assert_eq!(outcome.exit_code, 0);
        assert!(outcome
            .text
            .starts_with("AVISO x: campo desconhecido\n\nAgenda"));
    }

    #[test]
    fn errors_set_exit_code_one_and_are_listed_first() {
        let outcome = report(vec![
            problem("a", "aviso", Severity::Warning),
            problem("b", "falha", Severity::Error),
        ]);
        assert_eq!(outcome.exit_code, 1);
        let lines: Vec<&str> = outcome.text.lines().collect();
        assert_eq!(lines[0], "ERRO b: falha");
        assert_eq!(lines[1], "AVISO a: aviso");
        assert!(outcome.text.contains("linha da prévia"));
    }

    #[test]
    fn execution_failure_is_reported_as_an_error() {
        let parse = |_: &str| -> Result<ParsedSnapshot, SnapshotParseFailure> { unreachable!() };
        let preview = |_: &WidgetSnapshot| -> Vec<String> { unreachable!() };
        let outcome = build_check_report(
            Err(SourceError::Failed("plugin quebrou".to_string())),
            &parse,
            &preview,
        );
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.text, "ERRO plugin quebrou");
    }

    #[test]
    fn invalid_json_is_reported_as_an_error() {
        let parse = |_: &str| Err(SnapshotParseFailure("JSON ruim".to_string()));
        let preview = |_: &WidgetSnapshot| -> Vec<String> { unreachable!() };
        let outcome = build_check_report(Ok("{".to_string()), &parse, &preview);
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.text, "ERRO JSON ruim");
    }

    #[test]
    fn mini_is_shown_between_the_title_and_the_preview() {
        let parse = |_: &str| -> Result<ParsedSnapshot, SnapshotParseFailure> {
            Ok(ParsedSnapshot {
                snapshot: WidgetSnapshot {
                    mini: Some(Mini {
                        value: "12".to_string(),
                        text: "tarefas".to_string(),
                        tone: Tone::Warn,
                    }),
                    ..snapshot()
                },
                problems: vec![],
            })
        };
        let preview = |_: &WidgetSnapshot| vec!["linha da prévia".to_string()];
        let outcome = build_check_report(Ok("{}".to_string()), &parse, &preview);
        assert_eq!(outcome.text, "Agenda\nmini: 12 tarefas\nlinha da prévia");
    }

    #[test]
    fn problem_lines_have_the_documented_format() {
        let outcome = report(vec![
            problem(
                "summary.blocks[0].rows[2].text",
                "campo obrigatório ausente",
                Severity::Error,
            ),
            problem("protocol", "x", Severity::Warning),
        ]);
        let lines: Vec<&str> = outcome.text.lines().collect();
        assert_eq!(
            lines[0],
            "ERRO summary.blocks[0].rows[2].text: campo obrigatório ausente"
        );
        assert_eq!(lines[1], "AVISO protocol: x");
    }
}
