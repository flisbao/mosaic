use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::domain::widget::WidgetId;

pub const DEFAULT_INTERVAL_SECONDS: u64 = 30;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 5;
pub const DEFAULT_SPAN: u16 = 1;

const CONFIG_DIRECTORY_NAME: &str = "panel-tui";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSettings {
    pub id: WidgetId,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub span: u16,
    pub tab: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub widgets: Vec<WidgetSettings>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("falha de leitura ou escrita: {0}")]
    Io(String),
    #[error("configuração inválida: {0}")]
    Parse(String),
    #[error("widget duplicado na configuração: '{0}'")]
    DuplicateWidgetId(String),
    #[error("não foi possível descobrir o diretório de configuração")]
    MissingConfigDirectory,
}

#[derive(Deserialize)]
struct RawConfig {
    #[serde(default)]
    widgets: Vec<RawWidget>,
}

#[derive(Deserialize)]
struct RawWidget {
    id: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    interval_seconds: Option<u64>,
    timeout_seconds: Option<u64>,
    span: Option<u16>,
    #[serde(default)]
    tab: bool,
}

pub fn parse_config(text: &str) -> Result<AppConfig, ConfigError> {
    let raw: RawConfig =
        toml::from_str(text).map_err(|error| ConfigError::Parse(error.to_string()))?;
    let mut seen = HashSet::new();
    let mut widgets = Vec::new();
    for widget in raw.widgets {
        if !seen.insert(widget.id.clone()) {
            return Err(ConfigError::DuplicateWidgetId(widget.id));
        }
        widgets.push(WidgetSettings {
            id: WidgetId::new(widget.id),
            command: widget.command,
            args: widget.args,
            env: widget.env.into_iter().collect(),
            interval_seconds: widget.interval_seconds.unwrap_or(DEFAULT_INTERVAL_SECONDS),
            timeout_seconds: widget.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS),
            span: widget.span.unwrap_or(DEFAULT_SPAN),
            tab: widget.tab,
        });
    }
    Ok(AppConfig { widgets })
}

pub fn config_directory() -> Result<PathBuf, ConfigError> {
    dirs::config_dir()
        .map(|directory| directory.join(CONFIG_DIRECTORY_NAME))
        .ok_or(ConfigError::MissingConfigDirectory)
}

pub fn config_file_path(directory: &Path) -> PathBuf {
    directory.join(CONFIG_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_widget_with_env() {
        let config = parse_config(
            r#"
            [[widgets]]
            id = "kanban"
            command = "python3"
            args = ["plugins/kanban.py"]
            interval_seconds = 10
            timeout_seconds = 3
            span = 2
            tab = true

            [widgets.env]
            KANBAN_DIR = "/home/x/kanban"
            "#,
        )
        .unwrap();

        assert_eq!(config.widgets.len(), 1);
        let widget = &config.widgets[0];
        assert_eq!(widget.id.as_str(), "kanban");
        assert_eq!(widget.command, "python3");
        assert_eq!(widget.args, vec!["plugins/kanban.py".to_string()]);
        assert_eq!(
            widget.env,
            vec![("KANBAN_DIR".to_string(), "/home/x/kanban".to_string())]
        );
        assert_eq!(widget.interval_seconds, 10);
        assert_eq!(widget.timeout_seconds, 3);
        assert_eq!(widget.span, 2);
        assert!(widget.tab);
    }

    #[test]
    fn applies_defaults_for_omitted_fields() {
        let config = parse_config("[[widgets]]\nid = \"a\"\ncommand = \"echo\"\n").unwrap();
        let widget = &config.widgets[0];
        assert_eq!(widget.interval_seconds, DEFAULT_INTERVAL_SECONDS);
        assert_eq!(widget.timeout_seconds, DEFAULT_TIMEOUT_SECONDS);
        assert_eq!(widget.span, DEFAULT_SPAN);
        assert!(!widget.tab);
        assert!(widget.args.is_empty());
        assert!(widget.env.is_empty());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let result = parse_config(
            "[[widgets]]\nid = \"kanban\"\ncommand = \"a\"\n[[widgets]]\nid = \"kanban\"\ncommand = \"b\"\n",
        );
        assert_eq!(
            result.unwrap_err(),
            ConfigError::DuplicateWidgetId("kanban".to_string())
        );
    }

    #[test]
    fn invalid_toml_reports_parse_error() {
        assert!(matches!(
            parse_config("[[widgets"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn widget_order_is_preserved() {
        let config = parse_config(
            "[[widgets]]\nid = \"b\"\ncommand = \"x\"\n[[widgets]]\nid = \"a\"\ncommand = \"x\"\n",
        )
        .unwrap();
        let ids: Vec<&str> = config.widgets.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a"]);
    }

    #[test]
    fn config_file_lives_inside_the_directory() {
        let path = config_file_path(std::path::Path::new("/etc/panel"));
        assert_eq!(path, std::path::PathBuf::from("/etc/panel/config.toml"));
    }
}
