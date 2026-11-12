#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command as Process, Stdio};

use mosaic::application::ports::{SetupCatalog, WidgetConfigWriter};
use mosaic::application::setup_session::{PromptInput, SetupState};
use mosaic::infrastructure::setup_catalog::{FileSetupCatalog, FileWidgetConfigWriter};
use mosaic::presentation::app_state::{AddWidgetStep, AppState, Command, Mode};
use mosaic::presentation::key_input::KeyInput;

const MANIFEST: &str = r#"
version = "0.0.1"
min_mosaic_version = "0.0.10"
plugin = "demo"
script = "plugins/demo/demo.py"
default_id = "demo"
interval_seconds = 120
timeout_seconds = 30

[[steps]]
kind = "variable"
name = "DEMO_SITE"
prompt = "Site do demo"
required = true

[[steps]]
kind = "variable"
name = "DEMO_METHOD"
prompt = "Como autenticar"
type = "choice"
default = "cli"
choices = [
  { value = "cli", description = "usa o login da máquina" },
  { value = "token", description = "usa um token" },
]

[[steps]]
kind = "argument"
flag = "--title"
prompt = "Título do widget"
default = "Demo"

[[steps]]
kind = "variable"
name = "DEMO_DAYS"
prompt = "Dias de histórico"
type = "integer"
min = 1
max = 90

[[steps]]
kind = "secret"
prompt = "Token do demo"
environment_key = "DEMO_TOKEN_ENV"
file_key = "DEMO_TOKEN_FILE"
list = true
both = true
required = true

[[notes]]
text = "exporte o token antes de abrir o mosaic"
"#;
const OTHER_MANIFEST: &str = r#"
plugin = "alfa"
script = "plugins/alfa/alfa.py"
default_id = "alfa"
interval_seconds = 60
timeout_seconds = 10
"#;
const SINGLE_TAB_CONFIG: &str = "# meu painel\n[[widgets]]\nid = \"outro\"\ncommand = \"x\"\n";
const TWO_TABS_CONFIG: &str = "[[tabs]]\nname = \"Trabalho\"\n\n[[tabs]]\nname = \"Casa\"\n\n[[widgets]]\nid = \"outro\"\ncommand = \"x\"\ntab = \"Trabalho\"\n";

fn config_directory(config: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, manifest) in [("demo", MANIFEST), ("alfa", OTHER_MANIFEST)] {
        let plugin = directory.path().join("plugins").join(name);
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("setup.toml"), manifest).unwrap();
    }
    fs::write(directory.path().join("mosaic.toml"), config).unwrap();
    directory
}

fn config_text(directory: &Path) -> String {
    fs::read_to_string(directory.join("mosaic.toml")).unwrap()
}

fn terminal_flow(directory: &Path, answers: &[&str]) -> String {
    let mut input = answers.join("\n");
    input.push_str("\ns\n");
    let mut child = Process::new(env!("CARGO_BIN_EXE_mosaic"))
        .args(["plugins", "setup", "demo", "--config-dir"])
        .arg(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    config_text(directory)
}

fn open_modal(directory: &Path) -> AppState {
    let snapshot = FileSetupCatalog::new(directory.to_path_buf())
        .load()
        .unwrap();
    let mut state = AppState::new(Vec::new(), Vec::new());
    assert_eq!(
        state.handle_key(KeyInput::Character('+')),
        Some(Command::OpenAddWidget)
    );
    state.open_add_widget(snapshot);
    state
}

fn press(state: &mut AppState, keys: &[KeyInput]) -> Option<Command> {
    keys.iter().fold(None, |_, key| state.handle_key(*key))
}

struct CurrentPrompt {
    is_text: bool,
    typed_length: usize,
    row_answers: Vec<String>,
}

fn current_prompt(state: &AppState) -> CurrentPrompt {
    let Mode::AddingWidget(flow) = state.mode() else {
        panic!("o modal fechou");
    };
    let AddWidgetStep::Answering(answering) = flow.step() else {
        panic!("esperava perguntas");
    };
    let SetupState::Asking(prompt) = answering.state() else {
        panic!("a entrevista terminou");
    };
    CurrentPrompt {
        is_text: prompt.input == PromptInput::Text,
        typed_length: answering.field().text().chars().count(),
        row_answers: answering.rows().into_iter().map(|row| row.answer).collect(),
    }
}

fn answer_in_modal(state: &mut AppState, answer: &str) -> Option<Command> {
    let prompt = current_prompt(state);
    if prompt.is_text {
        press(state, &[KeyInput::End]);
        for _ in 0..prompt.typed_length {
            press(state, &[KeyInput::Backspace]);
        }
        for character in answer.chars() {
            press(state, &[KeyInput::Character(character)]);
        }
        return press(state, &[KeyInput::Enter]);
    }
    let wanted = match answer.parse::<usize>() {
        Ok(number) => prompt
            .row_answers
            .iter()
            .filter(|row| !row.is_empty())
            .nth(number - 1)
            .cloned()
            .unwrap(),
        Err(_) => answer.to_string(),
    };
    let row = prompt
        .row_answers
        .iter()
        .position(|candidate| *candidate == wanted)
        .unwrap();
    press(state, &[KeyInput::Home]);
    for _ in 0..row {
        press(state, &[KeyInput::Down]);
    }
    press(state, &[KeyInput::Enter])
}

fn modal_flow(directory: &Path, plugin_position: usize, answers: &[&str]) -> String {
    let mut state = open_modal(directory);
    for _ in 0..plugin_position {
        press(&mut state, &[KeyInput::Down]);
    }
    press(&mut state, &[KeyInput::Enter]);
    for answer in answers {
        assert_eq!(answer_in_modal(&mut state, answer), None, "{answer}");
    }
    let Some(Command::AppendWidget(widget)) = press(&mut state, &[KeyInput::Enter]) else {
        panic!("esperava o pedido de gravação");
    };
    FileWidgetConfigWriter::new(directory.to_path_buf())
        .append(&widget.blocks)
        .unwrap();
    config_text(directory)
}

const DEMO_POSITION: usize = 1;

fn assert_same_file(config: &str, answers: &[&str]) {
    let terminal = config_directory(config);
    let modal = config_directory(config);
    let from_terminal = terminal_flow(terminal.path(), answers);
    let from_modal = modal_flow(modal.path(), DEMO_POSITION, answers);
    assert_eq!(from_modal, from_terminal);
    assert_ne!(from_modal, config, "nada foi gravado");
}

#[test]
fn a_single_tab_panel_gets_the_same_widget_from_the_modal_and_the_terminal() {
    assert_same_file(
        SINGLE_TAB_CONFIG,
        &[
            "demo-um",
            "",
            "empresa.exemplo",
            "",
            "",
            "",
            "1",
            "DEMO_TOKEN_EMPRESA",
            "",
            "",
        ],
    );
}

#[test]
fn invalid_answers_are_corrected_in_both_flows_and_the_result_is_the_same() {
    assert_same_file(
        SINGLE_TAB_CONFIG,
        &[
            "Meu Widget",
            "outro",
            "demo-dois",
            "",
            "",
            "empresa.exemplo",
            "token",
            "Meu painel",
            "0",
            "14",
            "3",
            "a b",
            "LINK_A, LINK_B",
            "/tmp/links.txt",
            "5",
            "60",
            "0",
            "20",
        ],
    );
}

#[test]
fn choosing_a_declared_tab_writes_the_same_tab_key() {
    assert_same_file(
        TWO_TABS_CONFIG,
        &[
            "demo-casa",
            "2",
            "empresa.exemplo",
            "",
            "",
            "",
            "2",
            "~/.segredos/demo",
            "",
            "",
        ],
    );
}

#[test]
fn creating_a_new_tab_next_to_declared_ones_appends_the_tab_block_first() {
    assert_same_file(
        TWO_TABS_CONFIG,
        &[
            "demo-estudos",
            "3",
            "Casa",
            "Estudos",
            "empresa.exemplo",
            "",
            "",
            "",
            "1",
            "DEMO_TOKEN_EMPRESA",
            "",
            "",
        ],
    );
}

#[test]
fn declaring_the_first_tab_moves_the_existing_widgets_the_same_way() {
    assert_same_file(
        SINGLE_TAB_CONFIG,
        &[
            "demo-trabalho",
            "Trabalho",
            "empresa.exemplo",
            "",
            "",
            "",
            "1",
            "DEMO_TOKEN_EMPRESA",
            "",
            "",
        ],
    );
}

#[test]
fn escape_at_every_step_writes_nothing() {
    let answers = [
        "demo-um",
        "",
        "empresa.exemplo",
        "",
        "",
        "",
        "1",
        "DEMO_TOKEN_EMPRESA",
        "",
        "",
    ];
    for steps_answered in 0..=answers.len() {
        let directory = config_directory(SINGLE_TAB_CONFIG);
        let mut state = open_modal(directory.path());
        press(&mut state, &[KeyInput::Down, KeyInput::Enter]);
        for answer in &answers[..steps_answered] {
            answer_in_modal(&mut state, answer);
        }
        assert_eq!(press(&mut state, &[KeyInput::Escape]), None);
        assert_eq!(state.mode(), &Mode::Browsing);
        assert_eq!(config_text(directory.path()), SINGLE_TAB_CONFIG);
    }
}

#[test]
fn a_widget_id_taken_after_the_modal_opened_is_refused_and_nothing_is_written() {
    let directory = config_directory(SINGLE_TAB_CONFIG);
    let mut state = open_modal(directory.path());
    press(&mut state, &[KeyInput::Down, KeyInput::Enter]);
    let answers = [
        "demo-um",
        "",
        "empresa.exemplo",
        "",
        "",
        "",
        "1",
        "A",
        "",
        "",
    ];
    for answer in answers {
        answer_in_modal(&mut state, answer);
    }
    let taken = "[[widgets]]\nid = \"demo-um\"\ncommand = \"x\"\n";
    let current = format!("{SINGLE_TAB_CONFIG}\n{taken}");
    fs::write(directory.path().join("mosaic.toml"), &current).unwrap();
    let Some(Command::AppendWidget(widget)) = press(&mut state, &[KeyInput::Enter]) else {
        panic!("esperava o pedido de gravação");
    };
    let error = FileWidgetConfigWriter::new(directory.path().to_path_buf())
        .append(&widget.blocks)
        .unwrap_err();
    state.fail_adding_widget(error.to_string());
    assert!(matches!(state.mode(), Mode::AddingWidget(_)));
    assert_eq!(config_text(directory.path()), current);
}
