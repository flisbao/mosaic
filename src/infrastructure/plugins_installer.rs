use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

const RUNTIME_EXTENSIONS: [&str; 2] = ["py", "js"];
const SHIPPED_DOCUMENT_NAMES: [&str; 3] = ["README.md", "SKILL.md", "setup.toml"];
const EXCLUDED_FOLDER_NAMES: [&str; 1] = ["tests"];
const TEMPORARY_FOLDER_MARKER: &str = ".tmp-";
const BACKUP_FOLDER_MARKER: &str = ".old-";

#[derive(Debug, Error, PartialEq)]
pub enum PluginsError {
    #[error("pasta de plugins não encontrada: {0}")]
    SourceMissing(String),
    #[error("plugin não encontrado na origem: {0}")]
    UnknownPlugin(String),
    #[error("nome de plugin inválido: {0}")]
    InvalidName(String),
    #[error("{path}: {reason}")]
    Io { path: String, reason: String },
}

impl PluginsError {
    fn io(path: &Path) -> impl FnOnce(io::Error) -> PluginsError + '_ {
        move |error| PluginsError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    Keep,
    Replace,
}

#[derive(Debug, PartialEq)]
pub enum PluginOutcome {
    Installed(String),
    Replaced(String),
    AlreadyInstalled(String),
}

pub fn available_plugins(source: &Path) -> Result<Vec<String>, PluginsError> {
    if !source.is_dir() {
        return Err(PluginsError::SourceMissing(source.display().to_string()));
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(source).map_err(PluginsError::io(source))? {
        let entry = entry.map_err(PluginsError::io(source))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_plugin_folder(&entry.path(), &name)? {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub fn install_plugins(
    source: &Path,
    target: &Path,
    names: &[String],
    overwrite: Overwrite,
    mut on_outcome: impl FnMut(PluginOutcome),
) -> Result<(), PluginsError> {
    let selected = select_plugins(source, names)?;
    fs::create_dir_all(target).map_err(PluginsError::io(target))?;
    for name in selected {
        on_outcome(install_one(&source.join(&name), target, name, overwrite)?);
    }
    Ok(())
}

fn select_plugins(source: &Path, names: &[String]) -> Result<Vec<String>, PluginsError> {
    let available = available_plugins(source)?;
    if names.is_empty() {
        return Ok(available);
    }
    let mut selected = Vec::new();
    for name in names {
        if !is_plain_name(name) {
            return Err(PluginsError::InvalidName(name.clone()));
        }
        if !available.contains(name) {
            return Err(PluginsError::UnknownPlugin(name.clone()));
        }
        if !selected.contains(name) {
            selected.push(name.clone());
        }
    }
    selected.sort();
    Ok(selected)
}

fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn is_plugin_folder(path: &Path, name: &str) -> Result<bool, PluginsError> {
    let metadata = fs::symlink_metadata(path).map_err(PluginsError::io(path))?;
    let is_candidate =
        metadata.is_dir() && is_plain_name(name) && !EXCLUDED_FOLDER_NAMES.contains(&name);
    if !is_candidate {
        return Ok(false);
    }
    let files = copyable_files(path)?;
    Ok(files.iter().any(|file| has_runtime_extension(file)))
}

fn has_runtime_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| RUNTIME_EXTENSIONS.contains(&extension))
}

fn copyable_files(folder: &Path) -> Result<Vec<PathBuf>, PluginsError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).map_err(PluginsError::io(folder))? {
        let entry = entry.map_err(PluginsError::io(folder))?;
        let path = entry.path();
        let is_regular_file = fs::symlink_metadata(&path)
            .map_err(PluginsError::io(&path))?
            .is_file();
        let is_documentation = SHIPPED_DOCUMENT_NAMES
            .iter()
            .any(|name| entry.file_name() == *name);
        if is_regular_file && (has_runtime_extension(&path) || is_documentation) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn install_one(
    plugin_folder: &Path,
    target: &Path,
    name: String,
    overwrite: Overwrite,
) -> Result<PluginOutcome, PluginsError> {
    let destination = target.join(&name);
    let already_exists = fs::symlink_metadata(&destination).is_ok();
    if already_exists && overwrite == Overwrite::Keep {
        return Ok(PluginOutcome::AlreadyInstalled(name));
    }
    let staging = target.join(format!(
        ".{name}{TEMPORARY_FOLDER_MARKER}{}",
        std::process::id()
    ));
    copy_into_staging(plugin_folder, &staging)?;
    let backup = target.join(format!(
        ".{name}{BACKUP_FOLDER_MARKER}{}",
        std::process::id()
    ));
    if already_exists {
        discard(&backup);
        fs::rename(&destination, &backup)
            .map_err(PluginsError::io(&destination))
            .inspect_err(|_| discard(&staging))?;
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        discard(&staging);
        if already_exists {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(PluginsError::io(&destination)(error));
    }
    if already_exists {
        discard(&backup);
    }
    Ok(if already_exists {
        PluginOutcome::Replaced(name)
    } else {
        PluginOutcome::Installed(name)
    })
}

fn copy_into_staging(plugin_folder: &Path, staging: &Path) -> Result<(), PluginsError> {
    discard(staging);
    fs::create_dir_all(staging).map_err(PluginsError::io(staging))?;
    copy_files(plugin_folder, staging).inspect_err(|_| discard(staging))
}

fn copy_files(plugin_folder: &Path, staging: &Path) -> Result<(), PluginsError> {
    for file in copyable_files(plugin_folder)? {
        let destination = staging.join(file.file_name().unwrap_or_default());
        fs::copy(&file, destination).map_err(PluginsError::io(&file))?;
    }
    Ok(())
}

fn discard(folder: &Path) {
    let _ = fs::remove_dir_all(folder);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn source_with_plugins() -> tempfile::TempDir {
        let source = tempfile::tempdir().unwrap();
        write(&source.path().join("jira/jira.py"), "print('jira')");
        write(&source.path().join("jira/README.md"), "# jira");
        write(&source.path().join("github/github_prs.py"), "print('gh')");
        write(
            &source.path().join("calendar/calendar_widget.py"),
            "print('cal')",
        );
        source
    }

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn install(
        source: &Path,
        target: &Path,
        wanted: &[&str],
        overwrite: Overwrite,
    ) -> Vec<PluginOutcome> {
        let (outcomes, result) = try_install(source, target, wanted, overwrite);
        result.unwrap();
        outcomes
    }

    fn try_install(
        source: &Path,
        target: &Path,
        wanted: &[&str],
        overwrite: Overwrite,
    ) -> (Vec<PluginOutcome>, Result<(), PluginsError>) {
        let mut outcomes = Vec::new();
        let result = install_plugins(source, target, &names(wanted), overwrite, |outcome| {
            outcomes.push(outcome)
        });
        (outcomes, result)
    }

    #[test]
    fn lists_only_folders_with_runtime_files() {
        let source = source_with_plugins();
        write(&source.path().join("tests/test_x.py"), "");
        write(&source.path().join("__pycache__/x.pyc"), "");
        write(&source.path().join("empty/notes.txt"), "");
        write(&source.path().join("loose.py"), "");
        assert_eq!(
            available_plugins(source.path()).unwrap(),
            names(&["calendar", "github", "jira"])
        );
    }

    #[test]
    fn installs_every_plugin_when_no_names_are_given() {
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        let outcomes = install(source.path(), target.path(), &[], Overwrite::Keep);
        assert_eq!(
            outcomes,
            vec![
                PluginOutcome::Installed("calendar".to_string()),
                PluginOutcome::Installed("github".to_string()),
                PluginOutcome::Installed("jira".to_string()),
            ]
        );
        assert!(target.path().join("jira/jira.py").is_file());
    }

    #[test]
    fn installs_only_the_named_plugins() {
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        install(source.path(), target.path(), &["jira"], Overwrite::Keep);
        assert!(target.path().join("jira").is_dir());
        assert!(!target.path().join("github").exists());
    }

    #[test]
    fn copies_only_runtime_files_and_the_readme() {
        let source = source_with_plugins();
        write(&source.path().join("jira/notes.txt"), "x");
        write(&source.path().join("jira/tests/test_jira.py"), "x");
        write(&source.path().join("jira/__pycache__/jira.pyc"), "x");
        let target = tempfile::tempdir().unwrap();
        install(source.path(), target.path(), &["jira"], Overwrite::Keep);
        let mut copied: Vec<String> = fs::read_dir(target.path().join("jira"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        copied.sort();
        assert_eq!(copied, names(&["README.md", "jira.py"]));
    }

    #[test]
    fn rejects_names_that_are_not_plain_folder_names() {
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        for bad in ["", "..", "a/b", "Jira", "jira.py", "../jira"] {
            let (_, result) = try_install(source.path(), target.path(), &[bad], Overwrite::Keep);
            assert_eq!(
                result,
                Err(PluginsError::InvalidName(bad.to_string())),
                "{bad:?}"
            );
        }
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
    }

    #[test]
    fn unknown_plugin_is_reported_and_nothing_is_copied() {
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        let (_, result) = try_install(
            source.path(),
            target.path(),
            &["jira", "nope"],
            Overwrite::Keep,
        );
        assert_eq!(result, Err(PluginsError::UnknownPlugin("nope".to_string())));
        assert!(!target.path().join("jira").exists());
    }

    #[test]
    fn keeps_an_existing_plugin_unless_forced() {
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        write(&target.path().join("jira/jira.py"), "meu ajuste");
        let kept = install(source.path(), target.path(), &["jira"], Overwrite::Keep);
        assert_eq!(
            kept,
            vec![PluginOutcome::AlreadyInstalled("jira".to_string())]
        );
        assert_eq!(
            fs::read_to_string(target.path().join("jira/jira.py")).unwrap(),
            "meu ajuste"
        );
        let replaced = install(source.path(), target.path(), &["jira"], Overwrite::Replace);
        assert_eq!(replaced, vec![PluginOutcome::Replaced("jira".to_string())]);
        assert_eq!(
            fs::read_to_string(target.path().join("jira/jira.py")).unwrap(),
            "print('jira')"
        );
    }

    #[test]
    fn replacing_leaves_no_temporary_folders_behind() {
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        install(source.path(), target.path(), &["jira"], Overwrite::Keep);
        install(source.path(), target.path(), &["jira"], Overwrite::Replace);
        let mut entries: Vec<String> = fs::read_dir(target.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(entries, names(&["jira"]));
    }

    #[test]
    fn a_failed_replacement_keeps_the_previous_plugin() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let source = source_with_plugins();
        let target = tempfile::tempdir().unwrap();
        write(&target.path().join("jira/jira.py"), "meu ajuste");
        fs::set_permissions(target.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let (_, result) = try_install(source.path(), target.path(), &["jira"], Overwrite::Replace);
        fs::set_permissions(target.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(result, Err(PluginsError::Io { .. })));
        assert_eq!(
            fs::read_to_string(target.path().join("jira/jira.py")).unwrap(),
            "meu ajuste"
        );
    }

    #[test]
    fn does_not_follow_symlinks_in_the_source() {
        let source = source_with_plugins();
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("secret.py"), "segredo");
        symlink(
            outside.path().join("secret.py"),
            source.path().join("jira/linked.py"),
        )
        .unwrap();
        symlink(outside.path(), source.path().join("linked-folder")).unwrap();
        let target = tempfile::tempdir().unwrap();
        install(source.path(), target.path(), &[], Overwrite::Keep);
        assert!(!target.path().join("jira/linked.py").exists());
        assert!(!target.path().join("linked-folder").exists());
    }

    #[test]
    fn a_failed_copy_leaves_no_partial_plugin() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let source = source_with_plugins();
        let unreadable = source.path().join("jira/jira.py");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        let target = tempfile::tempdir().unwrap();
        let (_, result) = try_install(source.path(), target.path(), &["jira"], Overwrite::Keep);
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(result, Err(PluginsError::Io { .. })));
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
    }

    #[test]
    fn outcomes_before_a_failure_are_still_reported() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let source = source_with_plugins();
        let unreadable = source.path().join("jira/jira.py");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        let target = tempfile::tempdir().unwrap();
        let (outcomes, result) = try_install(source.path(), target.path(), &[], Overwrite::Keep);
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(result, Err(PluginsError::Io { .. })));
        assert_eq!(
            outcomes,
            vec![
                PluginOutcome::Installed("calendar".to_string()),
                PluginOutcome::Installed("github".to_string()),
            ]
        );
    }

    #[test]
    fn an_unreadable_plugin_folder_is_an_error_not_a_missing_plugin() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let source = source_with_plugins();
        let folder = source.path().join("jira");
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o000)).unwrap();
        let result = available_plugins(source.path());
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(result, Err(PluginsError::Io { .. })));
    }

    #[test]
    fn keeps_the_executable_bit() {
        let source = source_with_plugins();
        let script = source.path().join("jira/jira.py");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let target = tempfile::tempdir().unwrap();
        install(source.path(), target.path(), &["jira"], Overwrite::Keep);
        let mode = fs::metadata(target.path().join("jira/jira.py"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    #[test]
    fn creates_the_target_directory_when_missing() {
        let source = source_with_plugins();
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join(".config/mosaic/plugins");
        install(source.path(), &target, &["jira"], Overwrite::Keep);
        assert!(target.join("jira/jira.py").is_file());
    }

    #[test]
    fn missing_source_is_an_error() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("nao-existe");
        assert_eq!(
            available_plugins(&missing),
            Err(PluginsError::SourceMissing(missing.display().to_string()))
        );
    }
}
