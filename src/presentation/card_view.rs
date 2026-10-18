use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::domain::card::{Block, CardLine, Row, Tag, Tone};
use crate::domain::widget::Flag;

const ELLIPSIS: char = '…';
const MARKER_COLUMN_WIDTH: usize = 2;
const BLANK_MARKER: &str = "  ";
const TAG_SEPARATOR: &str = " ";

type Piece = (String, Style);

pub fn tone_style(tone: Tone) -> Style {
    match tone {
        Tone::Normal => Style::default(),
        Tone::Muted => Style::default().fg(Color::DarkGray),
        Tone::Ok => Style::default().fg(Color::Green),
        Tone::Warn => Style::default().fg(Color::Yellow),
        Tone::Danger => Style::default().fg(Color::Red),
        Tone::Current => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    }
}

pub fn flagged(flag: Option<Flag>, text: &str) -> String {
    match flag {
        Some(flag) => format!("{} {text}", flag.symbol()),
        None => text.to_string(),
    }
}

pub fn truncate_to_width(text: &str, width: usize) -> String {
    if text.width() <= width {
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

pub fn render_blocks(blocks: &[Block], width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![];
    }
    blocks
        .iter()
        .flat_map(|block| render_block(block, width))
        .collect()
}

pub fn lines_to_plain_text(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

fn render_block(block: &Block, width: usize) -> Vec<Line<'static>> {
    match block {
        Block::Lines(lines) => lines
            .iter()
            .map(|line| render_card_line(line, width))
            .collect(),
        Block::Rows(rows) => render_rows(rows, width),
        Block::Stat { .. } | Block::Table { .. } => vec![],
    }
}

fn render_card_line(line: &CardLine, width: usize) -> Line<'static> {
    let text = truncate_to_width(&flagged(line.flag, &line.text), width);
    Line::from(Span::styled(text, tone_style(line.tone)))
}

fn pieces_width(pieces: &[Piece]) -> usize {
    pieces.iter().map(|(text, _)| text.width()).sum()
}

fn clip_pieces(pieces: Vec<Piece>, width: usize) -> Line<'static> {
    let mut remaining = width;
    let mut spans = vec![];
    for (text, style) in pieces {
        let clipped = hard_clip(&text, remaining);
        remaining -= clipped.width();
        if !clipped.is_empty() {
            spans.push(Span::styled(clipped, style));
        }
    }
    Line::from(spans)
}

fn render_rows(rows: &[Row], width: usize) -> Vec<Line<'static>> {
    let time_width = rows
        .iter()
        .filter_map(|row| row.time.as_deref())
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|row| render_row(row, time_width, width))
        .collect()
}

fn marker_text(marker: Option<char>) -> String {
    let Some(marker) = marker else {
        return BLANK_MARKER.to_string();
    };
    match marker.width().unwrap_or(0) {
        0 => BLANK_MARKER.to_string(),
        marker_width if marker_width >= MARKER_COLUMN_WIDTH => marker.to_string(),
        _ => format!("{marker} "),
    }
}

fn render_row(row: &Row, time_width: usize, width: usize) -> Line<'static> {
    let row_style = tone_style(row.tone);
    let mut pieces: Vec<Piece> = vec![(marker_text(row.marker), row_style)];
    if time_width > 0 {
        let time = row.time.as_deref().unwrap_or("");
        let padding = " ".repeat(time_width - time.width());
        pieces.push((format!("{time}{padding}{TAG_SEPARATOR}"), Style::default()));
    }
    let remaining = width.saturating_sub(pieces_width(&pieces));
    let text = flagged(row.flag, &row.text);
    let tags = tag_pieces(&row.tags);
    let tags_width = pieces_width(&tags);
    let separator_width = TAG_SEPARATOR.width();
    let (text_budget, tags_budget) = if tags.is_empty() {
        (remaining, 0)
    } else if text.width() + separator_width + tags_width <= remaining {
        (text.width(), tags_width)
    } else {
        let text_budget = remaining.saturating_sub(tags_width + separator_width);
        let separator = if text_budget > 0 { separator_width } else { 0 };
        (
            text_budget,
            remaining.saturating_sub(text_budget + separator),
        )
    };
    let text = truncate_to_width(&text, text_budget);
    let used_by_text = text.width();
    pieces.push((text, row_style));
    let tags = fit_pieces(tags, tags_budget);
    let gap = remaining.saturating_sub(used_by_text + pieces_width(&tags));
    if !tags.is_empty() {
        pieces.push((" ".repeat(gap), Style::default()));
        pieces.extend(tags);
    }
    clip_pieces(pieces, width)
}

fn tag_pieces(tags: &[Tag]) -> Vec<Piece> {
    let mut pieces = vec![];
    for (index, tag) in tags.iter().enumerate() {
        if index > 0 {
            pieces.push((TAG_SEPARATOR.to_string(), Style::default()));
        }
        pieces.push((format!("[{}]", tag.text), tone_style(tag.tone)));
    }
    pieces
}

fn fit_pieces(pieces: Vec<Piece>, budget: usize) -> Vec<Piece> {
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
        remaining -= shown.width();
        fitted.push((shown, style));
    }
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::card::{CardLine, Row, Tag};
    use crate::domain::widget::Flag;
    use ratatui::style::{Color, Modifier};

    fn line(text: &str, flag: Option<Flag>) -> CardLine {
        CardLine {
            text: text.to_string(),
            tone: Tone::Normal,
            flag,
        }
    }

    fn row(time: Option<&str>, text: &str, tags: &[&str], marker: Option<char>) -> Row {
        Row {
            time: time.map(str::to_string),
            text: text.to_string(),
            tags: tags
                .iter()
                .map(|tag| Tag {
                    text: tag.to_string(),
                    tone: Tone::Warn,
                })
                .collect(),
            tone: Tone::Normal,
            marker,
            flag: None,
        }
    }

    fn plain(blocks: &[Block], width: usize) -> Vec<String> {
        lines_to_plain_text(&render_blocks(blocks, width))
    }

    fn widest(lines: &[String]) -> usize {
        lines.iter().map(|text| text.width()).max().unwrap_or(0)
    }

    #[test]
    fn lines_block_renders_flag_before_text() {
        let blocks = [Block::Lines(vec![
            line("alfa", Some(Flag::Warning)),
            line("beta", None),
        ])];
        assert_eq!(plain(&blocks, 20), vec!["⚠ alfa", "beta"]);
    }

    #[test]
    fn lines_are_truncated_to_the_width() {
        let blocks = [Block::Lines(vec![line("abcdefghij", None)])];
        assert_eq!(plain(&blocks, 5), vec!["abcd…"]);
    }

    #[test]
    fn rows_align_the_time_column() {
        let blocks = [Block::Rows(vec![
            row(Some("9:00"), "um", &[], None),
            row(Some("10:30"), "dois", &[], None),
            row(None, "tres", &[], None),
        ])];
        assert_eq!(
            plain(&blocks, 30),
            vec!["  9:00  um", "  10:30 dois", "        tres"]
        );
    }

    #[test]
    fn rows_right_align_tags() {
        let blocks = [Block::Rows(vec![row(
            None,
            "reunião",
            &["agora", "x"],
            None,
        )])];
        let expected = format!("  reunião{}[agora] [x]", " ".repeat(10));
        assert_eq!(plain(&blocks, 30), vec![expected]);
    }

    #[test]
    fn rows_marker_is_shown_or_replaced_by_spaces() {
        let blocks = [Block::Rows(vec![
            row(None, "a", &[], Some('▶')),
            row(None, "b", &[], None),
        ])];
        assert_eq!(plain(&blocks, 10), vec!["▶ a", "  b"]);
    }

    #[test]
    fn rows_text_gives_way_to_tags_when_narrow() {
        let blocks = [Block::Rows(vec![row(
            None,
            "reunião longa demais",
            &["agora"],
            None,
        )])];
        let lines = plain(&blocks, 18);
        assert_eq!(lines, vec!["  reunião… [agora]"]);
    }

    #[test]
    fn rows_cut_tags_when_even_they_do_not_fit() {
        let blocks = [Block::Rows(vec![row(
            None,
            "texto",
            &["muito-longa"],
            None,
        )])];
        let lines = plain(&blocks, 8);
        assert!(lines[0].width() <= 8, "{:?}", lines);
        assert!(lines[0].contains('['));
    }

    #[test]
    fn no_line_is_ever_wider_than_the_width() {
        let wide = "日本語の長いテキスト".repeat(3);
        let blocks = [
            Block::Lines(vec![line(&wide, Some(Flag::Updated))]),
            Block::Rows(vec![
                row(
                    Some("12:34:56.789"),
                    &wide,
                    &["etiqueta-longa", "日本", "x"],
                    Some('日'),
                ),
                row(None, "curto", &["a"], None),
            ]),
        ];
        for width in 0..=40 {
            let lines = plain(&blocks, width);
            assert!(widest(&lines) <= width, "largura {width}: {lines:?}");
        }
    }

    #[test]
    fn width_zero_and_one_do_not_panic() {
        let blocks = [Block::Rows(vec![row(Some("1"), "a", &["t"], Some('x'))])];
        assert!(render_blocks(&blocks, 0).is_empty());
        assert!(widest(&plain(&blocks, 1)) <= 1);
    }

    #[test]
    fn tone_style_maps_every_tone() {
        let normal = tone_style(Tone::Normal);
        for tone in [
            Tone::Muted,
            Tone::Ok,
            Tone::Warn,
            Tone::Danger,
            Tone::Current,
        ] {
            assert_ne!(tone_style(tone), normal, "{tone:?}");
        }
        assert_eq!(tone_style(Tone::Ok).fg, Some(Color::Green));
        assert_eq!(tone_style(Tone::Warn).fg, Some(Color::Yellow));
        assert_eq!(tone_style(Tone::Danger).fg, Some(Color::Red));
        assert_eq!(tone_style(Tone::Muted).fg, Some(Color::DarkGray));
    }

    #[test]
    fn current_tone_is_bold() {
        let style = tone_style(Tone::Current);
        assert_eq!(style.fg, Some(Color::Cyan));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }
}
