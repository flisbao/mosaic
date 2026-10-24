use ratatui::text::{Line, Span};

use super::BlockRenderer;
use crate::domain::blocks::{lines::WidgetLine, Block};
use crate::presentation::text_fit::{flagged, truncate_to_width};
use crate::presentation::theme_styles::ThemeStyles;

pub struct LinesRenderer;

impl BlockRenderer for LinesRenderer {
    fn render(
        &self,
        block: &Block,
        width: usize,
        styles: &ThemeStyles,
    ) -> Option<Vec<Line<'static>>> {
        match block {
            Block::Lines(lines) => Some(
                lines
                    .iter()
                    .map(|line| render_widget_line(line, width, styles))
                    .collect(),
            ),
            _ => None,
        }
    }
}

fn render_widget_line(line: &WidgetLine, width: usize, styles: &ThemeStyles) -> Line<'static> {
    let text = truncate_to_width(&flagged(line.flag, &line.text), width);
    Line::from(Span::styled(text, styles.tone(line.tone)))
}
