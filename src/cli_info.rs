use std::process::ExitCode;

use mosaic::domain::language::Language;
use mosaic::infrastructure::app_paths::config_directory;
use mosaic::infrastructure::default_files::reference_config_text;

const EXIT_CODE_USAGE: u8 = 2;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DOCUMENTATION_URL: &str = "https://github.com/flisbao/mosaic";
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
    let language = Language::detect_from_environment();
    Some(match recognize(first) {
        Request::Version => {
            println!("{}", version_text());
            ExitCode::SUCCESS
        }
        Request::Help => {
            println!("{}", help_text(language));
            ExitCode::SUCCESS
        }
        Request::DefaultConfig => {
            print!("{}", reference_config_text(language));
            ExitCode::SUCCESS
        }
        Request::Unknown(argument) => {
            eprintln!("{}", unknown_argument_message(language, argument));
            ExitCode::from(EXIT_CODE_USAGE)
        }
    })
}

fn version_text() -> String {
    format!("mosaic {VERSION}")
}

fn help_text(language: Language) -> String {
    let configuration = config_directory()
        .map(|directory| directory.display().to_string())
        .unwrap_or_else(|| unknown_config_directory(language).to_string());
    match language {
        Language::English => format!(
        "mosaic {VERSION}: terminal dashboard built from widgets

Usage:
  mosaic                            opens the panel
  mosaic --help | -h | help         shows this help
  mosaic --version | -V | version   shows the installed version
  mosaic --default-config           prints the reference mosaic.toml, with every option commented
  mosaic plugins list               lists the official shortcuts, offline
  mosaic plugins install owner/repo[/subdir] [--yes]  downloads a public plugin with Git; asks for confirmation [y/N]
  mosaic plugins install NAME...   installs official shortcuts; --from DIR copies local plugins
  mosaic plugins setup NAME         terminal interview that builds the widget and writes it to mosaic.toml
  mosaic tabs list | add NAME       lists the tabs in mosaic.toml or creates a new tab
  mosaic themes [NAME]              lists the color themes or shows one
  mosaic check COMMAND...           validates the output of a widget; 'mosaic check --config' validates mosaic.toml
  mosaic skills install             links the Claude Code skills
  mosaic update [--check] [--yes]   checks for a new version; without --check, updates the binary (asks [y/N])

Quick start:
  mosaic plugins list
  mosaic plugins install jira
  mosaic plugins install flisbao/mosaic/plugins/jira --yes
  mosaic plugins setup jira
  mosaic                          (press + to add a widget without leaving the panel; R reloads mosaic.toml)

Configuration: {configuration}
  mosaic.toml  widgets, tabs, theme ([theme] name = \"tokyo-night\"), pomodoro and AI
  plugins/     the installed plugins
  Created on the first run; XDG_CONFIG_HOME changes the folder.

New version notice: at most one query per day to github.com when the panel opens.
  Turn it off with [update] check = false in mosaic.toml or MOSAIC_NO_UPDATE_CHECK=1.

In the panel: ? shows the keys, + adds a widget, R reloads, r refreshes, q quits.
Each subcommand shows its usage when called without arguments or with a wrong argument.
Documentation: {DOCUMENTATION_URL}"
        ),
        Language::BrazilianPortuguese => format!(
        "mosaic {VERSION}: painel de terminal feito de widgets

Uso:
  mosaic                            abre o painel
  mosaic --help | -h | help         mostra esta ajuda
  mosaic --version | -V | version   mostra a versão instalada
  mosaic --default-config           imprime o mosaic.toml de referência, com todas as opções comentadas
  mosaic plugins list               lista os atalhos oficiais, sem rede
  mosaic plugins install owner/repo[/subdir] [--yes]  baixa um plugin público com Git; pede confirmação [s/N]\n  mosaic plugins install NOME...   instala atalhos oficiais; --from DIR copia plugins locais
  mosaic plugins setup NOME         entrevista no terminal que monta o widget e grava no mosaic.toml
  mosaic tabs list | add NOME       lista as abas do mosaic.toml ou cria uma aba nova
  mosaic themes [NOME]              lista os temas de cor ou mostra um
  mosaic check COMANDO...           valida a saída de um widget; 'mosaic check --config' valida o mosaic.toml
  mosaic skills install             liga as skills do Claude Code
  mosaic update [--check] [--yes]   confere se há versão nova; sem --check, atualiza o binário (pergunta [s/N])

Primeiros passos:
  mosaic plugins list
  mosaic plugins install jira
  mosaic plugins install flisbao/mosaic/plugins/jira --yes
  mosaic plugins setup jira
  mosaic                          (aperte + para adicionar um widget sem sair do painel; R recarrega o mosaic.toml)

Configuração: {configuration}
  mosaic.toml  widgets, abas, tema ([theme] name = \"tokyo-night\"), pomodoro e IA
  plugins/     os plugins instalados
  Criada na primeira execução; XDG_CONFIG_HOME muda a pasta.

Aviso de versão nova: no máximo uma consulta por dia ao github.com ao abrir o painel.
  Desligue com [update] check = false no mosaic.toml ou MOSAIC_NO_UPDATE_CHECK=1.

No painel: ? mostra as teclas, + adiciona um widget, R recarrega, r atualiza, q sai.
Cada subcomando mostra o seu uso quando chamado sem argumentos ou com argumento errado.
Documentação: {DOCUMENTATION_URL}"
        ),
    }
}

fn unknown_config_directory(language: Language) -> &'static str {
    match language {
        Language::English => "(system configuration folder)",
        Language::BrazilianPortuguese => "(pasta de configuração do sistema)",
    }
}

fn unknown_argument_message(language: Language, argument: &str) -> String {
    match language {
        Language::English => format!("mosaic: unknown argument: {argument}\nuse: mosaic --help"),
        Language::BrazilianPortuguese => {
            format!("mosaic: argumento desconhecido: {argument}\nuse: mosaic --help")
        }
    }
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
        let help = help_text(Language::BrazilianPortuguese);
        for subcommand in [
            "plugins install",
            "plugins setup",
            "themes",
            "check",
            "skills install",
            "update",
        ] {
            assert!(
                help.contains(&format!("mosaic {subcommand}")),
                "{subcommand}"
            );
        }
        assert!(help.contains("mosaic update [--check] [--yes]"));
        assert!(help.contains("MOSAIC_NO_UPDATE_CHECK=1"));
        assert!(help.contains("Primeiros passos"));
        assert!(help.contains("mosaic --default-config"));
    }

    #[test]
    fn no_arguments_means_open_the_panel() {
        assert!(run(&[]).is_none());
    }

    #[test]
    fn the_english_help_is_translated_and_lists_every_subcommand() {
        let help = help_text(Language::English);
        for expected in [
            "Usage:",
            "Quick start:",
            "press + to add a widget",
            "mosaic update [--check] [--yes]",
            "asks for confirmation [y/N]",
            "MOSAIC_NO_UPDATE_CHECK=1",
        ] {
            assert!(help.contains(expected), "{expected}");
        }
        assert!(!help.contains("Primeiros passos"));
    }

    #[test]
    fn the_unknown_argument_message_is_translated() {
        assert_eq!(
            unknown_argument_message(Language::English, "--x"),
            "mosaic: unknown argument: --x\nuse: mosaic --help"
        );
        assert_eq!(
            unknown_argument_message(Language::BrazilianPortuguese, "--x"),
            "mosaic: argumento desconhecido: --x\nuse: mosaic --help"
        );
    }
}
