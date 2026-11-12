pub mod app_paths;
pub mod archive_tools;
pub mod child_environment;
pub mod concurrency_gate;
pub mod config;
pub mod config_appender;
pub mod curl_release_source;
pub mod default_files;
pub mod external_tool;
pub mod git_plugin_fetcher;
pub mod installation_probe;
pub mod llm_cli;
pub mod notifiers;
pub mod plugin_protocol;
pub mod plugin_runner;
pub mod plugins_installer;
pub mod priority_prompt;
pub mod private_directory;
pub mod private_file;
pub mod process;
pub mod protocol_blocks;
pub mod protocol_report;
pub mod recommendation_file;
pub mod refresh_signal;
pub mod remote_plugins;
pub mod setup_catalog;
pub mod setup_manifest;
pub mod shutdown;
pub mod skills_installer;
pub mod stream_prompter;
pub mod system_clock;
pub mod update_check_file;
pub mod widget_runtime;

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
