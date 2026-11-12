use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use thiserror::Error;

use crate::confirmation::confirm_default_no;
use mosaic::application::ports::{
    ArchiveExtractor, ChecksumCalculator, ExecutableReplacer, InstallationProbe, PromptError,
    Prompter, ReleaseError, ReleaseSource,
};
use mosaic::application::update::{
    check_latest, install_update, plan_update, PlatformIdentity, UpdateError, UpdatePlan,
    UpdateTools,
};
use mosaic::domain::plugin_package::PluginPackageError;
use mosaic::domain::release::{
    debian_package_name, InstallMethod, ReleaseAssets, ReleaseTarget, ReleaseVersion, UpdateStatus,
};
use mosaic::infrastructure::archive_tools::{ShaSumChecksum, TarExtractor};
use mosaic::infrastructure::curl_release_source::{CurlReleaseSource, DOWNLOAD_BASE_VARIABLE};
use mosaic::infrastructure::installation_probe::{
    AtomicExecutableReplacer, SystemInstallationProbe,
};
use mosaic::infrastructure::stream_prompter::StreamPrompter;

const EXIT_CODE_FAILURE: u8 = 1;
const EXIT_CODE_USAGE: u8 = 2;
const CHECK_FLAG: &str = "--check";
const YES_FLAG: &str = "--yes";
const UPDATE_USAGE: &str = "uso: mosaic update [--check] [--yes]\n  --check  só informa se há versão nova\n  --yes    não pergunta antes de trocar o executável";
const STAGING_PREFIX: &str = ".mosaic-update-";
const CONFIRMATION_QUESTION: &str = "Atualizar agora? [s/N]: ";
const CANCELLED_MESSAGE: &str = "Atualização cancelada; nada foi alterado.";
const CARGO_GIT_URL: &str = "https://github.com/flisbao/mosaic";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateOptions {
    pub check_only: bool,
    pub assume_yes: bool,
}

#[derive(Debug, Error, PartialEq)]
pub enum UpdateUsageError {
    #[error("argumento desconhecido: {0}\n{UPDATE_USAGE}")]
    UnknownArgument(String),
}

pub fn parse_arguments(arguments: &[String]) -> Result<UpdateOptions, UpdateUsageError> {
    let mut options = UpdateOptions::default();
    for argument in arguments {
        match argument.as_str() {
            CHECK_FLAG => options.check_only = true,
            YES_FLAG => options.assume_yes = true,
            other => return Err(UpdateUsageError::UnknownArgument(other.to_string())),
        }
    }
    Ok(options)
}

#[derive(Debug, Error)]
pub enum UpdateCommandError {
    #[error(transparent)]
    Version(#[from] PluginPackageError),
    #[error(transparent)]
    Update(#[from] UpdateError),
    #[error(transparent)]
    Release(#[from] ReleaseError),
    #[error(transparent)]
    Prompt(#[from] PromptError),
    #[error("não consegui escrever na saída: {0}")]
    Output(String),
    #[error("não consegui preparar uma pasta temporária ao lado do executável: {0}")]
    Staging(String),
}

impl UpdateCommandError {
    pub fn diagnostic(&self) -> String {
        match self {
            Self::Update(UpdateError::Release(ReleaseError::NotWritable(reason)))
            | Self::Release(ReleaseError::NotWritable(reason)) => not_writable_text(reason),
            Self::Staging(reason) => not_writable_text(reason),
            other => other.to_string(),
        }
    }
}

fn not_writable_text(reason: &str) -> String {
    format!(
        "{reason}. O mosaic não usa sudo: instale de novo numa pasta sua com MOSAIC_INSTALL_DIR (veja install.sh) ou ajuste as permissões da pasta do executável"
    )
}

pub struct UpdateServices<'a> {
    pub source: &'a dyn ReleaseSource,
    pub probe: &'a dyn InstallationProbe,
    pub checksums: &'a dyn ChecksumCalculator,
    pub extractor: &'a dyn ArchiveExtractor,
    pub replacer: &'a dyn ExecutableReplacer,
    pub platform: PlatformIdentity<'a>,
    pub download_base: &'a str,
}

fn write_line(output: &mut impl Write, text: &str) -> Result<(), UpdateCommandError> {
    writeln!(output, "{text}").map_err(|error| UpdateCommandError::Output(error.to_string()))
}

pub fn execute_check(
    source: &dyn ReleaseSource,
    current: &ReleaseVersion,
    output: &mut impl Write,
) -> Result<(), UpdateCommandError> {
    let text = match check_latest(source, current)? {
        UpdateStatus::Available { latest } => available_text(&latest, current),
        UpdateStatus::UpToDate => up_to_date_text(current),
    };
    write_line(output, &text)
}

fn available_text(latest: &ReleaseVersion, current: &ReleaseVersion) -> String {
    format!("mosaic {latest} disponível (instalada: {current})")
}

fn up_to_date_text(current: &ReleaseVersion) -> String {
    format!("mosaic {current} é a versão mais recente")
}

pub fn execute_update(
    services: &UpdateServices<'_>,
    current: &ReleaseVersion,
    options: UpdateOptions,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
) -> Result<(), UpdateCommandError> {
    let plan = plan_update(services.source, services.probe, current, &services.platform)?;
    match plan {
        UpdatePlan::UpToDate => write_line(output, &up_to_date_text(current)),
        UpdatePlan::Guidance { method, latest } => {
            let text = guidance_text(method, &latest, current, services);
            write_line(output, &text)
        }
        UpdatePlan::Replace {
            latest,
            executable,
            target,
        } => {
            write_line(output, &available_text(&latest, current))?;
            write_line(output, &format!("caminho: {}", executable.display()))?;
            if !options.assume_yes && !confirmed(prompter)? {
                return write_line(output, CANCELLED_MESSAGE);
            }
            replace_executable(services, &latest, target, &executable)?;
            write_line(
                output,
                &format!(
                    "mosaic atualizado para {latest} em {}",
                    executable.display()
                ),
            )
        }
    }
}

fn confirmed(prompter: &mut impl Prompter) -> Result<bool, UpdateCommandError> {
    match confirm_default_no(prompter, CONFIRMATION_QUESTION) {
        Ok(answer) => Ok(answer),
        Err(PromptError::EndOfInput) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn replace_executable(
    services: &UpdateServices<'_>,
    latest: &ReleaseVersion,
    target: ReleaseTarget,
    executable: &Path,
) -> Result<(), UpdateCommandError> {
    let directory = executable.parent().unwrap_or_else(|| Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(STAGING_PREFIX)
        .tempdir_in(directory)
        .map_err(|error| UpdateCommandError::Staging(error.to_string()))?;
    let tools = UpdateTools {
        source: services.source,
        checksums: services.checksums,
        extractor: services.extractor,
        replacer: services.replacer,
    };
    let assets = ReleaseAssets {
        version: latest.clone(),
        target,
    };
    install_update(&tools, &assets, executable, staging.path())?;
    Ok(())
}

fn guidance_text(
    method: InstallMethod,
    latest: &ReleaseVersion,
    current: &ReleaseVersion,
    services: &UpdateServices<'_>,
) -> String {
    let headline = available_text(latest, current);
    match method {
        InstallMethod::DebianPackage => {
            let has_package =
                ReleaseTarget::detect(services.platform.operating_system, services.platform.architecture)
                    .is_some_and(ReleaseTarget::has_debian_package);
            let release_page = format!("{}/tag/v{latest}", services.download_base);
            if has_package {
                let package = debian_package_name(latest);
                format!(
                    "{headline}\nInstalado pelo pacote .deb; o mosaic não troca arquivos do sistema.\nBaixe: {}/download/v{latest}/{package}\nInstale: sudo apt install ./{package}",
                    services.download_base
                )
            } else {
                format!(
                    "{headline}\nInstalado por um pacote do sistema; o mosaic não troca arquivos do sistema.\nBaixe a versão nova em: {release_page}"
                )
            }
        }
        InstallMethod::Homebrew => {
            format!("{headline}\nInstalado pelo Homebrew. Rode: brew upgrade mosaic")
        }
        InstallMethod::Cargo => format!(
            "{headline}\nInstalado com cargo ou compilado do repositório. Atualize com: cargo install --path . (dentro do checkout) ou cargo install --git {CARGO_GIT_URL}"
        ),
        InstallMethod::SelfManaged => headline,
    }
}

pub fn run(arguments: &[String]) -> ExitCode {
    let options = match parse_arguments(arguments) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    match run_with_system_services(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mosaic: {}", error.diagnostic());
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn run_with_system_services(options: UpdateOptions) -> Result<(), UpdateCommandError> {
    let current = ReleaseVersion::parse(env!("CARGO_PKG_VERSION"))?;
    let download_base = std::env::var(DOWNLOAD_BASE_VARIABLE)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let source = CurlReleaseSource::new(download_base);
    let mut output = io::stdout();
    if options.check_only {
        return execute_check(&source, &current, &mut output);
    }
    let probe = SystemInstallationProbe;
    let services = UpdateServices {
        source: &source,
        probe: &probe,
        checksums: &ShaSumChecksum,
        extractor: &TarExtractor,
        replacer: &AtomicExecutableReplacer,
        platform: PlatformIdentity {
            operating_system: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
        },
        download_base: source.base(),
    };
    let mut prompter = StreamPrompter::new(io::stdin().lock(), io::stdout());
    execute_update(&services, &current, options, &mut prompter, &mut output)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;

    fn version(text: &str) -> ReleaseVersion {
        ReleaseVersion::parse(text).unwrap()
    }

    struct FakeSource(Result<ReleaseVersion, ReleaseError>);
    impl ReleaseSource for FakeSource {
        fn latest_version(&self) -> Result<ReleaseVersion, ReleaseError> {
            self.0.clone()
        }
        fn download_text(&self, _: &ReleaseVersion, _: &str) -> Result<String, ReleaseError> {
            Ok(format!("{}  x\n", "a".repeat(64)))
        }
        fn download_file(&self, _: &ReleaseVersion, _: &str, _: &Path) -> Result<(), ReleaseError> {
            Ok(())
        }
    }

    struct FakeProbe {
        executable: PathBuf,
        debian_owned: bool,
        homebrew_prefix: Option<PathBuf>,
    }
    impl InstallationProbe for FakeProbe {
        fn executable_path(&self) -> Result<PathBuf, ReleaseError> {
            Ok(self.executable.clone())
        }
        fn owned_by_debian_package(&self, _: &Path) -> bool {
            self.debian_owned
        }
        fn homebrew_prefix(&self) -> Option<PathBuf> {
            self.homebrew_prefix.clone()
        }
        fn cargo_home(&self) -> Option<PathBuf> {
            None
        }
    }

    struct FakeChecksum(String);
    impl ChecksumCalculator for FakeChecksum {
        fn sha256(&self, _: &Path) -> Result<String, ReleaseError> {
            Ok(self.0.clone())
        }
    }

    struct FakeExtractor;
    impl ArchiveExtractor for FakeExtractor {
        fn extract(&self, _: &Path, _: &Path) -> Result<(), ReleaseError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeReplacer {
        replaced: RefCell<Vec<PathBuf>>,
        denied: bool,
    }
    impl ExecutableReplacer for FakeReplacer {
        fn replace(&self, _: &Path, current: &Path) -> Result<(), ReleaseError> {
            if self.denied {
                return Err(ReleaseError::NotWritable(format!(
                    "sem permissão para escrever em {}",
                    current.display()
                )));
            }
            self.replaced.borrow_mut().push(current.to_path_buf());
            Ok(())
        }
    }

    struct Scripted(std::vec::IntoIter<&'static str>, Vec<String>);
    impl Prompter for Scripted {
        fn ask(&mut self, question: &str) -> Result<String, PromptError> {
            self.1.push(question.to_string());
            self.0
                .next()
                .map(str::to_string)
                .ok_or(PromptError::EndOfInput)
        }
        fn tell(&mut self, _: &str) {}
    }
    fn answers(answers: Vec<&'static str>) -> Scripted {
        Scripted(answers.into_iter(), Vec::new())
    }

    struct Scenario {
        directory: tempfile::TempDir,
        latest: Result<ReleaseVersion, ReleaseError>,
        debian_owned: bool,
        homebrew_prefix: Option<PathBuf>,
        checksum: String,
        denied: bool,
        executable_relative: &'static str,
    }

    impl Scenario {
        fn new() -> Self {
            Self {
                directory: tempfile::tempdir().unwrap(),
                latest: Ok(version("0.0.11")),
                debian_owned: false,
                homebrew_prefix: None,
                checksum: "a".repeat(64),
                denied: false,
                executable_relative: "bin/mosaic",
            }
        }

        fn run(
            &self,
            options: UpdateOptions,
            prompter: &mut Scripted,
        ) -> (Result<(), UpdateCommandError>, String, Vec<PathBuf>) {
            let executable = self.directory.path().join(self.executable_relative);
            if !executable.starts_with("/usr") {
                fs::create_dir_all(executable.parent().unwrap()).unwrap();
                fs::write(&executable, "antigo").unwrap();
            }
            let probe = FakeProbe {
                executable: executable.clone(),
                debian_owned: self.debian_owned,
                homebrew_prefix: self.homebrew_prefix.clone(),
            };
            let source = FakeSource(self.latest.clone());
            let checksums = FakeChecksum(self.checksum.clone());
            let replacer = FakeReplacer {
                denied: self.denied,
                ..FakeReplacer::default()
            };
            let services = UpdateServices {
                source: &source,
                probe: &probe,
                checksums: &checksums,
                extractor: &FakeExtractor,
                replacer: &replacer,
                platform: PlatformIdentity {
                    operating_system: "linux",
                    architecture: "x86_64",
                },
                download_base: "https://example.test/releases",
            };
            let mut output = Vec::new();
            let result = execute_update(
                &services,
                &version("0.0.10"),
                options,
                prompter,
                &mut output,
            );
            let replaced = replacer.replaced.borrow().clone();
            (result, String::from_utf8(output).unwrap(), replaced)
        }
    }

    fn options(check_only: bool, assume_yes: bool) -> UpdateOptions {
        UpdateOptions {
            check_only,
            assume_yes,
        }
    }

    #[test]
    fn arguments_accept_check_and_yes_and_reject_anything_else() {
        let parse = |arguments: &[&str]| {
            parse_arguments(&arguments.iter().map(|a| a.to_string()).collect::<Vec<_>>())
        };
        assert_eq!(parse(&[]), Ok(options(false, false)));
        assert_eq!(parse(&["--check"]), Ok(options(true, false)));
        assert_eq!(parse(&["--yes", "--check"]), Ok(options(true, true)));
        assert!(matches!(
            parse(&["--force"]),
            Err(UpdateUsageError::UnknownArgument(_))
        ));
    }

    #[test]
    fn check_prints_the_new_version_or_that_this_is_the_latest() {
        let mut output = Vec::new();
        execute_check(
            &FakeSource(Ok(version("0.0.11"))),
            &version("0.0.10"),
            &mut output,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "mosaic 0.0.11 disponível (instalada: 0.0.10)\n"
        );
        let mut output = Vec::new();
        execute_check(
            &FakeSource(Ok(version("0.0.10"))),
            &version("0.0.10"),
            &mut output,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "mosaic 0.0.10 é a versão mais recente\n"
        );
    }

    #[test]
    fn check_reports_network_errors() {
        let error = execute_check(
            &FakeSource(Err(ReleaseError::Failed("sem rede".into()))),
            &version("0.0.10"),
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.diagnostic().contains("sem rede"));
    }

    #[test]
    fn up_to_date_says_so_and_changes_nothing() {
        let mut scenario = Scenario::new();
        scenario.latest = Ok(version("0.0.10"));
        let (result, output, replaced) = scenario.run(options(false, true), &mut answers(vec![]));
        result.unwrap();
        assert_eq!(output, "mosaic 0.0.10 é a versão mais recente\n");
        assert!(replaced.is_empty());
    }

    #[test]
    fn a_debian_install_prints_the_deb_url_and_the_apt_command_without_replacing() {
        let mut scenario = Scenario::new();
        scenario.debian_owned = true;
        scenario.executable_relative = "/usr/bin/mosaic";
        let (result, output, replaced) = scenario.run(options(false, true), &mut answers(vec![]));
        result.unwrap();
        assert!(replaced.is_empty());
        assert!(output
            .contains("https://example.test/releases/download/v0.0.11/mosaic_0.0.11_amd64.deb"));
        assert!(output.contains("sudo apt install ./mosaic_0.0.11_amd64.deb"));
    }

    #[test]
    fn homebrew_and_cargo_installs_print_their_own_command() {
        let mut brew = Scenario::new();
        brew.executable_relative = "Cellar/mosaic/0.0.10/bin/mosaic";
        let (result, output, replaced) = brew.run(options(false, true), &mut answers(vec![]));
        result.unwrap();
        assert!(output.contains("brew upgrade mosaic"));
        assert!(replaced.is_empty());

        let mut cargo = Scenario::new();
        cargo.executable_relative = "projeto/target/release/mosaic";
        let (result, output, replaced) = cargo.run(options(false, true), &mut answers(vec![]));
        result.unwrap();
        assert!(output.contains("cargo install --path ."));
        assert!(replaced.is_empty());
    }

    #[test]
    fn a_self_managed_install_asks_with_default_no_and_replaces_on_yes() {
        let scenario = Scenario::new();
        for declined in [vec!["n"], vec![""], vec![]] {
            let (result, output, replaced) =
                scenario.run(options(false, false), &mut answers(declined));
            result.unwrap();
            assert!(output.contains("mosaic 0.0.11 disponível (instalada: 0.0.10)"));
            assert!(output.contains("Atualização cancelada; nada foi alterado."));
            assert!(replaced.is_empty());
        }
        let mut accepting = answers(vec!["s"]);
        let (result, output, replaced) = scenario.run(options(false, false), &mut accepting);
        result.unwrap();
        assert_eq!(accepting.1, ["Atualizar agora? [s/N]: "]);
        assert_eq!(replaced.len(), 1);
        assert!(output.contains("mosaic atualizado para 0.0.11"));
    }

    #[test]
    fn yes_skips_the_question() {
        let scenario = Scenario::new();
        let mut prompter = answers(vec![]);
        let (result, _, replaced) = scenario.run(options(false, true), &mut prompter);
        result.unwrap();
        assert!(prompter.1.is_empty());
        assert_eq!(replaced.len(), 1);
    }

    #[test]
    fn a_checksum_mismatch_fails_without_replacing_and_leaves_no_staging_directory() {
        let mut scenario = Scenario::new();
        scenario.checksum = "b".repeat(64);
        let (result, _, replaced) = scenario.run(options(false, true), &mut answers(vec![]));
        assert!(result.unwrap_err().diagnostic().contains("SHA-256"));
        assert!(replaced.is_empty());
        let leftovers: Vec<_> = fs::read_dir(scenario.directory.path().join("bin"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["mosaic"]);
    }

    #[test]
    fn a_denied_write_explains_the_way_out_without_sudo() {
        let mut scenario = Scenario::new();
        scenario.denied = true;
        let (result, _, _) = scenario.run(options(false, true), &mut answers(vec![]));
        let diagnostic = result.unwrap_err().diagnostic();
        assert!(diagnostic.contains("sem permissão"), "{diagnostic}");
        assert!(diagnostic.contains("MOSAIC_INSTALL_DIR"), "{diagnostic}");
        assert!(diagnostic.contains("não usa sudo"), "{diagnostic}");
    }

    #[test]
    fn an_unsupported_platform_is_reported() {
        let scenario = Scenario::new();
        let executable = scenario.directory.path().join("mosaic");
        fs::write(&executable, "x").unwrap();
        let probe = FakeProbe {
            executable,
            debian_owned: false,
            homebrew_prefix: None,
        };
        let source = FakeSource(Ok(version("0.0.11")));
        let services = UpdateServices {
            source: &source,
            probe: &probe,
            checksums: &FakeChecksum(String::new()),
            extractor: &FakeExtractor,
            replacer: &FakeReplacer::default(),
            platform: PlatformIdentity {
                operating_system: "freebsd",
                architecture: "x86_64",
            },
            download_base: "x",
        };
        let error = execute_update(
            &services,
            &version("0.0.10"),
            options(false, true),
            &mut answers(vec![]),
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.diagnostic().contains("freebsd/x86_64"));
    }
}
