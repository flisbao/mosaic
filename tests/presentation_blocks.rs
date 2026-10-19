use mosaic::infrastructure::protocol_report::parse_snapshot_report;
use mosaic::presentation::widget_view::{lines_to_plain_text, render_blocks};

#[test]
fn renders_every_block_type_from_the_valid_all_blocks_fixture() {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/protocol/valid_all_blocks.json"
    ))
    .unwrap();
    let parsed = parse_snapshot_report(&json).unwrap();
    let lines = render_blocks(&parsed.snapshot.summary.blocks, 40);
    assert_eq!(
        lines_to_plain_text(&lines),
        vec!["resumo", "  09:00 Daily", "1 item", "A", "x"]
    );
}
