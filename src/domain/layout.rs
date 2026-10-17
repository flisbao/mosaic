pub const SINGLE_COLUMN_MAXIMUM_WIDTH: u16 = 79;
pub const TWO_COLUMN_MAXIMUM_WIDTH: u16 = 139;
pub const MINIMUM_FULL_WIDTH: u16 = 60;
pub const MINIMUM_FULL_HEIGHT: u16 = 20;
pub const MINIMUM_BOX_HEIGHT: u16 = 4;
pub const TOP_BAR_HEIGHT: u16 = 1;

const SINGLE_COLUMN: u16 = 1;
const TWO_COLUMNS: u16 = 2;
const THREE_COLUMNS: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub widget_index: usize,
    pub area: CellRect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridLayout {
    Grid(Vec<Placement>),
    Compact(Placement),
    Empty,
}

pub fn column_count(width: u16) -> u16 {
    if width <= SINGLE_COLUMN_MAXIMUM_WIDTH {
        SINGLE_COLUMN
    } else if width <= TWO_COLUMN_MAXIMUM_WIDTH {
        TWO_COLUMNS
    } else {
        THREE_COLUMNS
    }
}

pub fn arrange(spans: &[u16], size: TerminalSize) -> GridLayout {
    if spans.is_empty() || size.width == 0 || size.height <= TOP_BAR_HEIGHT {
        return GridLayout::Empty;
    }
    let usable_height = size.height - TOP_BAR_HEIGHT;
    if size.width < MINIMUM_FULL_WIDTH || size.height < MINIMUM_FULL_HEIGHT {
        return GridLayout::Compact(Placement {
            widget_index: 0,
            area: CellRect {
                x: 0,
                y: TOP_BAR_HEIGHT,
                width: size.width,
                height: usable_height,
            },
        });
    }
    let columns = column_count(size.width);
    let maximum_rows = usize::from(usable_height / MINIMUM_BOX_HEIGHT);
    let mut rows = flow_into_rows(spans, columns);
    rows.truncate(maximum_rows);
    GridLayout::Grid(place_rows(&rows, columns, size, usable_height))
}

fn flow_into_rows(spans: &[u16], columns: u16) -> Vec<Vec<(usize, u16)>> {
    let mut rows: Vec<Vec<(usize, u16)>> = Vec::new();
    let mut used_columns = columns;
    for (index, requested) in spans.iter().enumerate() {
        let span = (*requested).clamp(1, columns);
        if used_columns + span > columns {
            rows.push(Vec::new());
            used_columns = 0;
        }
        if let Some(row) = rows.last_mut() {
            row.push((index, span));
        }
        used_columns += span;
    }
    rows
}

fn place_rows(
    rows: &[Vec<(usize, u16)>],
    columns: u16,
    size: TerminalSize,
    usable_height: u16,
) -> Vec<Placement> {
    let row_count = rows.len() as u16;
    let row_height = usable_height / row_count;
    let column_width = size.width / columns;
    let mut placements = Vec::new();
    for (row_position, row) in rows.iter().enumerate() {
        let y = TOP_BAR_HEIGHT + row_position as u16 * row_height;
        let is_last_row = row_position + 1 == rows.len();
        let height = if is_last_row {
            size.height - y
        } else {
            row_height
        };
        let mut column_start = 0;
        for (widget_index, span) in row {
            let x = column_start * column_width;
            let reaches_last_column = column_start + span == columns;
            let width = if reaches_last_column {
                size.width - x
            } else {
                span * column_width
            };
            placements.push(Placement {
                widget_index: *widget_index,
                area: CellRect {
                    x,
                    y,
                    width,
                    height,
                },
            });
            column_start += span;
        }
    }
    placements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: u16, height: u16) -> TerminalSize {
        TerminalSize { width, height }
    }

    fn placements(layout: GridLayout) -> Vec<Placement> {
        match layout {
            GridLayout::Grid(placements) => placements,
            other => panic!("esperava Grid, veio {other:?}"),
        }
    }

    #[test]
    fn column_count_boundaries() {
        assert_eq!(column_count(79), 1);
        assert_eq!(column_count(80), 2);
        assert_eq!(column_count(139), 2);
        assert_eq!(column_count(140), 3);
    }

    #[test]
    fn three_widgets_in_two_columns_make_two_rows() {
        let result = placements(arrange(&[1, 1, 1], size(100, 31)));
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].area.y, TOP_BAR_HEIGHT);
        assert_eq!(result[1].area.y, TOP_BAR_HEIGHT);
        assert_eq!(result[1].area.x, 50);
        assert_eq!(result[2].area.y, result[0].area.y + result[0].area.height);
        assert_eq!(result[2].area.x, 0);
    }

    #[test]
    fn span_is_clamped_to_available_columns() {
        let result = placements(arrange(&[3], size(70, 25)));
        assert_eq!(result[0].area.width, 70);
    }

    #[test]
    fn spanning_widget_takes_the_full_row_width_in_two_columns() {
        let result = placements(arrange(&[2, 1], size(100, 31)));
        assert_eq!(result[0].area.width, 100);
        assert_eq!(result[1].area.y, result[0].area.y + result[0].area.height);
    }

    #[test]
    fn rows_that_do_not_fit_are_dropped() {
        let result = placements(arrange(&[1, 1, 1, 1, 1, 1], size(70, 20)));
        assert_eq!(result.len(), 4);
        assert!(result
            .iter()
            .all(|placement| placement.area.height >= MINIMUM_BOX_HEIGHT));
    }

    #[test]
    fn below_minimum_size_is_compact_with_first_widget() {
        let layout = arrange(&[1, 1], size(59, 30));
        match layout {
            GridLayout::Compact(placement) => {
                assert_eq!(placement.widget_index, 0);
                assert_eq!(placement.area.y, TOP_BAR_HEIGHT);
                assert_eq!(placement.area.height, 29);
            }
            other => panic!("esperava Compact, veio {other:?}"),
        }
        assert!(matches!(
            arrange(&[1], size(100, 19)),
            GridLayout::Compact(_)
        ));
    }

    #[test]
    fn zero_sized_terminal_is_empty() {
        assert_eq!(arrange(&[1], size(0, 0)), GridLayout::Empty);
        assert_eq!(arrange(&[1], size(100, 0)), GridLayout::Empty);
        assert_eq!(arrange(&[1], size(0, 30)), GridLayout::Empty);
        assert_eq!(arrange(&[], size(100, 30)), GridLayout::Empty);
    }

    #[test]
    fn placements_never_exceed_terminal_area() {
        let spans = [1, 2, 3, 1, 1, 1, 2, 0];
        for width in 0..=200u16 {
            for height in 0..=200u16 {
                let areas: Vec<CellRect> = match arrange(&spans, size(width, height)) {
                    GridLayout::Grid(placements) => placements.iter().map(|p| p.area).collect(),
                    GridLayout::Compact(placement) => vec![placement.area],
                    GridLayout::Empty => vec![],
                };
                for area in areas {
                    assert!(area.x + area.width <= width, "{width}x{height}");
                    assert!(area.y + area.height <= height, "{width}x{height}");
                    assert!(area.width > 0 && area.height > 0, "{width}x{height}");
                }
            }
        }
    }
}
