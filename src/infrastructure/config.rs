use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::application::reload::{PreparedWidgets, WidgetSpec};
use crate::application::reload_plan::{plan_reload, WidgetIdentity};
use crate::domain::pomodoro::PomodoroSettings;
use crate::domain::widget::WidgetId;

pub const MINIMUM_INTERVAL_SECONDS: u64 = 30;
pub const DEFAULT_INTERVAL_SECONDS: u64 = MINIMUM_INTERVAL_SECONDS;
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
pub const IMPLICIT_TAB_NAME: &str = "Mosaic";
const RELOAD_DONE_NOTICE: &str = "Configuração recarregada";
const RESTART_REQUIRED_NOTICE: &str = "só valem após reiniciar";
const WARNING_SEPARATOR: &str = " | ";

const CONFIG_DIRECTORY_NAME: &str = "mosaic";
const CONFIG_FILE_NAME: &str = "config.toml";
const SECONDS_PER_MINUTE: u64 = 60;
const POMODORO_SECTION: &str = "pomodoro";
const LLM_SECTION: &str = "llm";
const FIRST_TAB_INDEX: usize = 0;

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSettings {
    pub id: WidgetId,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub interval_seconds: u64,
    pub interval_adjusted: bool,
    pub timeout_seconds: u64,
    pub span: u16,
    pub max_items: Option<usize>,
    pub llm_sections: Vec<String>,
    pub llm_max_items: usize,
    pub llm_max_characters_per_item: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabSettings {
    pub name: String,
    pub widget_ids: Vec<WidgetId>,
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
    pub tabs: Vec<TabSettings>,
    pub pomodoro: PomodoroConfig,
    pub llm: LlmConfig,
    pub warnings: Vec<String>,
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
    #[error("widget '{widget}': aba '{tab}' não existe na configuração")]
    UnknownTab { widget: String, tab: String },
    #[error("nome de aba duplicado na configuração: '{0}'")]
    DuplicateTabName(String),
    #[error("[llm]: harness desconhecido '{0}' (use \"claude\" ou \"opencode\")")]
    UnknownHarness(String),
    #[error("não foi possível descobrir o diretório de configuração")]
    MissingConfigDirectory,
}

#[derive(Deserialize)]
struct RawConfig {
    #[serde(default)]
    tabs: Vec<RawTabSection>,
    #[serde(default)]
    widgets: Vec<RawWidget>,
    #[serde(default)]
    pomodoro: RawPomodoro,
    #[serde(default)]
    llm: RawLlm,
}

#[derive(Deserialize)]
struct RawTabSection {
    name: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawTab {
    Legacy(bool),
    Name(String),
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
    tab: Option<RawTab>,
    max_items: Option<usize>,
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
    let raw: RawConfig = toml::from_str(text)
        .map_err(|error| ConfigError::Parse(describe_parse_error(text, &error)))?;
    let mut seen = HashSet::new();
    let mut widgets = Vec::new();
    let mut tab_assignments = Vec::new();
    let mut warnings = Vec::new();
    for mut widget in raw.widgets {
        if !seen.insert(widget.id.clone()) {
            return Err(ConfigError::DuplicateWidgetId(widget.id));
        }
        let tab_name = named_tab(&widget.id, widget.tab.take(), &mut warnings);
        tab_assignments.push(TabAssignment {
            widget_id: widget.id.clone(),
            tab_name,
        });
        widgets.push(convert_widget(widget)?);
    }
    Ok(AppConfig {
        widgets,
        tabs: build_tabs(raw.tabs, &tab_assignments)?,
        pomodoro: convert_pomodoro(raw.pomodoro)?,
        llm: convert_llm(raw.llm)?,
        warnings,
    })
}

struct TabAssignment {
    widget_id: String,
    tab_name: Option<String>,
}

fn named_tab(widget_id: &str, tab: Option<RawTab>, warnings: &mut Vec<String>) -> Option<String> {
    match tab {
        Some(RawTab::Name(name)) => Some(name),
        Some(RawTab::Legacy(true)) => {
            warnings.push(format!(
                "widget '{widget_id}': 'tab = true' foi aposentado e é ignorado"
            ));
            None
        }
        Some(RawTab::Legacy(false)) | None => None,
    }
}

fn build_tabs(
    raw_tabs: Vec<RawTabSection>,
    assignments: &[TabAssignment],
) -> Result<Vec<TabSettings>, ConfigError> {
    let mut tabs = if raw_tabs.is_empty() {
        vec![TabSettings {
            name: IMPLICIT_TAB_NAME.to_string(),
            widget_ids: Vec::new(),
        }]
    } else {
        unique_tabs(raw_tabs)?
    };
    for assignment in assignments {
        let index = match &assignment.tab_name {
            None => FIRST_TAB_INDEX,
            Some(name) => tabs
                .iter()
                .position(|tab| &tab.name == name)
                .ok_or_else(|| ConfigError::UnknownTab {
                    widget: assignment.widget_id.clone(),
                    tab: name.clone(),
                })?,
        };
        tabs[index]
            .widget_ids
            .push(WidgetId::new(assignment.widget_id.clone()));
    }
    Ok(tabs)
}

fn unique_tabs(raw_tabs: Vec<RawTabSection>) -> Result<Vec<TabSettings>, ConfigError> {
    let mut tabs: Vec<TabSettings> = Vec::with_capacity(raw_tabs.len());
    for raw_tab in raw_tabs {
        if tabs.iter().any(|tab| tab.name == raw_tab.name) {
            return Err(ConfigError::DuplicateTabName(raw_tab.name));
        }
        tabs.push(TabSettings {
            name: raw_tab.name,
            widget_ids: Vec::new(),
        });
    }
    Ok(tabs)
}

fn convert_widget(widget: RawWidget) -> Result<WidgetSettings, ConfigError> {
    let configured_interval = positive_or_default(
        widget.interval_seconds,
        DEFAULT_INTERVAL_SECONDS,
        &widget.id,
        "interval_seconds",
    )?;
    let interval_seconds = configured_interval.max(MINIMUM_INTERVAL_SECONDS);
    let timeout_seconds = positive_or_default(
        widget.timeout_seconds,
        DEFAULT_TIMEOUT_SECONDS,
        &widget.id,
        "timeout_seconds",
    )?;
    let max_items = positive_limit(widget.max_items, &widget.id, "max_items")?;
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
        interval_adjusted: interval_seconds != configured_interval,
        timeout_seconds,
        span: widget.span.unwrap_or(DEFAULT_SPAN),
        max_items,
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

fn positive_limit(
    value: Option<usize>,
    widget: &str,
    field: &'static str,
) -> Result<Option<usize>, ConfigError> {
    match value {
        Some(0) => Err(ConfigError::InvalidValue {
            widget: widget.to_string(),
            field,
        }),
        other => Ok(other),
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

pub fn read_config_file(path: &Path) -> Result<String, ConfigError> {
    std::fs::read_to_string(path).map_err(|error| ConfigError::Io(error.to_string()))
}

pub fn reload_notice(current: &AppConfig, next: &AppConfig) -> String {
    let restart_sections = restart_sections_changed(current, next);
    let mut notice = RELOAD_DONE_NOTICE.to_string();
    if !restart_sections.is_empty() {
        notice.push_str(&format!(
            "; {} {RESTART_REQUIRED_NOTICE}",
            restart_sections.join("/")
        ));
    }
    if !next.warnings.is_empty() {
        notice.push_str(&format!(". {}", next.warnings.join(WARNING_SEPARATOR)));
    }
    notice
}

fn restart_sections_changed(current: &AppConfig, next: &AppConfig) -> Vec<&'static str> {
    let mut sections = Vec::new();
    if current.llm != next.llm {
        sections.push("[llm]");
    }
    if current.pomodoro != next.pomodoro {
        sections.push("[pomodoro]/notificações");
    }
    sections
}

pub fn retain_restart_sections(active: &AppConfig, mut next: AppConfig) -> AppConfig {
    next.llm = active.llm.clone();
    next.pomodoro = active.pomodoro.clone();
    next
}

pub fn load_config(directory: &Path) -> Result<AppConfig, ConfigError> {
    parse_config(&read_config_file(&config_file_path(directory))?)
}

pub fn prepare_reload(
    current: &[WidgetSettings],
    text: &str,
) -> Result<PreparedWidgets<WidgetSettings, AppConfig>, ConfigError> {
    let config = parse_config(text)?;
    let current_identities: Vec<WidgetIdentity> =
        current.iter().map(WidgetIdentity::from).collect();
    let next_identities: Vec<WidgetIdentity> =
        config.widgets.iter().map(WidgetIdentity::from).collect();
    Ok(PreparedWidgets {
        plan: plan_reload(&current_identities, &next_identities),
        specs: config.widgets.clone(),
        config,
    })
}

impl WidgetSpec for WidgetSettings {
    fn widget_id(&self) -> &WidgetId {
        &self.id
    }
}

impl From<&WidgetSettings> for WidgetIdentity {
    fn from(settings: &WidgetSettings) -> Self {
        Self {
            id: settings.id.clone(),
            command: settings.command.clone(),
            args: settings.args.clone(),
            env: settings.env.clone(),
            interval_seconds: settings.interval_seconds,
            interval_adjusted: settings.interval_adjusted,
            timeout_seconds: settings.timeout_seconds,
        }
    }
}

fn describe_parse_error(text: &str, error: &toml::de::Error) -> String {
    match error.span() {
        Some(span) => {
            let (line, column) = line_and_column(text, span.start);
            format!("linha {line}, coluna {column}: {}", error.message())
        }
        None => error.message().to_string(),
    }
}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |text| text.chars().count())
        + 1;
    (line, column)
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
            interval_seconds = 60
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
        assert_eq!(widget.interval_seconds, 60);
        assert!(!widget.interval_adjusted);
        assert_eq!(widget.timeout_seconds, 3);
        assert_eq!(widget.span, 2);
        assert_eq!(widget.max_items, None);
    }

    #[test]
    fn applies_defaults_for_omitted_fields() {
        let config = parse_config("[[widgets]]\nid = \"a\"\ncommand = \"echo\"\n").unwrap();
        let widget = &config.widgets[0];
        assert_eq!(widget.interval_seconds, MINIMUM_INTERVAL_SECONDS);
        assert!(!widget.interval_adjusted);
        assert_eq!(widget.timeout_seconds, DEFAULT_TIMEOUT_SECONDS);
        assert_eq!(widget.span, DEFAULT_SPAN);
        assert_eq!(widget.max_items, None);
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

    fn interval_settings(interval: i64) -> WidgetSettings {
        let text =
            format!("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ninterval_seconds = {interval}\n");
        parse_config(&text).unwrap().widgets.remove(0)
    }

    #[test]
    fn interval_below_minimum_is_raised_and_flagged() {
        for interval in [1, 10, 29] {
            let widget = interval_settings(interval);
            assert_eq!(widget.interval_seconds, MINIMUM_INTERVAL_SECONDS);
            assert!(widget.interval_adjusted, "{interval}");
        }
    }

    #[test]
    fn interval_at_or_above_minimum_is_kept() {
        for interval in [30, 31, i64::MAX] {
            let widget = interval_settings(interval);
            assert_eq!(widget.interval_seconds, interval as u64);
            assert!(!widget.interval_adjusted, "{interval}");
        }
    }

    #[test]
    fn omitted_interval_defaults_to_the_minimum_without_flag() {
        let config = parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\n").unwrap();
        assert_eq!(config.widgets[0].interval_seconds, MINIMUM_INTERVAL_SECONDS);
        assert!(!config.widgets[0].interval_adjusted);
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

    fn tab_widget_ids(tab: &TabSettings) -> Vec<&str> {
        tab.widget_ids.iter().map(WidgetId::as_str).collect()
    }

    #[test]
    fn without_tabs_section_all_widgets_land_in_one_implicit_tab() {
        let config = parse_config(
            "[[widgets]]\nid = \"b\"\ncommand = \"x\"\n[[widgets]]\nid = \"a\"\ncommand = \"x\"\n",
        )
        .unwrap();
        assert_eq!(config.tabs.len(), 1);
        assert_eq!(config.tabs[0].name, IMPLICIT_TAB_NAME);
        assert_eq!(tab_widget_ids(&config.tabs[0]), vec!["b", "a"]);
    }

    #[test]
    fn widget_goes_to_the_tab_it_names_and_unnamed_goes_to_first() {
        let config = parse_config(
            r#"
            [[tabs]]
            name = "Trabalho"

            [[tabs]]
            name = "Pessoal"

            [[widgets]]
            id = "a"
            command = "x"
            tab = "Pessoal"

            [[widgets]]
            id = "b"
            command = "x"

            [[widgets]]
            id = "c"
            command = "x"
            tab = "Trabalho"
            "#,
        )
        .unwrap();
        assert_eq!(config.tabs[0].name, "Trabalho");
        assert_eq!(tab_widget_ids(&config.tabs[0]), vec!["b", "c"]);
        assert_eq!(config.tabs[1].name, "Pessoal");
        assert_eq!(tab_widget_ids(&config.tabs[1]), vec!["a"]);
    }

    #[test]
    fn unknown_tab_name_is_an_error_naming_widget_and_tab() {
        let result = parse_config(
            "[[tabs]]\nname = \"Trabalho\"\n[[widgets]]\nid = \"a\"\ncommand = \"x\"\ntab = \"Inexistente\"\n",
        );
        assert_eq!(
            result.unwrap_err(),
            ConfigError::UnknownTab {
                widget: "a".to_string(),
                tab: "Inexistente".to_string()
            }
        );
    }

    #[test]
    fn tab_name_without_tabs_section_is_unknown() {
        let result = parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ntab = \"Trabalho\"\n");
        assert_eq!(
            result.unwrap_err(),
            ConfigError::UnknownTab {
                widget: "a".to_string(),
                tab: "Trabalho".to_string()
            }
        );
    }

    #[test]
    fn duplicate_tab_names_are_rejected() {
        let result = parse_config("[[tabs]]\nname = \"A\"\n[[tabs]]\nname = \"A\"\n");
        assert_eq!(
            result.unwrap_err(),
            ConfigError::DuplicateTabName("A".to_string())
        );
    }

    #[test]
    fn tab_without_widgets_is_kept_and_empty() {
        let config = parse_config(
            "[[tabs]]\nname = \"Vazia\"\n[[tabs]]\nname = \"Cheia\"\n[[widgets]]\nid = \"a\"\ncommand = \"x\"\ntab = \"Cheia\"\n",
        )
        .unwrap();
        assert_eq!(config.tabs[0].name, "Vazia");
        assert!(config.tabs[0].widget_ids.is_empty());
        assert_eq!(tab_widget_ids(&config.tabs[1]), vec!["a"]);
    }

    #[test]
    fn legacy_tab_true_is_ignored_with_warning() {
        let config =
            parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ntab = true\n").unwrap();
        assert_eq!(config.widgets.len(), 1);
        assert_eq!(config.tabs.len(), 1);
        assert_eq!(
            config.warnings,
            vec!["widget 'a': 'tab = true' foi aposentado e é ignorado".to_string()]
        );
    }

    #[test]
    fn max_items_zero_is_rejected() {
        let result = parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\nmax_items = 0\n");
        assert_eq!(
            result.unwrap_err(),
            ConfigError::InvalidValue {
                widget: "a".to_string(),
                field: "max_items"
            }
        );
    }

    #[test]
    fn max_items_is_parsed() {
        let config =
            parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\nmax_items = 5\n").unwrap();
        assert_eq!(config.widgets[0].max_items, Some(5));
    }

    const RELOAD_BASE: &str = r#"
[[widgets]]
id = "jira"
command = "python3"
args = ["jira.py", "my-tasks"]
interval_seconds = 60

[[widgets]]
id = "clima"
command = "sh"
"#;

    fn reload_base_widgets() -> Vec<WidgetSettings> {
        parse_config(RELOAD_BASE).unwrap().widgets
    }

    #[test]
    fn invalid_toml_reports_line_and_column_in_portuguese() {
        let error = parse_config("[[widgets]]\nid = \"jira\"\ncommand = \n").unwrap_err();

        let message = error.to_string();
        assert!(message.contains("linha 3"), "{message}");
        assert!(message.contains("coluna "), "{message}");
    }

    #[test]
    fn reload_with_invalid_toml_reports_line_and_keeps_current() {
        let current = reload_base_widgets();

        let result = prepare_reload(&current, "[[widgets]]\nid = \"jira\"\ncommand = \n");

        assert!(
            matches!(&result, Err(ConfigError::Parse(message)) if message.contains("linha 3")),
            "{result:?}"
        );
    }

    #[test]
    fn reload_with_duplicate_ids_is_rejected_before_planning() {
        let current = reload_base_widgets();
        let duplicated = format!("{RELOAD_BASE}\n[[widgets]]\nid = \"jira\"\ncommand = \"sh\"\n");

        let result = prepare_reload(&current, &duplicated);

        assert_eq!(
            result,
            Err(ConfigError::DuplicateWidgetId("jira".to_string()))
        );
    }

    #[test]
    fn changed_only_max_items_tab_span_or_order_keeps() {
        let current = reload_base_widgets();
        let variants = [
            RELOAD_BASE.replace(
                "interval_seconds = 60",
                "interval_seconds = 60\nmax_items = 3",
            ),
            format!(
                "[[tabs]]\nname = \"Trabalho\"\n{}",
                RELOAD_BASE.replace(
                    "interval_seconds = 60",
                    "interval_seconds = 60\ntab = \"Trabalho\"\nspan = 2",
                )
            ),
            RELOAD_BASE.replace(
                "[[widgets]]\nid = \"jira\"",
                "[[tabs]]\nname = \"Trabalho\"\n\n[[widgets]]\nid = \"jira\"",
            ),
            RELOAD_BASE.replace(
                "id = \"clima\"\ncommand = \"sh\"",
                "id = \"clima\"\ncommand = \"sh\"\nllm_max_items = 5\nllm_sections = [\"Tarefas\"]",
            ),
            swap_widget_order(RELOAD_BASE),
        ];
        for variant in variants {
            let prepared = prepare_reload(&current, &variant).unwrap();

            assert_eq!(prepared.plan.changes.len(), 2, "{variant}");
            assert!(
                prepared
                    .plan
                    .changes
                    .iter()
                    .all(|(_, change)| *change
                        == crate::application::reload_plan::WidgetChange::Keep),
                "{variant}"
            );
        }
    }

    fn swap_widget_order(text: &str) -> String {
        let (head, tail) = text.split_once("[[widgets]]\nid = \"jira\"").unwrap();
        let (jira_body, clima_block) = tail.split_once("[[widgets]]").unwrap();
        format!("{head}[[widgets]]{clima_block}\n[[widgets]]\nid = \"jira\"{jira_body}")
    }

    #[test]
    fn changed_command_restarts_only_that_widget() {
        let current = reload_base_widgets();
        let changed = RELOAD_BASE.replace("command = \"sh\"", "command = \"bash\"");

        let prepared = prepare_reload(&current, &changed).unwrap();

        let changes: Vec<(&str, crate::application::reload_plan::WidgetChange)> = prepared
            .plan
            .changes
            .iter()
            .map(|(id, change)| (id.as_str(), *change))
            .collect();
        assert_eq!(
            changes,
            vec![
                ("jira", crate::application::reload_plan::WidgetChange::Keep),
                (
                    "clima",
                    crate::application::reload_plan::WidgetChange::Restart
                ),
            ]
        );
    }

    #[test]
    fn reload_notice_without_restart_sections_is_plain() {
        let current = parse_config(RELOAD_BASE).unwrap();
        let next = parse_config(RELOAD_BASE).unwrap();

        assert_eq!(reload_notice(&current, &next), "Configuração recarregada");
    }

    #[test]
    fn reload_notice_names_each_section_that_needs_restart() {
        let current = parse_config(RELOAD_BASE).unwrap();
        let llm = parse_config(&format!("{RELOAD_BASE}\n[llm]\nharness = \"opencode\"\n")).unwrap();
        let pomodoro =
            parse_config(&format!("{RELOAD_BASE}\n[pomodoro]\nfocus_minutes = 30\n")).unwrap();
        let both = parse_config(&format!(
            "{RELOAD_BASE}\n[llm]\nharness = \"opencode\"\n[pomodoro]\nfocus_minutes = 30\n"
        ))
        .unwrap();

        assert_eq!(
            reload_notice(&current, &llm),
            "Configuração recarregada; [llm] só valem após reiniciar"
        );
        assert_eq!(
            reload_notice(&current, &pomodoro),
            "Configuração recarregada; [pomodoro]/notificações só valem após reiniciar"
        );
        assert_eq!(
            reload_notice(&current, &both),
            "Configuração recarregada; [llm]/[pomodoro]/notificações só valem após reiniciar"
        );
    }

    #[test]
    fn reload_notice_appends_config_warnings() {
        let current = parse_config(RELOAD_BASE).unwrap();
        let next = parse_config(
            &RELOAD_BASE.replace("interval_seconds = 60", "interval_seconds = 60\ntab = true"),
        )
        .unwrap();

        let notice = reload_notice(&current, &next);

        assert!(notice.starts_with("Configuração recarregada. "), "{notice}");
        assert!(notice.contains("aposentado"), "{notice}");
    }

    #[test]
    fn retain_restart_sections_keeps_active_llm_and_pomodoro_but_takes_new_widgets() {
        let active = parse_config(RELOAD_BASE).unwrap();
        let next = parse_config(&format!(
            "{}\n[llm]\nharness = \"opencode\"\n",
            RELOAD_BASE.replace("command = \"sh\"", "command = \"bash\"")
        ))
        .unwrap();

        let retained = retain_restart_sections(&active, next.clone());

        assert_eq!(retained.llm, active.llm);
        assert_eq!(retained.pomodoro, active.pomodoro);
        assert_eq!(retained.widgets, next.widgets);
    }

    #[test]
    fn missing_config_file_is_io_error() {
        let directory = tempfile::tempdir().unwrap();

        let result = load_config(directory.path());

        assert!(matches!(result, Err(ConfigError::Io(_))), "{result:?}");
    }

    #[test]
    fn config_file_lives_inside_the_directory() {
        let path = config_file_path(std::path::Path::new("/etc/panel"));
        assert_eq!(path, std::path::PathBuf::from("/etc/panel/config.toml"));
    }
}
