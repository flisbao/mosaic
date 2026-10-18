use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

use super::BlockRenderer;
use crate::domain::widget_content::{Align, Block, Cell, Column};
use crate::presentation::text_fit::{
    clip_pieces, display_width, flagged, tone_style, truncate_to_width, Piece,
};

const COLUMN_SEPARATOR: &str = "  ";

pub struct TableRenderer;

impl BlockRenderer for TableRenderer {
    fn render(&self, block: &Block, width: usize) -> Option<Vec<Line<'static>>> {
        match block {
            Block::Table { columns, rows } => Some(render_table(columns, rows, width)),
            _ => None,
        }
    }
}

fn cell_text(cell: &Cell) -> String {
    flagged(cell.flag, &cell.text)
}

fn column_widths(columns: &[Column], rows: &[Vec<Cell>]) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let widest_cell = rows
                .iter()
                .filter_map(|row| row.get(index))
                .map(|cell| display_width(&cell_text(cell)))
                .max()
                .unwrap_or(0);
            display_width(&column.title).max(widest_cell)
        })
        .collect()
}

fn shrink_to_fit(widths: &mut [usize], width: usize) {
    let separators = display_width(COLUMN_SEPARATOR) * widths.len().saturating_sub(1);
    while widths.iter().sum::<usize>() + separators > width {
        let Some((widest, &size)) = widths
            .iter()
            .enumerate()
            .rev()
            .max_by_key(|(_, size)| **size)
        else {
            return;
        };
        if size <= 1 {
            return;
        }
        widths[widest] -= 1;
    }
}

fn render_table(columns: &[Column], rows: &[Vec<Cell>], width: usize) -> Vec<Line<'static>> {
    let separator_width = display_width(COLUMN_SEPARATOR);
    let visible = columns
        .len()
        .min((width + separator_width) / (1 + separator_width));
    let columns = &columns[..visible];
    let mut widths = column_widths(columns, rows);
    shrink_to_fit(&mut widths, width);
    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let header = columns
        .iter()
        .map(|column| (column.title.clone(), header_style))
        .collect::<Vec<_>>();
    let mut lines = vec![table_line(&header, columns, &widths, width)];
    for row in rows {
        let cells = (0..visible)
            .map(|index| match row.get(index) {
                Some(cell) => (cell_text(cell), tone_style(cell.tone)),
                None => (String::new(), Style::default()),
            })
            .collect::<Vec<_>>();
        lines.push(table_line(&cells, columns, &widths, width));
    }
    lines
}

fn table_line(
    cells: &[Piece],
    columns: &[Column],
    widths: &[usize],
    width: usize,
) -> Line<'static> {
    let mut pieces: Vec<Piece> = vec![];
    let last = cells.len().saturating_sub(1);
    for (index, (text, style)) in cells.iter().enumerate() {
        if index > 0 {
            pieces.push((COLUMN_SEPARATOR.to_string(), Style::default()));
        }
        let shown = truncate_to_width(text, widths[index]);
        let padding = " ".repeat(widths[index].saturating_sub(display_width(&shown)));
        match columns[index].align {
            Align::Right => pieces.push((format!("{padding}{shown}"), *style)),
            Align::Left if index == last => pieces.push((shown, *style)),
            Align::Left => pieces.push((format!("{shown}{padding}"), *style)),
        }
    }
    clip_pieces(pieces, width)
}
