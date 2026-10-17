use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::domain::pomodoro::PomodoroSettings;
use crate::domain::widget::WidgetId;

pub const DEFAULT_INTERVAL_SECONDS: u64 = 30;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 5;
pub const DEFAULT_SPAN: u16 = 1;
pub const DEFAULT_LLM_MAX_ITEMS: usize = 20;
pub const DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM: usize = 120;
pub const DEFAULT_FOCUS_MINUTES: u64 = 25;
pub const DEFAULT_SHORT_BREAK_MINUTES: u64 = 5;
pub const DEFAULT_LONG_BREAK_MINUTES: u64 = 15;
pub const DEFAULT_CYCLES_BEFORE_LONG_BREAK: u32 = 4;
pub const DEFAULT_LLM_TIMEOUT_SECONDS: u64 = 120;
pub const DEFAULT_PRIORITY_PROMPT: &str = "prompts/prioridade.md";
pub const DEFAULT_SUGGESTIONS: [&str; 3] = ["Beber água", "Levantar e alongar", "Tomar um café"];

const CONFIG_DIRECTORY_NAME: &str = "panel-tui";
const CONFIG_FILE_NAME: &str = "config.toml";
const SECONDS_PER_MINUTE: u64 = 60;
const POMODORO_SECTION: &str = "pomodoro";
const LLM_SECTION: &str = "llm";

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
    pub llm_sections: Vec<String>,
    pub llm_max_items: usize,
    pub llm_max_characters_per_item: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    Claude,
    OpenCode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PomodoroConfig {
    pub enabled: bool,
    pub focus_minutes: u64,
    pub short_break_minutes: u64,
    pub long_break_minutes: u64,
    pub cycles_before_long_break: u32,
    pub suggestions: Vec<String>,
    pub bell: bool,
    pub notify_command: Option<String>,
}

impl PomodoroConfig {
    pub fn settings(&self) -> PomodoroSettings {
        let minutes = |value: u64| Duration::from_secs(value * SECONDS_PER_MINUTE);
        PomodoroSettings {
            focus: minutes(self.focus_minutes),
            short_break: minutes(self.short_break_minutes),
            long_break: minutes(self.long_break_minutes),
            cycles_before_long_break: self.cycles_before_long_break,
            suggestions: self.suggestions.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmConfig {
    pub enabled: bool,
    pub harness: Harness,
    pub model: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub timeout_seconds: u64,
    pub confirm_before_send: bool,
    pub priority_prompt: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub widgets: Vec<WidgetSettings>,
    pub pomodoro: PomodoroConfig,
    pub llm: LlmConfig,
}

#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("falha de leitura ou escrita: {0}")]
    Io(String),
    #[error("configuração inválida: {0}")]
    Parse(String),
    #[error("widget duplicado na configuração: '{0}'")]
    DuplicateWidgetId(String),
    #[error("widget '{widget}': '{field}' precisa ser maior que zero")]
    InvalidValue { widget: String, field: &'static str },
    #[error("[{section}]: '{field}' precisa ser maior que zero")]
    InvalidSetting {
        section: String,
        field: &'static str,
    },
    #[error("[llm]: harness desconhecido '{0}' (use \"claude\" ou \"opencode\")")]
    UnknownHarness(String),
    #[error("não foi possível descobrir o diretório de configuração")]
    MissingConfigDirectory,
}

#[derive(Deserialize)]
struct RawConfig {
    #[serde(default)]
    widgets: Vec<RawWidget>,
    #[serde(default)]
    pomodoro: RawPomodoro,
    #[serde(default)]
    llm: RawLlm,
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
    #[serde(default)]
    llm_sections: Vec<String>,
    llm_max_items: Option<usize>,
    llm_max_characters_per_item: Option<usize>,
}

#[derive(Deserialize, Default)]
struct RawPomodoro {
    enabled: Option<bool>,
    focus_minutes: Option<u64>,
    short_break_minutes: Option<u64>,
    long_break_minutes: Option<u64>,
    cycles_before_long_break: Option<u32>,
    suggestions: Option<Vec<String>>,
    bell: Option<bool>,
    notify_command: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawLlm {
    enabled: Option<bool>,
    harness: Option<String>,
    model: Option<String>,
    max_budget_usd: Option<f64>,
    timeout_seconds: Option<u64>,
    confirm_before_send: Option<bool>,
    priority_prompt: Option<String>,
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
        widgets.push(convert_widget(widget)?);
    }
    Ok(AppConfig {
        widgets,
        pomodoro: convert_pomodoro(raw.pomodoro)?,
        llm: convert_llm(raw.llm)?,
    })
}

fn convert_widget(widget: RawWidget) -> Result<WidgetSettings, ConfigError> {
    let interval_seconds = positive_or_default(
        widget.interval_seconds,
        DEFAULT_INTERVAL_SECONDS,
        &widget.id,
        "interval_seconds",
    )?;
    let timeout_seconds = positive_or_default(
        widget.timeout_seconds,
        DEFAULT_TIMEOUT_SECONDS,
        &widget.id,
        "timeout_seconds",
    )?;
    let llm_max_items = positive_or_default(
        widget.llm_max_items.map(|value| value as u64),
        DEFAULT_LLM_MAX_ITEMS as u64,
        &widget.id,
        "llm_max_items",
    )? as usize;
    let llm_max_characters_per_item = positive_or_default(
        widget.llm_max_characters_per_item.map(|value| value as u64),
        DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM as u64,
        &widget.id,
        "llm_max_characters_per_item",
    )? as usize;
    Ok(WidgetSettings {
        id: WidgetId::new(widget.id),
        command: widget.command,
        args: widget.args,
        env: widget.env.into_iter().collect(),
        interval_seconds,
        timeout_seconds,
        span: widget.span.unwrap_or(DEFAULT_SPAN),
        tab: widget.tab,
        llm_sections: widget.llm_sections,
        llm_max_items,
        llm_max_characters_per_item,
    })
}

fn convert_pomodoro(raw: RawPomodoro) -> Result<PomodoroConfig, ConfigError> {
    let cycles = positive_setting(
        raw.cycles_before_long_break.map(u64::from),
        u64::from(DEFAULT_CYCLES_BEFORE_LONG_BREAK),
        POMODORO_SECTION,
        "cycles_before_long_break",
    )?;
    Ok(PomodoroConfig {
        enabled: raw.enabled.unwrap_or(true),
        focus_minutes: positive_setting(
            raw.focus_minutes,
            DEFAULT_FOCUS_MINUTES,
            POMODORO_SECTION,
            "focus_minutes",
        )?,
        short_break_minutes: positive_setting(
            raw.short_break_minutes,
            DEFAULT_SHORT_BREAK_MINUTES,
            POMODORO_SECTION,
            "short_break_minutes",
        )?,
        long_break_minutes: positive_setting(
            raw.long_break_minutes,
            DEFAULT_LONG_BREAK_MINUTES,
            POMODORO_SECTION,
            "long_break_minutes",
        )?,
        cycles_before_long_break: cycles as u32,
        suggestions: raw
            .suggestions
            .unwrap_or_else(|| DEFAULT_SUGGESTIONS.iter().map(|s| s.to_string()).collect()),
        bell: raw.bell.unwrap_or(true),
        notify_command: raw
            .notify_command
            .map(|command| command.trim().to_string())
            .filter(|command| !command.is_empty()),
    })
}

fn convert_llm(raw: RawLlm) -> Result<LlmConfig, ConfigError> {
    let harness = match raw.harness.as_deref() {
        None | Some("claude") => Harness::Claude,
        Some("opencode") => Harness::OpenCode,
        Some(other) => return Err(ConfigError::UnknownHarness(other.to_string())),
    };
    if raw.max_budget_usd.is_some_and(|budget| budget <= 0.0) {
        return Err(ConfigError::InvalidSetting {
            section: LLM_SECTION.to_string(),
            field: "max_budget_usd",
        });
    }
    Ok(LlmConfig {
        enabled: raw.enabled.unwrap_or(true),
        harness,
        model: raw.model.filter(|model| !model.trim().is_empty()),
        max_budget_usd: raw.max_budget_usd,
        timeout_seconds: positive_setting(
            raw.timeout_seconds,
            DEFAULT_LLM_TIMEOUT_SECONDS,
            LLM_SECTION,
            "timeout_seconds",
        )?,
        confirm_before_send: raw.confirm_before_send.unwrap_or(true),
        priority_prompt: raw
            .priority_prompt
            .unwrap_or_else(|| DEFAULT_PRIORITY_PROMPT.to_string()),
    })
}

fn positive_or_default(
    value: Option<u64>,
    default: u64,
    widget: &str,
    field: &'static str,
) -> Result<u64, ConfigError> {
    match value {
        Some(0) => Err(ConfigError::InvalidValue {
            widget: widget.to_string(),
            field,
        }),
        Some(amount) => Ok(amount),
        None => Ok(default),
    }
}

fn positive_setting(
    value: Option<u64>,
    default: u64,
    section: &str,
    field: &'static str,
) -> Result<u64, ConfigError> {
    match value {
        Some(0) => Err(ConfigError::InvalidSetting {
            section: section.to_string(),
            field,
        }),
        Some(amount) => Ok(amount),
        None => Ok(default),
    }
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
    fn zero_interval_is_rejected() {
        let result =
            parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ninterval_seconds = 0\n");
        assert_eq!(
            result.unwrap_err(),
            ConfigError::InvalidValue {
                widget: "a".to_string(),
                field: "interval_seconds"
            }
        );
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let result =
            parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ntimeout_seconds = 0\n");
        assert_eq!(
            result.unwrap_err(),
            ConfigError::InvalidValue {
                widget: "a".to_string(),
                field: "timeout_seconds"
            }
        );
    }

    #[test]
    fn missing_sections_use_defaults() {
        let config = parse_config("").unwrap();
        let pomodoro = &config.pomodoro;
        assert!(pomodoro.enabled);
        assert_eq!(
            (
                pomodoro.focus_minutes,
                pomodoro.short_break_minutes,
                pomodoro.long_break_minutes,
                pomodoro.cycles_before_long_break
            ),
            (25, 5, 15, 4)
        );
        assert_eq!(
            pomodoro.suggestions,
            vec!["Beber água", "Levantar e alongar", "Tomar um café"]
        );
        assert!(pomodoro.bell);
        assert_eq!(pomodoro.notify_command, None);
        let llm = &config.llm;
        assert!(llm.enabled);
        assert_eq!(llm.harness, Harness::Claude);
        assert_eq!(llm.model, None);
        assert_eq!(llm.max_budget_usd, None);
        assert_eq!(llm.timeout_seconds, 120);
        assert!(llm.confirm_before_send);
        assert_eq!(llm.priority_prompt, "prompts/prioridade.md");
    }

    #[test]
    fn parses_full_pomodoro_and_llm_sections() {
        let config = parse_config(
            r#"
            [pomodoro]
            enabled = false
            focus_minutes = 50
            short_break_minutes = 10
            long_break_minutes = 30
            cycles_before_long_break = 2
            suggestions = ["Água"]
            bell = false
            notify_command = "notify-send"

            [llm]
            enabled = false
            harness = "claude"
            model = "haiku"
            max_budget_usd = 0.05
            timeout_seconds = 60
            confirm_before_send = false
            priority_prompt = "meu.md"
            "#,
        )
        .unwrap();
        assert!(!config.pomodoro.enabled);
        assert_eq!(config.pomodoro.focus_minutes, 50);
        assert_eq!(config.pomodoro.suggestions, vec!["Água"]);
        assert!(!config.pomodoro.bell);
        assert_eq!(
            config.pomodoro.notify_command.as_deref(),
            Some("notify-send")
        );
        assert!(!config.llm.enabled);
        assert_eq!(config.llm.model.as_deref(), Some("haiku"));
        assert_eq!(config.llm.max_budget_usd, Some(0.05));
        assert_eq!(config.llm.timeout_seconds, 60);
        assert!(!config.llm.confirm_before_send);
        assert_eq!(config.llm.priority_prompt, "meu.md");
    }

    #[test]
    fn parses_widget_llm_fields_with_defaults() {
        let config = parse_config(
            "[[widgets]]\nid = \"a\"\ncommand = \"x\"\nllm_sections = [\"inbox\"]\n[[widgets]]\nid = \"b\"\ncommand = \"x\"\nllm_max_items = 5\nllm_max_characters_per_item = 50\n",
        )
        .unwrap();
        let first = &config.widgets[0];
        assert_eq!(first.llm_sections, vec!["inbox"]);
        assert_eq!(
            (first.llm_max_items, first.llm_max_characters_per_item),
            (20, 120)
        );
        let second = &config.widgets[1];
        assert!(second.llm_sections.is_empty());
        assert_eq!(
            (second.llm_max_items, second.llm_max_characters_per_item),
            (5, 50)
        );
    }

    #[test]
    fn opencode_harness_is_accepted() {
        let config = parse_config("[llm]\nharness = \"opencode\"\n").unwrap();
        assert_eq!(config.llm.harness, Harness::OpenCode);
    }

    #[test]
    fn unknown_harness_is_rejected() {
        assert_eq!(
            parse_config("[llm]\nharness = \"x\"\n").unwrap_err(),
            ConfigError::UnknownHarness("x".to_string())
        );
    }

    #[test]
    fn zero_pomodoro_durations_and_cycles_are_rejected() {
        for field in [
            "focus_minutes",
            "short_break_minutes",
            "long_break_minutes",
            "cycles_before_long_break",
        ] {
            let result = parse_config(&format!("[pomodoro]\n{field} = 0\n"));
            assert_eq!(
                result.unwrap_err(),
                ConfigError::InvalidSetting {
                    section: "pomodoro".to_string(),
                    field: match field {
                        "focus_minutes" => "focus_minutes",
                        "short_break_minutes" => "short_break_minutes",
                        "long_break_minutes" => "long_break_minutes",
                        _ => "cycles_before_long_break",
                    }
                },
                "{field}"
            );
        }
    }

    #[test]
    fn zero_llm_timeout_and_non_positive_budget_are_rejected() {
        assert_eq!(
            parse_config("[llm]\ntimeout_seconds = 0\n").unwrap_err(),
            ConfigError::InvalidSetting {
                section: "llm".to_string(),
                field: "timeout_seconds"
            }
        );
        assert_eq!(
            parse_config("[llm]\nmax_budget_usd = 0.0\n").unwrap_err(),
            ConfigError::InvalidSetting {
                section: "llm".to_string(),
                field: "max_budget_usd"
            }
        );
    }

    #[test]
    fn zero_llm_limits_are_rejected() {
        for field in ["llm_max_items", "llm_max_characters_per_item"] {
            let result = parse_config(&format!(
                "[[widgets]]\nid = \"a\"\ncommand = \"x\"\n{field} = 0\n"
            ));
            assert!(
                matches!(result, Err(ConfigError::InvalidValue { .. })),
                "{field}"
            );
        }
    }

    #[test]
    fn blank_notify_command_becomes_none() {
        let config = parse_config("[pomodoro]\nnotify_command = \"   \"\n").unwrap();
        assert_eq!(config.pomodoro.notify_command, None);
    }

    #[test]
    fn pomodoro_settings_convert_minutes_to_durations() {
        let settings = parse_config("").unwrap().pomodoro.settings();
        assert_eq!(settings.focus, std::time::Duration::from_secs(25 * 60));
        assert_eq!(settings.short_break, std::time::Duration::from_secs(5 * 60));
        assert_eq!(settings.long_break, std::time::Duration::from_secs(15 * 60));
        assert_eq!(settings.cycles_before_long_break, 4);
        assert_eq!(settings.suggestions.len(), 3);
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
