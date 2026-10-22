use unicode_width::UnicodeWidthStr;

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

    pub fn title(self) -> &'static str {
        match self {
            HintContext::Grid => "Ajuda · grade de widgets",
            HintContext::Focus => "Ajuda · widget aberto",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct KeyHint {
    pub keys: &'static str,
    pub description: &'static str,
}

const fn hint(keys: &'static str, description: &'static str) -> KeyHint {
    KeyHint { keys, description }
}

pub const HELP_HINT: KeyHint = hint("?", "ajuda");
const QUIT_FOOTER_HINT: KeyHint = hint("q", "sair");

const GRID_HINTS: [KeyHint; 6] = [
    hint("←↑↓→ hjkl", "mover"),
    hint("Enter", "abrir"),
    hint("Tab", "próxima aba"),
    hint("1-9", "ir para a aba"),
    hint("A-Z", "ir para o widget"),
    hint("Shift+Tab", "aba anterior"),
];

const FOCUS_HINTS: [KeyHint; 5] = [
    hint("↑↓ hjkl", "item"),
    hint("PgUp/PgDn", "página"),
    hint("Home/End", "início/fim"),
    hint("a", "ações"),
    hint("Esc", "voltar"),
];

const GLOBAL_HINTS: [KeyHint; 7] = [
    hint("r", "atualizar tudo"),
    hint("R", "recarregar config"),
    hint("a", "ações do widget focado"),
    hint("p", "iniciar/pausar pomodoro"),
    hint("x", "zerar pomodoro"),
    hint("i", "recomendação (IA)"),
    hint("q / Ctrl+C", "sair"),
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

pub fn footer_hints(context: HintContext, width: usize) -> Vec<&'static KeyHint> {
    let mut candidates: Vec<&'static KeyHint> = context_hints(context).iter().collect();
    if context == HintContext::Grid {
        candidates.push(&QUIT_FOOTER_HINT);
    }
    let mut remaining = width
        .saturating_sub(FOOTER_LEADING_WIDTH)
        .saturating_sub(segment_width(&HELP_HINT));
    let mut fitted = Vec::new();
    for candidate in candidates {
        let needed = segment_width(candidate);
        if needed > remaining {
            break;
        }
        remaining -= needed;
        fitted.push(candidate);
    }
    fitted.push(&HELP_HINT);
    fitted
}

fn segment_width(hint: &KeyHint) -> usize {
    hint.keys.width()
        + KEYS_DESCRIPTION_GAP_WIDTH
        + hint.description.width()
        + SEGMENT_SEPARATOR_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;

    const NARROW_WIDTH: usize = 60;
    const WIDE_WIDTH: usize = 200;
    const WIDTHS_TO_TRY: [usize; 6] = [0, 1, 10, 30, NARROW_WIDTH, WIDE_WIDTH];

    fn total_width(hints: &[&KeyHint]) -> usize {
        FOOTER_LEADING_WIDTH + hints.iter().map(|hint| segment_width(hint)).sum::<usize>()
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
                assert_eq!(footer_hints(context, width).last(), Some(&&HELP_HINT));
            }
        }
    }

    #[test]
    fn wide_footer_has_every_context_hint_and_quit_in_grid() {
        let footer = footer_hints(HintContext::Grid, WIDE_WIDTH);
        for hint in context_hints(HintContext::Grid) {
            assert!(footer.contains(&hint), "falta {}", hint.keys);
        }
        assert!(footer.iter().any(|hint| hint.keys == "q"));
        assert_eq!(footer.last(), Some(&&HELP_HINT));
    }

    #[test]
    fn narrow_footer_drops_trailing_hints_but_keeps_leading_ones() {
        let narrow = footer_hints(HintContext::Grid, NARROW_WIDTH);
        let wide = footer_hints(HintContext::Grid, WIDE_WIDTH);
        assert_eq!(
            narrow.first().copied(),
            context_hints(HintContext::Grid).first()
        );
        assert!(narrow.len() < wide.len());
        assert!(total_width(&narrow) <= NARROW_WIDTH);
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
