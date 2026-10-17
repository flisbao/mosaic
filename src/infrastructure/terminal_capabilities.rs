#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsSupport {
    Kitty,
    Sixel,
    None,
}

const KITTY_PROGRAMS: [&str; 2] = ["ghostty", "WezTerm"];

pub fn detect_graphics_support(environment: &dyn Fn(&str) -> Option<String>) -> GraphicsSupport {
    let terminal = environment("TERM").unwrap_or_default();
    let program = environment("TERM_PROGRAM").unwrap_or_default();
    if terminal.contains("kitty")
        || environment("KITTY_WINDOW_ID").is_some()
        || KITTY_PROGRAMS.contains(&program.as_str())
    {
        GraphicsSupport::Kitty
    } else if terminal.contains("sixel") {
        GraphicsSupport::Sixel
    } else {
        GraphicsSupport::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(variables: &[(&str, &str)]) -> GraphicsSupport {
        let lookup = |name: &str| {
            variables
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        };
        detect_graphics_support(&lookup)
    }

    #[test]
    fn kitty_term_is_detected() {
        assert_eq!(detect(&[("TERM", "xterm-kitty")]), GraphicsSupport::Kitty);
        assert_eq!(detect(&[("KITTY_WINDOW_ID", "3")]), GraphicsSupport::Kitty);
    }

    #[test]
    fn ghostty_program_is_detected() {
        assert_eq!(
            detect(&[("TERM_PROGRAM", "ghostty")]),
            GraphicsSupport::Kitty
        );
    }

    #[test]
    fn wezterm_program_is_detected() {
        assert_eq!(
            detect(&[("TERM_PROGRAM", "WezTerm")]),
            GraphicsSupport::Kitty
        );
    }

    #[test]
    fn sixel_term_is_detected() {
        assert_eq!(detect(&[("TERM", "foot-sixel")]), GraphicsSupport::Sixel);
    }

    #[test]
    fn plain_xterm_has_no_graphics() {
        assert_eq!(detect(&[("TERM", "xterm-256color")]), GraphicsSupport::None);
        assert_eq!(detect(&[]), GraphicsSupport::None);
    }

    #[test]
    fn alacritty_has_no_graphics() {
        assert_eq!(
            detect(&[("TERM", "alacritty"), ("TERM_PROGRAM", "alacritty")]),
            GraphicsSupport::None
        );
    }
}
