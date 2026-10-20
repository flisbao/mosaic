#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Pico de memória do parse com vários widgets atualizando juntos. Rode cada teste em um processo
//! separado, porque `VmHWM` é o máximo do processo inteiro:
//! `cargo test --release --test perf_memory -- --exact ungated_parse_peak --ignored --nocapture`
//! `cargo test --release --test perf_memory -- --exact gated_parse_peak --ignored --nocapture`

use std::fs;
use std::sync::Arc;
use std::thread;

use mosaic::infrastructure::concurrency_gate::ConcurrencyGate;
use mosaic::infrastructure::plugin_runner::MAXIMUM_CONCURRENT_PARSES;
use mosaic::infrastructure::protocol_report::parse_snapshot_report;

const WIDGETS: usize = 12;
const PARSES_PER_WIDGET: usize = 10;
const SECTIONS: usize = 100;
const ITEMS_PER_SECTION: usize = 100;
const KIBIBYTES_PER_MEBIBYTE: f64 = 1024.0;
const MAXIMUM_GATED_PEAK_MEBIBYTES: f64 = 120.0;

fn large_json() -> String {
    let item = r#"{"id":"abcdef","text":"texto do item com tamanho razoavel para o teste","meta":{"chave":"valor"}}"#;
    let items = vec![item; ITEMS_PER_SECTION].join(",");
    let section = format!(r#"{{"heading":"secao","items":[{items}]}}"#);
    let sections = vec![section; SECTIONS].join(",");
    format!(r#"{{"protocol":1,"summary":{{"title":"t"}},"detail":[{sections}]}}"#)
}

fn peak_mebibytes() -> f64 {
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let kibibytes: f64 = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap();
    kibibytes / KIBIBYTES_PER_MEBIBYTE
}

fn parse_everywhere(gate: Option<Arc<ConcurrencyGate>>) {
    let json = Arc::new(large_json());
    let workers: Vec<_> = (0..WIDGETS)
        .map(|_| {
            let (json, gate) = (Arc::clone(&json), gate.clone());
            thread::spawn(move || {
                for _ in 0..PARSES_PER_WIDGET {
                    let _permit = gate.as_ref().map(|gate| gate.acquire());
                    parse_snapshot_report(&json).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
#[ignore]
fn ungated_parse_peak() {
    parse_everywhere(None);
    println!(
        "PERF pico sem limite de parses: {:.1} MiB",
        peak_mebibytes()
    );
}

#[test]
#[ignore]
fn gated_parse_peak() {
    parse_everywhere(Some(Arc::new(ConcurrencyGate::new(
        MAXIMUM_CONCURRENT_PARSES,
    ))));
    let peak = peak_mebibytes();
    println!("PERF pico com {MAXIMUM_CONCURRENT_PARSES} parses simultâneos: {peak:.1} MiB");
    assert!(peak < MAXIMUM_GATED_PEAK_MEBIBYTES, "{peak:.1} MiB");
}
