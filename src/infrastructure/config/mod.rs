use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use thiserror::Error;

mod paths;
mod raw;
mod validation;

pub use paths::{config_directory, config_file_path, read_config_file};
use raw::RawConfig;
use validation::{
    build_tabs, convert_llm, convert_pomodoro, convert_widget, named_tab, TabAssignment,
};

use crate::application::launch_spec::{ProviderCommand, WidgetLaunchSpec, WidgetSchedule};
use crate::application::recommendation::LlmSelection;
use crate::application::reload::PreparedWidgets;
use crate::application::reload_plan::plan_reload;
use crate::application::reload_policy::RestartSections;
use crate::domain::pomodoro::{PomodoroSettings, SECONDS_PER_MINUTE};
use crate::domain::widget::WidgetId;
use crate::infrastructure::private_directory::verify_trusted_path;

pub const PLUGINS_DIRECTORY_NAME: &str = "plugins";

pub const MINIMUM_INTERVAL_SECONDS: u64 = 30;
pub const MAXIMUM_INTERVAL_SECONDS: u64 = 86_400;
pub const DEFAULT_INTERVAL_SECONDS: u64 = MINIMUM_INTERVAL_SECONDS;
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 5;
pub const MAXIMUM_TIMEOUT_SECONDS: u64 = 3_600;
pub const DEFAULT_SPAN: u16 = 1;
pub const DEFAULT_LLM_MAX_ITEMS: usize = 20;
pub const DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM: usize = 120;
pub const DEFAULT_FOCUS_MINUTES: u64 = 25;
pub const DEFAULT_SHORT_BREAK_MINUTES: u64 = 5;
pub const DEFAULT_LONG_BREAK_MINUTES: u64 = 15;
pub const DEFAULT_CYCLES_BEFORE_LONG_BREAK: u32 = 4;
pub const DEFAULT_LLM_TIMEOUT_SECONDS: u64 = 120;
pub const DEFAULT_PRIORITY_PROMPT: &str =
    crate::infrastructure::app_paths::PRIORITY_PROMPT_RELATIVE_PATH;
pub const DEFAULT_SUGGESTIONS: [&str; 3] = ["Beber água", "Levantar e alongar", "Tomar um café"];
pub const IMPLICIT_TAB_NAME: &str = "Mosaic";

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSettings {
    pub id: WidgetId,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub inherit_env: Option<Vec<String>>,
    pub interval_seconds: u64,
    pub interval_adjusted: bool,
    pub timeout_seconds: u64,
    pub span: u16,
    pub max_items: Option<usize>,
    pub llm_selection: LlmSelection,
}

impl WidgetSettings {
    pub fn for_command(
        id: WidgetId,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> Self {
        Self {
            id,
            command,
            args,
            env,
            inherit_env: None,
            interval_seconds: DEFAULT_INTERVAL_SECONDS,
            interval_adjusted: false,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            span: DEFAULT_SPAN,
            max_items: None,
            llm_selection: default_llm_selection(Vec::new()),
        }
    }

    pub fn launch_spec(&self) -> WidgetLaunchSpec {
        WidgetLaunchSpec {
            id: self.id.clone(),
            command: ProviderCommand {
                program: self.command.clone(),
                args: self.args.clone(),
                env: self.env.clone(),
                inherit_env: self.inherit_env.clone(),
                timeout: Duration::from_secs(self.timeout_seconds),
            },
            schedule: WidgetSchedule {
                interval: Duration::from_secs(self.interval_seconds),
                interval_adjusted: self.interval_adjusted,
            },
        }
    }
}

pub fn default_llm_selection(sections: Vec<String>) -> LlmSelection {
    LlmSelection {
        sections,
        max_items: DEFAULT_LLM_MAX_ITEMS,
        max_characters_per_item: DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM,
    }
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

const ALL_HARNESSES: [Harness; 2] = [Harness::Claude, Harness::OpenCode];

impl Harness {
    pub const fn program_name(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::OpenCode => "opencode",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ALL_HARNESSES
            .into_iter()
            .find(|harness| harness.program_name() == name)
    }
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
    pub notify_command: Option<Vec<String>>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum SettingLocation {
    Widget(String),
    Section(&'static str),
}

impl std::fmt::Display for SettingLocation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Widget(id) => write!(formatter, "widget '{id}'"),
            Self::Section(name) => write!(formatter, "[{name}]"),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("falha de leitura ou escrita em {path}: {reason}")]
    Io {
        path: String,
        kind: std::io::ErrorKind,
        reason: String,
    },
    #[error("configuração inválida: {0}")]
    Parse(String),
    #[error("widget duplicado na configuração: '{0}'")]
    DuplicateWidgetId(String),
    #[error("widget '{widget}': '{field}' não pode ser vazio")]
    EmptyValue { widget: String, field: &'static str },
    #[error("widget '{widget}': '{field}' contém um nome inválido")]
    InvalidValue { widget: String, field: &'static str },
    #[error("widget '{0}': id reservado pelo mosaic")]
    ReservedWidgetId(String),
    #[error("{location}: '{field}' precisa ser maior que zero")]
    NotPositive {
        location: SettingLocation,
        field: &'static str,
    },
    #[error("{location}: '{field}' não pode passar de {maximum}")]
    TooLarge {
        location: SettingLocation,
        field: &'static str,
        maximum: u64,
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

impl ConfigError {
    pub fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ConfigError + '_ {
        move |error| ConfigError::Io {
            path: path.display().to_string(),
            kind: error.kind(),
            reason: error.to_string(),
        }
    }
}

pub fn parse_config(text: &str) -> Result<AppConfig, ConfigError> {
    let raw: RawConfig = toml::from_str(text)
        .map_err(|error| ConfigError::Parse(describe_parse_error(text, &error)))?;
    let mut seen = HashSet::new();
    let mut widgets = Vec::new();
    let mut tab_assignments = Vec::new();
    let mut warnings = Vec::new();
    for mut widget in raw.widgets {
        require_text(&widget.id, &widget.id, "id")?;
        require_text(&widget.command, &widget.id, "command")?;
        if WidgetId::new(widget.id.clone()).is_recommendation() {
            return Err(ConfigError::ReservedWidgetId(widget.id));
        }
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
        pomodoro: convert_pomodoro(raw.pomodoro, &mut warnings)?,
        llm: convert_llm(raw.llm)?,
        warnings,
    })
}

fn require_text(value: &str, widget: &str, field: &'static str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::EmptyValue {
            widget: widget.to_string(),
            field,
        });
    }
    Ok(())
}

pub fn restart_sections(current: &AppConfig, next: &AppConfig) -> RestartSections {
    RestartSections {
        llm_changed: current.llm != next.llm,
        pomodoro_changed: current.pomodoro != next.pomodoro,
    }
}

pub fn retain_restart_sections(active: &AppConfig, mut next: AppConfig) -> AppConfig {
    next.llm = active.llm.clone();
    next.pomodoro = active.pomodoro.clone();
    next
}

pub fn load_config(directory: &Path) -> Result<AppConfig, ConfigError> {
    parse_config(&read_installed_config(directory)?)
}

pub fn read_installed_config(directory: &Path) -> Result<String, ConfigError> {
    for path in [
        directory.to_path_buf(),
        config_file_path(directory),
        directory.join(PLUGINS_DIRECTORY_NAME),
    ] {
        if path.exists() {
            verify_trusted_path(&path).map_err(|error| ConfigError::Parse(error.to_string()))?;
        }
    }
    read_config_file(&config_file_path(directory))
}

pub fn prepare_reload(
    current: &[WidgetSettings],
    text: &str,
) -> Result<PreparedWidgets<WidgetLaunchSpec, AppConfig>, ConfigError> {
    let config = parse_config(text)?;
    let current_identities: Vec<WidgetLaunchSpec> =
        current.iter().map(WidgetSettings::launch_spec).collect();
    let next_identities: Vec<WidgetLaunchSpec> = config
        .widgets
        .iter()
        .map(WidgetSettings::launch_spec)
        .collect();
    let plan = plan_reload(&current_identities, &next_identities);
    Ok(PreparedWidgets {
        plan,
        specs: next_identities,
        config,
    })
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
    fn widget_settings_project_to_a_launch_spec() {
        let config = parse_config(
            "[[widgets]]\nid = \"jira\"\ncommand = \"python3\"\nargs = [\"jira.py\"]\ninterval_seconds = 10\ntimeout_seconds = 7\n[widgets.env]\nTOKEN = \"abc\"\n",
        )
        .unwrap();

        let spec = config.widgets[0].launch_spec();

        assert_eq!(spec.id, WidgetId::new("jira"));
        assert_eq!(spec.command.program, "python3");
        assert_eq!(spec.command.args, vec!["jira.py".to_string()]);
        assert_eq!(
            spec.command.env,
            vec![("TOKEN".to_string(), "abc".to_string())]
        );
        assert_eq!(spec.command.timeout, Duration::from_secs(7));
        assert_eq!(
            spec.schedule.interval,
            Duration::from_secs(MINIMUM_INTERVAL_SECONDS)
        );
        assert!(spec.schedule.interval_adjusted);
    }

    #[test]
    fn widget_cannot_use_the_reserved_recommendation_id() {
        let result = parse_config("[[widgets]]\nid = \"ia\"\ncommand = \"x\"\n");
        assert_eq!(
            result.unwrap_err(),
            ConfigError::ReservedWidgetId("ia".to_string())
        );
    }

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
    fn inherit_env_is_optional_and_validated() {
        let absent = parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\n").unwrap();
        assert_eq!(absent.widgets[0].inherit_env, None);
        let listed = parse_config(
            "[[widgets]]\nid = \"a\"\ncommand = \"x\"\ninherit_env = [\"JIRA_TOKEN_A\"]\n",
        )
        .unwrap();
        assert_eq!(
            listed.widgets[0].inherit_env,
            Some(vec!["JIRA_TOKEN_A".to_string()])
        );
        assert!(matches!(
            parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ninherit_env = [\"A=B\"]\n"),
            Err(ConfigError::InvalidValue { .. })
        ));
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

    #[test]
    fn blank_widget_id_and_command_are_rejected() {
        let cases = [
            ("", "sh", "id"),
            ("  ", "sh", "id"),
            ("agenda", "", "command"),
            ("agenda", "  ", "command"),
        ];
        for (id, command, field) in cases {
            let text = format!("[[widgets]]\nid = \"{id}\"\ncommand = \"{command}\"\n");

            let error = parse_config(&text).unwrap_err();

            assert_eq!(
                error,
                ConfigError::EmptyValue {
                    widget: id.to_string(),
                    field,
                },
                "{text}"
            );
        }
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
        for interval in [30, 31, MAXIMUM_INTERVAL_SECONDS as i64] {
            let widget = interval_settings(interval);
            assert_eq!(widget.interval_seconds, interval as u64);
            assert!(!widget.interval_adjusted, "{interval}");
        }
    }

    #[test]
    fn interval_above_maximum_is_capped_and_flagged() {
        for interval in [MAXIMUM_INTERVAL_SECONDS as i64 + 1, i64::MAX] {
            let widget = interval_settings(interval);
            assert_eq!(widget.interval_seconds, MAXIMUM_INTERVAL_SECONDS);
            assert!(widget.interval_adjusted, "{interval}");
        }
    }

    #[test]
    fn widget_timeout_above_maximum_is_rejected() {
        let text = format!(
            "[[widgets]]\nid = \"a\"\ncommand = \"x\"\ntimeout_seconds = {}\n",
            MAXIMUM_TIMEOUT_SECONDS + 1
        );
        assert_eq!(
            parse_config(&text).unwrap_err(),
            ConfigError::TooLarge {
                location: SettingLocation::Widget("a".to_string()),
                field: "timeout_seconds",
                maximum: MAXIMUM_TIMEOUT_SECONDS,
            }
        );
    }

    #[test]
    fn llm_timeout_above_maximum_is_rejected() {
        let text = format!("[llm]\ntimeout_seconds = {}\n", MAXIMUM_TIMEOUT_SECONDS + 1);
        assert_eq!(
            parse_config(&text).unwrap_err(),
            ConfigError::TooLarge {
                location: SettingLocation::Section("llm"),
                field: "timeout_seconds",
                maximum: MAXIMUM_TIMEOUT_SECONDS,
            }
        );
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
            ConfigError::NotPositive {
                location: SettingLocation::Widget("a".to_string()),
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
            ConfigError::NotPositive {
                location: SettingLocation::Widget("a".to_string()),
                field: "timeout_seconds"
            }
        );
    }

    #[test]
    fn setting_errors_keep_the_user_facing_messages() {
        let widget =
            parse_config("[[widgets]]\nid = \"a\"\ncommand = \"x\"\ninterval_seconds = 0\n")
                .unwrap_err();
        assert_eq!(
            widget.to_string(),
            "widget 'a': 'interval_seconds' precisa ser maior que zero"
        );
        let section = parse_config("[pomodoro]\nfocus_minutes = 0\n").unwrap_err();
        assert_eq!(
            section.to_string(),
            "[pomodoro]: 'focus_minutes' precisa ser maior que zero"
        );
        let too_large = parse_config(&format!(
            "[llm]\ntimeout_seconds = {}\n",
            MAXIMUM_TIMEOUT_SECONDS + 1
        ))
        .unwrap_err();
        assert_eq!(
            too_large.to_string(),
            format!("[llm]: 'timeout_seconds' não pode passar de {MAXIMUM_TIMEOUT_SECONDS}")
        );
    }

    #[test]
    fn harness_names_round_trip_and_unknown_names_are_rejected() {
        for harness in [Harness::Claude, Harness::OpenCode] {
            assert_eq!(Harness::from_name(harness.program_name()), Some(harness));
        }
        assert_eq!(Harness::Claude.program_name(), "claude");
        assert_eq!(Harness::OpenCode.program_name(), "opencode");
        assert_eq!(Harness::from_name("outro"), None);
    }

    #[test]
    fn for_command_uses_the_documented_defaults() {
        let settings = WidgetSettings::for_command(
            WidgetId::new("x"),
            "cmd".to_string(),
            vec!["a".to_string()],
            vec![("K".to_string(), "v".to_string())],
        );
        assert_eq!(settings.interval_seconds, DEFAULT_INTERVAL_SECONDS);
        assert!(!settings.interval_adjusted);
        assert_eq!(settings.timeout_seconds, DEFAULT_TIMEOUT_SECONDS);
        assert_eq!(settings.span, DEFAULT_SPAN);
        assert_eq!(settings.max_items, None);
        assert_eq!(settings.llm_selection, default_llm_selection(Vec::new()));
        assert_eq!(settings.llm_selection.max_items, DEFAULT_LLM_MAX_ITEMS);
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
            config.pomodoro.notify_command,
            Some(vec!["notify-send".to_string()])
        );
        assert!(
            config
                .warnings
                .iter()
                .any(|warning| warning.contains("notify_command")),
            "{:?}",
            config.warnings
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
        let first = &config.widgets[0].llm_selection;
        assert_eq!(first.sections, vec!["inbox"]);
        assert_eq!((first.max_items, first.max_characters_per_item), (20, 120));
        let second = &config.widgets[1].llm_selection;
        assert!(second.sections.is_empty());
        assert_eq!((second.max_items, second.max_characters_per_item), (5, 50));
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
                ConfigError::NotPositive {
                    location: SettingLocation::Section("pomodoro"),
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
            ConfigError::NotPositive {
                location: SettingLocation::Section("llm"),
                field: "timeout_seconds"
            }
        );
        assert_eq!(
            parse_config("[llm]\nmax_budget_usd = 0.0\n").unwrap_err(),
            ConfigError::NotPositive {
                location: SettingLocation::Section("llm"),
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
                matches!(result, Err(ConfigError::NotPositive { .. })),
                "{field}"
            );
        }
    }

    #[test]
    fn blank_notify_command_becomes_none() {
        let config = parse_config("[pomodoro]\nnotify_command = \"   \"\n").unwrap();
        assert_eq!(config.pomodoro.notify_command, None);
        assert!(config.warnings.is_empty());
    }

    #[test]
    fn notify_command_accepts_a_list_of_arguments() {
        let config = parse_config(
            "[pomodoro]\nnotify_command = [\"/opt/meu notifier/run\", \"--urgente\"]\n",
        )
        .unwrap();
        assert_eq!(
            config.pomodoro.notify_command,
            Some(vec![
                "/opt/meu notifier/run".to_string(),
                "--urgente".to_string()
            ])
        );
        assert!(config.warnings.is_empty());
    }

    #[test]
    fn legacy_notify_command_string_is_split_with_a_warning() {
        let config =
            parse_config("[pomodoro]\nnotify_command = \"notify-send --urgency=low\"\n").unwrap();
        assert_eq!(
            config.pomodoro.notify_command,
            Some(vec!["notify-send".to_string(), "--urgency=low".to_string()])
        );
        assert!(
            config
                .warnings
                .iter()
                .any(|warning| warning.contains("notify_command")),
            "{:?}",
            config.warnings
        );
    }

    #[test]
    fn empty_notify_command_list_becomes_none() {
        let config = parse_config("[pomodoro]\nnotify_command = []\n").unwrap();
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
            ConfigError::NotPositive {
                location: SettingLocation::Widget("a".to_string()),
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
    fn restart_sections_flags_changed_llm_and_pomodoro() {
        let current = parse_config(RELOAD_BASE).unwrap();
        let llm = parse_config(&format!("{RELOAD_BASE}\n[llm]\nharness = \"opencode\"\n")).unwrap();
        let pomodoro =
            parse_config(&format!("{RELOAD_BASE}\n[pomodoro]\nfocus_minutes = 30\n")).unwrap();

        assert_eq!(
            restart_sections(&current, &current),
            RestartSections::default()
        );
        assert_eq!(
            restart_sections(&current, &llm),
            RestartSections {
                llm_changed: true,
                pomodoro_changed: false
            }
        );
        assert_eq!(
            restart_sections(&current, &pomodoro),
            RestartSections {
                llm_changed: false,
                pomodoro_changed: true
            }
        );
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

        match result {
            Err(ConfigError::Io { path, kind, .. }) => {
                assert_eq!(kind, std::io::ErrorKind::NotFound);
                assert!(path.ends_with("mosaic.toml"), "{path}");
            }
            other => panic!("esperava ConfigError::Io, veio {other:?}"),
        }
    }

    #[test]
    fn config_file_lives_inside_the_directory() {
        let path = config_file_path(std::path::Path::new("/etc/panel"));
        assert_eq!(path, std::path::PathBuf::from("/etc/panel/mosaic.toml"));
    }
}
