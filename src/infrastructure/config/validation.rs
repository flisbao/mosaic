use super::raw::{
    RawLlm, RawNotifyCommand, RawPomodoro, RawTab, RawTabSection, RawTheme, RawWidget,
};
use super::{
    ConfigError, Harness, LlmConfig, PomodoroConfig, SettingLocation, TabSettings, WidgetSettings,
    DEFAULT_CYCLES_BEFORE_LONG_BREAK, DEFAULT_FOCUS_MINUTES, DEFAULT_INTERVAL_SECONDS,
    DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM, DEFAULT_LLM_MAX_ITEMS, DEFAULT_LLM_TIMEOUT_SECONDS,
    DEFAULT_LONG_BREAK_MINUTES, DEFAULT_PRIORITY_PROMPT, DEFAULT_SHORT_BREAK_MINUTES, DEFAULT_SPAN,
    DEFAULT_SUGGESTIONS, DEFAULT_TIMEOUT_SECONDS, IMPLICIT_TAB_NAME, MAXIMUM_INTERVAL_SECONDS,
    MAXIMUM_TIMEOUT_SECONDS, MINIMUM_INTERVAL_SECONDS,
};
use crate::application::recommendation::LlmSelection;
use crate::domain::theme::{Rgb, Theme, ThemeError, ThemeRole, DEFAULT_THEME_NAME};
use crate::domain::widget::WidgetId;
use crate::infrastructure::child_environment::is_valid_variable_name;

const POMODORO_SECTION: &str = "pomodoro";
const LEGACY_NOTIFY_COMMAND_WARNING: &str =
    "pomodoro.notify_command como texto é legado; use uma lista, ex.: [\"notify-send\"]";
const LLM_SECTION: &str = "llm";
const THEME_SECTION: &str = "theme";
const THEME_NAME_KEY: &str = "name";
const THEME_PAINT_BACKGROUND_KEY: &str = "paint_background";
const UNKNOWN_THEME_HINT: &str = "; veja `mosaic themes`";
const MAXIMUM_WARNING_VALUE_CHARACTERS: usize = 40;
const ELLIPSIS: char = '…';
const FIRST_TAB_INDEX: usize = 0;

pub(super) struct TabAssignment {
    pub(super) widget_id: String,
    pub(super) tab_name: Option<String>,
}

pub(super) fn named_tab(
    widget_id: &str,
    tab: Option<RawTab>,
    warnings: &mut Vec<String>,
) -> Option<String> {
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

pub(super) fn build_tabs(
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

pub(super) fn convert_widget(widget: RawWidget) -> Result<WidgetSettings, ConfigError> {
    if widget
        .inherit_env
        .as_ref()
        .is_some_and(|names| !names.iter().all(|name| is_valid_variable_name(name)))
    {
        return Err(ConfigError::InvalidValue {
            widget: widget.id.clone(),
            field: "inherit_env",
        });
    }
    let location = SettingLocation::Widget(widget.id.clone());
    let configured_interval = positive_or_default(
        widget.interval_seconds,
        DEFAULT_INTERVAL_SECONDS,
        &location,
        "interval_seconds",
    )?;
    let interval_seconds =
        configured_interval.clamp(MINIMUM_INTERVAL_SECONDS, MAXIMUM_INTERVAL_SECONDS);
    let timeout_seconds = at_most(
        positive_or_default(
            widget.timeout_seconds,
            DEFAULT_TIMEOUT_SECONDS,
            &location,
            "timeout_seconds",
        )?,
        MAXIMUM_TIMEOUT_SECONDS,
        &location,
        "timeout_seconds",
    )?;
    let max_items = positive_optional(widget.max_items, &location, "max_items")?;
    let llm_max_items = positive_or_default(
        widget.llm_max_items,
        DEFAULT_LLM_MAX_ITEMS,
        &location,
        "llm_max_items",
    )?;
    let llm_max_characters_per_item = positive_or_default(
        widget.llm_max_characters_per_item,
        DEFAULT_LLM_MAX_CHARACTERS_PER_ITEM,
        &location,
        "llm_max_characters_per_item",
    )?;
    Ok(WidgetSettings {
        id: WidgetId::new(widget.id),
        command: widget.command,
        args: widget.args,
        env: widget.env.into_iter().collect(),
        inherit_env: widget.inherit_env,
        interval_seconds,
        interval_adjusted: interval_seconds != configured_interval,
        timeout_seconds,
        span: widget.span.unwrap_or(DEFAULT_SPAN),
        max_items,
        llm_selection: LlmSelection {
            sections: widget.llm_sections,
            max_items: llm_max_items,
            max_characters_per_item: llm_max_characters_per_item,
        },
    })
}

pub(super) fn convert_pomodoro(
    raw: RawPomodoro,
    warnings: &mut Vec<String>,
) -> Result<PomodoroConfig, ConfigError> {
    let location = SettingLocation::Section(POMODORO_SECTION);
    let cycles_before_long_break = positive_or_default(
        raw.cycles_before_long_break,
        DEFAULT_CYCLES_BEFORE_LONG_BREAK,
        &location,
        "cycles_before_long_break",
    )?;
    Ok(PomodoroConfig {
        enabled: raw.enabled.unwrap_or(true),
        focus_minutes: positive_or_default(
            raw.focus_minutes,
            DEFAULT_FOCUS_MINUTES,
            &location,
            "focus_minutes",
        )?,
        short_break_minutes: positive_or_default(
            raw.short_break_minutes,
            DEFAULT_SHORT_BREAK_MINUTES,
            &location,
            "short_break_minutes",
        )?,
        long_break_minutes: positive_or_default(
            raw.long_break_minutes,
            DEFAULT_LONG_BREAK_MINUTES,
            &location,
            "long_break_minutes",
        )?,
        cycles_before_long_break,
        suggestions: raw
            .suggestions
            .unwrap_or_else(|| DEFAULT_SUGGESTIONS.iter().map(|s| s.to_string()).collect()),
        bell: raw.bell.unwrap_or(true),
        notify_command: convert_notify_command(raw.notify_command, warnings),
    })
}

fn convert_notify_command(
    raw: Option<RawNotifyCommand>,
    warnings: &mut Vec<String>,
) -> Option<Vec<String>> {
    let arguments = match raw? {
        RawNotifyCommand::Arguments(arguments) => arguments,
        RawNotifyCommand::Line(line) => {
            let arguments: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            if !arguments.is_empty() {
                warnings.push(LEGACY_NOTIFY_COMMAND_WARNING.to_string());
            }
            arguments
        }
    };
    let cleaned: Vec<String> = arguments
        .into_iter()
        .filter(|argument| !argument.trim().is_empty())
        .collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

pub(super) fn convert_llm(raw: RawLlm) -> Result<LlmConfig, ConfigError> {
    let harness = match raw.harness.as_deref() {
        None => Harness::Claude,
        Some(name) => {
            Harness::from_name(name).ok_or_else(|| ConfigError::UnknownHarness(name.to_string()))?
        }
    };
    let location = SettingLocation::Section(LLM_SECTION);
    if raw.max_budget_usd.is_some_and(|budget| budget <= 0.0) {
        return Err(ConfigError::NotPositive {
            location,
            field: "max_budget_usd",
        });
    }
    let timeout_seconds = at_most(
        positive_or_default(
            raw.timeout_seconds,
            DEFAULT_LLM_TIMEOUT_SECONDS,
            &location,
            "timeout_seconds",
        )?,
        MAXIMUM_TIMEOUT_SECONDS,
        &location,
        "timeout_seconds",
    )?;
    Ok(LlmConfig {
        enabled: raw.enabled.unwrap_or(true),
        harness,
        model: raw.model.filter(|model| !model.trim().is_empty()),
        max_budget_usd: raw.max_budget_usd,
        timeout_seconds,
        confirm_before_send: raw.confirm_before_send.unwrap_or(true),
        priority_prompt: raw
            .priority_prompt
            .unwrap_or_else(|| DEFAULT_PRIORITY_PROMPT.to_string()),
    })
}

fn positive_or_default<T: Default + PartialEq>(
    value: Option<T>,
    default: T,
    location: &SettingLocation,
    field: &'static str,
) -> Result<T, ConfigError> {
    match value {
        Some(amount) if amount == T::default() => Err(ConfigError::NotPositive {
            location: location.clone(),
            field,
        }),
        Some(amount) => Ok(amount),
        None => Ok(default),
    }
}

fn positive_optional<T: Default + PartialEq>(
    value: Option<T>,
    location: &SettingLocation,
    field: &'static str,
) -> Result<Option<T>, ConfigError> {
    match value {
        Some(amount) if amount == T::default() => Err(ConfigError::NotPositive {
            location: location.clone(),
            field,
        }),
        other => Ok(other),
    }
}

fn at_most(
    value: u64,
    maximum: u64,
    location: &SettingLocation,
    field: &'static str,
) -> Result<u64, ConfigError> {
    if value > maximum {
        return Err(ConfigError::TooLarge {
            location: location.clone(),
            field,
            maximum,
        });
    }
    Ok(value)
}

pub(super) fn build_theme(raw: RawTheme, warnings: &mut Vec<String>) -> Theme {
    let requested = theme_text(THEME_NAME_KEY, raw.name.as_ref(), warnings)
        .map(str::trim)
        .unwrap_or_default();
    let name = if requested.is_empty() {
        DEFAULT_THEME_NAME
    } else {
        requested
    };
    let (base, resolved) = match Theme::preset(name) {
        Ok(theme) => (theme, true),
        Err(error) => {
            warnings.push(theme_warning(&error));
            (Theme::Ansi, false)
        }
    };
    if base == Theme::Ansi {
        if resolved && !raw.colors.is_empty() {
            warnings.push(format!(
                "[{THEME_SECTION}.colors] não vale para o tema '{DEFAULT_THEME_NAME}' (ele usa as cores do terminal)"
            ));
        }
        return Theme::Ansi;
    }
    let themed = base.with_overrides(&theme_overrides(&raw.colors, warnings));
    let paints_background = match raw.paint_background {
        Some(toml::Value::Boolean(value)) => value,
        Some(_) => {
            warnings.push(format!(
                "[{THEME_SECTION}] {THEME_PAINT_BACKGROUND_KEY}: use true ou false"
            ));
            true
        }
        None => true,
    };
    if !paints_background {
        themed.without_background()
    } else {
        themed
    }
}

fn theme_text<'a>(
    key: &str,
    value: Option<&'a toml::Value>,
    warnings: &mut Vec<String>,
) -> Option<&'a str> {
    match value {
        Some(toml::Value::String(text)) => Some(text),
        Some(_) => {
            warnings.push(format!(
                "[{THEME_SECTION}] {}: use um texto entre aspas",
                shortened(key)
            ));
            None
        }
        None => None,
    }
}

fn theme_overrides(
    colors: &std::collections::BTreeMap<String, toml::Value>,
    warnings: &mut Vec<String>,
) -> Vec<(ThemeRole, Rgb)> {
    let mut overrides = Vec::new();
    for (key, value) in colors {
        let role = match ThemeRole::from_key(key) {
            Ok(role) => role,
            Err(error) => {
                warnings.push(theme_warning(&error));
                continue;
            }
        };
        let Some(value) = theme_text(key, Some(value), warnings) else {
            continue;
        };
        match Rgb::from_hex(value) {
            Ok(rgb) => overrides.push((role, rgb)),
            Err(error) => warnings.push(format!(
                "[{THEME_SECTION}.colors] {}: {}",
                shortened(key),
                shortened_theme_error(error)
            )),
        }
    }
    overrides
}

fn theme_warning(error: &ThemeError) -> String {
    let hint = match error {
        ThemeError::UnknownTheme(_) => UNKNOWN_THEME_HINT,
        ThemeError::InvalidHex(_) | ThemeError::UnknownRole(_) => "",
    };
    format!(
        "[{THEME_SECTION}] {}{hint}",
        shortened_theme_error(error.clone())
    )
}

fn shortened_theme_error(error: ThemeError) -> ThemeError {
    match error {
        ThemeError::UnknownTheme(name) => ThemeError::UnknownTheme(shortened(&name)),
        ThemeError::InvalidHex(value) => ThemeError::InvalidHex(shortened(&value)),
        ThemeError::UnknownRole(role) => ThemeError::UnknownRole(shortened(&role)),
    }
}

fn shortened(value: &str) -> String {
    if value.chars().count() <= MAXIMUM_WARNING_VALUE_CHARACTERS {
        return value.to_string();
    }
    let kept: String = value
        .chars()
        .take(MAXIMUM_WARNING_VALUE_CHARACTERS - 1)
        .collect();
    format!("{kept}{ELLIPSIS}")
}
