use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;

const PENDING_REQUEST_CAPACITY: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshWake {
    Requested,
    IntervalElapsed,
    Closed,
}

#[derive(Clone)]
pub struct RefreshSignal {
    sender: SyncSender<()>,
}

pub struct RefreshWaiter {
    receiver: Receiver<()>,
}

pub fn refresh_channel() -> (RefreshSignal, RefreshWaiter) {
    let (sender, receiver) = mpsc::sync_channel(PENDING_REQUEST_CAPACITY);
    (RefreshSignal { sender }, RefreshWaiter { receiver })
}

impl RefreshSignal {
    pub fn request(&self) {
        let _ = self.sender.try_send(());
    }
}

impl RefreshWaiter {
    pub fn wait(&self, interval: Duration) -> RefreshWake {
        match self.receiver.recv_timeout(interval) {
            Ok(()) => RefreshWake::Requested,
            Err(RecvTimeoutError::Timeout) => RefreshWake::IntervalElapsed,
            Err(RecvTimeoutError::Disconnected) => RefreshWake::Closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const SHORT_INTERVAL: Duration = Duration::from_millis(50);

    #[test]
    fn many_requests_coalesce_into_one_pending_refresh() {
        let (signal, waiter) = refresh_channel();
        for _ in 0..20 {
            signal.request();
        }
        assert_eq!(waiter.wait(SHORT_INTERVAL), RefreshWake::Requested);
        assert_eq!(waiter.wait(SHORT_INTERVAL), RefreshWake::IntervalElapsed);
    }

    #[test]
    fn waiting_without_requests_returns_after_the_interval() {
        let (_signal, waiter) = refresh_channel();
        assert_eq!(waiter.wait(SHORT_INTERVAL), RefreshWake::IntervalElapsed);
    }

    #[test]
    fn dropping_the_signal_closes_the_waiter() {
        let (signal, waiter) = refresh_channel();
        drop(signal);
        assert_eq!(waiter.wait(SHORT_INTERVAL), RefreshWake::Closed);
    }

    #[test]
    fn request_after_the_waiter_is_dropped_does_not_panic() {
        let (signal, waiter) = refresh_channel();
        drop(waiter);
        signal.request();
    }

    #[test]
    fn cloned_signals_share_one_pending_refresh() {
        let (signal, waiter) = refresh_channel();
        let copy = signal.clone();
        signal.request();
        copy.request();
        assert_eq!(waiter.wait(SHORT_INTERVAL), RefreshWake::Requested);
        assert_eq!(waiter.wait(SHORT_INTERVAL), RefreshWake::IntervalElapsed);
    }
}
