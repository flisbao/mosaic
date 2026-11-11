use super::raw::{LLM_KEYS, POMODORO_KEYS, ROOT_KEYS, TAB_KEYS, THEME_KEYS, WIDGET_KEYS};

const TABS_KEY: &str = "tabs";
const WIDGETS_KEY: &str = "widgets";
const WIDGET_ID_KEY: &str = "id";
const SECTIONS: [(&str, &[&str]); 3] = [
    ("pomodoro", &POMODORO_KEYS),
    ("llm", &LLM_KEYS),
    ("theme", &THEME_KEYS),
];
const MAXIMUM_KEY_CHARACTERS: usize = 40;
const ELLIPSIS: char = '…';
const CONTROL_CHARACTER_REPLACEMENT: char = '?';

pub(super) fn unknown_key_warnings(document: &toml::Table) -> Vec<String> {
    let mut warnings: Vec<String> = unknown_keys(document, &ROOT_KEYS)
        .map(|key| format!("chave desconhecida '{key}' no mosaic.toml (ignorada)"))
        .collect();
    for tab in tables_of(document, TABS_KEY) {
        warnings.extend(unknown_keys(tab, &TAB_KEYS).map(|key| warning("[[tabs]]", &key)));
    }
    for widget in tables_of(document, WIDGETS_KEY) {
        let id = widget
            .get(WIDGET_ID_KEY)
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        let location = format!("widget '{id}'");
        warnings.extend(unknown_keys(widget, &WIDGET_KEYS).map(|key| warning(&location, &key)));
    }
    for (section, known) in SECTIONS {
        if let Some(table) = document.get(section).and_then(toml::Value::as_table) {
            let location = format!("[{section}]");
            warnings.extend(unknown_keys(table, known).map(|key| warning(&location, &key)));
        }
    }
    warnings
}

fn warning(location: &str, key: &str) -> String {
    format!("{location}: chave desconhecida '{key}' (ignorada)")
}

fn tables_of<'a>(document: &'a toml::Table, key: &str) -> impl Iterator<Item = &'a toml::Table> {
    document
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
}

fn unknown_keys<'a>(
    table: &'a toml::Table,
    known: &'a [&'a str],
) -> impl Iterator<Item = String> + 'a {
    table
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .map(|key| printable(key))
}

fn printable(key: &str) -> String {
    let cleaned = |character: char| {
        if character.is_control() {
            CONTROL_CHARACTER_REPLACEMENT
        } else {
            character
        }
    };
    let mut shown: String = key
        .chars()
        .take(MAXIMUM_KEY_CHARACTERS)
        .map(cleaned)
        .collect();
    if key.chars().count() > MAXIMUM_KEY_CHARACTERS {
        shown.push(ELLIPSIS);
    }
    shown
}
