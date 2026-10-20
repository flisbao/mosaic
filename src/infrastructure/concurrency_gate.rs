use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

pub struct ConcurrencyGate {
    available: Mutex<usize>,
    released: Condvar,
}

pub struct GatePermit<'a> {
    gate: &'a ConcurrencyGate,
}

impl ConcurrencyGate {
    pub fn new(permits: usize) -> Self {
        Self {
            available: Mutex::new(permits.max(1)),
            released: Condvar::new(),
        }
    }

    pub fn acquire(&self) -> GatePermit<'_> {
        let mut available = self.lock_available();
        while *available == 0 {
            available = self
                .released
                .wait(available)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *available -= 1;
        GatePermit { gate: self }
    }

    fn lock_available(&self) -> MutexGuard<'_, usize> {
        self.available
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for GatePermit<'_> {
    fn drop(&mut self) {
        *self.gate.lock_available() += 1;
        self.gate.released.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    const PERMITS: usize = 2;
    const WORKERS: usize = 8;
    const HOLD_TIME: Duration = Duration::from_millis(20);

    #[test]
    fn never_lets_more_holders_than_permits_run_at_once() {
        let gate = Arc::new(ConcurrencyGate::new(PERMITS));
        let running = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = (0..WORKERS)
            .map(|_| {
                let (gate, running, maximum) = (
                    Arc::clone(&gate),
                    Arc::clone(&running),
                    Arc::clone(&maximum),
                );
                thread::spawn(move || {
                    let _permit = gate.acquire();
                    let now_running = running.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(now_running, Ordering::SeqCst);
                    thread::sleep(HOLD_TIME);
                    running.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(maximum.load(Ordering::SeqCst), PERMITS);
    }

    #[test]
    fn dropping_a_permit_lets_the_next_holder_in() {
        let gate = ConcurrencyGate::new(1);
        drop(gate.acquire());
        drop(gate.acquire());
    }

    #[test]
    fn zero_permits_are_raised_to_one_so_the_gate_cannot_deadlock() {
        let gate = ConcurrencyGate::new(0);
        drop(gate.acquire());
    }
}
