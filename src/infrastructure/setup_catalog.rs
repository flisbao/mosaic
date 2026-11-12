use std::fs;
use std::path::PathBuf;

use crate::application::ports::{
    ConfigWriteError, SetupCatalog, SetupCatalogError, SetupCatalogSnapshot, WidgetConfigWriter,
};
use crate::domain::plugin_setup::InstalledPlugin;
use crate::infrastructure::config::PLUGINS_DIRECTORY_NAME;
use crate::infrastructure::config_appender::{append_blocks, inspect_config};
use crate::infrastructure::setup_manifest::{read_manifest_with_version, MANIFEST_FILE_NAME};

pub struct FileSetupCatalog {
    config_directory: PathBuf,
}

impl FileSetupCatalog {
    pub fn new(config_directory: PathBuf) -> Self {
        Self { config_directory }
    }

    fn installed_plugins(&self) -> (Vec<InstalledPlugin>, Vec<String>) {
        let mut plugins = Vec::new();
        let mut problems = Vec::new();
        let plugins_directory = self.config_directory.join(PLUGINS_DIRECTORY_NAME);
        let Ok(entries) = fs::read_dir(&plugins_directory) else {
            return (plugins, problems);
        };
        for entry in entries.flatten() {
            let manifest_path = entry.path().join(MANIFEST_FILE_NAME);
            if !manifest_path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            match read_manifest_with_version(&manifest_path) {
                Ok((manifest, version)) => plugins.push(InstalledPlugin {
                    name,
                    version: version.map(|version| version.to_string()),
                    manifest,
                }),
                Err(error) => problems.push(error.to_string()),
            }
        }
        plugins.sort_by(|left, right| left.name.cmp(&right.name));
        problems.sort();
        (plugins, problems)
    }
}

impl SetupCatalog for FileSetupCatalog {
    fn load(&self) -> Result<SetupCatalogSnapshot, SetupCatalogError> {
        let overview = inspect_config(&self.config_directory)
            .map_err(|error| SetupCatalogError(error.to_string()))?;
        let (plugins, problems) = self.installed_plugins();
        Ok(SetupCatalogSnapshot {
            plugins,
            problems,
            widget_ids: overview.widget_ids,
            declared_tabs: overview.declared_tabs,
        })
    }
}

pub struct FileWidgetConfigWriter {
    config_directory: PathBuf,
}

impl FileWidgetConfigWriter {
    pub fn new(config_directory: PathBuf) -> Self {
        Self { config_directory }
    }
}

impl WidgetConfigWriter for FileWidgetConfigWriter {
    fn append(&self, blocks: &[String]) -> Result<(), ConfigWriteError> {
        append_blocks(&self.config_directory, blocks)
            .map_err(|error| ConfigWriteError(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
version = "0.0.4"
min_mosaic_version = "0.0.10"
plugin = "demo"
script = "plugins/demo/demo.py"
default_id = "demo"
interval_seconds = 120
timeout_seconds = 30
"#;
    const BARE_MANIFEST: &str = r#"
plugin = "bare"
script = "plugins/bare/bare.py"
default_id = "bare"
interval_seconds = 60
timeout_seconds = 10
"#;
    const CONFIG: &str = "[[widgets]]\nid = \"um\"\ncommand = \"x\"\n\n[[widgets]]\nid = \"dois\"\ncommand = \"x\"\n";

    fn directory_with(config: Option<&str>) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        if let Some(config) = config {
            fs::write(directory.path().join("mosaic.toml"), config).unwrap();
        }
        directory
    }

    fn install(directory: &tempfile::TempDir, name: &str, manifest: &str) {
        let plugin = directory.path().join("plugins").join(name);
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("setup.toml"), manifest).unwrap();
    }

    fn catalog(directory: &tempfile::TempDir) -> FileSetupCatalog {
        FileSetupCatalog::new(directory.path().to_path_buf())
    }

    #[test]
    fn installed_plugins_are_listed_by_name_with_their_version() {
        let directory = directory_with(Some(CONFIG));
        install(&directory, "zeta", BARE_MANIFEST);
        install(&directory, "demo", MANIFEST);
        let snapshot = catalog(&directory).load().unwrap();
        let listed: Vec<(&str, Option<&str>)> = snapshot
            .plugins
            .iter()
            .map(|plugin| (plugin.name.as_str(), plugin.version.as_deref()))
            .collect();
        assert_eq!(listed, vec![("demo", Some("0.0.4")), ("zeta", None)]);
        assert_eq!(snapshot.widget_ids, vec!["um", "dois"]);
        assert!(snapshot.declared_tabs.is_empty());
        assert!(snapshot.problems.is_empty());
    }

    #[test]
    fn folders_without_a_manifest_and_a_missing_plugins_folder_are_not_plugins() {
        let directory = directory_with(Some(CONFIG));
        assert!(catalog(&directory).load().unwrap().plugins.is_empty());
        fs::create_dir_all(directory.path().join("plugins/__pycache__")).unwrap();
        fs::write(directory.path().join("plugins/exemplo.py"), "").unwrap();
        assert!(catalog(&directory).load().unwrap().plugins.is_empty());
    }

    #[test]
    fn an_invalid_manifest_is_reported_without_hiding_the_valid_ones() {
        let directory = directory_with(Some(CONFIG));
        install(&directory, "demo", MANIFEST);
        install(&directory, "quebrado", "isto não é um roteiro");
        let snapshot = catalog(&directory).load().unwrap();
        assert_eq!(snapshot.plugins.len(), 1);
        assert_eq!(snapshot.problems.len(), 1);
        assert!(snapshot.problems[0].contains("quebrado"));
    }

    #[test]
    fn an_invalid_or_missing_config_is_an_error() {
        let missing = directory_with(None);
        assert!(catalog(&missing).load().is_err());
        let invalid = directory_with(Some("[[widgets]]\nid = \"a\"\n"));
        let error = catalog(&invalid).load().unwrap_err();
        assert!(error.0.contains("mosaic.toml"), "{error}");
    }

    #[test]
    fn declared_tabs_come_from_the_config() {
        let config = format!("[[tabs]]\nname = \"Trabalho\"\n\n{CONFIG}");
        let directory = directory_with(Some(&config));
        let snapshot = catalog(&directory).load().unwrap();
        assert_eq!(snapshot.declared_tabs, vec!["Trabalho"]);
        let options = snapshot.setup_options();
        assert_eq!(options.existing_ids, vec!["um", "dois"]);
        assert!(!options.include_advanced);
        assert_eq!(options.tab_context.unwrap().existing_widgets, 2);
    }

    #[test]
    fn appending_keeps_the_file_and_a_rejected_block_changes_nothing() {
        let directory = directory_with(Some(CONFIG));
        let writer = FileWidgetConfigWriter::new(directory.path().to_path_buf());
        let block = "[[widgets]]\nid = \"tres\"\ncommand = \"x\"\n".to_string();
        writer.append(&[block]).unwrap();
        let written = fs::read_to_string(directory.path().join("mosaic.toml")).unwrap();
        assert!(written.starts_with(CONFIG));
        assert!(written.contains("id = \"tres\""));
        let duplicate = "[[widgets]]\nid = \"um\"\ncommand = \"x\"\n".to_string();
        assert!(writer.append(&[duplicate]).is_err());
        assert_eq!(
            fs::read_to_string(directory.path().join("mosaic.toml")).unwrap(),
            written
        );
    }
}
