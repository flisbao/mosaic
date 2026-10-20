#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Latência de `run_process`. Uso:
//! `cargo test --release --test perf_process -- --ignored --nocapture`

use std::process::Command;
use std::time::{Duration, Instant};

use mosaic::infrastructure::process::{run_process, CancelFlag, ProcessLimits};

const RUNS: usize = 50;
const TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_LIMIT_BYTES: usize = 1_048_576;
const MAXIMUM_MEDIAN: Duration = Duration::from_millis(15);

#[test]
#[ignore]
fn median_latency_of_a_trivial_process_is_far_below_the_old_poll_interval() {
    let limits = ProcessLimits {
        timeout: TIMEOUT,
        maximum_output_bytes: OUTPUT_LIMIT_BYTES,
    };
    let mut durations: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let mut command = Command::new("printf");
            command.arg("x");
            let started = Instant::now();
            run_process(command, None, &limits, &CancelFlag::default()).unwrap();
            started.elapsed()
        })
        .collect();
    durations.sort();
    let median = durations[RUNS / 2];
    let slowest = durations[RUNS - 1];
    println!("PERF run_process(printf x): p50 {median:?}, máximo {slowest:?}");
    assert!(median < MAXIMUM_MEDIAN, "p50 {median:?}");
}
