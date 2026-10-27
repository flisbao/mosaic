use std::fs;
use std::io::Write;
use std::path::Path;

use thiserror::Error;

use crate::infrastructure::config::{config_file_path, parse_config, ConfigError};

const FILE_NEWLINE: &str = "\n";

#[derive(Debug, Error, PartialEq)]
pub enum AppendError {
    #[error("{path}: {reason}")]
    Io { path: String, reason: String },
    #[error("o mosaic.toml atual já tem erro: {0}")]
    CurrentConfigInvalid(ConfigError),
    #[error("o widget novo deixaria o mosaic.toml inválido (nada foi gravado): {0}")]
    NewConfigInvalid(ConfigError),
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> AppendError + '_ {
    move |error| AppendError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    }
}

pub fn existing_widget_ids(directory: &Path) -> Result<Vec<String>, AppendError> {
    let path = config_file_path(directory);
    let text = fs::read_to_string(&path).map_err(io_error(&path))?;
    let config = parse_config(&text).map_err(AppendError::CurrentConfigInvalid)?;
    Ok(config
        .widgets
        .iter()
        .map(|widget| widget.id.as_str().to_string())
        .collect())
}

pub fn append_widget(directory: &Path, block: &str) -> Result<(), AppendError> {
    let path = config_file_path(directory);
    let current = fs::read_to_string(&path).map_err(io_error(&path))?;
    parse_config(&current).map_err(AppendError::CurrentConfigInvalid)?;
    let updated = joined(&current, block);
    parse_config(&updated).map_err(AppendError::NewConfigInvalid)?;
    replace_file(&path, &updated)
}

fn joined(current: &str, block: &str) -> String {
    let mut updated = current.to_string();
    if !updated.is_empty() && !updated.ends_with(FILE_NEWLINE) {
        updated.push_str(FILE_NEWLINE);
    }
    if !updated.is_empty() {
        updated.push_str(FILE_NEWLINE);
    }
    updated.push_str(block);
    updated
}

fn replace_file(path: &Path, content: &str) -> Result<(), AppendError> {
    let permissions = fs::metadata(path).map_err(io_error(path))?.permissions();
    let temporary = path.with_extension("toml.tmp");
    let _ = fs::remove_file(&temporary);
    let mut file = create_temporary(&temporary, &permissions).map_err(io_error(&temporary))?;
    file.write_all(content.as_bytes())
        .map_err(io_error(&temporary))?;
    fs::set_permissions(&temporary, permissions).map_err(io_error(&temporary))?;
    fs::rename(&temporary, path).map_err(io_error(path))
}

fn create_temporary(path: &Path, permissions: &fs::Permissions) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    restrict_creation_mode(&mut options, permissions);
    options.open(path)
}

#[cfg(unix)]
fn restrict_creation_mode(options: &mut fs::OpenOptions, permissions: &fs::Permissions) {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    options.mode(permissions.mode());
}

#[cfg(not(unix))]
fn restrict_creation_mode(_options: &mut fs::OpenOptions, _permissions: &fs::Permissions) {}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "[[widgets]]\nid = \"novo\"\ncommand = \"python3\"\n";

    fn directory_with(config: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::write(config_file_path(directory.path()), config).unwrap();
        directory
    }

    #[test]
    fn the_block_is_appended_after_the_existing_text_keeping_it_intact() {
        let original = "# meu painel\n[[widgets]]\nid = \"a\"\ncommand = \"x\"\n";
        let directory = directory_with(original);
        append_widget(directory.path(), BLOCK).unwrap();
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, format!("{original}\n{BLOCK}"));
        assert_eq!(
            existing_widget_ids(directory.path()).unwrap(),
            vec!["a", "novo"]
        );
    }

    #[test]
    fn a_missing_final_newline_is_added_before_the_block() {
        let directory = directory_with("# sem quebra no fim");
        append_widget(directory.path(), BLOCK).unwrap();
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert!(text.starts_with("# sem quebra no fim\n\n[[widgets]]"));
    }

    #[test]
    fn a_duplicate_id_is_refused_and_the_file_is_not_touched() {
        let original = "[[widgets]]\nid = \"novo\"\ncommand = \"x\"\n";
        let directory = directory_with(original);
        let error = append_widget(directory.path(), BLOCK).unwrap_err();
        assert!(matches!(error, AppendError::NewConfigInvalid(_)));
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, original);
    }

    #[test]
    fn a_config_that_is_already_broken_is_not_modified() {
        let directory = directory_with("isto não é toml [");
        let error = append_widget(directory.path(), BLOCK).unwrap_err();
        assert!(matches!(error, AppendError::CurrentConfigInvalid(_)));
    }

    #[test]
    fn a_missing_config_file_names_the_path() {
        let directory = tempfile::tempdir().unwrap();
        let error = append_widget(directory.path(), BLOCK).unwrap_err();
        assert!(error.to_string().contains("mosaic.toml"));
    }

    #[test]
    fn a_stale_temporary_file_does_not_block_the_append() {
        let directory = directory_with("");
        let temporary = directory.path().join("mosaic.toml.tmp");
        fs::write(&temporary, "sobra de uma execução interrompida").unwrap();
        append_widget(directory.path(), BLOCK).unwrap();
        assert!(!temporary.exists());
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, BLOCK);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_at_the_temporary_path_is_never_followed() {
        let directory = directory_with("");
        let victim = directory.path().join("alvo-do-link");
        fs::write(&victim, "intacto").unwrap();
        std::os::unix::fs::symlink(&victim, directory.path().join("mosaic.toml.tmp")).unwrap();
        append_widget(directory.path(), BLOCK).unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "intacto");
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, BLOCK);
    }

    #[cfg(unix)]
    #[test]
    fn the_temporary_file_is_born_private_when_the_config_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = directory_with("");
        let permissions = fs::Permissions::from_mode(0o600);
        fs::set_permissions(config_file_path(directory.path()), permissions.clone()).unwrap();
        let temporary = directory.path().join("mosaic.toml.tmp");
        let file = create_temporary(&temporary, &permissions).unwrap();
        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = directory_with("");
        let path = config_file_path(directory.path());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        append_widget(directory.path(), BLOCK).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
