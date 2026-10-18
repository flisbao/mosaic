use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::domain::card::{Align, Block, CardLine, Cell, Column, Row, StatItem, Tag, Tone};
use crate::domain::widget::Flag;

const ELLIPSIS: char = '…';
const MARKER_COLUMN_WIDTH: usize = 2;
const BLANK_MARKER: &str = "  ";
const TAG_SEPARATOR: &str = " ";
const STAT_SEPARATOR: &str = "   ";
const COLUMN_SEPARATOR: &str = "  ";

type Piece = (String, Style);

pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| character.width().unwrap_or(0))
        .sum()
}

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
        Block::Stat { items, hint } => render_stat(items, hint.as_deref(), width),
        Block::Table { columns, rows } => render_table(columns, rows, width),
    }
}

fn render_card_line(line: &CardLine, width: usize) -> Line<'static> {
    let text = truncate_to_width(&flagged(line.flag, &line.text), width);
    Line::from(Span::styled(text, tone_style(line.tone)))
}

fn pieces_width(pieces: &[Piece]) -> usize {
    pieces.iter().map(|(text, _)| display_width(text)).sum()
}

fn clip_pieces(pieces: Vec<Piece>, width: usize) -> Line<'static> {
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

fn render_rows(rows: &[Row], width: usize) -> Vec<Line<'static>> {
    let time_width = rows
        .iter()
        .filter_map(|row| row.time.as_deref())
        .map(display_width)
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
        let padding = " ".repeat(time_width.saturating_sub(display_width(time)));
        pieces.push((format!("{time}{padding}{TAG_SEPARATOR}"), Style::default()));
    }
    let remaining = width.saturating_sub(pieces_width(&pieces));
    let text = flagged(row.flag, &row.text);
    let tags = tag_pieces(&row.tags);
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

fn stat_item_pieces(item: &StatItem) -> Vec<Piece> {
    let value_style = tone_style(item.tone).add_modifier(Modifier::BOLD);
    vec![
        (item.value.clone(), value_style),
        (TAG_SEPARATOR.to_string(), Style::default()),
        (item.label.clone(), Style::default()),
    ]
}

fn render_stat(items: &[StatItem], hint: Option<&str>, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut current: Vec<Piece> = vec![];
    for item in items {
        let pieces = stat_item_pieces(item);
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
        lines.push(Line::from(Span::styled(text, tone_style(Tone::Muted))));
    }
    lines
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
        remaining = remaining.saturating_sub(display_width(&shown));
        fitted.push((shown, style));
    }
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::card::{Align, CardLine, Cell, Column, Row, StatItem, Tag};
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
        lines
            .iter()
            .map(|text| display_width(text))
            .max()
            .unwrap_or(0)
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
        assert!(display_width(&lines[0]) <= 8, "{:?}", lines);
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

    fn stat_item(value: &str, label: &str) -> StatItem {
        StatItem {
            value: value.to_string(),
            label: label.to_string(),
            tone: Tone::Ok,
        }
    }

    fn stat(items: Vec<StatItem>, hint: Option<&str>) -> Block {
        Block::Stat {
            items,
            hint: hint.map(str::to_string),
        }
    }

    fn column(title: &str, align: Align) -> Column {
        Column {
            title: title.to_string(),
            align,
        }
    }

    fn cell(text: &str) -> Cell {
        Cell {
            text: text.to_string(),
            tone: Tone::Normal,
            flag: None,
        }
    }

    fn table(columns: Vec<Column>, rows: Vec<Vec<Cell>>) -> Block {
        Block::Table { columns, rows }
    }

    #[test]
    fn stat_items_render_side_by_side_when_they_fit() {
        let blocks = [stat(
            vec![stat_item("3", "reuniões"), stat_item("2h", "livre")],
            None,
        )];
        assert_eq!(plain(&blocks, 40), vec!["3 reuniões   2h livre"]);
    }

    #[test]
    fn stat_items_wrap_when_narrow_without_splitting_an_item() {
        let blocks = [stat(
            vec![stat_item("3", "reuniões"), stat_item("2h", "livre")],
            None,
        )];
        assert_eq!(plain(&blocks, 12), vec!["3 reuniões", "2h livre"]);
    }

    #[test]
    fn stat_value_is_bold_in_the_item_tone() {
        let blocks = [stat(vec![stat_item("3", "x")], None)];
        let lines = render_blocks(&blocks, 20);
        let value = &lines[0].spans[0];
        assert_eq!(value.content, "3");
        assert_eq!(value.style.fg, Some(Color::Green));
        assert!(value.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn stat_hint_is_a_muted_line_below() {
        let blocks = [stat(vec![stat_item("3", "x")], Some("hoje"))];
        let lines = render_blocks(&blocks, 20);
        assert_eq!(lines_to_plain_text(&lines), vec!["3 x", "hoje"]);
        assert_eq!(lines[1].spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn stat_item_wider_than_width_is_truncated() {
        let blocks = [stat(vec![stat_item("100", "muitíssimo-longo")], None)];
        assert_eq!(plain(&blocks, 8), vec!["100 mui…"]);
    }

    #[test]
    fn table_renders_header_and_aligned_columns() {
        let blocks = [table(
            vec![column("Nome", Align::Left), column("Qtd", Align::Right)],
            vec![vec![cell("ana"), cell("3")], vec![cell("bia"), cell("10")]],
        )];
        assert_eq!(
            plain(&blocks, 30),
            vec!["Nome  Qtd", "ana     3", "bia    10"]
        );
    }

    #[test]
    fn table_right_alignment() {
        let blocks = [table(
            vec![column("Qtd", Align::Right)],
            vec![vec![cell("1000")]],
        )];
        assert_eq!(plain(&blocks, 30), vec![" Qtd", "1000"]);
    }

    #[test]
    fn table_header_is_bold() {
        let blocks = [table(vec![column("A", Align::Left)], vec![])];
        let lines = render_blocks(&blocks, 10);
        assert!(lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD));
    }

    #[test]
    fn table_shrinks_widest_column_to_fit() {
        let blocks = [table(
            vec![column("A", Align::Left), column("B", Align::Left)],
            vec![vec![cell("aaaaaaaaaa"), cell("bb")]],
        )];
        assert_eq!(plain(&blocks, 10), vec!["A       B", "aaaaa…  bb"]);
    }

    #[test]
    fn table_drops_rightmost_columns_when_nothing_fits() {
        let columns = ["A", "B", "C", "D"]
            .iter()
            .map(|title| column(title, Align::Left))
            .collect();
        let row = ["aaaaaaaa", "bbbbbbbb", "cccccccc", "dddddddd"]
            .iter()
            .map(|text| cell(text))
            .collect();
        let lines = plain(&[table(columns, vec![row])], 5);
        assert!(widest(&lines) <= 5, "{lines:?}");
        let joined = lines.join("\n");
        assert!(joined.contains('A') || joined.contains('a'));
        assert!(!joined.contains(['C', 'D', 'c', 'd']), "{lines:?}");
    }

    #[test]
    fn table_cells_show_flags_and_tones() {
        let flagged_cell = Cell {
            text: "x".to_string(),
            tone: Tone::Danger,
            flag: Some(Flag::Warning),
        };
        let blocks = [table(
            vec![column("A", Align::Left)],
            vec![vec![flagged_cell]],
        )];
        let lines = render_blocks(&blocks, 10);
        assert_eq!(lines_to_plain_text(&lines)[1], "⚠ x");
        assert_eq!(lines[1].spans[0].style.fg, Some(Color::Red));
    }

    fn pathological_blocks() -> Vec<Block> {
        let wide = "日本語のとても長いセル".repeat(5);
        let columns = (0..10)
            .map(|index| {
                column(
                    &format!("列{index}"),
                    if index % 2 == 0 {
                        Align::Left
                    } else {
                        Align::Right
                    },
                )
            })
            .collect::<Vec<_>>();
        let rows = (0..200)
            .map(|_| (0..10).map(|_| cell(&wide)).collect())
            .collect::<Vec<Vec<Cell>>>();
        let items = (0..8).map(|_| stat_item(&wide, &wide)).collect();
        vec![table(columns, rows), stat(items, Some(&wide))]
    }

    #[test]
    fn no_line_is_ever_wider_than_the_width_for_stat_and_table() {
        let blocks = pathological_blocks();
        for width in 0..=40 {
            let lines = plain(&blocks, width);
            assert!(widest(&lines) <= width, "largura {width}");
        }
    }

    #[test]
    fn extreme_block_counts_are_bounded() {
        let blocks: Vec<Block> = (0..20).flat_map(|_| pathological_blocks()).collect();
        let started = std::time::Instant::now();
        let lines = render_blocks(&blocks, 10);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(!lines.is_empty());
    }

    #[test]
    fn emoji_with_variation_selector_never_panics_or_overflows() {
        let heart = "❤\u{FE0F}".repeat(3);
        let blocks = [
            Block::Lines(vec![line(&heart, Some(Flag::Updated))]),
            Block::Rows(vec![row(
                Some("❤\u{FE0F}"),
                &heart,
                &["❤\u{FE0F}"],
                Some('❤'),
            )]),
            stat(vec![stat_item(&heart, &heart)], Some(&heart)),
            table(
                vec![column(&heart, Align::Left), column(&heart, Align::Right)],
                vec![vec![cell(&heart), cell(&heart)]],
            ),
        ];
        for width in 0..=40 {
            let lines = plain(&blocks, width);
            assert!(widest(&lines) <= width, "largura {width}: {lines:?}");
        }
    }
}
