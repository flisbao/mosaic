use thiserror::Error;

pub const DEFAULT_THEME_NAME: &str = "default";
const HEX_PREFIX: char = '#';
const HEX_DIGIT_COUNT: usize = 6;
const HEX_DIGITS_PER_CHANNEL: usize = 2;
const HEX_RADIX: u32 = 16;
const BITS_PER_CHANNEL: u32 = 8;
const CHANNEL_MASK: u32 = 0xff;
const CHANNEL_MAXIMUM: f64 = 255.0;
const LINEAR_THRESHOLD: f64 = 0.03928;
const LINEAR_DIVISOR: f64 = 12.92;
const GAMMA_OFFSET: f64 = 0.055;
const GAMMA_DIVISOR: f64 = 1.055;
const GAMMA_EXPONENT: f64 = 2.4;
const RED_WEIGHT: f64 = 0.2126;
const GREEN_WEIGHT: f64 = 0.7152;
const BLUE_WEIGHT: f64 = 0.0722;
const CONTRAST_OFFSET: f64 = 0.05;

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ThemeError {
    #[error("tema desconhecido '{0}'")]
    UnknownTheme(String),
    #[error("cor inválida '{0}': use #rrggbb")]
    InvalidHex(String),
    #[error("papel de cor desconhecido '{0}'")]
    UnknownRole(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn from_u24(value: u32) -> Self {
        Self::new(
            ((value >> (2 * BITS_PER_CHANNEL)) & CHANNEL_MASK) as u8,
            ((value >> BITS_PER_CHANNEL) & CHANNEL_MASK) as u8,
            (value & CHANNEL_MASK) as u8,
        )
    }

    pub fn from_hex(text: &str) -> Result<Self, ThemeError> {
        let invalid = || ThemeError::InvalidHex(text.to_string());
        let digits = text.strip_prefix(HEX_PREFIX).ok_or_else(invalid)?;
        let is_well_formed = digits.len() == HEX_DIGIT_COUNT
            && digits.chars().all(|digit| digit.is_ascii_hexdigit());
        if !is_well_formed {
            return Err(invalid());
        }
        let channel = |index: usize| {
            let start = index * HEX_DIGITS_PER_CHANNEL;
            u8::from_str_radix(&digits[start..start + HEX_DIGITS_PER_CHANNEL], HEX_RADIX)
                .map_err(|_| invalid())
        };
        Ok(Self::new(channel(0)?, channel(1)?, channel(2)?))
    }

    pub fn to_hex(self) -> String {
        format!(
            "{HEX_PREFIX}{:02x}{:02x}{:02x}",
            self.red, self.green, self.blue
        )
    }

    pub fn relative_luminance(self) -> f64 {
        RED_WEIGHT * linear(self.red)
            + GREEN_WEIGHT * linear(self.green)
            + BLUE_WEIGHT * linear(self.blue)
    }

    pub fn contrast_ratio(self, other: Rgb) -> f64 {
        let (first, second) = (self.relative_luminance(), other.relative_luminance());
        let (lighter, darker) = if first >= second {
            (first, second)
        } else {
            (second, first)
        };
        (lighter + CONTRAST_OFFSET) / (darker + CONTRAST_OFFSET)
    }
}

fn linear(channel: u8) -> f64 {
    let value = f64::from(channel) / CHANNEL_MAXIMUM;
    if value <= LINEAR_THRESHOLD {
        value / LINEAR_DIVISOR
    } else {
        ((value + GAMMA_OFFSET) / GAMMA_DIVISOR).powf(GAMMA_EXPONENT)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeRole {
    Foreground,
    Background,
    Muted,
    Ok,
    Warn,
    Danger,
    Accent,
    OnAccent,
}

impl ThemeRole {
    pub const ALL: [ThemeRole; 8] = [
        ThemeRole::Foreground,
        ThemeRole::Background,
        ThemeRole::Muted,
        ThemeRole::Ok,
        ThemeRole::Warn,
        ThemeRole::Danger,
        ThemeRole::Accent,
        ThemeRole::OnAccent,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ThemeRole::Foreground => "foreground",
            ThemeRole::Background => "background",
            ThemeRole::Muted => "muted",
            ThemeRole::Ok => "ok",
            ThemeRole::Warn => "warn",
            ThemeRole::Danger => "danger",
            ThemeRole::Accent => "accent",
            ThemeRole::OnAccent => "on_accent",
        }
    }

    pub fn from_key(key: &str) -> Result<ThemeRole, ThemeError> {
        Self::ALL
            .into_iter()
            .find(|role| role.key() == key)
            .ok_or_else(|| ThemeError::UnknownRole(key.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub muted: Rgb,
    pub ok: Rgb,
    pub warn: Rgb,
    pub danger: Rgb,
    pub accent: Rgb,
    pub on_accent: Rgb,
}

impl Palette {
    pub fn get(&self, role: ThemeRole) -> Rgb {
        match role {
            ThemeRole::Foreground => self.foreground,
            ThemeRole::Background => self.background,
            ThemeRole::Muted => self.muted,
            ThemeRole::Ok => self.ok,
            ThemeRole::Warn => self.warn,
            ThemeRole::Danger => self.danger,
            ThemeRole::Accent => self.accent,
            ThemeRole::OnAccent => self.on_accent,
        }
    }

    pub fn with(mut self, role: ThemeRole, color: Rgb) -> Palette {
        match role {
            ThemeRole::Foreground => self.foreground = color,
            ThemeRole::Background => self.background = color,
            ThemeRole::Muted => self.muted = color,
            ThemeRole::Ok => self.ok = color,
            ThemeRole::Warn => self.warn = color,
            ThemeRole::Danger => self.danger = color,
            ThemeRole::Accent => self.accent = color,
            ThemeRole::OnAccent => self.on_accent = color,
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Theme {
    Ansi,
    Colored {
        name: String,
        palette: Palette,
        paints_background: bool,
    },
}

struct PresetSpec {
    name: &'static str,
    palette: Palette,
}

// foreground, background, muted, ok, warn, danger, accent (on_accent é o fundo)
const fn preset(name: &'static str, colors: [u32; 7]) -> PresetSpec {
    let background = Rgb::from_u24(colors[1]);
    PresetSpec {
        name,
        palette: Palette {
            foreground: Rgb::from_u24(colors[0]),
            background,
            muted: Rgb::from_u24(colors[2]),
            ok: Rgb::from_u24(colors[3]),
            warn: Rgb::from_u24(colors[4]),
            danger: Rgb::from_u24(colors[5]),
            accent: Rgb::from_u24(colors[6]),
            on_accent: background,
        },
    }
}

const PRESETS: &[PresetSpec] = &[
    preset(
        "tokyo-night",
        [
            0xc0caf5, 0x1a1b26, 0x565f89, 0x9ece6a, 0xe0af68, 0xf7768e, 0x7aa2f7,
        ],
    ),
    preset(
        "tokyo-night-storm",
        [
            0xc0caf5, 0x24283b, 0x565f89, 0x9ece6a, 0xe0af68, 0xf7768e, 0x7aa2f7,
        ],
    ),
    preset(
        "solarized-dark",
        [
            0x839496, 0x002b36, 0x586e75, 0x859900, 0xb58900, 0xdc322f, 0x268bd2,
        ],
    ),
    preset(
        "solarized-light",
        [
            0x657b83, 0xfdf6e3, 0x93a1a1, 0x859900, 0xb58900, 0xdc322f, 0x268bd2,
        ],
    ),
    preset(
        "gruvbox-dark",
        [
            0xebdbb2, 0x282828, 0x928374, 0xb8bb26, 0xfabd2f, 0xfb4934, 0x83a598,
        ],
    ),
    preset(
        "gruvbox-light",
        [
            0x3c3836, 0xfbf1c7, 0x7c6f64, 0x79740e, 0xb57614, 0x9d0006, 0x076678,
        ],
    ),
    preset(
        "dracula",
        [
            0xf8f8f2, 0x282a36, 0x6272a4, 0x50fa7b, 0xf1fa8c, 0xff5555, 0xbd93f9,
        ],
    ),
    preset(
        "nord",
        [
            0xd8dee9, 0x2e3440, 0x616e88, 0xa3be8c, 0xebcb8b, 0xbf616a, 0x88c0d0,
        ],
    ),
    preset(
        "catppuccin-mocha",
        [
            0xcdd6f4, 0x1e1e2e, 0x6c7086, 0xa6e3a1, 0xf9e2af, 0xf38ba8, 0x89b4fa,
        ],
    ),
    preset(
        "one-dark",
        [
            0xabb2bf, 0x282c34, 0x7f848e, 0x98c379, 0xe5c07b, 0xe06c75, 0x61afef,
        ],
    ),
    preset(
        "monokai",
        [
            0xf8f8f2, 0x272822, 0x75715e, 0xa6e22e, 0xe6db74, 0xf92672, 0x66d9ef,
        ],
    ),
    preset(
        "everforest-dark",
        [
            0xd3c6aa, 0x2d353b, 0x859289, 0xa7c080, 0xdbbc7f, 0xe67e80, 0x83c092,
        ],
    ),
];

pub fn preset_names() -> Vec<&'static str> {
    std::iter::once(DEFAULT_THEME_NAME)
        .chain(PRESETS.iter().map(|spec| spec.name))
        .collect()
}

impl Theme {
    pub fn preset(name: &str) -> Result<Theme, ThemeError> {
        if name == DEFAULT_THEME_NAME {
            return Ok(Theme::Ansi);
        }
        PRESETS
            .iter()
            .find(|spec| spec.name == name)
            .map(|spec| Theme::Colored {
                name: spec.name.to_string(),
                palette: spec.palette.clone(),
                paints_background: true,
            })
            .ok_or_else(|| ThemeError::UnknownTheme(name.to_string()))
    }

    pub fn with_overrides(self, overrides: &[(ThemeRole, Rgb)]) -> Theme {
        match self {
            Theme::Ansi => Theme::Ansi,
            Theme::Colored {
                name,
                palette,
                paints_background,
            } => Theme::Colored {
                name,
                palette: overrides.iter().fold(palette, |palette, (role, color)| {
                    palette.with(*role, *color)
                }),
                paints_background,
            },
        }
    }

    pub fn without_background(self) -> Theme {
        match self {
            Theme::Ansi => Theme::Ansi,
            Theme::Colored { name, palette, .. } => Theme::Colored {
                name,
                palette,
                paints_background: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMUM_TEXT_CONTRAST: f64 = 4.0;
    const MINIMUM_SEMANTIC_CONTRAST: f64 = 2.9;
    const MINIMUM_MUTED_CONTRAST: f64 = 2.3;
    const MINIMUM_ON_ACCENT_CONTRAST: f64 = 3.0;
    const LIGHT_PRESETS: [&str; 2] = ["solarized-light", "gruvbox-light"];
    const LIGHT_BACKGROUND_MINIMUM_LUMINANCE: f64 = 0.5;
    const DARK_BACKGROUND_MAXIMUM_LUMINANCE: f64 = 0.2;
    const DOCUMENTED_NAMES: &[&str] = &[
        "default",
        "tokyo-night",
        "tokyo-night-storm",
        "solarized-dark",
        "solarized-light",
        "gruvbox-dark",
        "gruvbox-light",
        "dracula",
        "nord",
        "catppuccin-mocha",
        "one-dark",
        "monokai",
        "everforest-dark",
    ];

    fn palette_of(name: &str) -> Palette {
        match Theme::preset(name).unwrap() {
            Theme::Colored { palette, .. } => palette,
            Theme::Ansi => panic!("{name} não tem paleta"),
        }
    }

    #[test]
    fn parses_lowercase_and_uppercase_hex() {
        assert_eq!(
            Rgb::from_hex("#1a2B3c").unwrap(),
            Rgb::new(0x1a, 0x2b, 0x3c)
        );
        assert_eq!(Rgb::from_hex("#FFFFFF").unwrap(), Rgb::new(255, 255, 255));
    }

    #[test]
    fn invalid_hex_is_rejected() {
        for bad in [
            "#12345", "#1234567", "#gggggg", "123456", "", " #123456", "#12 456", "#+12345",
            "#-12345",
        ] {
            assert_eq!(
                Rgb::from_hex(bad),
                Err(ThemeError::InvalidHex(bad.to_string())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn to_hex_round_trips() {
        let color = Rgb::new(0, 171, 255);
        assert_eq!(color.to_hex(), "#00abff");
        assert_eq!(Rgb::from_hex(&color.to_hex()).unwrap(), color);
    }

    #[test]
    fn from_u24_splits_the_channels() {
        assert_eq!(Rgb::from_u24(0x1a2b3c), Rgb::new(0x1a, 0x2b, 0x3c));
    }

    #[test]
    fn contrast_ratio_matches_known_values() {
        let black = Rgb::new(0, 0, 0);
        let white = Rgb::new(255, 255, 255);
        assert!((black.contrast_ratio(white) - 21.0).abs() < 0.001);
        assert!((white.contrast_ratio(black) - 21.0).abs() < 0.001);
        let gray = Rgb::new(0x77, 0x77, 0x77);
        assert!((gray.contrast_ratio(white) - 4.48).abs() < 0.01);
        assert!((gray.contrast_ratio(gray) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn role_keys_round_trip_and_unknown_keys_fail() {
        for role in ThemeRole::ALL {
            assert_eq!(ThemeRole::from_key(role.key()).unwrap(), role);
        }
        assert_eq!(
            ThemeRole::from_key("fundo"),
            Err(ThemeError::UnknownRole("fundo".to_string()))
        );
    }

    #[test]
    fn every_preset_name_is_kebab_case_and_unique() {
        let names = preset_names();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        for name in names {
            assert!(
                name.chars()
                    .all(|character| character.is_ascii_lowercase() || character == '-'),
                "{name}"
            );
        }
    }

    #[test]
    fn catalog_has_exactly_the_documented_names() {
        assert_eq!(preset_names(), DOCUMENTED_NAMES);
    }

    #[test]
    fn preset_resolves_each_catalog_name() {
        for name in preset_names() {
            assert!(Theme::preset(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn unknown_preset_names_the_theme() {
        let error = Theme::preset("neon").unwrap_err();
        assert_eq!(error, ThemeError::UnknownTheme("neon".to_string()));
        assert_eq!(error.to_string(), "tema desconhecido 'neon'");
    }

    #[test]
    fn default_preset_is_ansi_and_has_no_palette() {
        assert_eq!(Theme::preset("default").unwrap(), Theme::Ansi);
    }

    #[test]
    fn colored_presets_paint_the_background_by_default() {
        match Theme::preset("nord").unwrap() {
            Theme::Colored {
                name,
                paints_background,
                ..
            } => {
                assert_eq!(name, "nord");
                assert!(paints_background);
            }
            Theme::Ansi => panic!("nord deveria ser colorido"),
        }
    }

    #[test]
    fn on_accent_equals_background_in_every_preset() {
        for name in preset_names()
            .into_iter()
            .filter(|name| *name != DEFAULT_THEME_NAME)
        {
            let palette = palette_of(name);
            assert_eq!(palette.on_accent, palette.background, "{name}");
        }
    }

    #[test]
    fn every_preset_meets_the_contrast_minimums() {
        for name in preset_names()
            .into_iter()
            .filter(|name| *name != DEFAULT_THEME_NAME)
        {
            let palette = palette_of(name);
            let background = palette.background;
            let checks = [
                (
                    "foreground",
                    palette.foreground.contrast_ratio(background),
                    MINIMUM_TEXT_CONTRAST,
                ),
                (
                    "muted",
                    palette.muted.contrast_ratio(background),
                    MINIMUM_MUTED_CONTRAST,
                ),
                (
                    "ok",
                    palette.ok.contrast_ratio(background),
                    MINIMUM_SEMANTIC_CONTRAST,
                ),
                (
                    "warn",
                    palette.warn.contrast_ratio(background),
                    MINIMUM_SEMANTIC_CONTRAST,
                ),
                (
                    "danger",
                    palette.danger.contrast_ratio(background),
                    MINIMUM_SEMANTIC_CONTRAST,
                ),
                (
                    "accent",
                    palette.accent.contrast_ratio(background),
                    MINIMUM_SEMANTIC_CONTRAST,
                ),
                (
                    "on_accent",
                    palette.on_accent.contrast_ratio(palette.accent),
                    MINIMUM_ON_ACCENT_CONTRAST,
                ),
            ];
            for (role, ratio, minimum) in checks {
                assert!(ratio >= minimum, "{name}/{role}: {ratio:.2} < {minimum}");
            }
        }
    }

    #[test]
    fn light_and_dark_presets_have_the_expected_background_luminance() {
        for name in preset_names()
            .into_iter()
            .filter(|name| *name != DEFAULT_THEME_NAME)
        {
            let luminance = palette_of(name).background.relative_luminance();
            if LIGHT_PRESETS.contains(&name) {
                assert!(luminance > LIGHT_BACKGROUND_MINIMUM_LUMINANCE, "{name}");
            } else {
                assert!(luminance < DARK_BACKGROUND_MAXIMUM_LUMINANCE, "{name}");
            }
        }
    }

    #[test]
    fn overrides_replace_only_the_given_roles() {
        let original = palette_of("tokyo-night");
        let orange = Rgb::new(0xff, 0x9e, 0x64);
        let theme = Theme::preset("tokyo-night")
            .unwrap()
            .with_overrides(&[(ThemeRole::Accent, orange)]);
        let Theme::Colored { palette, .. } = theme else {
            panic!("deveria continuar colorido");
        };
        assert_eq!(palette.accent, orange);
        assert_eq!(palette.with(ThemeRole::Accent, original.accent), original);
    }

    #[test]
    fn overrides_on_the_ansi_theme_do_nothing() {
        let theme = Theme::Ansi.with_overrides(&[(ThemeRole::Accent, Rgb::new(1, 2, 3))]);
        assert_eq!(theme, Theme::Ansi);
    }

    #[test]
    fn without_background_keeps_the_palette() {
        let theme = Theme::preset("dracula").unwrap().without_background();
        match theme {
            Theme::Colored {
                palette,
                paints_background,
                ..
            } => {
                assert!(!paints_background);
                assert_eq!(palette, palette_of("dracula"));
            }
            Theme::Ansi => panic!("deveria continuar colorido"),
        }
        assert_eq!(Theme::Ansi.without_background(), Theme::Ansi);
    }
}
