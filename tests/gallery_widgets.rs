#![cfg(unix)]

use std::path::Path;
use std::process::Command;

use mosaic::application::ports::SourceError;
use mosaic::domain::card::{Block, BLOCK_TYPE_NAMES};
use mosaic::domain::snapshot_report::Severity;
use mosaic::domain::widget::WidgetId;
use mosaic::infrastructure::config::WidgetSettings;
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::protocol_report::parse_snapshot_report;

fn interpreter_is_available(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn gallery_output(command: &str, script: &str) -> Result<String, SourceError> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let settings = WidgetSettings {
        id: WidgetId::new("gallery"),
        command: command.to_string(),
        args: vec![root.join(script).to_string_lossy().to_string()],
        env: vec![],
        interval_seconds: 30,
        interval_adjusted: false,
        timeout_seconds: 10,
        span: 1,
        tab: false,
        llm_sections: vec![],
        llm_max_items: 20,
        llm_max_characters_per_item: 120,
    };
    PluginRunner::new(settings, root.to_path_buf()).raw_snapshot_output()
}

fn block_type_names(blocks: &[Block]) -> Vec<&'static str> {
    blocks
        .iter()
        .map(|block| match block {
            Block::Lines(_) => BLOCK_TYPE_NAMES[0],
            Block::Rows(_) => BLOCK_TYPE_NAMES[1],
            Block::Stat { .. } => BLOCK_TYPE_NAMES[2],
            Block::Table { .. } => BLOCK_TYPE_NAMES[3],
        })
        .collect()
}

fn assert_gallery_is_valid_and_complete(command: &str, script: &str) {
    let output = gallery_output(command, script).unwrap();
    let parsed = parse_snapshot_report(&output).unwrap();
    let errors: Vec<_> = parsed
        .problems
        .iter()
        .filter(|problem| problem.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let types = block_type_names(&parsed.snapshot.summary.blocks);
    for expected in BLOCK_TYPE_NAMES {
        assert!(
            types.contains(&expected),
            "falta o bloco {expected}: {types:?}"
        );
    }
    assert!(!parsed.snapshot.alerts.is_empty());
}

#[test]
fn python_gallery_snapshot_is_valid_and_complete() {
    if !interpreter_is_available("python3") {
        return;
    }
    assert_gallery_is_valid_and_complete("python3", "plugins/gallery.py");
}

#[test]
fn node_gallery_snapshot_is_valid_and_complete() {
    if !interpreter_is_available("node") {
        return;
    }
    assert_gallery_is_valid_and_complete("node", "plugins/gallery.js");
}
