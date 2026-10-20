#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use mosaic::infrastructure::process::{run_process, CancelFlag, ProcessLimits};

const TIMEOUT: Duration = Duration::from_secs(1);
const MAXIMUM_OUTPUT_BYTES: usize = 1024;
const EXIT_WAIT_LIMIT: Duration = Duration::from_secs(5);
const EXIT_CHECK_INTERVAL: Duration = Duration::from_millis(20);

fn is_running(pid: &str) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit_once(')')
            .is_some_and(|(_, state)| !state.trim_start().starts_with('Z'))
    })
}

fn wait_until_exited(pid: &str) -> bool {
    let deadline = Instant::now() + EXIT_WAIT_LIMIT;
    while is_running(pid) && Instant::now() < deadline {
        thread::sleep(EXIT_CHECK_INTERVAL);
    }
    !is_running(pid)
}

fn read_pid(path: &Path) -> String {
    fs::read_to_string(path).unwrap().trim().to_string()
}

fn run_background_script(script: &str) -> String {
    let directory = tempfile::tempdir().unwrap();
    let pid_file = directory.path().join("pid.txt");
    let script = script.replace("PID_FILE", &pid_file.display().to_string());
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    let limits = ProcessLimits {
        timeout: TIMEOUT,
        maximum_output_bytes: MAXIMUM_OUTPUT_BYTES,
    };
    let error = run_process(command, None, &limits, &CancelFlag::default()).unwrap_err();
    assert!(error.to_string().contains("tempo"), "{error}");
    read_pid(&pid_file)
}

#[test]
fn background_child_of_an_exited_leader_does_not_survive_the_timeout() {
    let pid = run_background_script("sleep 30 & echo $! > 'PID_FILE'; echo '{}'");
    let exited = wait_until_exited(&pid);
    if !exited {
        let _ = Command::new("kill").args(["-KILL", &pid]).status();
    }
    assert!(exited, "processo {pid} ficou órfão depois do timeout");
}
