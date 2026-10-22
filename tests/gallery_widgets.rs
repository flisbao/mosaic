#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use mosaic::application::launch_spec::ProviderCommand;
use mosaic::application::ports::SourceError;
use mosaic::domain::blocks::{Block, BLOCK_TYPE_NAMES};
use mosaic::domain::snapshot_report::Severity;
use mosaic::infrastructure::plugin_runner::PluginRunner;
use mosaic::infrastructure::protocol_report::parse_snapshot_report;

const AGENDA_DETAIL_HEADING: &str = "Agenda de hoje";
const AGENDA_SUMMARY_ROW_COUNT: usize = 3;

fn interpreter_is_available(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn gallery_output(command: &str, script: &str) -> Result<String, SourceError> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let provider = ProviderCommand {
        program: command.to_string(),
        args: vec![root.join(script).to_string_lossy().to_string()],
        env: vec![],
        inherit_env: None,
        timeout: std::time::Duration::from_secs(10),
    };
    PluginRunner::new(provider, root.to_path_buf()).raw_snapshot_output()
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
    let detail = parsed.snapshot.detail.as_ref().unwrap();
    assert!(detail
        .iter()
        .any(|section| section.heading == AGENDA_DETAIL_HEADING));
    let detail_item_count: usize = detail.iter().map(|section| section.items.len()).sum();
    assert!(detail_item_count > AGENDA_SUMMARY_ROW_COUNT);
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
