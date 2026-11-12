use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::infrastructure::private_directory::prepare_private_directory;

const PRIVATE_FILE_MODE: u32 = 0o600;
const TEMPORARY_FILE_SUFFIX: &str = ".tmp";
const NO_PARENT_DIRECTORY_MESSAGE: &str = "o caminho do arquivo não tem diretório pai";

pub fn write_private_file_atomically(path: &Path, content: &str) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other(NO_PARENT_DIRECTORY_MESSAGE))?;
    prepare_private_directory(parent).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut temporary_name = path.as_os_str().to_os_string();
    temporary_name.push(TEMPORARY_FILE_SUFFIX);
    let temporary = PathBuf::from(temporary_name);
    let _ = fs::remove_file(&temporary);
    let mut file = open_private_file(&temporary)?;
    let written = file
        .write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            fs::rename(&temporary, path)
        });
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

pub fn read_regular_file(path: &Path, maximum_bytes: u64) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > maximum_bytes {
        return None;
    }
    fs::read_to_string(path).ok()
}

#[cfg(unix)]
fn open_private_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
}

#[cfg(not(unix))]
fn open_private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}
