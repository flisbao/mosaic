#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Medição do parser de protocolo. Uso:
//! `cargo test --release --test perf_parse -- --ignored --nocapture --test-threads=1`

use std::time::Instant;

use mosaic::infrastructure::protocol_report::parse_snapshot_report;

const SECTIONS: usize = 100;
const ITEMS_PER_SECTION: usize = 400;
const MEASURED_PARSES: u32 = 5;
const MAXIMUM_SLOWDOWN_FACTOR: f64 = 1.3;

fn json_with_blocks_per_section(blocks_per_section: usize) -> String {
    let block = r#"{"type":"lines","lines":[{"text":"a"}]}"#;
    let blocks = vec![block; blocks_per_section].join(",");
    let item = r#"{"text":"item","extra":1}"#;
    let items = vec![item; ITEMS_PER_SECTION].join(",");
    let section = format!(r#"{{"heading":"s","blocks":[{blocks}],"items":[{items}]}}"#);
    let sections = vec![section; SECTIONS].join(",");
    format!(r#"{{"protocol":1,"summary":{{"title":"t"}},"detail":{{"sections":[{sections}]}}}}"#)
}

fn milliseconds_per_parse(json: &str) -> f64 {
    parse_snapshot_report(json).unwrap();
    let started = Instant::now();
    for _ in 0..MEASURED_PARSES {
        parse_snapshot_report(json).unwrap();
    }
    started.elapsed().as_secs_f64() * 1000.0 / f64::from(MEASURED_PARSES)
}

#[test]
#[ignore]
fn parse_time_does_not_grow_with_the_number_of_blocks() {
    let without_blocks = milliseconds_per_parse(&json_with_blocks_per_section(0));
    let with_blocks = milliseconds_per_parse(&json_with_blocks_per_section(20));
    println!("PERF parse sem blocos: {without_blocks:.1} ms; com 20 blocos por seção: {with_blocks:.1} ms");
    assert!(
        with_blocks < without_blocks * MAXIMUM_SLOWDOWN_FACTOR,
        "{with_blocks:.1} ms contra {without_blocks:.1} ms"
    );
}
