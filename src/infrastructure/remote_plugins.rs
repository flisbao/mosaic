use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::display_in_portuguese;
use crate::domain::language::{Language, LocalizedMessage};

use super::plugins_installer::{
    install_plugin_directory, is_copyable_file, is_plain_name, Overwrite, PluginOutcome,
    PluginsError,
};
use super::setup_manifest::{parse_package_manifest, PackageManifest, MANIFEST_FILE_NAME};
use crate::application::ports::{PluginFetchError, PluginFetcher};
use crate::domain::plugin_package::{GitHubSource, PluginPackageError, PluginVersion};

#[derive(Debug, Error)]
pub enum RemotePluginError {
    Fetch(#[from] PluginFetchError),
    Package(#[from] PluginPackageError),
    Install(#[from] PluginsError),
    InvalidPackage(String),
    Io { path: String, reason: String },
}

impl LocalizedMessage for RemotePluginError {
    fn message(&self, language: Language) -> String {
        match self {
            Self::Fetch(error) => error.to_string(),
            Self::Package(error) => error.message(language),
            Self::Install(error) => error.message(language),
            Self::InvalidPackage(detail) => language.pick(
                format!("invalid plugin package: {detail}"),
                format!("pacote de plugin inválido: {detail}"),
            ),
            Self::Io { path, reason } => format!("{path}: {reason}"),
        }
    }
}

display_in_portuguese!(RemotePluginError);

impl RemotePluginError {
    fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Self + '_ {
        move |error| Self::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        }
    }
}

pub struct PreparedRemotePlugin {
    _temporary: tempfile::TempDir,
    pub source: GitHubSource,
    pub directory: PathBuf,
    pub manifest: PackageManifest,
}

pub fn prepare_remote_plugin(
    fetcher: &impl PluginFetcher,
    source: &GitHubSource,
    current: &PluginVersion,
) -> Result<PreparedRemotePlugin, RemotePluginError> {
    let source = GitHubSource::parse(&source.reference())?;
    let temporary =
        tempfile::tempdir().map_err(RemotePluginError::io(std::env::temp_dir().as_path()))?;
    let mut directory = temporary.path().join("repository");
    fetcher.fetch(&source, &directory)?;
    require_directory(&directory)?;
    for component in &source.subdirectory {
        directory.push(component);
        require_directory(&directory)?;
    }
    for entry in fs::read_dir(&directory).map_err(RemotePluginError::io(&directory))? {
        let entry = entry.map_err(RemotePluginError::io(&directory))?;
        let path = entry.path();
        if is_copyable_file(&path) {
            require_regular_file(&path)?;
        }
    }
    let manifest_path = directory.join(MANIFEST_FILE_NAME);
    require_regular_file(&manifest_path)?;
    require_regular_file(&directory.join("SKILL.md"))?;
    let text = fs::read_to_string(&manifest_path).map_err(RemotePluginError::io(&manifest_path))?;
    let manifest = parse_package_manifest(&text).map_err(RemotePluginError::InvalidPackage)?;
    manifest.metadata.ensure_compatible(current)?;
    let name = &manifest.setup.plugin;
    if !is_plain_name(name) {
        return Err(PluginsError::InvalidName(name.clone()).into());
    }
    let prefix = format!("plugins/{name}/");
    let filename = manifest
        .setup
        .script
        .strip_prefix(&prefix)
        .filter(|file| {
            !file.is_empty() && !file.contains(['/', '\\']) && !file.chars().any(char::is_control)
        })
        .filter(|file| {
            Path::new(file)
                .extension()
                .is_some_and(|extension| extension == "py" || extension == "js")
        })
        .ok_or_else(|| {
            RemotePluginError::InvalidPackage(format!(
                "script deve ser plugins/{name}/<arquivo>.py ou .js"
            ))
        })?;
    require_regular_file(&directory.join(filename))?;
    Ok(PreparedRemotePlugin {
        _temporary: temporary,
        source,
        directory,
        manifest,
    })
}

fn require_directory(path: &Path) -> Result<(), RemotePluginError> {
    let metadata = fs::symlink_metadata(path).map_err(RemotePluginError::io(path))?;
    if !metadata.is_dir() {
        return Err(RemotePluginError::InvalidPackage(format!(
            "{} não é uma pasta regular",
            path.display()
        )));
    }
    Ok(())
}

fn require_regular_file(path: &Path) -> Result<(), RemotePluginError> {
    let metadata = fs::symlink_metadata(path).map_err(RemotePluginError::io(path))?;
    if !metadata.is_file() {
        return Err(RemotePluginError::InvalidPackage(format!(
            "{} não é um arquivo regular",
            path.display()
        )));
    }
    Ok(())
}

pub fn install_prepared_plugin(
    prepared: &PreparedRemotePlugin,
    target: &Path,
    overwrite: Overwrite,
) -> Result<PluginOutcome, RemotePluginError> {
    install_plugin_directory(
        &prepared.directory,
        target,
        &prepared.manifest.setup.plugin,
        overwrite,
    )
    .map_err(Into::into)
}

#[cfg(all(test, unix))]
#[path = "../../tests/common/remote_plugins.rs"]
mod fixtures;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::application::ports::{PluginFetchError, PluginFetcher};
    use std::cell::RefCell;
    use std::os::unix::fs::symlink;

    struct LocalFetcher(std::path::PathBuf);
    impl PluginFetcher for LocalFetcher {
        fn fetch(&self, _: &GitHubSource, destination: &Path) -> Result<(), PluginFetchError> {
            fixtures::copy_tree(&self.0, destination);
            Ok(())
        }
    }

    fn prepare(root: &Path, reference: &str) -> Result<PreparedRemotePlugin, RemotePluginError> {
        prepare_remote_plugin(
            &LocalFetcher(root.to_path_buf()),
            &GitHubSource::parse(reference).unwrap(),
            &PluginVersion::parse("0.0.10").unwrap(),
        )
    }

    #[test]
    fn prepares_root_and_nested_packages_and_copies_only_selected_files() {
        let root = tempfile::tempdir().unwrap();
        fixtures::write_package(root.path(), "demo", "0.0.4", "0.0.10");
        assert!(prepare(root.path(), "owner/repo").is_ok());
        let selected = root.path().join("addons/review");
        fixtures::write_package(&selected, "github", "0.0.4", "0.0.10");
        fs::write(selected.join("notes.txt"), "ignored").unwrap();
        fs::create_dir(selected.join("tests")).unwrap();
        fs::write(selected.join("tests/test.py"), "ignored").unwrap();
        let candidate = prepare(root.path(), "owner/repo/addons/review").unwrap();
        let clone = candidate
            .directory
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let target = tempfile::tempdir().unwrap();
        assert_eq!(
            install_prepared_plugin(&candidate, target.path(), Overwrite::Keep).unwrap(),
            PluginOutcome::Installed("github".into())
        );
        assert!(target.path().join("github/github.py").is_file());
        assert!(!target.path().join("github/notes.txt").exists());
        assert!(!target.path().join("github/tests").exists());
        assert!(!target.path().join("demo").exists());
        drop(candidate);
        assert!(!clone.exists());
    }

    #[test]
    fn rejects_missing_and_incompatible_metadata_and_invalid_script() {
        let root = tempfile::tempdir().unwrap();
        fixtures::write_package(root.path(), "demo", "0.0.4", "99.0.0");
        assert!(prepare(root.path(), "owner/repo").is_err());
        fixtures::write_package(root.path(), "demo", "0.0.4", "0.0.0");
        let manifest_path = root.path().join("setup.toml");
        let text = fs::read_to_string(&manifest_path).unwrap();
        for bad in [
            "../demo.py",
            "plugins/other/demo.py",
            "plugins/demo/missing.py",
            "plugins/demo/sub/demo.py",
        ] {
            fs::write(&manifest_path, text.replace("plugins/demo/demo.py", bad)).unwrap();
            assert!(prepare(root.path(), "owner/repo").is_err(), "{bad}");
        }
        fs::write(
            &manifest_path,
            text.replace("version = \"0.0.4\"\n", "")
                .replace("min_mosaic_version = \"0.0.0\"\n", ""),
        )
        .unwrap();
        assert!(prepare(root.path(), "owner/repo").is_err());
    }

    #[test]
    fn rejects_symlink_components_and_copyable_files() {
        let root = tempfile::tempdir().unwrap();
        fixtures::write_package(&root.path().join("real"), "demo", "0.0.4", "0.0.0");
        symlink("real", root.path().join("linked")).unwrap();
        assert!(prepare(root.path(), "owner/repo/linked").is_err());
        for file in ["setup.toml", "SKILL.md", "demo.py", "other.py", "README.md"] {
            let folder = root.path().join("real");
            let original = folder.join(file);
            let saved = folder.join("saved");
            if original.exists() {
                fs::rename(&original, &saved).unwrap();
            }
            symlink("saved", &original).unwrap();
            assert!(prepare(root.path(), "owner/repo/real").is_err(), "{file}");
            fs::remove_file(&original).unwrap();
            if saved.exists() {
                fs::rename(saved, original).unwrap();
            }
        }
    }

    #[test]
    fn fetch_failure_cleans_temporary_directory() {
        struct FailingFetcher(RefCell<Option<std::path::PathBuf>>);
        impl PluginFetcher for FailingFetcher {
            fn fetch(&self, _: &GitHubSource, destination: &Path) -> Result<(), PluginFetchError> {
                self.0
                    .replace(Some(destination.parent().unwrap().to_path_buf()));
                Err(PluginFetchError::Failed("fixture".into()))
            }
        }
        let fetcher = FailingFetcher(RefCell::new(None));
        assert!(prepare_remote_plugin(
            &fetcher,
            &GitHubSource::parse("owner/repo").unwrap(),
            &PluginVersion::parse("0.0.10").unwrap()
        )
        .is_err());
        assert!(!fetcher.0.borrow().as_ref().unwrap().exists());
    }
}
