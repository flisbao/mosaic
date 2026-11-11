use std::process::ExitCode;

use mosaic::infrastructure::app_paths::config_directory;
use mosaic::infrastructure::default_files::reference_config_text;

const EXIT_CODE_USAGE: u8 = 2;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DOCUMENTATION_URL: &str = "https://github.com/flisbao/mosaic";
const UNKNOWN_CONFIG_DIRECTORY: &str = "(pasta de configuração do sistema)";
const VERSION_ARGUMENTS: [&str; 3] = ["--version", "-V", "version"];
const HELP_ARGUMENTS: [&str; 3] = ["--help", "-h", "help"];
const DEFAULT_CONFIG_ARGUMENT: &str = "--default-config";

enum Request<'a> {
    Version,
    Help,
    DefaultConfig,
    Unknown(&'a str),
}

fn recognize(argument: &str) -> Request<'_> {
    if VERSION_ARGUMENTS.contains(&argument) {
        Request::Version
    } else if HELP_ARGUMENTS.contains(&argument) {
        Request::Help
    } else if argument == DEFAULT_CONFIG_ARGUMENT {
        Request::DefaultConfig
    } else {
        Request::Unknown(argument)
    }
}

pub fn run(arguments: &[String]) -> Option<ExitCode> {
    let first = arguments.first()?;
    Some(match recognize(first) {
        Request::Version => {
            println!("{}", version_text());
            ExitCode::SUCCESS
        }
        Request::Help => {
            println!("{}", help_text());
            ExitCode::SUCCESS
        }
        Request::DefaultConfig => {
            print!("{}", reference_config_text());
            ExitCode::SUCCESS
        }
        Request::Unknown(argument) => {
            eprintln!("mosaic: argumento desconhecido: {argument}\nuse: mosaic --help");
            ExitCode::from(EXIT_CODE_USAGE)
        }
    })
}

fn version_text() -> String {
    format!("mosaic {VERSION}")
}

fn help_text() -> String {
    let configuration = config_directory()
        .map(|directory| directory.display().to_string())
        .unwrap_or_else(|| UNKNOWN_CONFIG_DIRECTORY.to_string());
    format!(
        "mosaic {VERSION}: painel de terminal feito de widgets

Uso:
  mosaic                            abre o painel
  mosaic --help | -h | help         mostra esta ajuda
  mosaic --version | -V | version   mostra a versão instalada
  mosaic --default-config           imprime o mosaic.toml de referência, com todas as opções comentadas
  mosaic plugins list               lista os plugins de serviço (Jira, GitHub, GitLab, Azure, agenda)
  mosaic plugins install [NOME...]  copia plugins para a pasta de configuração e liga a skill do Claude Code
  mosaic plugins setup NOME         entrevista no terminal que monta o widget e grava no mosaic.toml
  mosaic tabs list | add NOME       lista as abas do mosaic.toml ou cria uma aba nova
  mosaic themes [NOME]              lista os temas de cor ou mostra um
  mosaic check COMANDO...           valida a saída de um widget; 'mosaic check --config' valida o mosaic.toml
  mosaic skills install             liga as skills do Claude Code

Primeiros passos:
  mosaic plugins list
  mosaic plugins install jira
  mosaic plugins setup jira
  mosaic                          (aperte R para recarregar o mosaic.toml)

Configuração: {configuration}
  mosaic.toml  widgets, abas, tema ([theme] name = \"tokyo-night\"), pomodoro e IA
  plugins/     os plugins instalados
  Criada na primeira execução; XDG_CONFIG_HOME muda a pasta.

No painel: ? mostra as teclas, R recarrega, r atualiza, q sai.
Cada subcomando mostra o seu uso quando chamado sem argumentos ou com argumento errado.
Documentação: {DOCUMENTATION_URL}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_comes_from_the_package() {
        assert_eq!(
            version_text(),
            format!("mosaic {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn the_help_lists_every_subcommand_and_the_first_steps() {
        let help = help_text();
        for subcommand in [
            "plugins install",
            "plugins setup",
            "themes",
            "check",
            "skills install",
        ] {
            assert!(
                help.contains(&format!("mosaic {subcommand}")),
                "{subcommand}"
            );
        }
        assert!(help.contains("Primeiros passos"));
        assert!(help.contains("mosaic --default-config"));
    }

    #[test]
    fn no_arguments_means_open_the_panel() {
        assert!(run(&[]).is_none());
    }
}
