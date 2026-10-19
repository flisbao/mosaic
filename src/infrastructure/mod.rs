pub mod config;
pub mod default_files;
pub mod llm_cli;
pub mod notifiers;
pub mod plugin_protocol;
pub mod plugin_runner;
pub mod private_directory;
pub mod process;
pub mod protocol_blocks;
pub mod protocol_report;
pub mod recommendation_file;
pub mod refresh_signal;
pub mod shutdown;
pub mod system_clock;
pub mod terminal_capabilities;

#[cfg(all(test, unix))]
pub(crate) mod test_support {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    const PRIVATE_TEMPORARY_DIRECTORY_MODE: u32 = 0o700;

    pub fn private_tempdir() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            directory.path(),
            Permissions::from_mode(PRIVATE_TEMPORARY_DIRECTORY_MODE),
        )
        .unwrap();
        directory
    }
}
