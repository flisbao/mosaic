use ratatui::text::Line;

use crate::domain::widget_content::Block;
use crate::presentation::blocks::render_block;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget::Flag;
    use crate::domain::widget_content::{
        Align, Cell, Column, Row, StatItem, Tag, Tone, WidgetLine,
    };
    use crate::presentation::text_fit::{display_width, tone_style};
    use ratatui::style::{Color, Modifier};

    fn line(text: &str, flag: Option<Flag>) -> WidgetLine {
        WidgetLine {
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
