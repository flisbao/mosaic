use crate::domain::color_depth::ColorDepth;
use crate::domain::theme::Theme;
use crate::presentation::app_state::AppState;
use crate::presentation::theme_styles::resolve_theme;

pub fn apply_theme(
    state: &mut AppState,
    theme: &Theme,
    depth: ColorDepth,
    mut warnings: Vec<String>,
) -> Vec<String> {
    let resolution = resolve_theme(theme, depth);
    state.set_theme_styles(resolution.styles);
    warnings.extend(resolution.warning);
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::app_state::TabView;
    use crate::presentation::key_input::KeyInput;
    use crate::presentation::theme_styles::{ThemeStyles, BASIC_TERMINAL_WARNING};

    fn state() -> AppState {
        let tab = |name: &str| TabView {
            name: name.to_string(),
            entry_indexes: Vec::new(),
        };
        AppState::new(Vec::new(), vec![tab("Principal"), tab("Notas")])
    }

    fn nord() -> Theme {
        Theme::preset("nord").unwrap()
    }

    #[test]
    fn reload_applies_the_new_theme() {
        let mut app = state();
        assert!(!app.styles().paints_background());
        let warnings = apply_theme(&mut app, &nord(), ColorDepth::TrueColor, Vec::new());
        assert!(app.styles().paints_background());
        assert!(warnings.is_empty());
        let warnings = apply_theme(&mut app, &Theme::Ansi, ColorDepth::TrueColor, Vec::new());
        assert_eq!(app.styles(), &ThemeStyles::ansi());
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_colored_theme_on_a_basic_terminal_falls_back_and_warns() {
        let mut app = state();
        let warnings = apply_theme(&mut app, &nord(), ColorDepth::Basic, Vec::new());
        assert_eq!(app.styles(), &ThemeStyles::ansi());
        assert_eq!(warnings, vec![BASIC_TERMINAL_WARNING.to_string()]);
    }

    #[test]
    fn existing_warnings_come_first_and_are_kept() {
        let mut app = state();
        let warnings = apply_theme(
            &mut app,
            &nord(),
            ColorDepth::Basic,
            vec!["[theme] cor inválida".to_string()],
        );
        assert_eq!(
            warnings,
            vec![
                "[theme] cor inválida".to_string(),
                BASIC_TERMINAL_WARNING.to_string()
            ]
        );
    }

    #[test]
    fn applying_a_theme_keeps_tabs_and_focus() {
        let mut app = state();
        app.handle_key(KeyInput::Character('2'));
        let active_tab = app.active_tab();
        let tab_count = app.tabs().len();
        apply_theme(&mut app, &nord(), ColorDepth::TrueColor, Vec::new());
        assert_eq!(app.active_tab(), active_tab);
        assert_eq!(app.tabs().len(), tab_count);
        assert_eq!(app.focused_entry(), None);
    }
}
