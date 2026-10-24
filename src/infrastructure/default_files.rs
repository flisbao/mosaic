use std::fs;
use std::path::Path;
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

use crate::infrastructure::app_paths::PRIORITY_PROMPT_RELATIVE_PATH;
use crate::infrastructure::config::{config_file_path, ConfigError};

const PLUGINS_DIRECTORY_NAME: &str = "plugins";

const DEFAULT_CONFIG: &str = r#"[[widgets]]
id = "kanban"
command = "python3"
args = ["plugins/kanban.py"]
interval_seconds = 30
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

# --- Jira (descomente e ajuste; passo a passo em plugins/jira/README.md no repositório) ---
# Cada widget define o próprio JQL em args; o Mosaic acrescenta `snapshot`.
# Guarde o token numa variável de ambiente, por exemplo:
#   export JIRA_TOKEN_EMPRESA="seu-token-de-api"
#
# Minhas tarefas abertas.
# [[widgets]]
# id = "jira-minhas-tarefas"
# command = "python3"
# args = ["plugins/jira/jira.py", "--jql", "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC", "--title", "Minhas tarefas", "--view", "lines"]
# interval_seconds = 120
# timeout_seconds = 45
# llm_sections = ["Minhas tarefas"]
# inherit_env = ["JIRA_TOKEN_EMPRESA"]
#
# [widgets.env]
# JIRA_SITE = "empresa.atlassian.net"
# JIRA_EMAIL = "voce@empresa.com"
# JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"
#
# O que cada pessoa do time está pegando agora.
# [[widgets]]
# id = "jira-time-em-andamento"
# command = "python3"
# args = ["plugins/jira/jira.py", "--jql", "project = VHPJ AND statusCategory = \"In Progress\"", "--title", "Time em andamento", "--group-by", "assignee", "--view", "table", "--columns", "key,summary,status"]
# interval_seconds = 120
# timeout_seconds = 45
# inherit_env = ["JIRA_TOKEN_EMPRESA"]
#
# [widgets.env]
# JIRA_SITE = "empresa.atlassian.net"
# JIRA_EMAIL = "voce@empresa.com"
# JIRA_TOKEN_ENV = "JIRA_TOKEN_EMPRESA"

# --- Agenda Google (descomente uma opção; passo a passo em plugins/calendar/README.md no repositório) ---
# Link ICS secreto, guardado numa variável de ambiente:
#   export CALENDAR_ICS_PERSONAL="https://calendar.google.com/calendar/ical/.../basic.ics"
# [[widgets]]
# id = "agenda"
# command = "python3"
# args = ["plugins/calendar/calendar_widget.py"]
# interval_seconds = 300
# timeout_seconds = 20
# inherit_env = ["CALENDAR_ICS_PERSONAL"]
#
# [widgets.env]
# CALENDAR_SOURCE = "ics"
# CALENDAR_ICS_URL_ENV = "CALENDAR_ICS_PERSONAL"
#
# GNOME Online Accounts: troque as duas linhas de [widgets.env] por CALENDAR_SOURCE = "goa"
# e use inherit_env = ["DBUS_SESSION_BUS_ADDRESS"].
# gcalcli: CALENDAR_SOURCE = "gcalcli" (rode `gcalcli init` antes).

# --- Pull requests (descomente o seu; passo a passo em plugins/<github|azure-devops|azure-pipelines|gitlab>/README.md no repositório) ---
#   export GITHUB_TOKEN_PRS="..."   export GITLAB_TOKEN_MRS="..."   (Azure DevOps usa az login: sem token)
# [[widgets]]
# id = "github-prs"
# command = "python3"
# args = ["plugins/github/github_prs.py"]
# interval_seconds = 120
# timeout_seconds = 30
# inherit_env = ["GITHUB_TOKEN_PRS"]
#
# [widgets.env]
# GITHUB_TOKEN_ENV = "GITHUB_TOKEN_PRS"
#
# [[widgets]]
# id = "azure-devops-prs"
# command = "python3"
# args = ["plugins/azure-devops/azure_devops.py"]
# interval_seconds = 120
# timeout_seconds = 30
# inherit_env = []
#
# [widgets.env]
# AZDO_ORGANIZATION = "minha-org"
# AZDO_AUTH_METHOD = "azure-cli"
#
# [[widgets]]
# id = "azure-pipelines"
# command = "python3"
# args = ["plugins/azure-pipelines/azure_pipelines.py"]
# interval_seconds = 900
# timeout_seconds = 60
# inherit_env = []
#
# [widgets.env]
# AZDO_ORGANIZATION = "minha-org"
# AZDO_PROJECTS = "meu-projeto"
# AZDO_AUTH_METHOD = "azure-cli"
#
# [[widgets]]
# id = "gitlab-mrs"
# command = "python3"
# args = ["plugins/gitlab/gitlab_mrs.py"]
# interval_seconds = 120
# timeout_seconds = 30
# inherit_env = ["GITLAB_TOKEN_MRS"]
#
# [widgets.env]
# GITLAB_TOKEN_ENV = "GITLAB_TOKEN_MRS"

# --- Galeria de blocos (descomente para ver todos os blocos; veja PROTOCOL.md) ---
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

const BUNDLED_PLUGINS: [(&str, &str); 11] = [
    ("kanban.py", include_str!("../../plugins/kanban.py")),
    ("example.py", include_str!("../../plugins/example.py")),
    ("example.js", include_str!("../../plugins/example.js")),
    ("jira/jira.py", include_str!("../../plugins/jira/jira.py")),
    (
        "calendar/calendar_widget.py",
        include_str!("../../plugins/calendar/calendar_widget.py"),
    ),
    (
        "github/github_prs.py",
        include_str!("../../plugins/github/github_prs.py"),
    ),
    (
        "azure-devops/azure_devops.py",
        include_str!("../../plugins/azure-devops/azure_devops.py"),
    ),
    (
        "azure-pipelines/azure_pipelines.py",
        include_str!("../../plugins/azure-pipelines/azure_pipelines.py"),
    ),
    (
        "gitlab/gitlab_mrs.py",
        include_str!("../../plugins/gitlab/gitlab_mrs.py"),
    ),
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
    create_private_directory(&plugins_directory)?;
    for (name, content) in BUNDLED_PLUGINS {
        let plugin_path = plugins_directory.join(name);
        if let Some(plugin_directory) = plugin_path.parent() {
            create_private_directory(plugin_directory)?;
        }
        write_private_file(&plugin_path, content)?;
    }
    let prompt_path = directory.join(PRIORITY_PROMPT_RELATIVE_PATH);
    if let Some(prompts_directory) = prompt_path.parent() {
        create_private_directory(prompts_directory)?;
    }
    write_private_file(&prompt_path, PRIORITY_PROMPT)?;
    write_private_file(&config_path, DEFAULT_CONFIG)?;
    Ok(true)
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIRECTORY_MODE)
        .create(path)
        .map_err(ConfigError::io(path))
}
#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> Result<(), ConfigError> {
    fs::create_dir_all(path).map_err(ConfigError::io(path))
}

#[cfg(unix)]
fn write_private_file(path: &Path, content: &str) -> Result<(), ConfigError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .map_err(ConfigError::io(path))?;
    file.write_all(content.as_bytes())
        .map_err(ConfigError::io(path))
}
#[cfg(not(unix))]
fn write_private_file(path: &Path, content: &str) -> Result<(), ConfigError> {
    fs::write(path, content).map_err(ConfigError::io(path))
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
        assert!(config.warnings.is_empty());
        assert_eq!(config.tabs.len(), 1);
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
            config.widgets[0].llm_selection.sections,
            vec!["inbox", "in progress", "acompanhar_jira"]
        );
        assert!(config.widgets[1].llm_selection.sections.is_empty());
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
        let installed =
            std::fs::read_to_string(directory.path().join("plugins/jira/jira.py")).unwrap();
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
        assert!(text.contains("--jql"));
        assert!(text.contains("currentUser()"));
        assert!(text.contains("\"--group-by\", \"assignee\""));
        assert!(!text.contains("JIRA_MODE"));
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
    fn install_creates_calendar_plugin_in_its_own_folder() {
        let directory = tempfile::tempdir().unwrap();
        assert!(install_defaults_if_missing(directory.path()).unwrap());
        let plugin = directory.path().join("plugins/calendar/calendar_widget.py");
        let installed = std::fs::read_to_string(plugin).unwrap();
        assert!(installed.contains("def main("));
    }

    #[test]
    fn default_config_has_commented_calendar_widget() {
        let text = default_config_text();
        assert!(text.contains("# args = [\"plugins/calendar/calendar_widget.py\"]"));
        assert!(text.contains("# CALENDAR_SOURCE = \"ics\""));
    }

    #[test]
    fn install_creates_pull_request_plugins_in_their_own_folders() {
        let directory = tempfile::tempdir().unwrap();
        assert!(install_defaults_if_missing(directory.path()).unwrap());
        for relative in [
            "plugins/github/github_prs.py",
            "plugins/azure-devops/azure_devops.py",
            "plugins/gitlab/gitlab_mrs.py",
        ] {
            let installed = std::fs::read_to_string(directory.path().join(relative)).unwrap();
            assert!(installed.contains("def main("), "{relative}");
        }
    }

    #[test]
    fn default_config_has_commented_pull_request_widgets() {
        let text = default_config_text();
        assert!(text.contains("# args = [\"plugins/github/github_prs.py\"]"));
        assert!(text.contains("# args = [\"plugins/azure-devops/azure_devops.py\"]"));
        assert!(text.contains("# args = [\"plugins/gitlab/gitlab_mrs.py\"]"));
        assert!(text.contains("# AZDO_ORGANIZATION = \"minha-org\""));
        assert!(text.contains("# AZDO_AUTH_METHOD = \"azure-cli\""));
        assert!(!text.contains("AZDO_TOKEN"));
    }

    #[test]
    fn install_creates_the_azure_pipelines_plugin_in_its_own_folder() {
        let directory = tempfile::tempdir().unwrap();
        assert!(install_defaults_if_missing(directory.path()).unwrap());
        let plugin = directory
            .path()
            .join("plugins/azure-pipelines/azure_pipelines.py");
        let installed = std::fs::read_to_string(plugin).unwrap();
        assert!(installed.contains("def main("));
    }

    #[test]
    fn default_config_has_commented_azure_pipelines_widget() {
        let text = default_config_text();
        assert!(text.contains("# args = [\"plugins/azure-pipelines/azure_pipelines.py\"]"));
        assert!(text.contains("# AZDO_PROJECTS = \"meu-projeto\""));
        let config = parse_config(text).unwrap();
        assert!(config
            .widgets
            .iter()
            .all(|widget| widget.id.as_str() != "azure-pipelines"));
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
