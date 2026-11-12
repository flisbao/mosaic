use super::ConfigError;
use crate::domain::language::Language;

pub(super) fn config_error(error: &ConfigError, language: Language) -> String {
    match (error, language) {
        (ConfigError::Io { path, reason, .. }, Language::English) => {
            format!("failed to read or write {path}: {reason}")
        }
        (ConfigError::Io { path, reason, .. }, Language::BrazilianPortuguese) => {
            format!("falha de leitura ou escrita em {path}: {reason}")
        }
        (ConfigError::Parse(detail), Language::English) => {
            format!("invalid configuration: {detail}")
        }
        (ConfigError::Parse(detail), Language::BrazilianPortuguese) => {
            format!("configuração inválida: {detail}")
        }
        (ConfigError::DuplicateWidgetId(id), Language::English) => {
            format!("duplicate widget in the configuration: '{id}'")
        }
        (ConfigError::DuplicateWidgetId(id), Language::BrazilianPortuguese) => {
            format!("widget duplicado na configuração: '{id}'")
        }
        (ConfigError::EmptyValue { widget, field }, Language::English) => {
            format!("widget '{widget}': '{field}' cannot be empty")
        }
        (ConfigError::EmptyValue { widget, field }, Language::BrazilianPortuguese) => {
            format!("widget '{widget}': '{field}' não pode ser vazio")
        }
        (ConfigError::InvalidValue { widget, field }, Language::English) => {
            format!("widget '{widget}': '{field}' contains an invalid name")
        }
        (ConfigError::InvalidValue { widget, field }, Language::BrazilianPortuguese) => {
            format!("widget '{widget}': '{field}' contém um nome inválido")
        }
        (ConfigError::ReservedWidgetId(id), Language::English) => {
            format!("widget '{id}': id reserved by mosaic")
        }
        (ConfigError::ReservedWidgetId(id), Language::BrazilianPortuguese) => {
            format!("widget '{id}': id reservado pelo mosaic")
        }
        (ConfigError::NotPositive { location, field }, Language::English) => {
            format!("{location}: '{field}' must be greater than zero")
        }
        (ConfigError::NotPositive { location, field }, Language::BrazilianPortuguese) => {
            format!("{location}: '{field}' precisa ser maior que zero")
        }
        (
            ConfigError::TooLarge {
                location,
                field,
                maximum,
            },
            Language::English,
        ) => format!("{location}: '{field}' cannot exceed {maximum}"),
        (
            ConfigError::TooLarge {
                location,
                field,
                maximum,
            },
            Language::BrazilianPortuguese,
        ) => format!("{location}: '{field}' não pode passar de {maximum}"),
        (ConfigError::UnknownTab { widget, tab }, Language::English) => {
            format!("widget '{widget}': tab '{tab}' does not exist in the configuration")
        }
        (ConfigError::UnknownTab { widget, tab }, Language::BrazilianPortuguese) => {
            format!("widget '{widget}': aba '{tab}' não existe na configuração")
        }
        (ConfigError::DuplicateTabName(name), Language::English) => {
            format!("duplicate tab name in the configuration: '{name}'")
        }
        (ConfigError::DuplicateTabName(name), Language::BrazilianPortuguese) => {
            format!("nome de aba duplicado na configuração: '{name}'")
        }
        (ConfigError::UnknownHarness(name), Language::English) => {
            format!("[llm]: unknown harness '{name}' (use \"claude\" or \"opencode\")")
        }
        (ConfigError::UnknownHarness(name), Language::BrazilianPortuguese) => {
            format!("[llm]: harness desconhecido '{name}' (use \"claude\" ou \"opencode\")")
        }
        (ConfigError::MissingConfigDirectory, Language::English) => {
            "could not determine the configuration directory".to_string()
        }
        (ConfigError::MissingConfigDirectory, Language::BrazilianPortuguese) => {
            "não foi possível descobrir o diretório de configuração".to_string()
        }
    }
}

pub(super) fn parse_location(language: Language, line: usize, column: usize) -> String {
    match language {
        Language::English => format!("line {line}, column {column}"),
        Language::BrazilianPortuguese => format!("linha {line}, coluna {column}"),
    }
}

pub(super) fn retired_tab_flag(language: Language, widget_id: &str) -> String {
    match language {
        Language::English => {
            format!("widget '{widget_id}': 'tab = true' was retired and is ignored")
        }
        Language::BrazilianPortuguese => {
            format!("widget '{widget_id}': 'tab = true' foi aposentado e é ignorado")
        }
    }
}

pub(super) fn legacy_notify_command(language: Language) -> &'static str {
    match language {
        Language::English => {
            "pomodoro.notify_command as text is legacy; use a list, e.g.: [\"notify-send\"]"
        }
        Language::BrazilianPortuguese => {
            "pomodoro.notify_command como texto é legado; use uma lista, ex.: [\"notify-send\"]"
        }
    }
}

pub(super) fn unknown_theme_hint(language: Language) -> &'static str {
    match language {
        Language::English => "; see `mosaic themes`",
        Language::BrazilianPortuguese => "; veja `mosaic themes`",
    }
}

pub(super) fn colors_ignored_for_terminal_theme(
    language: Language,
    section: &str,
    theme: &str,
) -> String {
    match language {
        Language::English => {
            format!("[{section}.colors] does not apply to the '{theme}' theme (it uses the terminal colors)")
        }
        Language::BrazilianPortuguese => {
            format!(
                "[{section}.colors] não vale para o tema '{theme}' (ele usa as cores do terminal)"
            )
        }
    }
}

pub(super) fn expected_boolean(language: Language, section: &str, key: &str) -> String {
    match language {
        Language::English => format!("[{section}] {key}: use true or false"),
        Language::BrazilianPortuguese => format!("[{section}] {key}: use true ou false"),
    }
}

pub(super) fn expected_quoted_text(language: Language, section: &str, key: &str) -> String {
    match language {
        Language::English => format!("[{section}] {key}: use text between quotes"),
        Language::BrazilianPortuguese => {
            format!("[{section}] {key}: use um texto entre aspas")
        }
    }
}

pub(super) fn unknown_ui_language(language: Language, value: &str) -> String {
    match language {
        Language::English => {
            format!("[ui] language: unknown language '{value}'; use \"en\" or \"pt-BR\"")
        }
        Language::BrazilianPortuguese => {
            format!("[ui] language: idioma desconhecido '{value}'; use \"en\" ou \"pt-BR\"")
        }
    }
}

pub(super) fn unknown_root_key(language: Language, key: &str) -> String {
    match language {
        Language::English => format!("unknown key '{key}' in mosaic.toml (ignored)"),
        Language::BrazilianPortuguese => {
            format!("chave desconhecida '{key}' no mosaic.toml (ignorada)")
        }
    }
}

pub(super) fn unknown_key(language: Language, location: &str, key: &str) -> String {
    match language {
        Language::English => format!("{location}: unknown key '{key}' (ignored)"),
        Language::BrazilianPortuguese => {
            format!("{location}: chave desconhecida '{key}' (ignorada)")
        }
    }
}

pub fn default_suggestions(language: Language) -> [&'static str; 3] {
    match language {
        Language::English => ["Drink some water", "Stand up and stretch", "Have a coffee"],
        Language::BrazilianPortuguese => ["Beber água", "Levantar e alongar", "Tomar um café"],
    }
}
