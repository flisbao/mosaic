use crate::domain::language::Language;

pub fn required_answer(language: Language) -> &'static str {
    language.pick("This answer is required.", "Esta resposta é obrigatória.")
}

pub fn unknown_number(language: Language) -> &'static str {
    language.pick(
        "Answer with one of the listed numbers.",
        "Responda com um dos números listados.",
    )
}

pub fn unknown_preset(language: Language) -> &'static str {
    language.pick(
        "Unknown template; answer with one of the listed numbers.",
        "Modelo desconhecido; responda com um dos números listados.",
    )
}

pub fn invalid_widget_id(language: Language) -> &'static str {
    language.pick(
        "Use only lowercase letters, numbers, '-' and '_'.",
        "Use só letras minúsculas, números, '-' e '_'.",
    )
}

pub fn secret_explanation(language: Language) -> &'static str {
    language.pick(
        "The value is never asked for here: the plugin reads it from an environment variable or from a file.",
        "O valor nunca é pedido aqui: o plugin lê de uma variável de ambiente ou de um arquivo.",
    )
}

pub fn skip_label(language: Language) -> &'static str {
    language.pick("skip", "pular")
}

pub fn from_scratch_label(language: Language) -> &'static str {
    language.pick("build from scratch", "montar do zero")
}

pub fn new_tab_label(language: Language) -> &'static str {
    language.pick("create a new tab", "criar uma aba nova")
}

pub fn interval_title(language: Language) -> &'static str {
    language.pick(
        "Interval between refreshes, in seconds",
        "Intervalo entre atualizações, em segundos",
    )
}

pub fn timeout_title(language: Language) -> &'static str {
    language.pick(
        "Maximum time of each run, in seconds",
        "Tempo máximo de cada execução, em segundos",
    )
}

pub fn list_hint(language: Language) -> &'static str {
    language.pick("separated by commas", "separados por vírgula")
}

pub fn widget_id_taken(language: Language, id: &str) -> String {
    language.pick(
        format!("A widget with the id \"{id}\" already exists."),
        format!("Já existe um widget com o id \"{id}\"."),
    )
}

pub fn tab_name_taken(language: Language, name: &str) -> String {
    language.pick(
        format!("A tab named \"{name}\" already exists."),
        format!("Já existe uma aba chamada \"{name}\"."),
    )
}

pub fn new_tab_name_title(language: Language) -> &'static str {
    language.pick("New tab name", "Nome da aba nova")
}

pub fn new_tab_name_label(language: Language) -> &'static str {
    language.pick("New tab name: ", "Nome da aba nova: ")
}

pub fn file_path_title(language: Language) -> &'static str {
    language.pick("File path", "Caminho do arquivo")
}

pub fn file_path_label(language: Language, prompt: &str) -> String {
    language.pick(
        format!("File path ({prompt}): "),
        format!("Caminho do arquivo ({prompt}): "),
    )
}

pub fn first_tab_note(language: Language, widgets: usize) -> String {
    language.pick(
        format!("when you declare the first tab, the {widgets} existing widgets without 'tab' move into it (the first declared tab)"),
        format!("ao declarar a primeira aba, os {widgets} widgets que já existem sem 'tab' passam a ficar nela (a primeira aba declarada)"),
    )
}

pub fn preset_title(language: Language) -> &'static str {
    language.pick("Ready-made template", "Modelo pronto")
}

pub fn preset_label(language: Language) -> &'static str {
    language.pick(
        "Template (Enter to build from scratch): ",
        "Modelo (Enter para montar do zero): ",
    )
}

pub fn presets_introduction(language: Language, listing: &str) -> String {
    language.pick(
        format!("Ready-made templates:\n{listing}"),
        format!("Modelos prontos:\n{listing}"),
    )
}

pub fn widget_id_title(language: Language) -> &'static str {
    language.pick("Widget id", "Id do widget")
}

pub fn widget_id_label(language: Language, default: &str) -> String {
    language.pick(
        format!("Widget id [{default}]: "),
        format!("Id do widget [{default}]: "),
    )
}

pub fn widget_id_hint(language: Language) -> &'static str {
    language.pick(
        "lowercase letters, numbers, - and _",
        "letras minúsculas, números, - e _",
    )
}

pub fn first_tab_explanation(language: Language, moves_existing: bool) -> String {
    language.pick(
        format!(
            "mosaic.toml does not declare tabs yet: all widgets live in a single tab. Typing a name creates the first declared tab{}.",
            if moves_existing { " and moves the existing widgets into it" } else { "" }
        ),
        format!(
            "O mosaic.toml ainda não declara abas: todos os widgets ficam numa aba única. Digitar um nome cria a primeira aba declarada{}.",
            if moves_existing { " e move para ela os widgets que já existem" } else { "" }
        ),
    )
}

pub fn widget_tab_title(language: Language) -> &'static str {
    language.pick("Widget tab", "Aba do widget")
}

pub fn first_tab_label(language: Language) -> &'static str {
    language.pick(
        "New tab for this widget (Enter = keep the single tab): ",
        "Aba nova para este widget (Enter = manter a aba única): ",
    )
}

pub fn panel_tabs_introduction(language: Language, listing: &str) -> String {
    language.pick(
        format!("Panel tabs:\n{listing}"),
        format!("Abas do painel:\n{listing}"),
    )
}

pub fn first_tab_default_label(language: Language, first: &str) -> String {
    language.pick(
        format!("{first} (the first tab)"),
        format!("{first} (a primeira aba)"),
    )
}

pub fn declared_tab_label(language: Language, first: &str) -> String {
    language.pick(
        format!("Tab [Enter = {first}, the first]: "),
        format!("Aba [Enter = {first}, a primeira]: "),
    )
}

pub fn integer_hint(language: Language, minimum: u64, maximum: u64) -> String {
    language.pick(
        format!("{minimum} to {maximum}"),
        format!("de {minimum} a {maximum}"),
    )
}

pub fn integer_label_hint(language: Language, minimum: u64, maximum: u64) -> String {
    language.pick(
        format!(" ({minimum} to {maximum})"),
        format!(" ({minimum} a {maximum})"),
    )
}

pub fn default_marker(language: Language, default: &str) -> String {
    language.pick(
        format!("{default} (default)"),
        format!("{default} (padrão)"),
    )
}

pub fn choose_label(language: Language) -> &'static str {
    language.pick("Choose", "Escolha")
}

pub fn required_suffix(language: Language) -> &'static str {
    language.pick(" (required)", " (obrigatório)")
}

pub fn skip_suffix(language: Language) -> &'static str {
    language.pick(" (Enter to skip)", " (Enter para pular)")
}

pub fn secret_from_environment(language: Language) -> &'static str {
    language.pick("environment variable name", "nome de variável de ambiente")
}

pub fn secret_from_file(language: Language) -> &'static str {
    language.pick("path of a file", "caminho de um arquivo")
}

pub fn secret_from_both(language: Language) -> &'static str {
    language.pick("both", "os dois")
}

pub fn variable_names_title(language: Language) -> &'static str {
    language.pick("Variable names", "Nomes das variáveis")
}

pub fn variable_names_label(language: Language) -> &'static str {
    language.pick(
        "Variable names (separated by commas)",
        "Nomes das variáveis (separados por vírgula)",
    )
}

pub fn variable_name_title(language: Language) -> &'static str {
    language.pick("Variable name", "Nome da variável")
}

pub fn seconds_hint(language: Language, minimum: u64, maximum: u64) -> String {
    language.pick(
        format!("{minimum} to {maximum}"),
        format!("de {minimum} a {maximum}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_assertions::assert_translated;

    type FixedMessage = fn(Language) -> &'static str;

    #[test]
    fn every_setup_message_is_translated() {
        let fixed: [(&str, FixedMessage); 24] = [
            ("required", required_answer),
            ("number", unknown_number),
            ("preset", unknown_preset),
            ("id", invalid_widget_id),
            ("secret", secret_explanation),
            ("skip", skip_label),
            ("scratch", from_scratch_label),
            ("new tab", new_tab_label),
            ("interval", interval_title),
            ("timeout", timeout_title),
            ("list", list_hint),
            ("tab name title", new_tab_name_title),
            ("tab name label", new_tab_name_label),
            ("file title", file_path_title),
            ("preset title", preset_title),
            ("preset label", preset_label),
            ("widget id title", widget_id_title),
            ("widget id hint", widget_id_hint),
            ("widget tab", widget_tab_title),
            ("first tab label", first_tab_label),
            ("choose", choose_label),
            ("required suffix", required_suffix),
            ("skip suffix", skip_suffix),
            ("from env", secret_from_environment),
        ];
        for (label, message) in fixed {
            assert_translated(label, |language| message(language).to_string());
        }
        for (label, message) in [
            ("from file", secret_from_file as FixedMessage),
            ("both", secret_from_both),
            ("names title", variable_names_title),
            ("names label", variable_names_label),
            ("name title", variable_name_title),
        ] {
            assert_translated(label, |language| message(language).to_string());
        }
        assert_translated("taken", |language| widget_id_taken(language, "x"));
        assert_translated("tab taken", |language| tab_name_taken(language, "x"));
        assert_translated("file label", |language| file_path_label(language, "x"));
        assert_translated("note", |language| first_tab_note(language, 2));
        assert_translated("presets", |language| presets_introduction(language, "x"));
        assert_translated("id label", |language| widget_id_label(language, "x"));
        assert_translated("first tab", |language| {
            first_tab_explanation(language, true)
        });
        assert_translated("tabs", |language| panel_tabs_introduction(language, "x"));
        assert_translated("first default", |language| {
            first_tab_default_label(language, "x")
        });
        assert_translated("declared", |language| declared_tab_label(language, "x"));
        assert_translated("integer", |language| integer_hint(language, 1, 2));
        assert_translated("integer label", |language| {
            integer_label_hint(language, 1, 2)
        });
        assert_translated("default", |language| default_marker(language, "x"));
        assert_translated("seconds", |language| seconds_hint(language, 1, 2));
    }
}
