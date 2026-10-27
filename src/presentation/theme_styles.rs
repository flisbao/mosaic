use ratatui::style::{Color, Modifier, Style};

use crate::domain::color_depth::ColorDepth;
use crate::domain::theme::{Palette, Rgb, Theme};
use crate::domain::widget_content::Tone;
use crate::presentation::indexed_palette::nearest_indexed;

pub const BASIC_TERMINAL_WARNING: &str = "terminal sem 256 cores: usando o tema default";
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeStyles {
    foreground: Option<Color>,
    background: Option<Color>,
    muted: Color,
    ok: Color,
    warn: Color,
    danger: Color,
    accent: Color,
    on_accent: Color,
}

impl ThemeStyles {
    pub fn ansi() -> Self {
        Self {
            foreground: None,
            background: None,
            muted: Color::DarkGray,
            ok: Color::Green,
            warn: Color::Yellow,
            danger: Color::Red,
            accent: Color::Cyan,
            on_accent: Color::Black,
        }
    }

    pub fn new(theme: &Theme, depth: ColorDepth) -> Self {
        match (theme, depth) {
            (
                Theme::Colored {
                    palette,
                    paints_background,
                    ..
                },
                ColorDepth::TrueColor,
            ) => Self::from_palette(palette, *paints_background, true_color),
            (
                Theme::Colored {
                    palette,
                    paints_background,
                    ..
                },
                ColorDepth::Indexed256,
            ) => Self::from_palette(palette, *paints_background, indexed_color),
            _ => Self::ansi(),
        }
    }

    fn from_palette(
        palette: &Palette,
        paints_background: bool,
        color: impl Fn(Rgb) -> Color,
    ) -> Self {
        Self {
            foreground: paints_background.then(|| color(palette.foreground)),
            background: paints_background.then(|| color(palette.background)),
            muted: color(palette.muted),
            ok: color(palette.ok),
            warn: color(palette.warn),
            danger: color(palette.danger),
            accent: color(palette.accent),
            on_accent: color(palette.on_accent),
        }
    }

    pub fn paints_background(&self) -> bool {
        self.background.is_some()
    }

    pub fn base(&self) -> Style {
        with_colors(Style::default(), self.foreground, self.background)
    }

    pub fn tone(&self, tone: Tone) -> Style {
        match tone {
            Tone::Normal => with_colors(Style::default(), self.foreground, None),
            Tone::Muted => self.muted(),
            Tone::Ok => self.ok(),
            Tone::Warn => self.warn(),
            Tone::Danger => self.danger(),
            Tone::Current => self.accent().add_modifier(Modifier::BOLD),
        }
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }

    pub fn warn(&self) -> Style {
        Style::default().fg(self.warn)
    }

    pub fn danger(&self) -> Style {
        Style::default().fg(self.danger)
    }

    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn active_tab(&self) -> Style {
        Style::default()
            .fg(self.on_accent)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn key_hint(&self) -> Style {
        self.warn().add_modifier(Modifier::BOLD)
    }
}

pub struct ThemeResolution {
    pub styles: ThemeStyles,
    pub warning: Option<String>,
}

pub fn resolve_theme(theme: &Theme, depth: ColorDepth) -> ThemeResolution {
    let is_colored_on_basic_terminal =
        matches!(theme, Theme::Colored { .. }) && depth == ColorDepth::Basic;
    ThemeResolution {
        styles: ThemeStyles::new(theme, depth),
        warning: is_colored_on_basic_terminal.then(|| BASIC_TERMINAL_WARNING.to_string()),
    }
}

fn with_colors(style: Style, foreground: Option<Color>, background: Option<Color>) -> Style {
    let style = foreground.map_or(style, |color| style.fg(color));
    background.map_or(style, |color| style.bg(color))
}

fn true_color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.red, rgb.green, rgb.blue)
}

fn indexed_color(rgb: Rgb) -> Color {
    Color::Indexed(nearest_indexed(rgb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::theme::preset_names;

    const ALL_TONES: [Tone; 6] = [
        Tone::Normal,
        Tone::Muted,
        Tone::Ok,
        Tone::Warn,
        Tone::Danger,
        Tone::Current,
    ];

    fn colored(name: &str) -> Theme {
        Theme::preset(name).unwrap()
    }

    fn palette(name: &str) -> Palette {
        match colored(name) {
            Theme::Colored { palette, .. } => palette,
            Theme::Ansi => panic!("{name} é ANSI"),
        }
    }

    #[test]
    fn ansi_theme_keeps_todays_tone_colors() {
        let styles = ThemeStyles::ansi();
        assert_eq!(styles.tone(Tone::Normal), Style::default());
        assert_eq!(
            styles.tone(Tone::Muted),
            Style::default().fg(Color::DarkGray)
        );
        assert_eq!(styles.tone(Tone::Ok), Style::default().fg(Color::Green));
        assert_eq!(styles.tone(Tone::Warn), Style::default().fg(Color::Yellow));
        assert_eq!(styles.tone(Tone::Danger), Style::default().fg(Color::Red));
        assert_eq!(
            styles.tone(Tone::Current),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );
    }

    #[test]
    fn default_theme_keeps_the_ansi_colors_and_paints_nothing() {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Indexed256,
            ColorDepth::Basic,
        ] {
            let styles = ThemeStyles::new(&Theme::Ansi, depth);
            assert_eq!(styles, ThemeStyles::ansi());
            assert_eq!(styles.base(), Style::default());
            assert!(!styles.paints_background());
        }
    }

    #[test]
    fn ansi_chrome_styles_match_todays_interface() {
        let styles = ThemeStyles::ansi();
        assert_eq!(
            styles.active_tab(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            styles.key_hint(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(styles.accent(), Style::default().fg(Color::Cyan));
    }

    #[test]
    fn colored_theme_uses_rgb_on_truecolor() {
        let styles = ThemeStyles::new(&colored("tokyo-night"), ColorDepth::TrueColor);
        assert_eq!(
            styles.tone(Tone::Ok),
            Style::default().fg(Color::Rgb(0x9e, 0xce, 0x6a))
        );
        assert_eq!(
            styles.base(),
            Style::default()
                .fg(Color::Rgb(0xc0, 0xca, 0xf5))
                .bg(Color::Rgb(0x1a, 0x1b, 0x26))
        );
        assert!(styles.paints_background());
    }

    #[test]
    fn indexed_256_never_emits_rgb() {
        for name in preset_names().into_iter().filter(|name| *name != "default") {
            let styles = ThemeStyles::new(&colored(name), ColorDepth::Indexed256);
            let mut all_styles = vec![styles.base(), styles.active_tab()];
            all_styles.extend(ALL_TONES.map(|tone| styles.tone(tone)));
            for style in all_styles {
                for color in [style.fg, style.bg].into_iter().flatten() {
                    assert!(matches!(color, Color::Indexed(_)), "{name}: {color:?}");
                }
            }
        }
    }

    #[test]
    fn basic_depth_falls_back_to_the_ansi_styles() {
        let styles = ThemeStyles::new(&colored("dracula"), ColorDepth::Basic);
        assert_eq!(styles, ThemeStyles::ansi());
    }

    #[test]
    fn active_tab_uses_on_accent_over_accent() {
        let styles = ThemeStyles::new(&colored("nord"), ColorDepth::TrueColor);
        let nord = palette("nord");
        let style = styles.active_tab();
        assert_eq!(
            style.fg,
            Some(Color::Rgb(
                nord.on_accent.red,
                nord.on_accent.green,
                nord.on_accent.blue
            ))
        );
        assert_eq!(
            style.bg,
            Some(Color::Rgb(
                nord.accent.red,
                nord.accent.green,
                nord.accent.blue
            ))
        );
    }

    #[test]
    fn base_paints_foreground_and_background_only_when_enabled() {
        let without = colored("nord").without_background();
        let styles = ThemeStyles::new(&without, ColorDepth::TrueColor);
        assert_eq!(styles.base(), Style::default());
        assert_eq!(styles.tone(Tone::Normal), Style::default());
        assert!(!styles.paints_background());
        assert_ne!(styles.tone(Tone::Ok), Style::default());
    }

    #[test]
    fn normal_tone_uses_the_theme_foreground_when_painting() {
        let styles = ThemeStyles::new(&colored("nord"), ColorDepth::TrueColor);
        assert_eq!(
            styles.tone(Tone::Normal).fg,
            Some(Color::Rgb(0xd8, 0xde, 0xe9))
        );
        assert_eq!(styles.tone(Tone::Normal).bg, None);
    }

    const THEME_STYLES_FILE: &str = "theme_styles.rs";
    const TEST_SECTION_MARKER: &str = "#[cfg(test)]";
    const RAW_COLOR_MARKERS: [&str; 3] = ["Color::", ".fg(", ".bg("];

    fn rust_files(directory: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    #[test]
    fn no_presentation_file_depends_on_infrastructure() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/presentation");
        let mut files = Vec::new();
        rust_files(&directory, &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let production = text.split(TEST_SECTION_MARKER).next().unwrap();
            assert!(
                !production.contains("crate::infrastructure"),
                "{} importa infrastructure: presentation só enxerga domain e application",
                file.display()
            );
        }
    }

    #[test]
    fn no_presentation_file_uses_raw_colors() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/presentation");
        let mut files = Vec::new();
        rust_files(&directory, &mut files);
        assert!(files.len() > 10);
        for file in files {
            if file
                .file_name()
                .is_some_and(|name| name == THEME_STYLES_FILE)
            {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap();
            let production = text.split(TEST_SECTION_MARKER).next().unwrap();
            for marker in RAW_COLOR_MARKERS {
                assert!(
                    !production.contains(marker),
                    "{} usa '{marker}': use o ThemeStyles",
                    file.display()
                );
            }
        }
    }

    #[test]
    fn colored_theme_on_a_basic_terminal_falls_back_with_a_warning() {
        let resolution = resolve_theme(&colored("dracula"), ColorDepth::Basic);
        assert_eq!(resolution.styles, ThemeStyles::ansi());
        assert_eq!(resolution.warning.as_deref(), Some(BASIC_TERMINAL_WARNING));
    }

    #[test]
    fn colored_theme_on_truecolor_or_256_colors_has_no_warning() {
        for depth in [ColorDepth::TrueColor, ColorDepth::Indexed256] {
            let resolution = resolve_theme(&colored("dracula"), depth);
            assert_eq!(resolution.warning, None);
            assert!(resolution.styles.paints_background());
        }
    }

    #[test]
    fn ansi_theme_never_warns() {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Indexed256,
            ColorDepth::Basic,
        ] {
            let resolution = resolve_theme(&Theme::Ansi, depth);
            assert_eq!(resolution.warning, None);
            assert_eq!(resolution.styles, ThemeStyles::ansi());
        }
    }
}
