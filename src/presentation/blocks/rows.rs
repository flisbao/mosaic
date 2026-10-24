use ratatui::style::Style;
use ratatui::text::Line;

use super::BlockRenderer;
use crate::domain::blocks::{
    rows::{Row, Tag},
    Block,
};
use crate::presentation::text_fit::{
    clip_pieces, display_width, fit_pieces, flagged, pieces_width, truncate_to_width, Piece,
    TAG_SEPARATOR,
};
use crate::presentation::theme_styles::ThemeStyles;

pub struct RowsRenderer;

impl BlockRenderer for RowsRenderer {
    fn render(
        &self,
        block: &Block,
        width: usize,
        styles: &ThemeStyles,
    ) -> Option<Vec<Line<'static>>> {
        match block {
            Block::Rows(rows) => Some(render_rows(rows, width, styles)),
            _ => None,
        }
    }
}

fn render_rows(rows: &[Row], width: usize, styles: &ThemeStyles) -> Vec<Line<'static>> {
    let time_width = rows
        .iter()
        .filter_map(|row| row.time.as_deref())
        .map(display_width)
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|row| render_row(row, time_width, width, styles))
        .collect()
}

fn render_row(row: &Row, time_width: usize, width: usize, styles: &ThemeStyles) -> Line<'static> {
    let row_style = styles.tone(row.tone);
    let mut pieces: Vec<Piece> = vec![];
    if time_width > 0 {
        let time = row.time.as_deref().unwrap_or("");
        let padding = " ".repeat(time_width.saturating_sub(display_width(time)));
        pieces.push((format!("{time}{padding}{TAG_SEPARATOR}"), Style::default()));
    }
    let remaining = width.saturating_sub(pieces_width(&pieces));
    let text = flagged(row.flag, &row.text);
    let tags = tag_pieces(&row.tags, styles);
    let tags_width = pieces_width(&tags);
    let separator_width = display_width(TAG_SEPARATOR);
    let (text_budget, tags_budget) = if tags.is_empty() {
        (remaining, 0)
    } else if display_width(&text) + separator_width + tags_width <= remaining {
        (display_width(&text), tags_width)
    } else {
        let text_budget = remaining.saturating_sub(tags_width + separator_width);
        let separator = if text_budget > 0 { separator_width } else { 0 };
        (
            text_budget,
            remaining.saturating_sub(text_budget + separator),
        )
    };
    let text = truncate_to_width(&text, text_budget);
    let used_by_text = display_width(&text);
    pieces.push((text, row_style));
    let tags = fit_pieces(tags, tags_budget);
    let gap = remaining.saturating_sub(used_by_text + pieces_width(&tags));
    if !tags.is_empty() {
        pieces.push((" ".repeat(gap), Style::default()));
        pieces.extend(tags);
    }
    clip_pieces(pieces, width)
}

fn tag_pieces(tags: &[Tag], styles: &ThemeStyles) -> Vec<Piece> {
    let mut pieces = vec![];
    for (index, tag) in tags.iter().enumerate() {
        if index > 0 {
            pieces.push((TAG_SEPARATOR.to_string(), Style::default()));
        }
        pieces.push((format!("[{}]", tag.text), styles.tone(tag.tone)));
    }
    pieces
}
