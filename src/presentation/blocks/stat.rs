use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::BlockRenderer;
use crate::domain::blocks::{stat::StatItem, Block};
use crate::domain::widget_content::Tone;
use crate::presentation::text_fit::{
    clip_pieces, display_width, fit_pieces, pieces_width, truncate_to_width, Piece, TAG_SEPARATOR,
};
use crate::presentation::theme_styles::ThemeStyles;

const STAT_SEPARATOR: &str = "   ";

pub struct StatRenderer;

impl BlockRenderer for StatRenderer {
    fn render(
        &self,
        block: &Block,
        width: usize,
        styles: &ThemeStyles,
    ) -> Option<Vec<Line<'static>>> {
        match block {
            Block::Stat { items, hint } => Some(render_stat(items, hint.as_deref(), width, styles)),
            _ => None,
        }
    }
}

fn take_line(pieces: &mut Vec<Piece>, lines: &mut Vec<Line<'static>>) {
    if pieces.is_empty() {
        return;
    }
    let spans = pieces
        .drain(..)
        .map(|(text, style)| Span::styled(text, style))
        .collect::<Vec<_>>();
    lines.push(Line::from(spans));
}

fn stat_item_pieces(item: &StatItem, styles: &ThemeStyles) -> Vec<Piece> {
    let value_style = styles.tone(item.tone).add_modifier(Modifier::BOLD);
    vec![
        (item.value.clone(), value_style),
        (TAG_SEPARATOR.to_string(), Style::default()),
        (item.label.clone(), Style::default()),
    ]
}

fn render_stat(
    items: &[StatItem],
    hint: Option<&str>,
    width: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut current: Vec<Piece> = vec![];
    for item in items {
        let pieces = stat_item_pieces(item, styles);
        let item_width = pieces_width(&pieces);
        if item_width > width {
            take_line(&mut current, &mut lines);
            lines.push(clip_pieces(fit_pieces(pieces, width), width));
            continue;
        }
        let separator_width = display_width(STAT_SEPARATOR);
        if !current.is_empty() && pieces_width(&current) + separator_width + item_width > width {
            take_line(&mut current, &mut lines);
        }
        if !current.is_empty() {
            current.push((STAT_SEPARATOR.to_string(), Style::default()));
        }
        current.extend(pieces);
    }
    take_line(&mut current, &mut lines);
    if let Some(hint) = hint {
        let text = truncate_to_width(hint, width);
        lines.push(Line::from(Span::styled(text, styles.tone(Tone::Muted))));
    }
    lines
}
