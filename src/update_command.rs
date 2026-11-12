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
    check_latest, install_update, plan_update, release_error_text, PlatformIdentity, UpdateError,
    UpdatePlan, UpdateTools,
};
use mosaic::display_in_portuguese;
use mosaic::domain::language::{Language, LocalizedMessage};
use mosaic::domain::plugin_package::PluginPackageError;
use mosaic::domain::release::{
    debian_package_name, InstallMethod, ReleaseAssets, ReleaseTarget, ReleaseVersion, UpdateStatus,
};
use mosaic::infrastructure::archive_tools::{ShaSumChecksum, TarExtractor};
use mosaic::infrastructure::curl_release_source::{
    download_base_from_environment, CurlReleaseSource,
};
use mosaic::infrastructure::installation_probe::{
    AtomicExecutableReplacer, SystemInstallationProbe,
};
use mosaic::infrastructure::stream_prompter::StreamPrompter;

const EXIT_CODE_FAILURE: u8 = 1;
const EXIT_CODE_USAGE: u8 = 2;
const CHECK_FLAG: &str = "--check";
const YES_FLAG: &str = "--yes";
const STAGING_PREFIX: &str = ".mosaic-update-";
const CARGO_GIT_URL: &str = "https://github.com/flisbao/mosaic";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateOptions {
    pub check_only: bool,
    pub assume_yes: bool,
}

pub fn update_usage(language: Language) -> &'static str {
    language.pick(
        "usage: mosaic update [--check] [--yes]\n  --check  only reports whether there is a new version\n  --yes    does not ask before replacing the executable",
        "uso: mosaic update [--check] [--yes]\n  --check  só informa se há versão nova\n  --yes    não pergunta antes de trocar o executável",
    )
}

#[derive(Debug, Error, PartialEq)]
pub enum UpdateUsageError {
    UnknownArgument(String),
}

impl LocalizedMessage for UpdateUsageError {
    fn message(&self, language: Language) -> String {
        match self {
            Self::UnknownArgument(argument) => language.pick(
                format!("unknown argument: {argument}\n{}", update_usage(language)),
                format!(
                    "argumento desconhecido: {argument}\n{}",
                    update_usage(language)
                ),
            ),
        }
    }
}

display_in_portuguese!(UpdateUsageError);

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
    Version(#[from] PluginPackageError),
    Update(#[from] UpdateError),
    Release(#[from] ReleaseError),
    Prompt(#[from] PromptError),
    Output(String),
    Staging(String),
}

impl LocalizedMessage for UpdateCommandError {
    fn message(&self, language: Language) -> String {
        match self {
            Self::Version(error) => error.message(language),
            Self::Update(error) => error.message(language),
            Self::Release(error) => release_error_text(error, language),
            Self::Prompt(error) => error.to_string(),
            Self::Output(reason) => language.pick(
                format!("could not write to the output: {reason}"),
                format!("não consegui escrever na saída: {reason}"),
            ),
            Self::Staging(reason) => language.pick(
                format!("could not prepare a temporary folder next to the executable: {reason}"),
                format!(
                    "não consegui preparar uma pasta temporária ao lado do executável: {reason}"
                ),
            ),
        }
    }
}

display_in_portuguese!(UpdateCommandError);

impl UpdateCommandError {
    pub fn diagnostic(&self, language: Language) -> String {
        match self {
            Self::Update(UpdateError::Release(ReleaseError::NotWritable(reason)))
            | Self::Release(ReleaseError::NotWritable(reason)) => {
                not_writable_text(reason, language)
            }
            Self::Staging(reason) => not_writable_text(reason, language),
            other => other.message(language),
        }
    }
}

fn not_writable_text(reason: &str, language: Language) -> String {
    language.pick(
        format!("{reason}. mosaic does not use sudo: install again in a folder of yours with MOSAIC_INSTALL_DIR (see install.sh) or adjust the permissions of the executable's folder"),
        format!("{reason}. O mosaic não usa sudo: instale de novo numa pasta sua com MOSAIC_INSTALL_DIR (veja install.sh) ou ajuste as permissões da pasta do executável"),
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
    pub language: Language,
}

fn write_line(output: &mut impl Write, text: &str) -> Result<(), UpdateCommandError> {
    writeln!(output, "{text}").map_err(|error| UpdateCommandError::Output(error.to_string()))
}

pub fn execute_check(
    source: &dyn ReleaseSource,
    current: &ReleaseVersion,
    output: &mut impl Write,
    language: Language,
) -> Result<(), UpdateCommandError> {
    let text = match check_latest(source, current)? {
        UpdateStatus::Available { latest } => available_text(&latest, current, language),
        UpdateStatus::UpToDate => up_to_date_text(current, language),
    };
    write_line(output, &text)
}

fn available_text(latest: &ReleaseVersion, current: &ReleaseVersion, language: Language) -> String {
    language.pick(
        format!("mosaic {latest} available (installed: {current})"),
        format!("mosaic {latest} disponível (instalada: {current})"),
    )
}

fn up_to_date_text(current: &ReleaseVersion, language: Language) -> String {
    language.pick(
        format!("mosaic {current} is the latest version"),
        format!("mosaic {current} é a versão mais recente"),
    )
}

pub fn execute_update(
    services: &UpdateServices<'_>,
    current: &ReleaseVersion,
    options: UpdateOptions,
    prompter: &mut impl Prompter,
    output: &mut impl Write,
) -> Result<(), UpdateCommandError> {
    let language = services.language;
    let plan = plan_update(services.source, services.probe, current, &services.platform)?;
    match plan {
        UpdatePlan::UpToDate => write_line(output, &up_to_date_text(current, language)),
        UpdatePlan::Guidance { method, latest } => {
            let text = guidance_text(method, &latest, current, services);
            write_line(output, &text)
        }
        UpdatePlan::Replace {
            latest,
            executable,
            target,
        } => {
            write_line(output, &available_text(&latest, current, language))?;
            write_line(
                output,
                &format!(
                    "{}: {}",
                    language.pick("path", "caminho"),
                    executable.display()
                ),
            )?;
            if !options.assume_yes && !confirmed(prompter, language)? {
                return write_line(
                    output,
                    language.pick(
                        "Update cancelled; nothing was changed.",
                        "Atualização cancelada; nada foi alterado.",
                    ),
                );
            }
            replace_executable(services, &latest, target, &executable)?;
            write_line(
                output,
                &language.pick(
                    format!("mosaic updated to {latest} at {}", executable.display()),
                    format!(
                        "mosaic atualizado para {latest} em {}",
                        executable.display()
                    ),
                ),
            )
        }
    }
}

fn confirmed(prompter: &mut impl Prompter, language: Language) -> Result<bool, UpdateCommandError> {
    let question = language.pick("Update now? [y/N]: ", "Atualizar agora? [s/N]: ");
    match confirm_default_no(prompter, question, language) {
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
    let language = services.language;
    let headline = available_text(latest, current, language);
    match method {
        InstallMethod::DebianPackage => {
            let has_package =
                ReleaseTarget::detect(services.platform.operating_system, services.platform.architecture)
                    .is_some_and(ReleaseTarget::has_debian_package);
            let release_page = format!("{}/tag/v{latest}", services.download_base);
            if has_package {
                let package = debian_package_name(latest);
                language.pick(
                    format!(
                        "{headline}\nInstalled from the .deb package; mosaic does not replace system files.\nDownload: {}/download/v{latest}/{package}\nInstall: sudo apt install ./{package}",
                        services.download_base
                    ),
                    format!(
                        "{headline}\nInstalado pelo pacote .deb; o mosaic não troca arquivos do sistema.\nBaixe: {}/download/v{latest}/{package}\nInstale: sudo apt install ./{package}",
                        services.download_base
                    ),
                )
            } else {
                language.pick(
                    format!("{headline}\nInstalled from a system package; mosaic does not replace system files.\nDownload the new version at: {release_page}"),
                    format!("{headline}\nInstalado por um pacote do sistema; o mosaic não troca arquivos do sistema.\nBaixe a versão nova em: {release_page}"),
                )
            }
        }
        InstallMethod::Homebrew => {
            language.pick(
                format!("{headline}\nInstalled with Homebrew. Run: brew upgrade mosaic"),
                format!("{headline}\nInstalado pelo Homebrew. Rode: brew upgrade mosaic"),
            )
        }
        InstallMethod::Cargo => language.pick(
            format!("{headline}\nInstalled with cargo or built from the repository. Update with: cargo install --path . (inside the checkout) or cargo install --git {CARGO_GIT_URL}"),
            format!("{headline}\nInstalado com cargo ou compilado do repositório. Atualize com: cargo install --path . (dentro do checkout) ou cargo install --git {CARGO_GIT_URL}"),
        ),
        InstallMethod::SelfManaged => headline,
    }
}

pub fn run(arguments: &[String]) -> ExitCode {
    let language = Language::detect_from_environment();
    let options = match parse_arguments(arguments) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{}", error.message(language));
            return ExitCode::from(EXIT_CODE_USAGE);
        }
    };
    match run_with_system_services(options, language) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mosaic: {}", error.diagnostic(language));
            ExitCode::from(EXIT_CODE_FAILURE)
        }
    }
}

fn run_with_system_services(
    options: UpdateOptions,
    language: Language,
) -> Result<(), UpdateCommandError> {
    let current = ReleaseVersion::parse(env!("CARGO_PKG_VERSION"))?;
    let source = CurlReleaseSource::new(download_base_from_environment()).with_language(language);
    let mut output = io::stdout();
    if options.check_only {
        return execute_check(&source, &current, &mut output, language);
    }
    let probe = SystemInstallationProbe::default().with_language(language);
    let checksums = ShaSumChecksum::default().with_language(language);
    let extractor = TarExtractor::default().with_language(language);
    let replacer = AtomicExecutableReplacer::default().with_language(language);
    let services = UpdateServices {
        source: &source,
        probe: &probe,
        checksums: &checksums,
        extractor: &extractor,
        replacer: &replacer,
        platform: PlatformIdentity {
            operating_system: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
        },
        download_base: source.base(),
        language,
    };
    let mut prompter = StreamPrompter::new(io::stdin().lock(), io::stdout());
    execute_update(&services, &current, options, &mut prompter, &mut output)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use mosaic::domain::language::Language;
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
                language: Language::BrazilianPortuguese,
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
            Language::BrazilianPortuguese,
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
            Language::BrazilianPortuguese,
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
            Language::BrazilianPortuguese,
        )
        .unwrap_err();
        assert!(error
            .diagnostic(Language::BrazilianPortuguese)
            .contains("sem rede"));
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
        assert!(result
            .unwrap_err()
            .diagnostic(Language::BrazilianPortuguese)
            .contains("SHA-256"));
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
        let diagnostic = result
            .unwrap_err()
            .diagnostic(Language::BrazilianPortuguese);
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
            language: Language::BrazilianPortuguese,
        };
        let error = execute_update(
            &services,
            &version("0.0.10"),
            options(false, true),
            &mut answers(vec![]),
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error
            .diagnostic(Language::BrazilianPortuguese)
            .contains("freebsd/x86_64"));
    }
}

#[cfg(test)]
mod english_tests {
    use super::*;

    fn version(text: &str) -> ReleaseVersion {
        ReleaseVersion::parse(text).unwrap()
    }

    #[test]
    fn version_lines_are_translated() {
        assert_eq!(
            available_text(&version("0.2.0"), &version("0.1.0"), Language::English),
            "mosaic 0.2.0 available (installed: 0.1.0)"
        );
        assert_eq!(
            up_to_date_text(&version("0.1.0"), Language::English),
            "mosaic 0.1.0 is the latest version"
        );
    }

    #[test]
    fn usage_and_errors_are_translated() {
        assert!(update_usage(Language::English).starts_with("usage: mosaic update"));
        let error = UpdateUsageError::UnknownArgument("--x".to_string());
        assert!(error
            .message(Language::English)
            .starts_with("unknown argument: --x"));
        let error = UpdateCommandError::Staging("denied".to_string());
        let english = error.diagnostic(Language::English);
        assert!(english.contains("mosaic does not use sudo"), "{english}");
        assert!(error
            .diagnostic(Language::BrazilianPortuguese)
            .contains("O mosaic não usa sudo"));
    }
}
