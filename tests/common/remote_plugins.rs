#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

pub fn write_package(root: &Path, name: &str, version: &str, minimum: &str) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("setup.toml"), format!(
        "plugin = \"{name}\"\nversion = \"{version}\"\nmin_mosaic_version = \"{minimum}\"\nscript = \"plugins/{name}/{name}.py\"\ndefault_id = \"{name}\"\ninterval_seconds = 60\ntimeout_seconds = 5\n"
    )).unwrap();
    fs::write(root.join("SKILL.md"), "---\nname: fixture\n---\n").unwrap();
    fs::write(root.join(format!("{name}.py")), "print('fixture')\n").unwrap();
}

#[cfg(unix)]
pub fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let dest = target.join(entry.file_name());
        let kind = entry.file_type().unwrap();
        if kind.is_symlink() {
            symlink(fs::read_link(entry.path()).unwrap(), dest).unwrap();
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

#[cfg(unix)]
pub fn write_fake_git(bin_directory: &Path, repository: &Path) {
    fs::create_dir_all(bin_directory).unwrap();
    let quoted = repository.display().to_string().replace('\'', "'\"'\"'");
    let program = bin_directory.join("git");
    fs::write(&program, format!("#!/bin/sh\nfor argument do destination=$argument; done\n/bin/mkdir -p \"$destination\"\n/bin/cp -RP '{quoted}/.' \"$destination/\"\n")).unwrap();
    fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
}
