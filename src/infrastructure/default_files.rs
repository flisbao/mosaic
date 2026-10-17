use std::fs;
use std::path::Path;

use crate::infrastructure::config::{config_file_path, ConfigError};

const PLUGINS_DIRECTORY_NAME: &str = "plugins";

const DEFAULT_CONFIG: &str = r#"[[widgets]]
id = "kanban"
command = "python3"
args = ["plugins/kanban.py"]
interval_seconds = 15
tab = true

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
"#;

const BUNDLED_PLUGINS: [(&str, &str); 3] = [
    ("kanban.py", include_str!("../../plugins/kanban.py")),
    ("example.py", include_str!("../../plugins/example.py")),
    ("example.js", include_str!("../../plugins/example.js")),
];

pub fn default_config_text() -> &'static str {
    DEFAULT_CONFIG
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
    fn install_never_overwrites_existing_config() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(config_file_path(directory.path()), "# meu arquivo").unwrap();
        assert!(!install_defaults_if_missing(directory.path()).unwrap());
        let content = std::fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(content, "# meu arquivo");
        assert!(!directory.path().join("plugins").exists());
    }
}
