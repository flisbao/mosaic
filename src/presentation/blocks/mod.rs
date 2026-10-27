mod lines;
mod rows;
mod stat;
mod table;

use ratatui::text::Line;

use crate::domain::blocks::Block;
use crate::presentation::theme_styles::ThemeStyles;

use lines::LinesRenderer;
use rows::RowsRenderer;
use stat::StatRenderer;
use table::TableRenderer;

const INVALID_BLOCK_TEXT: &str = "⚠ bloco inválido";

pub trait BlockRenderer: Sync {
    fn render(
        &self,
        block: &Block,
        width: usize,
        styles: &ThemeStyles,
    ) -> Option<Vec<Line<'static>>>;
}

const RENDERERS: [&dyn BlockRenderer; 4] =
    [&LinesRenderer, &RowsRenderer, &StatRenderer, &TableRenderer];

pub fn render_block(block: &Block, width: usize, styles: &ThemeStyles) -> Vec<Line<'static>> {
    render_block_with(&RENDERERS, block, width, styles)
}

fn render_block_with(
    renderers: &[&dyn BlockRenderer],
    block: &Block,
    width: usize,
    styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    renderers
        .iter()
        .find_map(|renderer| renderer.render(block, width, styles))
        .unwrap_or_else(|| vec![Line::from(INVALID_BLOCK_TEXT)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::blocks::{
        lines::WidgetLine,
        rows::Row,
        stat::StatItem,
        table::{Cell, Column},
        BLOCK_TYPE_NAMES,
    };
    use crate::domain::widget_content::{Align, Tone};
    use crate::presentation::theme_styles::ThemeStyles;

    struct RejectAll;

    impl BlockRenderer for RejectAll {
        fn render(
            &self,
            _block: &Block,
            _width: usize,
            _styles: &ThemeStyles,
        ) -> Option<Vec<Line<'static>>> {
            None
        }
    }

    fn one_block_of_each_variant() -> Vec<Block> {
        vec![
            Block::Lines(vec![WidgetLine {
                text: "linha".to_string(),
                tone: Tone::Normal,
                flag: None,
            }]),
            Block::Rows(vec![Row {
                time: None,
                text: "linha".to_string(),
                tags: vec![],
                tone: Tone::Normal,
                flag: None,
            }]),
            Block::Stat {
                items: vec![StatItem {
                    value: "1".to_string(),
                    label: "x".to_string(),
                    tone: Tone::Normal,
                }],
                hint: None,
            },
            Block::Table {
                columns: vec![Column {
                    title: "A".to_string(),
                    align: Align::Left,
                }],
                rows: vec![vec![Cell {
                    text: "x".to_string(),
                    tone: Tone::Normal,
                    flag: None,
                }]],
            },
        ]
    }

    fn is_invalid_block_line(lines: &[Line<'static>]) -> bool {
        lines.len() == 1
            && lines[0]
                .spans
                .first()
                .is_some_and(|span| span.content == INVALID_BLOCK_TEXT)
    }

    #[test]
    fn every_block_variant_has_a_renderer() {
        for block in one_block_of_each_variant() {
            let lines = render_block(&block, 40, &ThemeStyles::ansi());
            assert!(!is_invalid_block_line(&lines), "{block:?}");
        }
    }

    #[test]
    fn renderer_registry_covers_every_block_type() {
        assert_eq!(RENDERERS.len(), BLOCK_TYPE_NAMES.len());
    }

    #[test]
    fn no_renderer_accepting_a_block_yields_invalid_block_line() {
        let lines = render_block_with(
            &[&RejectAll],
            &Block::Lines(vec![]),
            10,
            &ThemeStyles::ansi(),
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, INVALID_BLOCK_TEXT);
    }
}
