use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct RawConfig {
    #[serde(default)]
    pub(super) tabs: Vec<RawTabSection>,
    #[serde(default)]
    pub(super) widgets: Vec<RawWidget>,
    #[serde(default)]
    pub(super) pomodoro: RawPomodoro,
    #[serde(default)]
    pub(super) llm: RawLlm,
    #[serde(default)]
    pub(super) theme: RawTheme,
}

#[derive(Deserialize)]
pub(super) struct RawTabSection {
    pub(super) name: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawTab {
    Legacy(bool),
    Name(String),
}

#[derive(Deserialize)]
pub(super) struct RawWidget {
    pub(super) id: String,
    pub(super) command: String,
    #[serde(default)]
    pub(super) args: Vec<String>,
    #[serde(default)]
    pub(super) env: BTreeMap<String, String>,
    pub(super) inherit_env: Option<Vec<String>>,
    pub(super) interval_seconds: Option<u64>,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) span: Option<u16>,
    pub(super) tab: Option<RawTab>,
    pub(super) max_items: Option<usize>,
    #[serde(default)]
    pub(super) llm_sections: Vec<String>,
    pub(super) llm_max_items: Option<usize>,
    pub(super) llm_max_characters_per_item: Option<usize>,
}

#[derive(Deserialize, Default)]
pub(super) struct RawPomodoro {
    pub(super) enabled: Option<bool>,
    pub(super) focus_minutes: Option<u64>,
    pub(super) short_break_minutes: Option<u64>,
    pub(super) long_break_minutes: Option<u64>,
    pub(super) cycles_before_long_break: Option<u32>,
    pub(super) suggestions: Option<Vec<String>>,
    pub(super) bell: Option<bool>,
    pub(super) notify_command: Option<RawNotifyCommand>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawNotifyCommand {
    Line(String),
    Arguments(Vec<String>),
}

#[derive(Deserialize, Default)]
pub(super) struct RawLlm {
    pub(super) enabled: Option<bool>,
    pub(super) harness: Option<String>,
    pub(super) model: Option<String>,
    pub(super) max_budget_usd: Option<f64>,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) confirm_before_send: Option<bool>,
    pub(super) priority_prompt: Option<String>,
}

#[derive(Deserialize, Default)]
pub(super) struct RawTheme {
    pub(super) name: Option<String>,
    pub(super) paint_background: Option<bool>,
    #[serde(default)]
    pub(super) colors: BTreeMap<String, String>,
}
