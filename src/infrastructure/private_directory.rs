use std::fs::{self, DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

use thiserror::Error;

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const GROUP_AND_OTHERS_MASK: u32 = 0o077;
const PERMISSION_BITS_MASK: u32 = 0o777;
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

pub fn prepare_private_directory(path: &Path) -> Result<(), PrivateDirectoryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| PrivateDirectoryError::new(path, error.to_string()))?;
    }
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
    if let Ok(current) = fs::metadata(CURRENT_PROCESS_DIRECTORY) {
        if metadata.uid() != current.uid() {
            return Err(PrivateDirectoryError::new(path, "pertence a outro usuário"));
        }
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
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("a/b/llm");
        prepare_private_directory(&path).unwrap();
        assert!(path.is_dir());
        assert_eq!(mode_of(&path), 0o700);
    }

    #[test]
    fn accepts_an_existing_private_directory() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("llm");
        prepare_private_directory(&path).unwrap();
        prepare_private_directory(&path).unwrap();
    }

    #[test]
    fn tightens_loose_permissions_of_an_owned_directory() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("llm");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        prepare_private_directory(&path).unwrap();
        assert_eq!(mode_of(&path), 0o700);
    }

    #[test]
    fn rejects_a_symlink_even_when_it_points_to_a_directory() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("alvo");
        fs::create_dir(&target).unwrap();
        let link = base.path().join("llm");
        symlink(&target, &link).unwrap();
        assert!(prepare_private_directory(&link).is_err());
    }

    #[test]
    fn rejects_a_regular_file() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("llm");
        fs::write(&path, "x").unwrap();
        assert!(prepare_private_directory(&path).is_err());
    }
}
