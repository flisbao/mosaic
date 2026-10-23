use std::path::{Path, PathBuf};

use super::ConfigError;

const CONFIG_FILE_NAME: &str = "mosaic.toml";

pub fn config_directory() -> Result<PathBuf, ConfigError> {
    crate::infrastructure::app_paths::config_directory().ok_or(ConfigError::MissingConfigDirectory)
}

pub fn config_file_path(directory: &Path) -> PathBuf {
    directory.join(CONFIG_FILE_NAME)
}

pub fn read_config_file(path: &Path) -> Result<String, ConfigError> {
    std::fs::read_to_string(path).map_err(ConfigError::io(path))
}
