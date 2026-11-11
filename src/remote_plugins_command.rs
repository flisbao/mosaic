use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use thiserror::Error;

use crate::confirmation::confirm_default_no;
use mosaic::application::ports::{PluginFetcher, PromptError, Prompter};
use mosaic::domain::plugin_package::{GitHubSource, PluginPackageError, PluginVersion};
use mosaic::infrastructure::plugins_installer::{Overwrite, PluginOutcome, PluginsError};
use mosaic::infrastructure::remote_plugins::{
    install_prepared_plugin, prepare_remote_plugin, RemotePluginError,
};

const INSTALL_QUESTION: &str = "Plugins são código de terceiros e terão acesso ao seu usuário quando executados. Instalar? [s/N]: ";
const CANCELLED: &str = "Instalação cancelada; nenhum plugin foi alterado.";
const MAXIMUM_PACKAGE_DIAGNOSTIC_CHARACTERS: usize = 500;

pub struct RemoteInstallOptions {
    pub target: PathBuf,
    pub overwrite: Overwrite,
    pub assume_yes: bool,
}

#[derive(Debug, Error)]
pub enum RemoteCommandError {
    #[error(transparent)]
    Prepare(#[from] RemotePluginError),
    #[error(transparent)]
    Prompt(#[from] PromptError),
    #[error(transparent)]
    Version(#[from] PluginPackageError),
    #[error("não consegui escrever na saída: {reason}")]
    Output {
        reason: String,
        completed: Vec<PluginOutcome>,
    },
    #[error("{source}")]
    Installation {
        completed: Vec<PluginOutcome>,
        source: RemotePluginError,
    },
}

impl RemoteCommandError {
    pub fn diagnostic(&self) -> String {
        let message: String = self
            .to_string()
            .chars()
            .map(|character| {
                if character.is_control() {
                    '?'
                } else {
                    character
                }
            })
            .collect();
        match self {
            Self::Prepare(RemotePluginError::InvalidPackage(_))
            | Self::Prepare(RemotePluginError::Install(PluginsError::InvalidName(_))) => message
                .chars()
                .take(MAXIMUM_PACKAGE_DIAGNOSTIC_CHARACTERS)
                .collect(),
            _ => message,
        }
    }

    pub fn completed(&self) -> &[PluginOutcome] {
        match self {
            Self::Output { completed, .. } | Self::Installation { completed, .. } => completed,
            _ => &[],
        }
    }
}

pub fn execute_remote_install(
    sources: &[GitHubSource],
    options: &RemoteInstallOptions,
    fetcher: &impl PluginFetcher,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
) -> Result<Vec<PluginOutcome>, RemoteCommandError> {
    let current = PluginVersion::parse(env!("CARGO_PKG_VERSION"))?;
    let candidates = sources
        .iter()
        .map(|source| prepare_remote_plugin(fetcher, source, &current))
        .collect::<Result<Vec<_>, _>>()?;
    let mut names = HashSet::new();
    for candidate in &candidates {
        let name = &candidate.manifest.setup.plugin;
        if !names.insert(name) {
            return Err(RemotePluginError::InvalidPackage(format!(
                "mais de uma origem declara o plugin '{name}'"
            ))
            .into());
        }
    }
    for candidate in &candidates {
        let name = &candidate.manifest.setup.plugin;
        writeln!(
            output,
            "origem: {}; plugin: {name}; versão: {}; Mosaic mínimo: {}; destino: {}",
            candidate.source.reference(),
            candidate.manifest.metadata.version,
            candidate.manifest.metadata.min_mosaic_version,
            options.target.join(name).display()
        )
        .map_err(|error| RemoteCommandError::Output {
            reason: error.to_string(),
            completed: Vec::new(),
        })?;
    }
    if !options.assume_yes {
        let approved = match confirm_default_no(prompter, INSTALL_QUESTION) {
            Ok(approved) => approved,
            Err(PromptError::EndOfInput) => false,
            Err(error) => return Err(error.into()),
        };
        if !approved {
            writeln!(output, "{CANCELLED}").map_err(|error| RemoteCommandError::Output {
                reason: error.to_string(),
                completed: Vec::new(),
            })?;
            return Ok(Vec::new());
        }
    }
    let mut completed = Vec::new();
    for candidate in &candidates {
        let outcome = match install_prepared_plugin(candidate, &options.target, options.overwrite) {
            Ok(outcome) => outcome,
            Err(source) => return Err(RemoteCommandError::Installation { completed, source }),
        };
        let name = &candidate.manifest.setup.plugin;
        let message = match &outcome {
            PluginOutcome::Installed(_) => format!("instalado: {name}"),
            PluginOutcome::Replaced(_) => format!("substituído: {name}"),
            PluginOutcome::AlreadyInstalled(_) => {
                format!("já instalado: {name} (use --force para substituir)")
            }
        };
        let written = !matches!(outcome, PluginOutcome::AlreadyInstalled(_));
        completed.push(outcome);
        let result = (|| -> std::io::Result<()> {
            writeln!(output, "{message}")?;
            if written {
                writeln!(
                    output,
                    "versão do plugin {name}: {}",
                    candidate.manifest.metadata.version
                )?;
            }
            writeln!(output, "Para configurar: mosaic plugins setup {name}")
        })();
        if let Err(error) = result {
            return Err(RemoteCommandError::Output {
                reason: error.to_string(),
                completed,
            });
        }
    }
    Ok(completed)
}

#[cfg(all(test, unix))]
#[path = "../tests/common/remote_plugins.rs"]
mod fixtures;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use mosaic::application::ports::PluginFetchError;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    struct LocalFetcher {
        root: PathBuf,
        unreadable: bool,
    }
    impl PluginFetcher for LocalFetcher {
        fn fetch(&self, _: &GitHubSource, destination: &Path) -> Result<(), PluginFetchError> {
            fixtures::copy_tree(&self.root, destination);
            if self.unreadable {
                fs::set_permissions(
                    destination.join("plugins/gitlab/gitlab.py"),
                    fs::Permissions::from_mode(0o000),
                )
                .unwrap();
            }
            Ok(())
        }
    }
    struct NoPrompt;
    impl Prompter for NoPrompt {
        fn ask(&mut self, _: &str) -> Result<String, PromptError> {
            panic!("--yes não pergunta")
        }
        fn tell(&mut self, _: &str) {}
    }

    #[test]
    fn later_copy_failure_returns_completed_outcomes() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        for name in ["github", "gitlab"] {
            fixtures::write_package(
                &root.path().join(format!("plugins/{name}")),
                name,
                "0.0.4",
                "0.0.0",
            );
        }
        let fetcher = LocalFetcher {
            root: root.path().into(),
            unreadable: true,
        };
        let target = tempfile::tempdir().unwrap();
        let options = RemoteInstallOptions {
            target: target.path().into(),
            overwrite: Overwrite::Keep,
            assume_yes: true,
        };
        let sources: Vec<_> = ["github", "gitlab"]
            .into_iter()
            .map(|name| mosaic::application::plugin_catalog::resolve_plugin_source(name).unwrap())
            .collect();
        let error =
            execute_remote_install(&sources, &options, &fetcher, &mut NoPrompt, &mut Vec::new())
                .unwrap_err();
        assert_eq!(
            error.completed(),
            &[PluginOutcome::Installed("github".into())]
        );
        assert!(target.path().join("github/github.py").is_file());
        assert!(!target.path().join("gitlab").exists());
    }

    #[test]
    fn output_failure_after_installation_retains_completed_outcomes() {
        struct FailingOutput(PathBuf);
        impl Write for FailingOutput {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.join("github").exists() {
                    Err(std::io::Error::other("fixture"))
                } else {
                    Ok(bytes.len())
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        fixtures::write_package(root.path(), "github", "0.0.4", "0.0.0");
        let fetcher = LocalFetcher {
            root: root.path().into(),
            unreadable: false,
        };
        let target = tempfile::tempdir().unwrap();
        let options = RemoteInstallOptions {
            target: target.path().into(),
            overwrite: Overwrite::Keep,
            assume_yes: true,
        };
        let error = execute_remote_install(
            &[GitHubSource::parse("owner/repo").unwrap()],
            &options,
            &fetcher,
            &mut NoPrompt,
            &mut FailingOutput(target.path().into()),
        )
        .unwrap_err();
        assert_eq!(
            error.completed(),
            &[PluginOutcome::Installed("github".into())]
        );
        assert!(target.path().join("github/github.py").is_file());
    }
}
