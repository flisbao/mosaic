use crate::application::reload_policy::RestartSections;
use crate::domain::language::Language;
use crate::presentation::messages;
use crate::presentation::tab_layout::startup_notice;

const LABEL_SEPARATOR: &str = "/";

pub fn added_widget_notice(
    widget_id: &str,
    reload_notice: Option<&str>,
    language: Language,
) -> String {
    let added = messages::widget_added_notice(language, widget_id);
    match reload_notice {
        Some(reload) => format!("{added}. {reload}"),
        None => added,
    }
}

pub fn reload_notice(restart: &RestartSections, warnings: &[String], language: Language) -> String {
    let mut notice = messages::reload_done_notice(language).to_string();
    let labels = restart.labels(language);
    if !labels.is_empty() {
        notice.push_str(&format!(
            "; {} {}",
            labels.join(LABEL_SEPARATOR),
            messages::restart_required_notice(language)
        ));
    }
    if let Some(joined_warnings) = startup_notice(warnings) {
        notice.push_str(&format!(". {joined_warnings}"));
    }
    notice
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_added_widget_notice_names_the_widget_and_keeps_the_reload_result() {
        assert_eq!(
            added_widget_notice(
                "jira",
                Some("Configuração recarregada"),
                Language::BrazilianPortuguese
            ),
            "Widget \"jira\" gravado no mosaic.toml. Configuração recarregada"
        );
        assert_eq!(
            added_widget_notice("jira", None, Language::BrazilianPortuguese),
            "Widget \"jira\" gravado no mosaic.toml"
        );
    }

    #[test]
    fn plain_notice_without_restart_sections_or_warnings() {
        assert_eq!(
            reload_notice(
                &RestartSections::default(),
                &[],
                Language::BrazilianPortuguese
            ),
            "Configuração recarregada"
        );
    }

    #[test]
    fn names_each_section_that_needs_restart() {
        let llm = RestartSections {
            llm_changed: true,
            ..RestartSections::default()
        };
        let pomodoro = RestartSections {
            pomodoro_changed: true,
            ..RestartSections::default()
        };
        let both = RestartSections {
            llm_changed: true,
            pomodoro_changed: true,
            ..RestartSections::default()
        };
        let update = RestartSections {
            update_changed: true,
            ..RestartSections::default()
        };
        assert_eq!(
            reload_notice(&update, &[], Language::BrazilianPortuguese),
            "Configuração recarregada; [update] só valem após reiniciar"
        );
        assert_eq!(
            reload_notice(&llm, &[], Language::BrazilianPortuguese),
            "Configuração recarregada; [llm] só valem após reiniciar"
        );
        assert_eq!(
            reload_notice(&pomodoro, &[], Language::BrazilianPortuguese),
            "Configuração recarregada; [pomodoro]/notificações só valem após reiniciar"
        );
        assert_eq!(
            reload_notice(&both, &[], Language::BrazilianPortuguese),
            "Configuração recarregada; [llm]/[pomodoro]/notificações só valem após reiniciar"
        );
    }

    #[test]
    fn appends_warnings_joined_by_the_shared_separator() {
        let warnings = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            reload_notice(
                &RestartSections::default(),
                &warnings,
                Language::BrazilianPortuguese
            ),
            "Configuração recarregada. a | b"
        );
    }
}
