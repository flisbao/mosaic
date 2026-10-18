use std::fs;
use std::path::Path;

use crate::infrastructure::config::{config_file_path, ConfigError};

const PLUGINS_DIRECTORY_NAME: &str = "plugins";

const DEFAULT_CONFIG: &str = r#"[[widgets]]
id = "kanban"
command = "python3"
args = ["plugins/kanban.py"]
interval_seconds = 30
tab = true
llm_sections = ["inbox", "in progress", "acompanhar_jira"]

[widgets.env]
KANBAN_DIR = "~/kanban"

[[widgets]]
id = "example-python"
command = "python3"
args = ["plugins/example.py"]

[[widgets]]
id = "example-node"
command = "node"
args = ["plugins/example.js"]

[pomodoro]
enabled = true
focus_minutes = 25
short_break_minutes = 5
long_break_minutes = 15
cycles_before_long_break = 4
suggestions = ["Beber água", "Levantar e alongar", "Tomar um café"]
bell = true
notify_command = ""

[llm]
enabled = true
harness = "claude"
timeout_seconds = 120
confirm_before_send = true
priority_prompt = "prompts/prioridade.md"

# --- Jira (descomente e ajuste; veja docs/jira.md) ---
# Guarde o token numa variável de ambiente, por exemplo:
#   export JIRA_TOKEN_EMPRESA="seu-token-de-api"
#
# Gerente: acompanha o time.
# [[widgets]]
# id = "jira-empresa-time"
# command = "python3"
# args = ["plugins/jira.py"]
# interval_seconds = 120
# timeout_seconds = 45
# tab = true
# llm_sections = ["Sem dono", "Em andamento"]
#
# [widgets.env]
# JIRA_SITE = "empresa.atlassian.net"
# JIRA_EMAIL = "voce@empresa.com"
# JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
# JIRA_MODE = "manager"
# JIRA_PROJECT = "VHPJ"
# JIRA_BOARD = "42"
# JIRA_TEAM = "ana@empresa.com, bruno@empresa.com"
#
# Pessoa do projeto: só os seus tickets.
# [[widgets]]
# id = "jira-cliente-meus"
# command = "python3"
# args = ["plugins/jira.py"]
# interval_seconds = 120
# timeout_seconds = 45
# tab = true
# llm_sections = ["Meus tickets"]
#
# [widgets.env]
# JIRA_SITE = "cliente.atlassian.net"
# JIRA_EMAIL = "voce@consultoria.com"
# JIRA_TOKEN_ENV = "JIRA_TOKEN_CLIENTE"
# JIRA_MODE = "personal"
# JIRA_PROJECT = "CLI"

# --- Galeria de blocos (descomente para ver todos os blocos; veja docs/protocol.md) ---
# [[widgets]]
# id = "gallery-python"
# command = "python3"
# args = ["plugins/gallery.py"]
# span = 2
#
# [[widgets]]
# id = "gallery-node"
# command = "node"
# args = ["plugins/gallery.js"]
# span = 2
"#;

const PRIORITY_PROMPT: &str = include_str!("../../prompts/prioridade.md");
const PROMPTS_DIRECTORY_NAME: &str = "prompts";
const PRIORITY_PROMPT_FILE_NAME: &str = "prioridade.md";

const BUNDLED_PLUGINS: [(&str, &str); 6] = [
    ("kanban.py", include_str!("../../plugins/kanban.py")),
    ("example.py", include_str!("../../plugins/example.py")),
    ("example.js", include_str!("../../plugins/example.js")),
    ("jira.py", include_str!("../../plugins/jira.py")),
    ("gallery.py", include_str!("../../plugins/gallery.py")),
    ("gallery.js", include_str!("../../plugins/gallery.js")),
];

pub fn default_config_text() -> &'static str {
    DEFAULT_CONFIG
}

pub fn default_priority_prompt() -> &'static str {
    PRIORITY_PROMPT
}

pub fn install_defaults_if_missing(directory: &Path) -> Result<bool, ConfigError> {
    let config_path = config_file_path(directory);
    if config_path.exists() {
        return Ok(false);
    }
    let plugins_directory = directory.join(PLUGINS_DIRECTORY_NAME);
    fs::create_dir_all(&plugins_directory).map_err(to_io_error)?;
    for (name, content) in BUNDLED_PLUGINS {
        fs::write(plugins_directory.join(name), content).map_err(to_io_error)?;
    }
    let prompts_directory = directory.join(PROMPTS_DIRECTORY_NAME);
    fs::create_dir_all(&prompts_directory).map_err(to_io_error)?;
    fs::write(
        prompts_directory.join(PRIORITY_PROMPT_FILE_NAME),
        PRIORITY_PROMPT,
    )
    .map_err(to_io_error)?;
    fs::write(config_path, DEFAULT_CONFIG).map_err(to_io_error)?;
    Ok(true)
}

fn to_io_error(error: std::io::Error) -> ConfigError {
    ConfigError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::config::{config_file_path, parse_config};

    #[test]
    fn default_config_parses_without_duplicates() {
        let config = parse_config(default_config_text()).unwrap();
        let ids: Vec<&str> = config.widgets.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, vec!["kanban", "example-python", "example-node"]);
        assert!(config.widgets[0].tab);
    }

    #[test]
    fn install_creates_config_and_plugins_when_missing() {
        let directory = tempfile::tempdir().unwrap();
        assert!(install_defaults_if_missing(directory.path()).unwrap());
        assert!(config_file_path(directory.path()).is_file());
        for plugin in ["kanban.py", "example.py", "example.js"] {
            assert!(
                directory.path().join("plugins").join(plugin).is_file(),
                "{plugin}"
            );
        }
    }

    #[test]
    fn default_kanban_uses_the_minimum_interval() {
        let config = crate::infrastructure::config::parse_config(default_config_text()).unwrap();
        let kanban = &config.widgets[0];
        assert_eq!(kanban.id.as_str(), "kanban");
        assert_eq!(kanban.interval_seconds, 30);
        assert!(!kanban.interval_adjusted);
    }

    #[test]
    fn default_config_enables_kanban_for_the_llm() {
        let config = parse_config(default_config_text()).unwrap();
        assert_eq!(
            config.widgets[0].llm_sections,
            vec!["inbox", "in progress", "acompanhar_jira"]
        );
        assert!(config.widgets[1].llm_sections.is_empty());
    }

    #[test]
    fn default_config_parses_with_pomodoro_and_llm_sections() {
        let config = parse_config(default_config_text()).unwrap();
        assert!(config.pomodoro.enabled);
        assert_eq!(config.pomodoro.notify_command, None);
        assert!(config.llm.confirm_before_send);
        assert_eq!(config.llm.priority_prompt, "prompts/prioridade.md");
    }

    #[test]
    fn install_creates_the_priority_prompt() {
        let directory = tempfile::tempdir().unwrap();
        install_defaults_if_missing(directory.path()).unwrap();
        let installed =
            std::fs::read_to_string(directory.path().join("prompts/prioridade.md")).unwrap();
        assert_eq!(installed, default_priority_prompt());
    }

    #[test]
    fn default_priority_prompt_mentions_output_format() {
        assert!(default_priority_prompt().contains("Comece por"));
    }

    #[test]
    fn install_creates_the_jira_plugin() {
        let directory = tempfile::tempdir().unwrap();
        install_defaults_if_missing(directory.path()).unwrap();
        let installed = std::fs::read_to_string(directory.path().join("plugins/jira.py")).unwrap();
        assert!(installed.contains("def main("));
    }

    #[test]
    fn default_config_still_parses_and_has_no_active_jira_widget() {
        let config = parse_config(default_config_text()).unwrap();
        assert!(config
            .widgets
            .iter()
            .all(|widget| !widget.id.as_str().starts_with("jira")));
    }

    #[test]
    fn default_config_documents_jira_examples() {
        let text = default_config_text();
        assert!(text.contains("JIRA_SITE"));
        assert!(text.contains("JIRA_MODE = \"manager\""));
        assert!(text.contains("JIRA_MODE = \"personal\""));
    }

    #[test]
    fn install_creates_the_gallery_plugins() {
        let directory = tempfile::tempdir().unwrap();
        install_defaults_if_missing(directory.path()).unwrap();
        for name in ["plugins/gallery.py", "plugins/gallery.js"] {
            let installed = std::fs::read_to_string(directory.path().join(name)).unwrap();
            assert!(installed.contains("alerts"), "{name}");
        }
    }

    #[test]
    fn default_config_documents_the_gallery_examples() {
        let text = default_config_text();
        assert!(text.contains("# args = [\"plugins/gallery.py\"]"));
        assert!(text.contains("# args = [\"plugins/gallery.js\"]"));
        let config = parse_config(text).unwrap();
        assert!(config
            .widgets
            .iter()
            .all(|widget| !widget.id.as_str().starts_with("gallery")));
    }

    #[test]
    fn install_never_overwrites_existing_config() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(config_file_path(directory.path()), "# meu arquivo").unwrap();
        assert!(!install_defaults_if_missing(directory.path()).unwrap());
        let content = std::fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(content, "# meu arquivo");
        assert!(!directory.path().join("plugins").exists());
    }
}
