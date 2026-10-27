#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use mosaic::application::plugin_setup::{run_setup, SetupOptions, SetupResult};
use mosaic::application::ports::{PromptError, Prompter};
use mosaic::domain::plugin_setup::{AnswerKind, SetupManifest, Step, Target};
use mosaic::infrastructure::config::parse_config;
use mosaic::infrastructure::setup_manifest::{read_manifest, MANIFEST_FILE_NAME};

const NOT_PLUGIN_FOLDERS: [&str; 2] = ["tests", "__pycache__"];
const SERVICE_PLUGINS: [&str; 6] = [
    "azure-devops",
    "azure-pipelines",
    "calendar",
    "github",
    "gitlab",
    "jira",
];

struct ScriptedPrompter {
    answers: std::vec::IntoIter<String>,
}

impl Prompter for ScriptedPrompter {
    fn ask(&mut self, _question: &str) -> Result<String, PromptError> {
        self.answers.next().ok_or(PromptError::EndOfInput)
    }

    fn tell(&mut self, _message: &str) {}
}

fn plugins_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")
}

fn manifest_of(plugin: &str) -> SetupManifest {
    read_manifest(&plugins_directory().join(plugin).join(MANIFEST_FILE_NAME)).unwrap()
}

fn interview(plugin: &str, answers: &[&str]) -> SetupResult {
    let mut prompter = ScriptedPrompter {
        answers: answers
            .iter()
            .map(|answer| answer.to_string())
            .collect::<Vec<_>>()
            .into_iter(),
    };
    let result = run_setup(
        &manifest_of(plugin),
        &SetupOptions::default(),
        &mut prompter,
    )
    .unwrap();
    assert!(
        prompter.answers.next().is_none(),
        "{plugin}: sobraram respostas"
    );
    let config = parse_config(&result.draft.to_toml_block()).unwrap();
    assert_eq!(config.widgets.len(), 1, "{plugin}");
    result
}

fn environment(result: &SetupResult) -> Vec<(&str, &str)> {
    result
        .draft
        .environment
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect()
}

#[test]
fn every_service_plugin_has_a_valid_manifest_pointing_at_its_script() {
    for plugin in SERVICE_PLUGINS {
        let manifest = manifest_of(plugin);
        assert_eq!(manifest.plugin, plugin);
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(&manifest.script);
        assert!(script.is_file(), "{plugin}: {} não existe", manifest.script);
        assert!(manifest.script.starts_with(&format!("plugins/{plugin}/")));
    }
}

#[test]
fn every_plugin_folder_has_a_manifest() {
    for entry in fs::read_dir(plugins_directory()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().is_dir() || NOT_PLUGIN_FOLDERS.contains(&name.as_str()) {
            continue;
        }
        assert!(
            entry.path().join(MANIFEST_FILE_NAME).is_file(),
            "plugin '{name}' sem {MANIFEST_FILE_NAME}"
        );
    }
}

#[test]
fn defaults_are_valid_answers_and_presets_leave_no_placeholder_behind() {
    for plugin in SERVICE_PLUGINS {
        let manifest = manifest_of(plugin);
        for step in &manifest.steps {
            let Step::Question(question) = step else {
                continue;
            };
            if let Some(default) = &question.default {
                let normalized = question.kind.normalize(default);
                assert!(
                    normalized.is_ok(),
                    "{plugin}: default de {} inválido",
                    question.target.key()
                );
                if let AnswerKind::Choice(_) = question.kind {
                    assert_eq!(normalized.unwrap(), *default);
                }
            }
        }
        for preset in &manifest.presets {
            let placeholders: Vec<String> = preset
                .arguments
                .iter()
                .flat_map(|(_, template)| {
                    template
                        .split('{')
                        .skip(1)
                        .filter_map(|rest| rest.split('}').next().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .collect();
            let inputs: Vec<&str> = preset
                .inputs
                .iter()
                .map(|input| input.name.as_str())
                .collect();
            for placeholder in &placeholders {
                assert!(
                    inputs.contains(&placeholder.as_str()),
                    "{plugin}/{}: {{{placeholder}}} sem input",
                    preset.name
                );
            }
            for input in &inputs {
                assert!(
                    placeholders.iter().any(|name| name == input),
                    "{plugin}/{}: input {input} sem uso",
                    preset.name
                );
            }
        }
    }
}

#[test]
fn jira_preset_my_tasks_writes_the_documented_widget() {
    let result = interview(
        "jira",
        &[
            "1",
            "",
            "empresa.atlassian.net",
            "voce@empresa.com",
            "1",
            "JIRA_TOKEN_EMPRESA",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
        ],
    );
    assert_eq!(result.draft.id, "jira-minhas-tarefas");
    assert_eq!(
        result.draft.arguments,
        vec![
            "plugins/jira/jira.py",
            "--jql",
            "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
            "--title",
            "Minhas tarefas"
        ]
    );
    assert_eq!(
        environment(&result),
        vec![
            ("JIRA_SITE", "empresa.atlassian.net"),
            ("JIRA_EMAIL", "voce@empresa.com"),
            ("JIRA_TOKEN_ENV", "JIRA_TOKEN_EMPRESA"),
        ]
    );
    assert_eq!(result.draft.inherit_env, vec!["JIRA_TOKEN_EMPRESA"]);
    assert_eq!(
        (result.draft.interval_seconds, result.draft.timeout_seconds),
        (120, 45)
    );
}

#[test]
fn jira_table_view_asks_for_the_columns() {
    let result = interview(
        "jira",
        &[
            "",
            "jira",
            "s.atlassian.net",
            "a@b.c",
            "2",
            "~/.jira-token",
            "project = X",
            "Painel",
            "status",
            "table",
            "key,summary",
            "50",
            "",
            "",
        ],
    );
    assert_eq!(
        result.draft.arguments,
        vec![
            "plugins/jira/jira.py",
            "--jql",
            "project = X",
            "--title",
            "Painel",
            "--group-by",
            "status",
            "--view",
            "table",
            "--columns",
            "key,summary",
            "--max-issues",
            "50"
        ]
    );
    assert!(result.draft.inherit_env.is_empty());
}

#[test]
fn github_and_gitlab_ask_only_the_token_owners_and_title() {
    let github = interview(
        "github",
        &["", "1", "GITHUB_TOKEN_PRS", "minha-org, conta", "", "", ""],
    );
    assert_eq!(
        environment(&github),
        vec![
            ("GITHUB_TOKEN_ENV", "GITHUB_TOKEN_PRS"),
            ("GITHUB_OWNERS", "minha-org,conta")
        ]
    );
    let gitlab = interview("gitlab", &["", "1", "GITLAB_TOKEN_MRS", "", "", ""]);
    assert_eq!(
        environment(&gitlab),
        vec![("GITLAB_TOKEN_ENV", "GITLAB_TOKEN_MRS")]
    );
    assert_eq!(gitlab.draft.inherit_env, vec!["GITLAB_TOKEN_MRS"]);
}

#[test]
fn azure_devops_service_principal_collects_tenant_client_and_secret_name() {
    let result = interview(
        "azure-devops",
        &[
            "",
            "minha-org",
            "p1, p2",
            "service-principal",
            "tenant-guid",
            "client-guid",
            "1",
            "AZDO_SEGREDO",
            "",
            "",
            "",
        ],
    );
    assert_eq!(
        environment(&result),
        vec![
            ("AZDO_ORGANIZATION", "minha-org"),
            ("AZDO_PROJECTS", "p1,p2"),
            ("AZDO_AUTH_METHOD", "service-principal"),
            ("AZDO_TENANT_ID", "tenant-guid"),
            ("AZDO_CLIENT_ID", "client-guid"),
            ("AZDO_CLIENT_SECRET_ENV", "AZDO_SEGREDO"),
        ]
    );
    assert_eq!(result.draft.inherit_env, vec!["AZDO_SEGREDO"]);
}

#[test]
fn azure_devops_device_code_points_to_the_login_command() {
    let result = interview(
        "azure-devops",
        &[
            "",
            "minha-org",
            "",
            "device-code",
            "",
            "client-guid",
            "",
            "",
            "",
        ],
    );
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("azure_devops.py login")));
    assert!(result.draft.inherit_env.is_empty());
}

#[test]
fn azure_pipelines_with_the_default_method_asks_only_the_pipeline_settings() {
    let result = interview(
        "azure-pipelines",
        &[
            "",
            "minha-org",
            "meu-projeto",
            "30",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
        ],
    );
    assert_eq!(
        environment(&result),
        vec![
            ("AZDO_ORGANIZATION", "minha-org"),
            ("AZDO_PROJECTS", "meu-projeto"),
            ("AZDO_PIPELINE_DAYS", "30"),
        ]
    );
    assert_eq!(
        (result.draft.interval_seconds, result.draft.timeout_seconds),
        (900, 60)
    );
}

#[test]
fn calendar_ics_accepts_variables_and_a_file_together() {
    let result = interview(
        "calendar",
        &[
            "",
            "ics",
            "3",
            "CALENDAR_ICS_PERSONAL, CALENDAR_ICS_TRABALHO",
            "~/links.txt",
            "",
            "",
            "",
        ],
    );
    assert_eq!(
        environment(&result),
        vec![
            ("CALENDAR_SOURCE", "ics"),
            (
                "CALENDAR_ICS_URL_ENV",
                "CALENDAR_ICS_PERSONAL,CALENDAR_ICS_TRABALHO"
            ),
            ("CALENDAR_ICS_URL_FILE", "~/links.txt"),
        ]
    );
    assert_eq!(
        result.draft.inherit_env,
        vec!["CALENDAR_ICS_PERSONAL", "CALENDAR_ICS_TRABALHO"]
    );
}

#[test]
fn calendar_goa_inherits_the_session_bus() {
    let result = interview("calendar", &["", "goa", "", "", "", "", ""]);
    assert_eq!(environment(&result), vec![("CALENDAR_SOURCE", "goa")]);
    assert_eq!(result.draft.inherit_env, vec!["DBUS_SESSION_BUS_ADDRESS"]);
}

#[test]
fn every_manifest_variable_is_documented_in_the_plugin_readme() {
    for plugin in SERVICE_PLUGINS {
        let readme =
            fs::read_to_string(plugins_directory().join(plugin).join("README.md")).unwrap();
        for step in &manifest_of(plugin).steps {
            let keys: Vec<String> = match step {
                Step::Question(question) => vec![question.target.key().to_string()],
                Step::Secret(secret) => {
                    vec![secret.environment_key.clone(), secret.file_key.clone()]
                }
            };
            for key in keys {
                let documented = readme.contains(&format!("`{key}`"));
                let is_flag = matches!(step, Step::Question(question) if matches!(question.target, Target::Argument(_)));
                assert!(documented || is_flag, "{plugin}: {key} não está no README");
            }
        }
    }
}
