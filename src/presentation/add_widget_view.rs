use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::application::plugin_catalog::OFFICIAL_PLUGINS;
use crate::application::setup_session::{Prompt, PromptInput, SetupState};
use crate::domain::language::Language;
use crate::domain::plugin_setup::InstalledPlugin;
use crate::domain::widget_content::Tone;
use crate::presentation::app_state::{
    AddWidgetFlow, AddWidgetStep, Answering, ChoiceRow, Review, TextField,
};
use crate::presentation::messages;
use crate::presentation::text_fit::{display_width, truncate_to_width, wrap_to_width};
use crate::presentation::theme_styles::ThemeStyles;

pub const MODAL_WIDTH: u16 = 72;
const TITLE_SEPARATOR: &str = " · ";
const SELECTION_MARKER: &str = "> ";
const NO_SELECTION_MARKER: &str = "  ";
const INPUT_MARKER: &str = "> ";
const INDENT: &str = "  ";
const VERSION_PREFIX: &str = "v";
const COLUMN_GAP: &str = "  ";
const CUT_MARK: &str = "…";
const CURSOR_PLACEHOLDER: char = ' ';
const ERROR_PREFIX: &str = "! ";
const MORE_MARKER_ROWS: usize = 2;
const FOOTER_ROWS: usize = 2;
const HINT_SEPARATOR: &str = " · ";

pub struct ModalContent {
    pub title: String,
    pub lines: Vec<Line<'static>>,
}

#[derive(Clone, Copy)]
struct Dimensions {
    width: usize,
    height: usize,
}

struct Layout {
    title: String,
    body: Vec<Line<'static>>,
    footer: &'static str,
}

pub fn modal_content(
    flow: &AddWidgetFlow,
    width: usize,
    height: usize,
    styles: &ThemeStyles,
) -> ModalContent {
    let layout = match flow.step() {
        AddWidgetStep::ChoosingPlugin { selected } => {
            plugin_list(flow, *selected, Dimensions { width, height }, styles)
        }
        AddWidgetStep::Answering(answering) => {
            answering_layout(answering, Dimensions { width, height }, styles)
        }
        AddWidgetStep::Reviewing(review) => review_layout(review, width, styles),
    };
    ModalContent {
        title: layout.title,
        lines: fit_height(layout.body, layout.footer, height, styles),
    }
}

fn fit_height(
    mut body: Vec<Line<'static>>,
    footer: &str,
    height: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let room = height.saturating_sub(FOOTER_ROWS);
    if body.len() > room {
        body.truncate(room.saturating_sub(1));
        body.push(Line::styled(CUT_MARK, styles.muted()));
    }
    body.push(Line::default());
    body.push(Line::styled(footer.to_string(), styles.muted()));
    body
}

fn windowed(
    groups: Vec<Vec<Line<'static>>>,
    selected: usize,
    room: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let total: usize = groups.iter().map(Vec::len).sum();
    if total <= room {
        return groups.into_iter().flatten().collect();
    }
    let budget = room.saturating_sub(MORE_MARKER_ROWS).max(1);
    let selected = selected.min(groups.len().saturating_sub(1));
    let mut first = selected;
    let mut used = groups[selected].len();
    while first > 0 && used + groups[first - 1].len() <= budget {
        first -= 1;
        used += groups[first].len();
    }
    let mut last = selected + 1;
    while last < groups.len() && used + groups[last].len() <= budget {
        used += groups[last].len();
        last += 1;
    }
    let mut lines = Vec::new();
    if first > 0 {
        lines.push(Line::styled(
            messages::more_above(styles.language()),
            styles.muted(),
        ));
    }
    lines.extend(groups[first..last].iter().flatten().cloned());
    if last < groups.len() {
        lines.push(Line::styled(
            messages::more_below(styles.language()),
            styles.muted(),
        ));
    }
    lines
}

fn heading(text: &str, styles: &ThemeStyles) -> Line<'static> {
    Line::styled(
        text.to_string(),
        styles.accent().add_modifier(Modifier::BOLD),
    )
}

fn wrapped(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    text.lines()
        .flat_map(|line| wrap_to_width(line, width))
        .map(|line| Line::styled(line, style))
        .collect()
}

fn plugin_list(
    flow: &AddWidgetFlow,
    selected: usize,
    size: Dimensions,
    styles: &ThemeStyles,
) -> Layout {
    let width = size.width;
    if flow.plugins().is_empty() {
        return Layout {
            title: messages::add_widget_title(styles.language()).to_string(),
            body: no_plugins(flow, width, styles),
            footer: messages::empty_footer(styles.language()),
        };
    }
    let mut body = vec![
        heading(messages::choose_plugin_heading(styles.language()), styles),
        Line::default(),
    ];
    let problems = problem_lines(flow.problems(), width, styles);
    let room = size
        .height
        .saturating_sub(body.len() + problems.len() + FOOTER_ROWS);
    let groups = flow
        .plugins()
        .iter()
        .enumerate()
        .map(|(position, plugin)| vec![plugin_line(plugin, position == selected, width, styles)])
        .collect();
    body.extend(windowed(groups, selected, room, styles));
    body.extend(problems);
    Layout {
        title: messages::add_widget_title(styles.language()).to_string(),
        body,
        footer: messages::list_footer(styles.language()),
    }
}

fn plugin_line(
    plugin: &InstalledPlugin,
    selected: bool,
    width: usize,
    styles: &ThemeStyles,
) -> Line<'static> {
    let marker = if selected {
        SELECTION_MARKER
    } else {
        NO_SELECTION_MARKER
    };
    let name_style = if selected {
        styles.tone(Tone::Current)
    } else {
        Style::default()
    };
    let mut spans = vec![Span::styled(format!("{marker}{}", plugin.name), name_style)];
    if let Some(version) = &plugin.version {
        spans.push(Span::styled(
            format!("{COLUMN_GAP}{VERSION_PREFIX}{version}"),
            styles.muted(),
        ));
    }
    clip_line(spans, width)
}

fn clip_line(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let mut remaining = width;
    let mut clipped = Vec::new();
    for span in spans {
        if remaining == 0 {
            break;
        }
        let shown = truncate_to_width(&span.content, remaining);
        remaining = remaining.saturating_sub(display_width(&shown));
        clipped.push(Span::styled(shown, span.style));
    }
    Line::from(clipped)
}

fn problem_lines(problems: &[String], width: usize, styles: &ThemeStyles) -> Vec<Line<'static>> {
    if problems.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::default()];
    lines.extend(problems.iter().flat_map(|problem| {
        wrapped(
            &format!("{}{problem}", messages::ignored_prefix(styles.language())),
            width,
            styles.warn(),
        )
    }));
    lines
}

fn no_plugins(flow: &AddWidgetFlow, width: usize, styles: &ThemeStyles) -> Vec<Line<'static>> {
    let mut lines = vec![
        heading(messages::no_plugin_heading(styles.language()), styles),
        Line::from(messages::install_hint(styles.language())),
        Line::styled(
            format!("{INDENT}{}", messages::install_command(styles.language())),
            styles.accent(),
        ),
        Line::default(),
        Line::from(messages::official_plugins_heading(styles.language())),
    ];
    lines.extend(wrapped(
        &format!("{INDENT}{}", OFFICIAL_PLUGINS.join(", ")),
        width,
        styles.muted(),
    ));
    lines.extend(problem_lines(flow.problems(), width, styles));
    lines
}

fn answering_layout(answering: &Answering, size: Dimensions, styles: &ThemeStyles) -> Layout {
    let width = size.width;
    let title = format!(
        "{}{TITLE_SEPARATOR}{}",
        messages::add_widget_title(styles.language()),
        answering.plugin_name()
    );
    let SetupState::Asking(prompt) = answering.state() else {
        return Layout {
            title,
            body: Vec::new(),
            footer: messages::text_footer(styles.language()),
        };
    };
    let mut body = vec![heading(&prompt.title, styles)];
    if let Some(explanation) = &prompt.explanation {
        body.extend(wrapped(explanation, width, styles.muted()));
    }
    body.push(Line::styled(
        truncate_to_width(&hint_text(prompt, styles.language()), width),
        styles.muted(),
    ));
    body.push(Line::default());
    let errors = error_lines(answering.error(), width, styles);
    let footer = match &prompt.input {
        PromptInput::Text => {
            body.push(input_line(answering.field(), prompt, width, styles));
            messages::text_footer(styles.language())
        }
        PromptInput::Choice(_) => {
            let room = size
                .height
                .saturating_sub(body.len() + errors.len() + FOOTER_ROWS);
            body.extend(choice_lines(
                &answering.rows(),
                answering.selected_row(),
                width,
                room,
                styles,
            ));
            messages::choice_footer(styles.language())
        }
    };
    body.extend(errors);
    Layout {
        title,
        body,
        footer,
    }
}

fn error_lines(message: Option<&str>, width: usize, styles: &ThemeStyles) -> Vec<Line<'static>> {
    let Some(message) = message else {
        return Vec::new();
    };
    let mut lines = vec![Line::default()];
    lines.extend(wrapped(
        &format!("{ERROR_PREFIX}{message}"),
        width,
        styles.danger(),
    ));
    lines
}

fn hint_text(prompt: &Prompt, language: Language) -> String {
    let mut parts = Vec::new();
    if prompt.required {
        parts.push(messages::required_hint(language).to_string());
    } else if let Some(default) = prompt
        .default
        .as_ref()
        .filter(|_| prompt.input == PromptInput::Text)
    {
        parts.push(messages::default_hint(language, default));
    } else if prompt.blank.is_none() {
        parts.push(messages::optional_hint(language).to_string());
    }
    parts.extend(prompt.hint.clone());
    parts.join(HINT_SEPARATOR)
}

fn input_line(
    field: &TextField,
    prompt: &Prompt,
    width: usize,
    styles: &ThemeStyles,
) -> Line<'static> {
    let budget = width.saturating_sub(display_width(INPUT_MARKER));
    let (before, at, after) = visible_window(field, budget);
    let cursor_style = Style::default().add_modifier(Modifier::REVERSED);
    let mut spans = vec![
        Span::styled(INPUT_MARKER, styles.accent()),
        Span::raw(before),
        Span::styled(at.to_string(), cursor_style),
        Span::raw(after),
    ];
    if let Some(default) = prompt.default.as_ref().filter(|_| field.text().is_empty()) {
        spans.push(Span::styled(
            truncate_to_width(default, budget.saturating_sub(1)),
            styles.muted(),
        ));
    }
    Line::from(spans)
}

fn visible_window(field: &TextField, budget: usize) -> (String, char, String) {
    let characters: Vec<char> = field.text().chars().collect();
    let cursor = field.cursor().min(characters.len());
    let at = characters
        .get(cursor)
        .copied()
        .unwrap_or(CURSOR_PLACEHOLDER);
    let mut remaining = budget.saturating_sub(1);
    let mut start = cursor;
    while start > 0 {
        let needed = display_width(&characters[start - 1].to_string());
        if needed > remaining {
            break;
        }
        remaining -= needed;
        start -= 1;
    }
    let mut end = cursor + 1;
    while end <= characters.len() && end < characters.len() {
        let needed = display_width(&characters[end].to_string());
        if needed > remaining {
            break;
        }
        remaining -= needed;
        end += 1;
    }
    let before: String = characters[start..cursor].iter().collect();
    let after: String = characters
        .get(cursor + 1..end.max(cursor + 1))
        .unwrap_or(&[])
        .iter()
        .collect();
    (before, at, after)
}

fn choice_lines(
    rows: &[ChoiceRow],
    selected: usize,
    width: usize,
    room: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let groups = rows
        .iter()
        .enumerate()
        .map(|(position, row)| choice_row_lines(row, position == selected, width, styles))
        .collect();
    windowed(groups, selected, room, styles)
}

fn choice_row_lines(
    row: &ChoiceRow,
    selected: bool,
    width: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let marker = if selected {
        SELECTION_MARKER
    } else {
        NO_SELECTION_MARKER
    };
    let label_style = if selected {
        styles.tone(Tone::Current)
    } else {
        Style::default()
    };
    let mut lines = vec![clip_line(
        vec![Span::styled(format!("{marker}{}", row.label), label_style)],
        width,
    )];
    if !row.description.is_empty() {
        let indent_width = display_width(NO_SELECTION_MARKER) + display_width(INDENT);
        lines.extend(
            wrap_to_width(&row.description, width.saturating_sub(indent_width))
                .into_iter()
                .map(|line| {
                    Line::styled(
                        format!("{NO_SELECTION_MARKER}{INDENT}{line}"),
                        styles.muted(),
                    )
                }),
        );
    }
    lines
}

fn review_layout(review: &Review, width: usize, styles: &ThemeStyles) -> Layout {
    let mut body = vec![
        heading(messages::review_heading(styles.language()), styles),
        Line::default(),
    ];
    for (position, block) in review.blocks().iter().enumerate() {
        if position > 0 {
            body.push(Line::default());
        }
        body.extend(
            block
                .lines()
                .flat_map(|line| wrap_to_width(line, width).into_iter().map(Line::from)),
        );
    }
    for note in review.notes() {
        body.push(Line::default());
        body.extend(wrapped(
            &format!("{}{note}", messages::note_prefix(styles.language())),
            width,
            styles.warn(),
        ));
    }
    if let Some(message) = review.error() {
        body.push(Line::default());
        body.extend(wrapped(
            &format!(
                "{}{message}",
                messages::save_failed_prefix(styles.language())
            ),
            width,
            styles.danger(),
        ));
    }
    Layout {
        title: format!(
            "{}{TITLE_SEPARATOR}{}{TITLE_SEPARATOR}{}",
            messages::add_widget_title(styles.language()),
            review.plugin_name(),
            messages::review_title_suffix(styles.language())
        ),
        body,
        footer: messages::review_footer(styles.language()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::SetupCatalogSnapshot;
    use crate::domain::color_depth::ColorDepth;
    use crate::domain::plugin_setup::{
        AnswerKind, Choice, InheritRule, Note, Question, Requirement, SecretQuestion,
        SetupManifest, Step, Target,
    };
    use crate::domain::theme::Theme;
    use crate::presentation::app_state::AppState;
    use crate::presentation::key_input::KeyInput;
    use crate::presentation::render::render;
    use chrono::NaiveDate;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;
    use ratatui::Terminal;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 28;

    fn now() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 12)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    }

    fn demo_manifest() -> SetupManifest {
        let site = Step::Question(Question {
            target: Target::Variable("DEMO_SITE".to_string()),
            prompt: "Host do site do demo".to_string(),
            kind: AnswerKind::Text,
            requirement: Requirement::Always,
            default: None,
            advanced: false,
            condition: None,
        });
        let method = Step::Question(Question {
            target: Target::Variable("DEMO_METHOD".to_string()),
            prompt: "Como autenticar".to_string(),
            kind: AnswerKind::Choice(vec![
                Choice {
                    value: "cli".to_string(),
                    description: "usa o login da sua máquina".to_string(),
                },
                Choice {
                    value: "token".to_string(),
                    description: "usa um token de API".to_string(),
                },
            ]),
            requirement: Requirement::Optional,
            default: Some("cli".to_string()),
            advanced: false,
            condition: None,
        });
        let days = Step::Question(Question {
            target: Target::Variable("DEMO_DAYS".to_string()),
            prompt: "Dias de histórico".to_string(),
            kind: AnswerKind::Integer {
                minimum: 1,
                maximum: 90,
            },
            requirement: Requirement::Optional,
            default: None,
            advanced: false,
            condition: None,
        });
        let token = Step::Secret(SecretQuestion {
            prompt: "Token do demo".to_string(),
            environment_key: "DEMO_TOKEN_ENV".to_string(),
            file_key: "DEMO_TOKEN_FILE".to_string(),
            names_are_list: false,
            allow_both: false,
            requirement: Requirement::Optional,
            condition: None,
        });
        SetupManifest {
            plugin: "demo".to_string(),
            script: "plugins/demo/demo.py".to_string(),
            default_id: "demo".to_string(),
            interval_seconds: 120,
            timeout_seconds: 30,
            steps: vec![site, method, days, token],
            inherit_rules: vec![InheritRule {
                name: "HOME".to_string(),
                condition: None,
            }],
            presets: Vec::new(),
            notes: vec![Note {
                text: "exporte o token antes de abrir o mosaic".to_string(),
                condition: None,
            }],
        }
    }

    fn installed(name: &str, version: Option<&str>) -> InstalledPlugin {
        InstalledPlugin {
            name: name.to_string(),
            version: version.map(str::to_string),
            manifest: demo_manifest(),
        }
    }

    fn snapshot(plugins: Vec<InstalledPlugin>) -> SetupCatalogSnapshot {
        SetupCatalogSnapshot {
            plugins,
            ..SetupCatalogSnapshot::default()
        }
    }

    fn state_with(snapshot: SetupCatalogSnapshot) -> AppState {
        let mut state =
            AppState::new(Vec::new(), Vec::new()).with_language(Language::BrazilianPortuguese);
        state.open_add_widget(snapshot);
        state
    }

    fn demo_state() -> AppState {
        state_with(snapshot(vec![
            installed("azure-devops", Some("0.0.4")),
            installed("demo", Some("0.0.9")),
        ]))
    }

    fn press(state: &mut AppState, keys: &[KeyInput]) {
        for key in keys {
            state.handle_key(*key);
        }
    }

    fn type_text(state: &mut AppState, text: &str) {
        for character in text.chars() {
            state.handle_key(KeyInput::Character(character));
        }
    }

    fn buffer_of(state: &AppState, width: u16, height: u16) -> Buffer {
        let spans = vec![1; state.entries().len()];
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, state, &spans, now()))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn screen_of(buffer: &Buffer) -> String {
        let area = buffer.area;
        (0..area.height)
            .map(|row| {
                (0..area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn screen(state: &AppState) -> String {
        screen_of(&buffer_of(state, WIDTH, HEIGHT))
    }

    fn cell_of<'buffer>(buffer: &'buffer Buffer, needle: &str) -> &'buffer ratatui::buffer::Cell {
        let area = buffer.area;
        for row in 0..area.height {
            let symbols: Vec<&str> = (0..area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect();
            for start in 0..symbols.len() {
                if symbols[start..].concat().starts_with(needle) {
                    return &buffer[(u16::try_from(start).unwrap(), row)];
                }
            }
        }
        panic!("não achei {needle:?} na tela:\n{}", screen_of(buffer));
    }

    fn themed(name: &str) -> (AppState, ThemeStyles) {
        let theme = Theme::preset(name).unwrap();
        let styles = ThemeStyles::new(&theme, ColorDepth::TrueColor);
        let mut state = demo_state();
        state.set_theme_styles(styles.clone());
        (state, styles)
    }

    fn rgb_of(
        name: &str,
        role: impl Fn(&crate::domain::theme::Palette) -> crate::domain::theme::Rgb,
    ) -> Color {
        let Theme::Colored { palette, .. } = Theme::preset(name).unwrap() else {
            panic!("{name} é ANSI");
        };
        let rgb = role(&palette);
        Color::Rgb(rgb.red, rgb.green, rgb.blue)
    }

    #[test]
    fn the_plugin_list_shows_names_versions_and_the_selection() {
        let mut state = demo_state();
        let text = screen(&state);
        assert!(text.contains("Adicionar widget"), "{text}");
        assert!(text.contains("Escolha o plugin do widget"), "{text}");
        assert!(text.contains("> azure-devops  v0.0.4"), "{text}");
        assert!(text.contains("  demo  v0.0.9"), "{text}");
        assert!(text.contains("Enter seleciona  Esc cancela"), "{text}");
        press(&mut state, &[KeyInput::Down]);
        let text = screen(&state);
        assert!(text.contains("> demo  v0.0.9"), "{text}");
    }

    #[test]
    fn unreadable_plugins_are_listed_as_ignored() {
        let mut snapshot = snapshot(vec![installed("demo", None)]);
        snapshot.problems = vec!["quebrado/setup.toml: roteiro inválido".to_string()];
        let text = screen(&state_with(snapshot));
        assert!(text.contains("ignorado: quebrado/setup.toml"), "{text}");
        assert!(text.contains("> demo"), "{text}");
    }

    #[test]
    fn an_empty_list_shows_the_install_command_and_the_official_names() {
        let text = screen(&state_with(snapshot(Vec::new())));
        assert!(text.contains("Nenhum plugin instalado."), "{text}");
        assert!(text.contains("mosaic plugins install <nome>"), "{text}");
        assert!(text.contains("azure-devops, azure-pipelines"), "{text}");
        assert!(text.contains("github, gitlab, jira"), "{text}");
        assert!(text.contains("Esc fecha"), "{text}");
    }

    #[test]
    fn the_first_question_is_the_widget_id_with_its_default_and_a_cursor() {
        let mut state = demo_state();
        press(&mut state, &[KeyInput::Down, KeyInput::Enter]);
        let buffer = buffer_of(&state, WIDTH, HEIGHT);
        let text = screen_of(&buffer);
        assert!(text.contains("Adicionar widget · demo"), "{text}");
        assert!(text.contains("Id do widget"), "{text}");
        assert!(text.contains("Enter usa o padrão (demo)"), "{text}");
        assert!(text.contains("letras minúsculas"), "{text}");
        assert!(text.contains(">  demo"), "{text}");
        assert!(text.contains("Enter confirma  Esc cancela"), "{text}");
        let cursors = buffer
            .content()
            .iter()
            .filter(|cell| cell.modifier.contains(Modifier::REVERSED))
            .count();
        assert_eq!(cursors, 1);
    }

    #[test]
    fn typed_text_replaces_the_placeholder_and_the_cursor_follows() {
        let mut state = demo_state();
        press(&mut state, &[KeyInput::Enter]);
        type_text(&mut state, "meu-jira");
        press(&mut state, &[KeyInput::Left, KeyInput::Left]);
        let text = screen(&state);
        assert!(text.contains("> meu-jira"), "{text}");
        assert!(!text.contains("(demo)\n"), "{text}");
    }

    #[test]
    fn a_long_answer_scrolls_to_keep_the_cursor_visible() {
        let mut state = demo_state();
        press(&mut state, &[KeyInput::Enter]);
        type_text(&mut state, &"a".repeat(120));
        let text = screen(&state);
        let line = text.lines().find(|line| line.contains("> aaa")).unwrap();
        assert!(line.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!text.contains(&"a".repeat(100)), "{text}");
    }

    #[test]
    fn a_validation_error_appears_in_the_modal_and_keeps_the_question() {
        let mut state = demo_state();
        press(&mut state, &[KeyInput::Enter]);
        type_text(&mut state, "Meu Widget");
        press(&mut state, &[KeyInput::Enter]);
        let text = screen(&state);
        assert!(
            text.contains("! Use só letras minúsculas, números, '-' e '_'."),
            "{text}"
        );
        assert!(text.contains("> Meu Widget"), "{text}");
        assert!(text.contains("Id do widget"), "{text}");
    }

    #[test]
    fn a_required_question_and_an_integer_question_show_their_hints() {
        let mut state = demo_state();
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        let text = screen(&state);
        assert!(text.contains("Host do site do demo"), "{text}");
        assert!(text.contains("obrigatório"), "{text}");
        type_text(&mut state, "empresa.exemplo");
        press(&mut state, &[KeyInput::Enter, KeyInput::Enter]);
        let text = screen(&state);
        assert!(text.contains("Dias de histórico"), "{text}");
        assert!(text.contains("de 1 a 90"), "{text}");
        assert!(text.contains("opcional: Enter pula"), "{text}");
        type_text(&mut state, "0");
        press(&mut state, &[KeyInput::Enter]);
        let text = screen(&state);
        assert!(
            text.contains("! o valor precisa estar entre 1 e 90"),
            "{text}"
        );
    }

    #[test]
    fn a_choice_question_lists_the_default_row_and_the_options_with_descriptions() {
        let mut state = demo_state();
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        type_text(&mut state, "empresa.exemplo");
        press(&mut state, &[KeyInput::Enter]);
        let text = screen(&state);
        assert!(text.contains("Como autenticar"), "{text}");
        assert!(text.contains("> cli (padrão)"), "{text}");
        assert!(text.contains("  cli"), "{text}");
        assert!(text.contains("usa o login da sua máquina"), "{text}");
        assert!(text.contains("  token"), "{text}");
        assert!(
            text.contains("↑↓ escolhe  Enter confirma  Esc cancela"),
            "{text}"
        );
        press(&mut state, &[KeyInput::Down, KeyInput::Down]);
        let text = screen(&state);
        assert!(text.contains("> token"), "{text}");
    }

    #[test]
    fn a_secret_question_explains_that_the_value_is_never_asked() {
        let mut state = demo_state();
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        type_text(&mut state, "empresa.exemplo");
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        let text = screen(&state);
        assert!(text.contains("Token do demo"), "{text}");
        assert!(text.contains("O valor nunca é pedido aqui"), "{text}");
        assert!(text.contains("nome de variável de ambiente"), "{text}");
        assert!(text.contains("caminho de um arquivo"), "{text}");
    }

    fn reviewing_state() -> AppState {
        let mut state = demo_state();
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        type_text(&mut state, "empresa.exemplo");
        press(&mut state, &[KeyInput::Enter; 7]);
        state
    }

    #[test]
    fn the_review_shows_the_block_the_notes_and_the_save_hint() {
        let text = screen(&reviewing_state());
        assert!(
            text.contains("Adicionar widget · azure-devops · revisar"),
            "{text}"
        );
        assert!(
            text.contains("Este trecho será acrescentado ao mosaic.toml:"),
            "{text}"
        );
        assert!(text.contains("[[widgets]]"), "{text}");
        assert!(text.contains("id = \"demo\""), "{text}");
        assert!(text.contains("DEMO_SITE = \"empresa.exemplo\""), "{text}");
        assert!(text.contains("Atenção: exporte o token"), "{text}");
        assert!(
            text.contains("Enter grava e recarrega  Esc cancela"),
            "{text}"
        );
    }

    #[test]
    fn a_save_error_is_shown_in_the_review() {
        let mut state = reviewing_state();
        state.fail_adding_widget("o mosaic.toml mudou".to_string());
        let text = screen(&state);
        assert!(text.contains("Não gravou: o mosaic.toml mudou"), "{text}");
    }

    #[test]
    fn closing_the_flow_removes_the_modal() {
        let mut state = demo_state();
        press(&mut state, &[KeyInput::Escape]);
        assert!(!screen(&state).contains("Adicionar widget"));
    }

    #[test]
    fn a_short_terminal_keeps_the_key_hints_and_cuts_the_body() {
        let state = reviewing_state();
        let text = screen_of(&buffer_of(&state, WIDTH, 10));
        assert!(text.contains("Enter grava e recarrega"), "{text}");
        assert!(text.contains("…"), "{text}");
    }

    #[test]
    fn a_narrow_terminal_does_not_panic_and_clips_the_lines() {
        let state = reviewing_state();
        for width in [3, 8, 20, 40] {
            let text = screen_of(&buffer_of(&state, width, HEIGHT));
            assert!(text
                .lines()
                .all(|line| line.chars().count() <= usize::from(width)));
        }
    }

    #[test]
    fn the_modal_paints_the_theme_background_on_dark_and_light_themes() {
        for name in ["nord", "solarized-light"] {
            let (state, _) = themed(name);
            let buffer = buffer_of(&state, WIDTH, HEIGHT);
            let background = rgb_of(name, |palette| palette.background);
            let foreground = rgb_of(name, |palette| palette.foreground);
            let heading = cell_of(&buffer, "Escolha o plugin");
            assert_eq!(heading.bg, background, "{name}");
            let plugin = cell_of(&buffer, "  demo");
            assert_eq!(plugin.bg, background, "{name}");
            assert_eq!(plugin.fg, foreground, "{name}");
        }
    }

    #[test]
    fn selection_error_and_note_use_the_theme_roles() {
        for name in ["dracula", "solarized-light"] {
            let (mut state, _) = themed(name);
            let buffer = buffer_of(&state, WIDTH, HEIGHT);
            let selected = cell_of(&buffer, "> azure-devops");
            assert_eq!(
                selected.fg,
                rgb_of(name, |palette| palette.accent),
                "{name}"
            );
            press(&mut state, &[KeyInput::Enter]);
            type_text(&mut state, "Meu Widget");
            press(&mut state, &[KeyInput::Enter]);
            let buffer = buffer_of(&state, WIDTH, HEIGHT);
            let error = cell_of(&buffer, "! Use só");
            assert_eq!(error.fg, rgb_of(name, |palette| palette.danger), "{name}");
            let hint = cell_of(&buffer, "Enter confirma");
            assert_eq!(hint.fg, rgb_of(name, |palette| palette.muted), "{name}");
        }
    }

    #[test]
    fn the_review_note_uses_the_warning_role() {
        let (mut state, _) = themed("solarized-light");
        press(
            &mut state,
            &[KeyInput::Enter, KeyInput::Enter, KeyInput::Enter],
        );
        type_text(&mut state, "x");
        press(&mut state, &[KeyInput::Enter; 7]);
        let buffer = buffer_of(&state, WIDTH, HEIGHT);
        let note = cell_of(&buffer, "Atenção: exporte");
        assert_eq!(note.fg, rgb_of("solarized-light", |palette| palette.warn));
    }

    #[test]
    fn the_default_ansi_theme_keeps_ansi_colors_in_the_modal() {
        let state = demo_state();
        let buffer = buffer_of(&state, WIDTH, HEIGHT);
        assert_eq!(cell_of(&buffer, "> azure-devops").fg, Color::Cyan);
        assert_eq!(cell_of(&buffer, "Enter seleciona").fg, Color::DarkGray);
    }

    #[test]
    fn the_empty_panel_points_to_the_plus_key() {
        let state =
            AppState::new(Vec::new(), Vec::new()).with_language(Language::BrazilianPortuguese);
        let text = screen_of(&buffer_of(&state, WIDTH, HEIGHT));
        assert!(text.contains("nenhum widget ainda"), "{text}");
        assert!(
            text.contains("pressione + para adicionar um widget"),
            "{text}"
        );
        assert!(text.contains("mosaic plugins list"), "{text}");
    }

    #[test]
    fn the_empty_panel_hint_is_themed_and_hidden_once_a_widget_exists() {
        let theme = Theme::preset("nord").unwrap();
        let mut state =
            AppState::new(Vec::new(), Vec::new()).with_language(Language::BrazilianPortuguese);
        state.set_theme_styles(ThemeStyles::new(&theme, ColorDepth::TrueColor));
        let buffer = buffer_of(&state, WIDTH, HEIGHT);
        let title = cell_of(&buffer, "nenhum widget ainda");
        assert_eq!(title.fg, rgb_of("nord", |palette| palette.accent));
        let with_widget = AppState::new(
            vec![crate::presentation::app_state::WidgetEntry {
                id: crate::domain::widget::WidgetId::new("um"),
                record: crate::domain::widget::WidgetRecord::loading(),
                interval: std::time::Duration::from_secs(30),
                max_items: None,
            }],
            vec![crate::presentation::app_state::TabView {
                name: "Mosaic".to_string(),
                entry_indexes: vec![0],
            }],
        )
        .with_language(Language::BrazilianPortuguese);
        let text = screen_of(&buffer_of(&with_widget, WIDTH, HEIGHT));
        assert!(!text.contains("nenhum widget ainda"), "{text}");
    }
    fn many_presets_state(count: usize) -> AppState {
        let mut manifest = demo_manifest();
        manifest.presets = (0..count)
            .map(|number| crate::domain::plugin_setup::Preset {
                name: format!("modelo-{number:02}"),
                description: format!("descrição do modelo {number}"),
                widget_id: None,
                arguments: Vec::new(),
                inputs: Vec::new(),
            })
            .collect();
        let plugin = InstalledPlugin {
            name: "demo".to_string(),
            version: None,
            manifest,
        };
        let mut state = state_with(snapshot(vec![plugin]));
        press(&mut state, &[KeyInput::Enter]);
        state
    }

    fn box_corners(text: &str) -> (usize, usize) {
        (text.matches('┌').count(), text.matches('└').count())
    }

    #[test]
    fn a_choice_list_taller_than_the_terminal_scrolls_and_keeps_the_selection_visible() {
        let mut state = many_presets_state(12);
        let text = screen_of(&buffer_of(&state, WIDTH, 16));
        assert!(text.contains("> montar do zero"), "{text}");
        assert!(text.contains("↓ mais"), "{text}");
        assert!(!text.contains("↑ mais"), "{text}");
        assert!(text.contains("Enter confirma  Esc cancela"), "{text}");
        for _ in 0..12 {
            press(&mut state, &[KeyInput::Down]);
        }
        let text = screen_of(&buffer_of(&state, WIDTH, 16));
        assert!(text.contains("> modelo-11"), "{text}");
        assert!(text.contains("↑ mais"), "{text}");
        assert!(!text.contains("↓ mais"), "{text}");
        assert!(text.contains("Enter confirma  Esc cancela"), "{text}");
        assert_eq!(box_corners(&text), (1, 1), "{text}");
    }

    #[test]
    fn a_selection_in_the_middle_shows_markers_on_both_sides() {
        let mut state = many_presets_state(12);
        for _ in 0..6 {
            press(&mut state, &[KeyInput::Down]);
        }
        let text = screen_of(&buffer_of(&state, WIDTH, 16));
        assert!(text.contains("> modelo-05"), "{text}");
        assert!(text.contains("↑ mais") && text.contains("↓ mais"), "{text}");
    }

    #[test]
    fn a_plugin_list_taller_than_the_terminal_keeps_the_selection_visible() {
        let plugins = (0..15)
            .map(|number| installed(&format!("plugin-{number:02}"), None))
            .collect();
        let mut state = state_with(snapshot(plugins));
        for _ in 0..14 {
            press(&mut state, &[KeyInput::Down]);
        }
        let text = screen_of(&buffer_of(&state, WIDTH, 12));
        assert!(text.contains("> plugin-14"), "{text}");
        assert!(text.contains("Enter seleciona  Esc cancela"), "{text}");
        assert_eq!(box_corners(&text), (1, 1), "{text}");
    }
}
