use crate::domain::language::Language;

pub fn confirmation_hint(language: Language) -> &'static str {
    match language {
        Language::English => "[y] confirm  [n] cancel",
        Language::BrazilianPortuguese => "[s] confirma  [n] cancela",
    }
}

pub fn help_global_heading(language: Language) -> &'static str {
    match language {
        Language::English => "On any screen",
        Language::BrazilianPortuguese => "Em qualquer tela",
    }
}

pub fn help_dismiss_hint(language: Language) -> &'static str {
    match language {
        Language::English => "Esc or ? closes this help",
        Language::BrazilianPortuguese => "Esc ou ? fecha esta ajuda",
    }
}

pub fn empty_panel_title(language: Language) -> &'static str {
    match language {
        Language::English => "no widgets yet",
        Language::BrazilianPortuguese => "nenhum widget ainda",
    }
}

pub fn empty_panel_action(language: Language) -> &'static str {
    match language {
        Language::English => "press + to add a widget",
        Language::BrazilianPortuguese => "pressione + para adicionar um widget",
    }
}

pub fn empty_panel_list_hint(language: Language) -> &'static str {
    match language {
        Language::English => "or see the available plugins: mosaic plugins list",
        Language::BrazilianPortuguese => "ou veja os plugins disponíveis: mosaic plugins list",
    }
}

pub fn alert_dismiss_hint(language: Language) -> &'static str {
    match language {
        Language::English => "Any key closes",
        Language::BrazilianPortuguese => "Qualquer tecla fecha",
    }
}

pub fn send_to_assistant_title(language: Language) -> &'static str {
    match language {
        Language::English => "Send to AI",
        Language::BrazilianPortuguese => "Enviar à IA",
    }
}

pub fn actions_title(language: Language) -> &'static str {
    match language {
        Language::English => "Actions",
        Language::BrazilianPortuguese => "Ações",
    }
}

pub fn confirm_action_title(language: Language) -> &'static str {
    match language {
        Language::English => "Confirm action",
        Language::BrazilianPortuguese => "Confirmar ação",
    }
}

pub fn item_line(language: Language, item_id: &str) -> String {
    match language {
        Language::English => format!("Item: {item_id}"),
        Language::BrazilianPortuguese => format!("Item: {item_id}"),
    }
}

pub fn pomodoro_paused_suffix(language: Language) -> &'static str {
    match language {
        Language::English => " (paused)",
        Language::BrazilianPortuguese => " (pausado)",
    }
}

pub fn pomodoro_stopped_suffix(language: Language) -> &'static str {
    match language {
        Language::English => " (stopped)",
        Language::BrazilianPortuguese => " (parado)",
    }
}

pub fn enlarge_terminal_warning(language: Language, width: u16, height: u16) -> String {
    match language {
        Language::English => format!("Enlarge the terminal (minimum {width}x{height})"),
        Language::BrazilianPortuguese => format!("Amplie o terminal (mínimo {width}x{height})"),
    }
}

pub fn clock_date_format(language: Language) -> &'static str {
    match language {
        Language::English => "%Y-%m-%d",
        Language::BrazilianPortuguese => "%d/%m/%Y",
    }
}

pub fn loading(language: Language) -> &'static str {
    match language {
        Language::English => "loading…",
        Language::BrazilianPortuguese => "carregando…",
    }
}

pub fn failure_prefix(language: Language) -> &'static str {
    match language {
        Language::English => "⚠ failed: ",
        Language::BrazilianPortuguese => "⚠ falha: ",
    }
}

pub fn overflow_suffix(language: Language) -> &'static str {
    match language {
        Language::English => " items",
        Language::BrazilianPortuguese => " itens",
    }
}

pub fn stale_age_prefix(language: Language) -> &'static str {
    match language {
        Language::English => "not updated for ",
        Language::BrazilianPortuguese => "sem atualizar há ",
    }
}

pub fn age_minutes(minutes: i64) -> String {
    format!("{minutes} min")
}

pub fn age_hours(hours: i64) -> String {
    format!("{hours} h")
}

pub fn age_days(language: Language, days: i64) -> String {
    match (language, days) {
        (Language::English, 1) => "1 day".to_string(),
        (Language::English, days) => format!("{days} days"),
        (Language::BrazilianPortuguese, 1) => "1 dia".to_string(),
        (Language::BrazilianPortuguese, days) => format!("{days} dias"),
    }
}

pub fn invalid_block(language: Language) -> &'static str {
    match language {
        Language::English => "⚠ invalid block",
        Language::BrazilianPortuguese => "⚠ bloco inválido",
    }
}

pub fn no_actions_notice(language: Language) -> &'static str {
    match language {
        Language::English => "No actions available here",
        Language::BrazilianPortuguese => "Nenhuma ação disponível aqui",
    }
}

pub fn recommendation_busy_notice(language: Language) -> &'static str {
    match language {
        Language::English => "The AI is already processing a request",
        Language::BrazilianPortuguese => "A IA já está processando um pedido",
    }
}

pub fn action_pending_notice(language: Language) -> &'static str {
    match language {
        Language::English => "Action in progress",
        Language::BrazilianPortuguese => "Ação em andamento",
    }
}

pub fn no_detail_notice(language: Language) -> &'static str {
    match language {
        Language::English => "This widget has no detail to show yet",
        Language::BrazilianPortuguese => "Este widget ainda não tem detalhe para mostrar",
    }
}

pub fn pomodoro_disabled_notice(language: Language) -> &'static str {
    match language {
        Language::English => "Pomodoro is turned off in the configuration",
        Language::BrazilianPortuguese => "Pomodoro desligado na configuração",
    }
}

pub fn basic_terminal_warning(language: Language) -> &'static str {
    match language {
        Language::English => "terminal without 256 colors: using the default theme",
        Language::BrazilianPortuguese => "terminal sem 256 cores: usando o tema default",
    }
}

pub fn reload_done_notice(language: Language) -> &'static str {
    match language {
        Language::English => "Configuration reloaded",
        Language::BrazilianPortuguese => "Configuração recarregada",
    }
}

pub fn restart_required_notice(language: Language) -> &'static str {
    match language {
        Language::English => "only apply after a restart",
        Language::BrazilianPortuguese => "só valem após reiniciar",
    }
}

pub fn widget_added_notice(language: Language, widget_id: &str) -> String {
    match language {
        Language::English => format!("Widget \"{widget_id}\" saved to mosaic.toml"),
        Language::BrazilianPortuguese => format!("Widget \"{widget_id}\" gravado no mosaic.toml"),
    }
}

pub fn update_available_notice(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("version {version} available; run mosaic update"),
        Language::BrazilianPortuguese => {
            format!("versão {version} disponível; rode mosaic update")
        }
    }
}

pub fn action_done_fallback(language: Language) -> &'static str {
    match language {
        Language::English => "Action completed",
        Language::BrazilianPortuguese => "Ação concluída",
    }
}

pub fn recommendation_ready(language: Language) -> &'static str {
    match language {
        Language::English => "Recommendation updated",
        Language::BrazilianPortuguese => "Recomendação atualizada",
    }
}

pub fn recommendation_ready_but_unsaved(language: Language, reason: &str) -> String {
    match language {
        Language::English => format!(
            "{}, but it could not be saved: {reason}",
            recommendation_ready(language)
        ),
        Language::BrazilianPortuguese => format!(
            "{}, mas não foi possível salvar: {reason}",
            recommendation_ready(language)
        ),
    }
}

pub fn action_failed(language: Language, message: &str) -> String {
    match language {
        Language::English => format!("Failed: {message}"),
        Language::BrazilianPortuguese => format!("Falha: {message}"),
    }
}

pub fn add_widget_title(language: Language) -> &'static str {
    match language {
        Language::English => "Add widget",
        Language::BrazilianPortuguese => "Adicionar widget",
    }
}

pub fn review_title_suffix(language: Language) -> &'static str {
    match language {
        Language::English => "review",
        Language::BrazilianPortuguese => "revisar",
    }
}

pub fn choose_plugin_heading(language: Language) -> &'static str {
    match language {
        Language::English => "Choose the widget plugin",
        Language::BrazilianPortuguese => "Escolha o plugin do widget",
    }
}

pub fn no_plugin_heading(language: Language) -> &'static str {
    match language {
        Language::English => "No plugin installed.",
        Language::BrazilianPortuguese => "Nenhum plugin instalado.",
    }
}

pub fn install_hint(language: Language) -> &'static str {
    match language {
        Language::English => "Install one and open this flow again:",
        Language::BrazilianPortuguese => "Instale um e abra este fluxo de novo:",
    }
}

pub fn install_command(language: Language) -> &'static str {
    match language {
        Language::English => "mosaic plugins install <name>",
        Language::BrazilianPortuguese => "mosaic plugins install <nome>",
    }
}

pub fn official_plugins_heading(language: Language) -> &'static str {
    match language {
        Language::English => "Plugins in the official catalog:",
        Language::BrazilianPortuguese => "Plugins do catálogo oficial:",
    }
}

pub fn ignored_prefix(language: Language) -> &'static str {
    match language {
        Language::English => "ignored: ",
        Language::BrazilianPortuguese => "ignorado: ",
    }
}

pub fn review_heading(language: Language) -> &'static str {
    match language {
        Language::English => "This snippet will be appended to mosaic.toml:",
        Language::BrazilianPortuguese => "Este trecho será acrescentado ao mosaic.toml:",
    }
}

pub fn save_failed_prefix(language: Language) -> &'static str {
    match language {
        Language::English => "Not saved: ",
        Language::BrazilianPortuguese => "Não gravou: ",
    }
}

pub fn note_prefix(language: Language) -> &'static str {
    match language {
        Language::English => "Note: ",
        Language::BrazilianPortuguese => "Atenção: ",
    }
}

pub fn list_footer(language: Language) -> &'static str {
    match language {
        Language::English => "↑↓ choose  Enter select  Esc cancel",
        Language::BrazilianPortuguese => "↑↓ escolhe  Enter seleciona  Esc cancela",
    }
}

pub fn empty_footer(language: Language) -> &'static str {
    match language {
        Language::English => "Esc closes",
        Language::BrazilianPortuguese => "Esc fecha",
    }
}

pub fn text_footer(language: Language) -> &'static str {
    match language {
        Language::English => "Enter confirm  Esc cancel",
        Language::BrazilianPortuguese => "Enter confirma  Esc cancela",
    }
}

pub fn choice_footer(language: Language) -> &'static str {
    match language {
        Language::English => "↑↓ choose  Enter confirm  Esc cancel",
        Language::BrazilianPortuguese => "↑↓ escolhe  Enter confirma  Esc cancela",
    }
}

pub fn review_footer(language: Language) -> &'static str {
    match language {
        Language::English => "Enter save and reload  Esc cancel",
        Language::BrazilianPortuguese => "Enter grava e recarrega  Esc cancela",
    }
}

pub fn more_above(language: Language) -> &'static str {
    match language {
        Language::English => "  ↑ more",
        Language::BrazilianPortuguese => "  ↑ mais",
    }
}

pub fn more_below(language: Language) -> &'static str {
    match language {
        Language::English => "  ↓ more",
        Language::BrazilianPortuguese => "  ↓ mais",
    }
}

pub fn required_hint(language: Language) -> &'static str {
    match language {
        Language::English => "required",
        Language::BrazilianPortuguese => "obrigatório",
    }
}

pub fn default_hint(language: Language, default: &str) -> String {
    match language {
        Language::English => format!("Enter uses the default ({default})"),
        Language::BrazilianPortuguese => format!("Enter usa o padrão ({default})"),
    }
}

pub fn optional_hint(language: Language) -> &'static str {
    match language {
        Language::English => "optional: Enter skips",
        Language::BrazilianPortuguese => "opcional: Enter pula",
    }
}

pub fn llm_disabled_notice(language: Language) -> &'static str {
    match language {
        Language::English => "AI turned off in the configuration",
        Language::BrazilianPortuguese => "IA desligada na configuração",
    }
}

pub fn no_state_directory_llm_warning(language: Language) -> &'static str {
    match language {
        Language::English => "AI turned off: there is no user state directory",
        Language::BrazilianPortuguese => "IA desligada: não há diretório de estado do usuário",
    }
}

pub fn no_state_directory_store_warning(language: Language) -> &'static str {
    match language {
        Language::English => "recommendations will not be saved: there is no user state directory",
        Language::BrazilianPortuguese => {
            "recomendações não serão salvas: não há diretório de estado do usuário"
        }
    }
}

pub fn cannot_add_widget(language: Language, reason: &str) -> String {
    match language {
        Language::English => format!("could not add a widget: {reason}"),
        Language::BrazilianPortuguese => format!("não foi possível adicionar um widget: {reason}"),
    }
}

pub fn terminal_failure(language: Language, reason: &str) -> String {
    match language {
        Language::English => format!("terminal failure: {reason}"),
        Language::BrazilianPortuguese => format!("falha de terminal: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_assertions::assert_translated;

    type FixedMessage = fn(Language) -> &'static str;

    #[test]
    fn every_panel_message_is_translated() {
        let fixed: [(&str, FixedMessage); 35] = [
            ("confirmation", confirmation_hint),
            ("help global", help_global_heading),
            ("help dismiss", help_dismiss_hint),
            ("empty title", empty_panel_title),
            ("empty action", empty_panel_action),
            ("empty list", empty_panel_list_hint),
            ("alert dismiss", alert_dismiss_hint),
            ("send title", send_to_assistant_title),
            ("actions title", actions_title),
            ("confirm title", confirm_action_title),
            ("paused", pomodoro_paused_suffix),
            ("stopped", pomodoro_stopped_suffix),
            ("date", clock_date_format),
            ("loading", loading),
            ("failure", failure_prefix),
            ("overflow", overflow_suffix),
            ("stale", stale_age_prefix),
            ("invalid block", invalid_block),
            ("no actions", no_actions_notice),
            ("busy", recommendation_busy_notice),
            ("pending", action_pending_notice),
            ("no detail", no_detail_notice),
            ("pomodoro off", pomodoro_disabled_notice),
            ("basic terminal", basic_terminal_warning),
            ("reload done", reload_done_notice),
            ("restart", restart_required_notice),
            ("action fallback", action_done_fallback),
            ("recommendation", recommendation_ready),
            ("modal title", add_widget_title),
            ("review suffix", review_title_suffix),
            ("choose plugin", choose_plugin_heading),
            ("no plugin", no_plugin_heading),
            ("install hint", install_hint),
            ("install command", install_command),
            ("official", official_plugins_heading),
        ];
        for (label, message) in fixed {
            assert_translated(label, |language| message(language).to_string());
        }
        let more: [(&str, FixedMessage); 14] = [
            ("ignored", ignored_prefix),
            ("review heading", review_heading),
            ("save failed", save_failed_prefix),
            ("note", note_prefix),
            ("list footer", list_footer),
            ("empty footer", empty_footer),
            ("text footer", text_footer),
            ("choice footer", choice_footer),
            ("review footer", review_footer),
            ("more above", more_above),
            ("more below", more_below),
            ("required", required_hint),
            ("optional", optional_hint),
            ("failure again", failure_prefix),
        ];
        for (label, message) in more {
            assert_translated(label, |language| message(language).to_string());
        }
    }

    #[test]
    fn every_parameterized_panel_message_is_translated() {
        assert_translated("cannot add", |language| cannot_add_widget(language, "x"));
        assert_translated("terminal", |language| terminal_failure(language, "x"));
        assert_translated("llm off", |language| {
            llm_disabled_notice(language).to_string()
        });
        assert_translated("llm state", |language| {
            no_state_directory_llm_warning(language).to_string()
        });
        assert_translated("store state", |language| {
            no_state_directory_store_warning(language).to_string()
        });
        assert_translated("enlarge", |language| {
            enlarge_terminal_warning(language, 80, 24)
        });
        assert_translated("added", |language| widget_added_notice(language, "jira"));
        assert_translated("update", |language| {
            update_available_notice(language, "1.0.0")
        });
        assert_translated("unsaved", |language| {
            recommendation_ready_but_unsaved(language, "disk full")
        });
        assert_translated("failed", |language| action_failed(language, "boom"));
        assert_translated("default", |language| default_hint(language, "x"));
        assert_translated("days", |language| age_days(language, 3));
        assert_translated("one day", |language| age_days(language, 1));
    }

    #[test]
    fn english_notices_are_natural() {
        assert_eq!(
            widget_added_notice(Language::English, "jira"),
            "Widget \"jira\" saved to mosaic.toml"
        );
        assert_eq!(
            update_available_notice(Language::English, "0.2.0"),
            "version 0.2.0 available; run mosaic update"
        );
        assert_eq!(age_days(Language::English, 2), "2 days");
    }
}
