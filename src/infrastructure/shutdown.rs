use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[cfg(unix)]
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

#[cfg(unix)]
const SECOND_SIGNAL_EXIT_CODE: i32 = 1;

#[cfg(unix)]
pub fn termination_requested() -> io::Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    for signal in [SIGTERM, SIGINT, SIGHUP] {
        signal_hook::flag::register_conditional_shutdown(
            signal,
            SECOND_SIGNAL_EXIT_CODE,
            Arc::clone(&flag),
        )?;
        signal_hook::flag::register(signal, Arc::clone(&flag))?;
    }
    Ok(flag)
}

#[cfg(not(unix))]
pub fn termination_requested() -> io::Result<Arc<AtomicBool>> {
    Ok(Arc::new(AtomicBool::new(false)))
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::{Duration, Instant};

    use signal_hook::consts::SIGTERM;

    use super::{termination_requested, SECOND_SIGNAL_EXIT_CODE};

    const ESCALATION_CHILD_VARIABLE: &str = "MOSAIC_SHUTDOWN_ESCALATION_CHILD";
    const READY_MARKER: &str = "MOSAIC_SHUTDOWN_READY";
    const ESCALATION_CHILD_TEST: &str = "infrastructure::shutdown::tests::escalation_child_process";
    const CHILD_LIFETIME: Duration = Duration::from_secs(10);
    const SIGNAL_SETTLE_TIME: Duration = Duration::from_millis(300);
    const EXIT_WAIT_LIMIT: Duration = Duration::from_secs(3);
    const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(20);

    #[test]
    fn termination_flag_flips_on_sigterm() {
        let flag = termination_requested().expect("handlers should register");
        assert!(!flag.load(Ordering::SeqCst));

        signal_hook::low_level::raise(SIGTERM).expect("raise should succeed");

        assert!(flag.load(Ordering::SeqCst));
    }

    // Os handlers são globais ao processo: este teste roda em um filho para que o
    // segundo SIGTERM (que encerra com código 1) não derrube o runner de testes.
    #[test]
    fn second_termination_signal_exits_with_code_one() {
        let mut child = spawn_escalation_child();
        wait_for_ready(&mut child);

        send_signal(&child, "TERM");
        thread::sleep(SIGNAL_SETTLE_TIME);
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "o primeiro sinal não deve encerrar o processo"
        );

        send_signal(&child, "TERM");
        let status = wait_for_exit(&mut child);
        assert_eq!(
            status.and_then(|status| status.code()),
            Some(SECOND_SIGNAL_EXIT_CODE)
        );
    }

    #[test]
    fn escalation_child_process() {
        if std::env::var_os(ESCALATION_CHILD_VARIABLE).is_none() {
            return;
        }
        let _flag = termination_requested().expect("handlers should register");
        println!("{READY_MARKER}");
        thread::sleep(CHILD_LIFETIME);
    }

    fn spawn_escalation_child() -> Child {
        Command::new(std::env::current_exe().expect("test binary path"))
            .args([ESCALATION_CHILD_TEST, "--exact", "--nocapture"])
            .env(ESCALATION_CHILD_VARIABLE, "1")
            .stdout(Stdio::piped())
            .spawn()
            .expect("child should spawn")
    }

    fn wait_for_ready(child: &mut Child) {
        let stdout = child.stdout.take().expect("piped stdout");
        let ready = BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .any(|line| line.contains(READY_MARKER));
        assert!(ready, "o filho não sinalizou que os handlers estão prontos");
    }

    fn send_signal(child: &Child, name: &str) {
        let status = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(child.id().to_string())
            .status()
            .expect("kill should run");
        assert!(status.success());
    }

    fn wait_for_exit(child: &mut Child) -> Option<ExitStatus> {
        let deadline = Instant::now() + EXIT_WAIT_LIMIT;
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait().expect("try_wait") {
                return Some(status);
            }
            thread::sleep(EXIT_POLL_INTERVAL);
        }
        let _ = child.kill();
        let _ = child.wait();
        None
    }
}
