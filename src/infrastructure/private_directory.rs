use std::fs;
#[cfg(unix)]
use std::fs::{DirBuilder, Permissions};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

use thiserror::Error;

#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const GROUP_AND_OTHERS_MASK: u32 = 0o077;
#[cfg(unix)]
const PERMISSION_BITS_MASK: u32 = 0o777;
#[cfg(unix)]
const OTHERS_AND_GROUP_WRITE_MASK: u32 = 0o022;
#[cfg(unix)]
const STICKY_BIT: u32 = 0o1000;
#[cfg(unix)]
const OTHERS_WRITE_MASK: u32 = 0o002;
#[cfg(unix)]
const ROOT_USER_ID: u32 = 0;
#[cfg(unix)]
const CURRENT_PROCESS_DIRECTORY: &str = "/proc/self";

#[derive(Debug, Error)]
#[error("diretório privado inseguro '{path}': {reason}")]
pub struct PrivateDirectoryError {
    path: String,
    reason: String,
}

impl PrivateDirectoryError {
    fn new(path: &Path, reason: impl Into<String>) -> Self {
        Self {
            path: path.display().to_string(),
            reason: reason.into(),
        }
    }
}

#[cfg(unix)]
fn user_id_from_sources(sources: &[&Path]) -> Option<u32> {
    sources
        .iter()
        .find_map(|source| fs::metadata(source).ok())
        .map(|metadata| metadata.uid())
}

#[cfg(unix)]
fn current_user_id() -> Option<u32> {
    let home = dirs::home_dir();
    let mut sources: Vec<&Path> = vec![Path::new(CURRENT_PROCESS_DIRECTORY)];
    if let Some(home) = home.as_deref() {
        sources.push(home);
    }
    user_id_from_sources(&sources)
}

#[cfg(unix)]
fn verify_ancestors(
    path: &Path,
    user_id: u32,
    stop_at: Option<&Path>,
) -> Result<(), PrivateDirectoryError> {
    for ancestor in path.ancestors().skip(1) {
        if ancestor.as_os_str().is_empty() || Some(ancestor) == stop_at {
            break;
        }
        let metadata = fs::metadata(ancestor)
            .map_err(|error| PrivateDirectoryError::new(ancestor, error.to_string()))?;
        if metadata.uid() != user_id && metadata.uid() != ROOT_USER_ID {
            return Err(PrivateDirectoryError::new(
                ancestor,
                "diretório ancestral pertence a outro usuário",
            ));
        }
        let mode = metadata.permissions().mode();
        if mode & OTHERS_AND_GROUP_WRITE_MASK != 0 && mode & STICKY_BIT == 0 {
            return Err(PrivateDirectoryError::new(
                ancestor,
                "diretório ancestral gravável por outros usuários",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn prepare_private_directory(path: &Path) -> Result<(), PrivateDirectoryError> {
    if let Some(parent) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(PRIVATE_DIRECTORY_MODE)
            .create(parent)
            .map_err(|error| PrivateDirectoryError::new(path, error.to_string()))?;
    }
    let user_id = current_user_id().ok_or_else(|| {
        PrivateDirectoryError::new(path, "não foi possível determinar o usuário atual")
    })?;
    verify_ancestors(path, user_id, dirs::home_dir().as_deref())?;
    match DirBuilder::new().mode(PRIVATE_DIRECTORY_MODE).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(PrivateDirectoryError::new(path, error.to_string())),
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| PrivateDirectoryError::new(path, error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PrivateDirectoryError::new(
            path,
            "não é um diretório comum (link simbólico ou arquivo)",
        ));
    }
    if metadata.uid() != user_id {
        return Err(PrivateDirectoryError::new(path, "pertence a outro usuário"));
    }
    if metadata.permissions().mode() & GROUP_AND_OTHERS_MASK != 0 {
        fs::set_permissions(path, Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|error| PrivateDirectoryError::new(path, error.to_string()))?;
        let tightened = fs::metadata(path)
            .map_err(|error| PrivateDirectoryError::new(path, error.to_string()))?;
        if tightened.permissions().mode() & PERMISSION_BITS_MASK & GROUP_AND_OTHERS_MASK != 0 {
            return Err(PrivateDirectoryError::new(
                path,
                "permissões abertas para outros usuários",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn verify_trusted_path(path: &Path) -> Result<(), PrivateDirectoryError> {
    let metadata =
        fs::metadata(path).map_err(|error| PrivateDirectoryError::new(path, error.to_string()))?;
    let user_id = current_user_id().ok_or_else(|| {
        PrivateDirectoryError::new(path, "não foi possível determinar o usuário atual")
    })?;
    if metadata.uid() != user_id && metadata.uid() != ROOT_USER_ID {
        return Err(PrivateDirectoryError::new(path, "pertence a outro usuário"));
    }
    if metadata.permissions().mode() & OTHERS_WRITE_MASK != 0 {
        return Err(PrivateDirectoryError::new(
            path,
            "gravável por qualquer usuário",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn verify_trusted_path(_path: &Path) -> Result<(), PrivateDirectoryError> {
    Ok(())
}

#[cfg(not(unix))]
pub fn prepare_private_directory(path: &Path) -> Result<(), PrivateDirectoryError> {
    Err(PrivateDirectoryError::new(
        path,
        "diretório privado só é suportado em sistemas unix",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    use super::*;

    fn mode_of(path: &std::path::Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn creates_nested_directory_with_private_mode() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let path = base.path().join("a/b/llm");
        prepare_private_directory(&path).unwrap();
        assert!(path.is_dir());
        assert_eq!(mode_of(&path), 0o700);
    }

    #[test]
    fn accepts_an_existing_private_directory() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let path = base.path().join("llm");
        prepare_private_directory(&path).unwrap();
        prepare_private_directory(&path).unwrap();
    }

    #[test]
    fn tightens_loose_permissions_of_an_owned_directory() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let path = base.path().join("llm");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        prepare_private_directory(&path).unwrap();
        assert_eq!(mode_of(&path), 0o700);
    }

    #[test]
    fn rejects_a_symlink_even_when_it_points_to_a_directory() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let target = base.path().join("alvo");
        fs::create_dir(&target).unwrap();
        let link = base.path().join("llm");
        symlink(&target, &link).unwrap();
        assert!(prepare_private_directory(&link).is_err());
    }

    #[test]
    fn rejects_a_regular_file() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let path = base.path().join("llm");
        fs::write(&path, "x").unwrap();
        assert!(prepare_private_directory(&path).is_err());
    }

    #[test]
    fn rejects_a_parent_writable_by_others_without_sticky_bit() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let parent = base.path().join("aberto");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(prepare_private_directory(&parent.join("llm")).is_err());
    }

    #[test]
    fn accepts_a_parent_writable_by_others_with_sticky_bit() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let parent = base.path().join("compartilhado");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o1777)).unwrap();
        prepare_private_directory(&parent.join("llm")).unwrap();
    }

    #[test]
    fn user_id_is_unknown_when_every_source_is_missing() {
        let missing = Path::new("/caminho/que/nao/existe");
        assert_eq!(user_id_from_sources(&[missing]), None);
        assert_eq!(user_id_from_sources(&[]), None);
    }

    #[test]
    fn user_id_comes_from_the_first_existing_source() {
        let base = crate::infrastructure::test_support::private_tempdir();
        let missing = Path::new("/caminho/que/nao/existe");
        let expected = fs::metadata(base.path()).unwrap().uid();
        assert_eq!(
            user_id_from_sources(&[missing, base.path()]),
            Some(expected)
        );
    }
}
