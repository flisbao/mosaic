use chrono::NaiveDateTime;

use crate::application::ports::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> NaiveDateTime {
        chrono::Local::now().naive_local()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_close_to_local_time() {
        let difference = (chrono::Local::now().naive_local() - SystemClock.now()).num_seconds();
        assert!(difference.abs() < 5);
    }
}
