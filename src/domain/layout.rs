pub const SINGLE_COLUMN_MAXIMUM_WIDTH: u16 = 79;
pub const TWO_COLUMN_MAXIMUM_WIDTH: u16 = 139;
pub const MINIMUM_FULL_WIDTH: u16 = 60;
pub const MINIMUM_FULL_HEIGHT: u16 = 20;
pub const MINIMUM_BOX_HEIGHT: u16 = 3;
pub const TOP_BAR_HEIGHT: u16 = 1;
const BOX_BORDER_HEIGHT: u16 = 2;

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

pub fn visible_item_capacity(box_height: u16, max_items: Option<usize>) -> usize {
    let inner_height = usize::from(box_height.saturating_sub(BOX_BORDER_HEIGHT));
    max_items.map_or(inner_height, |limit| limit.min(inner_height))
}

pub fn arrange(spans: &[u16], size: TerminalSize) -> GridLayout {
    if spans.is_empty() || size.width == 0 || size.height <= TOP_BAR_HEIGHT {
        return GridLayout::Empty;
    }
    let usable_height = size.height.saturating_sub(TOP_BAR_HEIGHT);
    if size.width < MINIMUM_FULL_WIDTH || size.height < MINIMUM_FULL_HEIGHT {
        return compact_layout(size, usable_height);
    }
    let columns = column_count(size.width);
    let rows = flow_into_rows(spans, columns);
    let row_count = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    if row_count.saturating_mul(MINIMUM_BOX_HEIGHT) > usable_height {
        return compact_layout(size, usable_height);
    }
    GridLayout::Grid(place_rows(&rows, columns, size, usable_height))
}

fn compact_layout(size: TerminalSize, usable_height: u16) -> GridLayout {
    GridLayout::Compact(Placement {
        widget_index: 0,
        area: CellRect {
            x: 0,
            y: TOP_BAR_HEIGHT,
            width: size.width,
            height: usable_height,
        },
    })
}

fn flow_into_rows(spans: &[u16], columns: u16) -> Vec<Vec<(usize, u16)>> {
    let mut rows: Vec<Vec<(usize, u16)>> = Vec::new();
    let mut used_columns = columns;
    for (index, requested) in spans.iter().enumerate() {
        let span = (*requested).clamp(1, columns);
        if used_columns.saturating_add(span) > columns {
            rows.push(Vec::new());
            used_columns = 0;
        }
        if let Some(row) = rows.last_mut() {
            row.push((index, span));
        }
        used_columns = used_columns.saturating_add(span);
    }
    rows
}

fn place_rows(
    rows: &[Vec<(usize, u16)>],
    columns: u16,
    size: TerminalSize,
    usable_height: u16,
) -> Vec<Placement> {
    let row_count = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    let row_height = usable_height.checked_div(row_count).unwrap_or(0);
    let column_width = size.width.checked_div(columns).unwrap_or(0);
    let mut placements = Vec::new();
    for (row_position, row) in rows.iter().enumerate() {
        let row_offset = u16::try_from(row_position).unwrap_or(u16::MAX);
        let y = TOP_BAR_HEIGHT.saturating_add(row_offset.saturating_mul(row_height));
        let is_last_row = row_position.saturating_add(1) == rows.len();
        let height = if is_last_row {
            size.height.saturating_sub(y)
        } else {
            row_height
        };
        let mut column_start: u16 = 0;
        for (widget_index, span) in row {
            let x = column_start.saturating_mul(column_width);
            let reaches_last_column = column_start.saturating_add(*span) == columns;
            let width = if reaches_last_column {
                size.width.saturating_sub(x)
            } else {
                span.saturating_mul(column_width)
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
            column_start = column_start.saturating_add(*span);
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
    fn every_widget_is_placed_when_rows_fit_at_the_floor() {
        let result = placements(arrange(&[1; 6], size(70, 20)));
        assert_eq!(result.len(), 6);
        assert!(result
            .iter()
            .all(|placement| placement.area.height >= MINIMUM_BOX_HEIGHT));
    }

    #[test]
    fn too_many_rows_for_the_floor_falls_back_to_compact() {
        assert!(matches!(
            arrange(&[1; 7], size(70, 20)),
            GridLayout::Compact(_)
        ));
    }

    #[test]
    fn no_underflow_on_tiny_terminals() {
        for width in 0..=70u16 {
            for height in 0..=70u16 {
                for widget_count in [0usize, 1, 5] {
                    let spans = vec![1u16; widget_count];
                    let _ = arrange(&spans, size(width, height));
                }
            }
        }
    }

    #[test]
    fn capacity_is_min_of_inner_height_and_max_items() {
        assert_eq!(visible_item_capacity(12, Some(5)), 5);
        assert_eq!(visible_item_capacity(12, Some(50)), 10);
        assert_eq!(visible_item_capacity(12, None), 10);
        assert_eq!(visible_item_capacity(2, None), 0);
        assert_eq!(visible_item_capacity(0, None), 0);
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
