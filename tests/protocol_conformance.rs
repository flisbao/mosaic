#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use mosaic::domain::blocks::BLOCK_TYPE_NAMES;
use mosaic::domain::snapshot_report::Severity;
use mosaic::domain::widget::{
    FLAG_NAMES, MAXIMUM_MINI_TEXT_CHARACTERS, MAXIMUM_MINI_VALUE_CHARACTERS,
};
use mosaic::domain::widget_content::TONE_NAMES;
use mosaic::infrastructure::protocol_report::parse_snapshot_report;
use serde_json::Value;

const ERROR_PREFIX: &str = "ERRO ";

fn fixtures_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/protocol")
}

fn fixtures_with_prefix(prefix: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(fixtures_directory())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        })
        .collect();
    paths.sort();
    paths
}

fn error_paths(path: &Path) -> Vec<String> {
    let parsed = parse_snapshot_report(&fs::read_to_string(path).unwrap()).unwrap();
    parsed
        .problems
        .into_iter()
        .filter(|problem| problem.severity == Severity::Error)
        .map(|problem| problem.path)
        .collect()
}

#[test]
fn valid_fixtures_have_no_errors() {
    let fixtures = fixtures_with_prefix("valid_");
    assert!(fixtures.len() >= 8);
    for path in fixtures {
        assert!(error_paths(&path).is_empty(), "{}", path.display());
    }
}

#[test]
fn invalid_fixtures_report_the_expected_problem() {
    let fixtures = fixtures_with_prefix("invalid_");
    assert!(fixtures.len() >= 5);
    for path in fixtures {
        let expected = fs::read_to_string(path.with_extension("expected")).unwrap();
        let expected_path = expected.trim().strip_prefix(ERROR_PREFIX).unwrap();
        let found = error_paths(&path);
        assert!(
            found.iter().any(|candidate| candidate == expected_path),
            "{}: esperava {expected_path}, achou {found:?}",
            path.display()
        );
    }
}

fn schema() -> Value {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/protocol.schema.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn strings(values: &Value) -> Vec<String> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn schema_matches_the_code() {
    let schema = schema();
    let definitions = &schema["$defs"];
    assert_eq!(strings(&definitions["tone"]["enum"]), TONE_NAMES);
    assert_eq!(strings(&definitions["flag"]["enum"]), FLAG_NAMES);
    let block_types: Vec<String> = definitions["block"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| {
            variant["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(block_types, BLOCK_TYPE_NAMES);
    let mini = &schema["$defs"]["mini"];
    assert_eq!(
        mini["properties"]["value"]["maxLength"].as_u64(),
        Some(u64::try_from(MAXIMUM_MINI_VALUE_CHARACTERS).unwrap())
    );
    assert_eq!(
        mini["properties"]["text"]["maxLength"].as_u64(),
        Some(u64::try_from(MAXIMUM_MINI_TEXT_CHARACTERS).unwrap())
    );
    assert_eq!(mini["properties"]["value"]["minLength"].as_u64(), Some(1));
    assert_eq!(mini["properties"]["text"]["minLength"].as_u64(), Some(1));
}
