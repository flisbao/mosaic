use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[cfg(unix)]
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

#[cfg(unix)]
pub fn termination_requested() -> io::Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    for signal in [SIGTERM, SIGINT, SIGHUP] {
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
    use std::sync::atomic::Ordering;

    use signal_hook::consts::SIGTERM;

    use super::termination_requested;

    #[test]
    fn termination_flag_flips_on_sigterm() {
        let flag = termination_requested().expect("handlers should register");
        assert!(!flag.load(Ordering::SeqCst));

        signal_hook::low_level::raise(SIGTERM).expect("raise should succeed");

        assert!(flag.load(Ordering::SeqCst));
    }
}
