use unicode_width::UnicodeWidthStr;

use crate::domain::language::Language;

const SEGMENT_SEPARATOR_WIDTH: usize = 2;
const KEYS_DESCRIPTION_GAP_WIDTH: usize = 1;
const FOOTER_LEADING_WIDTH: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintContext {
    Grid,
    Focus,
}

impl HintContext {
    pub fn from_expanded(expanded: bool) -> Self {
        if expanded {
            HintContext::Focus
        } else {
            HintContext::Grid
        }
    }

    pub fn title(self, language: Language) -> &'static str {
        match (self, language) {
            (HintContext::Grid, Language::English) => "Help · widget grid",
            (HintContext::Focus, Language::English) => "Help · open widget",
            (HintContext::Grid, Language::BrazilianPortuguese) => "Ajuda · grade de widgets",
            (HintContext::Focus, Language::BrazilianPortuguese) => "Ajuda · widget aberto",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct KeyHint {
    pub keys: &'static str,
    english: &'static str,
    portuguese: &'static str,
}

impl KeyHint {
    pub fn description(&self, language: Language) -> &'static str {
        match language {
            Language::English => self.english,
            Language::BrazilianPortuguese => self.portuguese,
        }
    }
}

const fn hint(keys: &'static str, english: &'static str, portuguese: &'static str) -> KeyHint {
    KeyHint {
        keys,
        english,
        portuguese,
    }
}

pub const HELP_HINT: KeyHint = hint("?", "help", "ajuda");
const QUIT_FOOTER_HINT: KeyHint = hint("q", "quit", "sair");
const ADD_WIDGET_HINT: KeyHint = hint("+", "add widget", "adicionar widget");

const GRID_HINTS: [KeyHint; 6] = [
    hint("←↑↓→ hjkl", "move", "mover"),
    hint("Enter", "open", "abrir"),
    hint("Tab", "next tab", "próxima aba"),
    hint("1-9", "go to tab", "ir para a aba"),
    hint("Shift/Alt+A-Z", "go to widget", "ir para o widget"),
    hint("Shift+Tab", "previous tab", "aba anterior"),
];

const FOCUS_HINTS: [KeyHint; 5] = [
    hint("↑↓ hjkl", "item", "item"),
    hint("PgUp/PgDn", "page", "página"),
    hint("Home/End", "start/end", "início/fim"),
    hint("a", "actions", "ações"),
    hint("Esc", "back", "voltar"),
];

const GLOBAL_HINTS: [KeyHint; 8] = [
    ADD_WIDGET_HINT,
    hint("r", "refresh all", "atualizar tudo"),
    hint("R", "reload config", "recarregar config"),
    hint("a", "focused widget actions", "ações do widget focado"),
    hint("p", "start/pause pomodoro", "iniciar/pausar pomodoro"),
    hint("x", "reset pomodoro", "zerar pomodoro"),
    hint("i", "recommendation (AI)", "recomendação (IA)"),
    hint("q / Ctrl+C", "quit", "sair"),
];

pub fn context_hints(context: HintContext) -> &'static [KeyHint] {
    match context {
        HintContext::Grid => &GRID_HINTS,
        HintContext::Focus => &FOCUS_HINTS,
    }
}

pub fn global_hints() -> &'static [KeyHint] {
    &GLOBAL_HINTS
}

pub fn footer_hints(
    context: HintContext,
    width: usize,
    language: Language,
) -> Vec<&'static KeyHint> {
    let mut candidates: Vec<&'static KeyHint> = context_hints(context).iter().collect();
    if context == HintContext::Grid {
        candidates.push(&ADD_WIDGET_HINT);
        candidates.push(&QUIT_FOOTER_HINT);
    }
    let mut remaining = width
        .saturating_sub(FOOTER_LEADING_WIDTH)
        .saturating_sub(segment_width(&HELP_HINT, language));
    let mut fitted = Vec::new();
    for candidate in candidates {
        let needed = segment_width(candidate, language);
        if needed > remaining {
            break;
        }
        remaining -= needed;
        fitted.push(candidate);
    }
    fitted.push(&HELP_HINT);
    fitted
}

fn segment_width(hint: &KeyHint, language: Language) -> usize {
    hint.keys.width()
        + KEYS_DESCRIPTION_GAP_WIDTH
        + hint.description(language).width()
        + SEGMENT_SEPARATOR_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;

    const NARROW_WIDTH: usize = 60;
    const WIDE_WIDTH: usize = 200;
    const WIDTHS_TO_TRY: [usize; 6] = [0, 1, 10, 30, NARROW_WIDTH, WIDE_WIDTH];

    fn total_width(hints: &[&KeyHint]) -> usize {
        FOOTER_LEADING_WIDTH
            + hints
                .iter()
                .map(|hint| segment_width(hint, Language::BrazilianPortuguese))
                .sum::<usize>()
    }

    #[test]
    fn from_expanded_maps_both_states() {
        assert_eq!(HintContext::from_expanded(false), HintContext::Grid);
        assert_eq!(HintContext::from_expanded(true), HintContext::Focus);
    }

    #[test]
    fn footer_always_ends_with_help_hint_at_any_width() {
        for context in [HintContext::Grid, HintContext::Focus] {
            for width in WIDTHS_TO_TRY {
                assert_eq!(
                    footer_hints(context, width, Language::BrazilianPortuguese).last(),
                    Some(&&HELP_HINT)
                );
            }
        }
    }

    #[test]
    fn wide_footer_has_every_context_hint_and_quit_in_grid() {
        let footer = footer_hints(HintContext::Grid, WIDE_WIDTH, Language::BrazilianPortuguese);
        for hint in context_hints(HintContext::Grid) {
            assert!(footer.contains(&hint), "falta {}", hint.keys);
        }
        assert!(footer.iter().any(|hint| hint.keys == "q"));
        assert_eq!(footer.last(), Some(&&HELP_HINT));
    }

    #[test]
    fn narrow_footer_drops_trailing_hints_but_keeps_leading_ones() {
        let narrow = footer_hints(
            HintContext::Grid,
            NARROW_WIDTH,
            Language::BrazilianPortuguese,
        );
        let wide = footer_hints(HintContext::Grid, WIDE_WIDTH, Language::BrazilianPortuguese);
        assert_eq!(
            narrow.first().copied(),
            context_hints(HintContext::Grid).first()
        );
        assert!(narrow.len() < wide.len());
        assert!(total_width(&narrow) <= NARROW_WIDTH);
    }

    #[test]
    fn the_add_widget_key_is_in_the_help_and_in_the_grid_footer_only() {
        assert!(global_hints().iter().any(|hint| hint.keys == "+"));
        let grid = footer_hints(HintContext::Grid, WIDE_WIDTH, Language::BrazilianPortuguese);
        assert!(grid.iter().any(|hint| hint.keys == "+"));
        let focus = footer_hints(
            HintContext::Focus,
            WIDE_WIDTH,
            Language::BrazilianPortuguese,
        );
        assert!(!focus.iter().any(|hint| hint.keys == "+"));
    }

    #[test]
    fn no_hint_list_repeats_a_key_label() {
        for hints in [
            context_hints(HintContext::Grid),
            context_hints(HintContext::Focus),
            global_hints(),
        ] {
            let mut labels: Vec<&str> = hints.iter().map(|hint| hint.keys).collect();
            labels.sort_unstable();
            labels.dedup();
            assert_eq!(labels.len(), hints.len());
        }
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;
    use crate::language_assertions::assert_translated;

    #[test]
    fn every_hint_and_title_is_translated() {
        for hint in GRID_HINTS
            .iter()
            .chain(&GLOBAL_HINTS)
            .chain(&FOCUS_HINTS)
            .chain([&HELP_HINT, &QUIT_FOOTER_HINT])
        {
            assert!(!hint.description(Language::English).is_empty());
            assert!(!hint.description(Language::BrazilianPortuguese).is_empty());
        }
        for context in [HintContext::Grid, HintContext::Focus] {
            assert_translated("title", |language| context.title(language).to_string());
        }
        assert_translated("quit", |language| {
            QUIT_FOOTER_HINT.description(language).to_string()
        });
    }

    #[test]
    fn english_footer_lists_the_english_descriptions() {
        let hints = footer_hints(HintContext::Grid, 200, Language::English);
        let descriptions: Vec<&str> = hints
            .iter()
            .map(|hint| hint.description(Language::English))
            .collect();
        assert_eq!(descriptions.last(), Some(&"help"));
        assert!(descriptions.contains(&"add widget"));
    }
}
