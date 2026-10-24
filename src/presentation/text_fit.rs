use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::domain::widget::Flag;
use crate::domain::widget_content::ELLIPSIS;

pub const TAG_SEPARATOR: &str = " ";

pub type Piece = (String, Style);

pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| character.width().unwrap_or(0))
        .sum()
}

pub fn flagged(flag: Option<Flag>, text: &str) -> String {
    match flag {
        Some(flag) => format!("{} {text}", flag.symbol()),
        None => text.to_string(),
    }
}

pub fn truncate_to_width(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(ELLIPSIS.width().unwrap_or(1));
    let mut truncated = hard_clip(text, budget);
    if width > 0 {
        truncated.push(ELLIPSIS);
    }
    truncated
}

fn hard_clip(text: &str, budget: usize) -> String {
    let mut clipped = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > budget {
            break;
        }
        clipped.push(character);
        used += character_width;
    }
    clipped
}

pub fn pieces_width(pieces: &[Piece]) -> usize {
    pieces.iter().map(|(text, _)| display_width(text)).sum()
}

pub fn clip_pieces(pieces: Vec<Piece>, width: usize) -> Line<'static> {
    let mut remaining = width;
    let mut spans = vec![];
    for (text, style) in pieces {
        let clipped = hard_clip(&text, remaining);
        remaining = remaining.saturating_sub(display_width(&clipped));
        if !clipped.is_empty() {
            spans.push(Span::styled(clipped, style));
        }
    }
    Line::from(spans)
}

pub fn fit_pieces(pieces: Vec<Piece>, budget: usize) -> Vec<Piece> {
    if pieces_width(&pieces) <= budget {
        return pieces;
    }
    let mut remaining = budget;
    let mut fitted = vec![];
    for (text, style) in pieces {
        if remaining == 0 {
            break;
        }
        let shown = truncate_to_width(&text, remaining);
        remaining = remaining.saturating_sub(display_width(&shown));
        fitted.push((shown, style));
    }
    fitted
}
