use std::fs;
use std::io::Write;
use std::path::Path;

use thiserror::Error;

use crate::infrastructure::config::{config_file_path, parse_config, ConfigError};

const FILE_NEWLINE: &str = "\n";
const TABS_KEY: &str = "tabs";
const TAB_NAME_KEY: &str = "name";

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

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigOverview {
    pub widget_ids: Vec<String>,
    pub declared_tabs: Vec<String>,
    pub tabs: Vec<TabOverview>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabOverview {
    pub name: String,
    pub widget_ids: Vec<String>,
}

pub fn inspect_config(directory: &Path) -> Result<ConfigOverview, AppendError> {
    let path = config_file_path(directory);
    let text = fs::read_to_string(&path).map_err(io_error(&path))?;
    let config = parse_config(&text).map_err(AppendError::CurrentConfigInvalid)?;
    Ok(ConfigOverview {
        widget_ids: config
            .widgets
            .iter()
            .map(|widget| widget.id.as_str().to_string())
            .collect(),
        declared_tabs: declared_tab_names(&text),
        tabs: config
            .tabs
            .iter()
            .map(|tab| TabOverview {
                name: tab.name.clone(),
                widget_ids: tab
                    .widget_ids
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
            })
            .collect(),
    })
}

fn declared_tab_names(text: &str) -> Vec<String> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    table
        .get(TABS_KEY)
        .and_then(toml::Value::as_array)
        .map(|tabs| {
            tabs.iter()
                .filter_map(|tab| tab.get(TAB_NAME_KEY))
                .filter_map(toml::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn append_blocks(directory: &Path, blocks: &[String]) -> Result<(), AppendError> {
    let path = config_file_path(directory);
    let current = fs::read_to_string(&path).map_err(io_error(&path))?;
    parse_config(&current).map_err(AppendError::CurrentConfigInvalid)?;
    let updated = blocks
        .iter()
        .fold(current, |text, block| joined(&text, block));
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
        append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap();
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, format!("{original}\n{BLOCK}"));
        assert_eq!(
            inspect_config(directory.path()).unwrap().widget_ids,
            vec!["a", "novo"]
        );
    }

    #[test]
    fn a_missing_final_newline_is_added_before_the_block() {
        let directory = directory_with("# sem quebra no fim");
        append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap();
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert!(text.starts_with("# sem quebra no fim\n\n[[widgets]]"));
    }

    #[test]
    fn a_duplicate_id_is_refused_and_the_file_is_not_touched() {
        let original = "[[widgets]]\nid = \"novo\"\ncommand = \"x\"\n";
        let directory = directory_with(original);
        let error = append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap_err();
        assert!(matches!(error, AppendError::NewConfigInvalid(_)));
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, original);
    }

    #[test]
    fn a_config_that_is_already_broken_is_not_modified() {
        let directory = directory_with("isto não é toml [");
        let error = append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap_err();
        assert!(matches!(error, AppendError::CurrentConfigInvalid(_)));
    }

    #[test]
    fn a_missing_config_file_names_the_path() {
        let directory = tempfile::tempdir().unwrap();
        let error = append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap_err();
        assert!(error.to_string().contains("mosaic.toml"));
    }

    #[test]
    fn several_blocks_are_validated_together_and_written_in_order() {
        let directory = directory_with("# painel\n");
        let blocks = [
            "[[tabs]]\nname = \"Trabalho\"\n".to_string(),
            "[[widgets]]\nid = \"w\"\ncommand = \"x\"\ntab = \"Trabalho\"\n".to_string(),
        ];
        append_blocks(directory.path(), &blocks).unwrap();
        let overview = inspect_config(directory.path()).unwrap();
        assert_eq!(overview.declared_tabs, vec!["Trabalho"]);
        assert_eq!(overview.tabs[0].widget_ids, vec!["w"]);
    }

    #[test]
    fn a_widget_for_a_tab_that_does_not_exist_is_refused_without_writing() {
        let original = "# painel\n";
        let directory = directory_with(original);
        let block = "[[widgets]]\nid = \"w\"\ncommand = \"x\"\ntab = \"Fantasma\"\n".to_string();
        let error = append_blocks(directory.path(), &[block]).unwrap_err();
        assert!(matches!(error, AppendError::NewConfigInvalid(_)));
        let text = fs::read_to_string(config_file_path(directory.path())).unwrap();
        assert_eq!(text, original);
    }

    #[test]
    fn the_overview_tells_declared_tabs_from_the_implicit_one() {
        let without = directory_with("[[widgets]]\nid = \"a\"\ncommand = \"x\"\n");
        let overview = inspect_config(without.path()).unwrap();
        assert!(overview.declared_tabs.is_empty());
        assert_eq!(overview.tabs.len(), 1);
        assert_eq!(overview.tabs[0].widget_ids, vec!["a"]);
        let with = directory_with("[[tabs]]\nname = \"Work\"\n[[tabs]]\nname = \"Home\"\n");
        assert_eq!(
            inspect_config(with.path()).unwrap().declared_tabs,
            vec!["Work", "Home"]
        );
    }

    #[test]
    fn a_stale_temporary_file_does_not_block_the_append() {
        let directory = directory_with("");
        let temporary = directory.path().join("mosaic.toml.tmp");
        fs::write(&temporary, "sobra de uma execução interrompida").unwrap();
        append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap();
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
        append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap();
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
        append_blocks(directory.path(), &[BLOCK.to_string()]).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
