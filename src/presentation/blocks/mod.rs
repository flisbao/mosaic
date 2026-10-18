mod lines;
mod rows;
mod stat;
mod table;

use ratatui::text::Line;

use crate::domain::blocks::Block;

use lines::LinesRenderer;
use rows::RowsRenderer;
use stat::StatRenderer;
use table::TableRenderer;

const INVALID_BLOCK_TEXT: &str = "⚠ bloco inválido";

pub trait BlockRenderer: Sync {
    fn render(&self, block: &Block, width: usize) -> Option<Vec<Line<'static>>>;
}

const RENDERERS: [&dyn BlockRenderer; 4] =
    [&LinesRenderer, &RowsRenderer, &StatRenderer, &TableRenderer];

pub fn render_block(block: &Block, width: usize) -> Vec<Line<'static>> {
    render_block_with(&RENDERERS, block, width)
}

fn render_block_with(
    renderers: &[&dyn BlockRenderer],
    block: &Block,
    width: usize,
) -> Vec<Line<'static>> {
    renderers
        .iter()
        .find_map(|renderer| renderer.render(block, width))
        .unwrap_or_else(|| vec![Line::from(INVALID_BLOCK_TEXT)])
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RejectAll;

    impl BlockRenderer for RejectAll {
        fn render(&self, _block: &Block, _width: usize) -> Option<Vec<Line<'static>>> {
            None
        }
    }

    #[test]
    fn no_renderer_accepting_a_block_yields_invalid_block_line() {
        let lines = render_block_with(&[&RejectAll], &Block::Lines(vec![]), 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, INVALID_BLOCK_TEXT);
    }
}
