const COLOR_TERMINAL_VARIABLE: &str = "COLORTERM";
const TERMINAL_VARIABLE: &str = "TERM";
const TRUE_COLOR_VALUES: [&str; 2] = ["truecolor", "24bit"];
const INDEXED_TERMINAL_SUFFIX: &str = "256color";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Indexed256,
    Basic,
}

impl ColorDepth {
    pub fn detect(variable: impl Fn(&str) -> Option<String>) -> ColorDepth {
        let announces_true_color = variable(COLOR_TERMINAL_VARIABLE)
            .map(|value| value.trim().to_ascii_lowercase())
            .is_some_and(|value| TRUE_COLOR_VALUES.contains(&value.as_str()));
        if announces_true_color {
            return ColorDepth::TrueColor;
        }
        let has_indexed_palette = variable(TERMINAL_VARIABLE).is_some_and(|value| {
            value
                .trim()
                .to_ascii_lowercase()
                .ends_with(INDEXED_TERMINAL_SUFFIX)
        });
        if has_indexed_palette {
            ColorDepth::Indexed256
        } else {
            ColorDepth::Basic
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(pairs: &[(&str, &str)]) -> ColorDepth {
        ColorDepth::detect(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn colorterm_truecolor_and_24bit_are_truecolor() {
        for value in ["truecolor", "24bit", "TrueColor", "24BIT", " truecolor "] {
            assert_eq!(
                detect(&[("COLORTERM", value)]),
                ColorDepth::TrueColor,
                "{value:?}"
            );
        }
    }

    #[test]
    fn term_with_256color_is_indexed() {
        assert_eq!(
            detect(&[("TERM", "xterm-256color")]),
            ColorDepth::Indexed256
        );
        assert_eq!(
            detect(&[("TERM", "screen-256color")]),
            ColorDepth::Indexed256
        );
    }

    #[test]
    fn colorterm_wins_over_term() {
        let pairs = [("COLORTERM", "truecolor"), ("TERM", "xterm")];
        assert_eq!(detect(&pairs), ColorDepth::TrueColor);
    }

    #[test]
    fn an_unrelated_colorterm_value_does_not_hide_the_term_palette() {
        let pairs = [("COLORTERM", "yes"), ("TERM", "xterm-256color")];
        assert_eq!(detect(&pairs), ColorDepth::Indexed256);
    }

    #[test]
    fn plain_xterm_is_basic() {
        assert_eq!(detect(&[("TERM", "xterm")]), ColorDepth::Basic);
        assert_eq!(detect(&[("TERM", "linux")]), ColorDepth::Basic);
    }

    #[test]
    fn no_variables_is_basic() {
        assert_eq!(detect(&[]), ColorDepth::Basic);
    }

    #[test]
    fn no_color_does_not_change_the_depth() {
        let pairs = [("NO_COLOR", "1"), ("COLORTERM", "truecolor")];
        assert_eq!(detect(&pairs), ColorDepth::TrueColor);
    }
}
