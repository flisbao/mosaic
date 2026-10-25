use std::io::IsTerminal;
use std::process::ExitCode;

use mosaic::domain::color_depth::ColorDepth;
use mosaic::domain::theme::{
    preset_names, Palette, Rgb, Theme, ThemeError, ThemeRole, DEFAULT_THEME_NAME,
};

const EXIT_CODE_USAGE: u8 = 2;
const THEMES_USAGE: &str = "uso: mosaic themes [NOME]";
const THEME_NAME_SEPARATOR: &str = ", ";
const USAGE_HINT: &str = "use em mosaic.toml:";
const SWATCH_WIDTH: usize = 2;
const BACKGROUND_ESCAPE_PREFIX: &str = "\x1b[48;2;";
const RESET_ESCAPE: &str = "\x1b[0m";
const NAME_COLUMN_WIDTH: usize = 20;
const ROLE_COLUMN_WIDTH: usize = 12;
const DEFAULT_THEME_DESCRIPTION: &str = "usa as cores do terminal, sem pintar o fundo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwatchStyle {
    Plain,
    TrueColor,
}

impl SwatchStyle {
    fn detect(is_terminal: bool, depth: ColorDepth) -> Self {
        if is_terminal && depth == ColorDepth::TrueColor {
            Self::TrueColor
        } else {
            Self::Plain
        }
    }

    fn sample(self, color: Rgb) -> String {
        match self {
            Self::Plain => String::new(),
            Self::TrueColor => swatch(color),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Output(String),
    Usage(String),
}

fn swatch(color: Rgb) -> String {
    format!(
        "{BACKGROUND_ESCAPE_PREFIX}{};{};{}m{}{RESET_ESCAPE}",
        color.red,
        color.green,
        color.blue,
        " ".repeat(SWATCH_WIDTH)
    )
}

fn palette_of(name: &str) -> Option<Palette> {
    match Theme::preset(name) {
        Ok(Theme::Colored { palette, .. }) => Some(palette),
        _ => None,
    }
}

fn listing(style: SwatchStyle) -> String {
    let mut lines = Vec::new();
    for name in preset_names() {
        let swatches = palette_of(name)
            .map(|palette| {
                ThemeRole::ALL
                    .iter()
                    .map(|role| style.sample(palette.get(*role)))
                    .collect::<String>()
            })
            .unwrap_or_default();
        lines.push(
            format!("{name:<NAME_COLUMN_WIDTH$}{swatches}")
                .trim_end()
                .to_string(),
        );
    }
    lines.push(String::new());
    lines.push(format!("{USAGE_HINT} [theme] name = \"<nome>\""));
    lines.join("\n")
}

fn contrast_against(role: ThemeRole, palette: &Palette) -> Option<f64> {
    match role {
        ThemeRole::Background => None,
        ThemeRole::OnAccent => Some(palette.get(role).contrast_ratio(palette.accent)),
        _ => Some(palette.get(role).contrast_ratio(palette.background)),
    }
}

fn details(name: &str, style: SwatchStyle) -> Result<String, ThemeError> {
    Theme::preset(name)?;
    let Some(palette) = palette_of(name) else {
        return Ok(format!("{DEFAULT_THEME_NAME}: {DEFAULT_THEME_DESCRIPTION}"));
    };
    let mut lines = vec![name.to_string()];
    for role in ThemeRole::ALL {
        let color = palette.get(role);
        let ratio = contrast_against(role, &palette)
            .map(|ratio| format!("{ratio:.1}:1"))
            .unwrap_or_default();
        let sample = style.sample(color);
        lines.push(
            format!(
                "{:<ROLE_COLUMN_WIDTH$}{}  {ratio}  {sample}",
                role.key(),
                color.to_hex()
            )
            .trim_end()
            .to_string(),
        );
    }
    Ok(lines.join("\n"))
}

fn execute(arguments: &[String], style: SwatchStyle) -> Outcome {
    match arguments {
        [] => Outcome::Output(listing(style)),
        [name] => match details(name, style) {
            Ok(text) => Outcome::Output(text),
            Err(error) => Outcome::Usage(format!(
                "{error}; temas disponíveis: {}\n{THEMES_USAGE}",
                preset_names().join(THEME_NAME_SEPARATOR)
            )),
        },
        _ => Outcome::Usage(THEMES_USAGE.to_string()),
    }
}

pub fn run(arguments: &[String]) -> ExitCode {
    let style = SwatchStyle::detect(
        std::io::stdout().is_terminal(),
        ColorDepth::detect(|name| std::env::var(name).ok()),
    );
    match execute(arguments, style) {
        Outcome::Output(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Outcome::Usage(message) => {
            eprintln!("{message}");
            ExitCode::from(EXIT_CODE_USAGE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn output(outcome: Outcome) -> String {
        match outcome {
            Outcome::Output(text) => text,
            Outcome::Usage(message) => panic!("era para ter saída, veio uso: {message}"),
        }
    }

    #[test]
    fn listing_has_every_preset_name_and_the_config_hint() {
        let text = output(execute(&[], SwatchStyle::Plain));
        for name in preset_names() {
            assert!(text.lines().any(|line| line.starts_with(name)), "{name}");
        }
        assert!(text.contains("[theme] name = \"<nome>\""));
    }

    #[test]
    fn plain_listing_has_no_escape_codes_without_truecolor() {
        assert!(!output(execute(&[], SwatchStyle::Plain)).contains('\x1b'));
        assert!(!output(execute(&arguments(&["nord"]), SwatchStyle::Plain)).contains('\x1b'));
    }

    #[test]
    fn truecolor_listing_has_one_swatch_per_role_on_each_colored_line() {
        let text = output(execute(&[], SwatchStyle::TrueColor));
        let tokyo = text
            .lines()
            .find(|line| line.starts_with("tokyo-night "))
            .unwrap();
        assert_eq!(
            tokyo.matches(BACKGROUND_ESCAPE_PREFIX).count(),
            ThemeRole::ALL.len()
        );
        let default = text
            .lines()
            .find(|line| line.starts_with("default"))
            .unwrap();
        assert!(!default.contains('\x1b'));
    }

    #[test]
    fn swatches_need_both_a_terminal_and_truecolor() {
        let detect = SwatchStyle::detect;
        assert_eq!(detect(true, ColorDepth::TrueColor), SwatchStyle::TrueColor);
        assert_eq!(detect(false, ColorDepth::TrueColor), SwatchStyle::Plain);
        assert_eq!(detect(true, ColorDepth::Indexed256), SwatchStyle::Plain);
    }

    #[test]
    fn details_show_each_role_with_hex_and_contrast() {
        let text = output(execute(&arguments(&["tokyo-night"]), SwatchStyle::Plain));
        assert_eq!(text.lines().count(), 1 + ThemeRole::ALL.len());
        assert!(text.contains("foreground") && text.contains("#c0caf5"));
        assert!(text.contains("10.6:1"), "{text}");
        let background = text
            .lines()
            .find(|line| line.starts_with("background"))
            .unwrap();
        assert!(!background.contains(":1"));
    }

    #[test]
    fn details_for_the_default_theme_explain_it_uses_the_terminal_colors() {
        let text = output(execute(&arguments(&["default"]), SwatchStyle::Plain));
        assert!(text.contains("cores do terminal"));
    }

    #[test]
    fn unknown_theme_is_a_usage_error_that_lists_the_names() {
        match execute(&arguments(&["neon"]), SwatchStyle::Plain) {
            Outcome::Usage(message) => {
                assert!(
                    message.contains("neon") && message.contains("tokyo-night"),
                    "{message}"
                );
                assert!(message.contains(THEMES_USAGE));
            }
            Outcome::Output(text) => panic!("deveria ser erro de uso: {text}"),
        }
    }

    #[test]
    fn extra_arguments_are_a_usage_error() {
        assert_eq!(
            execute(&arguments(&["a", "b"]), SwatchStyle::Plain),
            Outcome::Usage(THEMES_USAGE.to_string())
        );
    }
}
