const ELLIPSIS: char = '…';

const TONES: [(&str, Tone); 6] = [
    ("normal", Tone::Normal),
    ("muted", Tone::Muted),
    ("ok", Tone::Ok),
    ("warn", Tone::Warn),
    ("danger", Tone::Danger),
    ("current", Tone::Current),
];

pub const TONE_NAMES: [&str; 6] = tone_names();

const fn tone_names() -> [&'static str; 6] {
    let mut names = [""; 6];
    let mut index = 0;
    while index < TONES.len() {
        names[index] = TONES[index].0;
        index += 1;
    }
    names
}

const FORMAT_CHARACTER_RANGES: [(u32, u32); 19] = [
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE007F),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Muted,
    Ok,
    Warn,
    Danger,
    Current,
}

impl Tone {
    pub fn parse(text: &str) -> Option<Tone> {
        TONES
            .iter()
            .find(|(name, _)| *name == text)
            .map(|(_, tone)| *tone)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertLevel {
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub body: String,
    pub level: AlertLevel,
}

fn is_format_character(character: char) -> bool {
    let code = character as u32;
    FORMAT_CHARACTER_RANGES
        .iter()
        .any(|&(first, last)| (first..=last).contains(&code))
}

pub fn clean_text(text: &str) -> String {
    let spaced: String = text
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            other => other,
        })
        .filter(|&character| !character.is_control() && !is_format_character(character))
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clean_text_limited(text: &str, limit: usize) -> String {
    let cleaned = clean_text(text);
    if cleaned.chars().count() <= limit {
        return cleaned;
    }
    let mut truncated: String = cleaned.chars().take(limit.saturating_sub(1)).collect();
    if limit > 0 {
        truncated.push(ELLIPSIS);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_parse_accepts_known_names_only() {
        for name in TONE_NAMES {
            assert!(Tone::parse(name).is_some(), "{name}");
        }
        assert_eq!(Tone::parse("danger"), Some(Tone::Danger));
        assert_eq!(Tone::parse("Danger"), None);
        assert_eq!(Tone::parse("rosa"), None);
    }

    #[test]
    fn tone_names_follow_the_declaration_order() {
        assert_eq!(
            TONE_NAMES,
            ["normal", "muted", "ok", "warn", "danger", "current"]
        );
    }

    #[test]
    fn clean_text_removes_control_and_format_characters() {
        assert_eq!(clean_text("a\u{1b}[31mb\u{202e}c\u{0}d"), "a[31mbcd");
    }

    #[test]
    fn clean_text_turns_line_breaks_and_tabs_into_single_spaces() {
        assert_eq!(clean_text("  a\n\n b\t\tc\r\nd  "), "a b c d");
    }

    #[test]
    fn clean_text_limited_truncates_with_ellipsis_by_characters() {
        assert_eq!(clean_text_limited("abcdef", 4), "abc…");
        assert_eq!(clean_text_limited("abcd", 4), "abcd");
        assert_eq!(clean_text_limited("日本語日本語", 3), "日本…");
    }

    #[test]
    fn clean_text_handles_empty_and_whitespace_only() {
        assert_eq!(clean_text(""), "");
        assert_eq!(clean_text(" \n\t "), "");
        assert_eq!(clean_text_limited("", 0), "");
    }
}
