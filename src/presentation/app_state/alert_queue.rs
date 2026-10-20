use std::collections::VecDeque;

use crate::application::pomodoro_notifications::AlertMessage;

use super::MAXIMUM_QUEUED_ALERTS;

#[derive(Default)]
pub(super) struct AlertQueue {
    messages: VecDeque<AlertMessage>,
}

impl AlertQueue {
    pub(super) fn front(&self) -> Option<&AlertMessage> {
        self.messages.front()
    }

    pub(super) fn pending_count(&self) -> usize {
        self.messages.len()
    }

    pub(super) fn push(&mut self, message: AlertMessage) {
        if self.messages.len() < MAXIMUM_QUEUED_ALERTS {
            self.messages.push_back(message);
        }
    }

    pub(super) fn dismiss_front(&mut self) -> bool {
        self.messages.pop_front().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(title: &str) -> AlertMessage {
        AlertMessage {
            title: title.to_string(),
            body: String::new(),
        }
    }

    fn front_title(queue: &AlertQueue) -> Option<&str> {
        queue.front().map(|alert| alert.title.as_str())
    }

    #[test]
    fn messages_are_dismissed_in_arrival_order() {
        let mut queue = AlertQueue::default();
        queue.push(message("um"));
        queue.push(message("dois"));

        assert_eq!(queue.pending_count(), 2);
        assert_eq!(front_title(&queue), Some("um"));
        assert!(queue.dismiss_front());
        assert_eq!(front_title(&queue), Some("dois"));
    }

    #[test]
    fn dismissing_an_empty_queue_reports_nothing_dismissed() {
        let mut queue = AlertQueue::default();
        assert!(!queue.dismiss_front());
        assert!(queue.front().is_none());
    }

    #[test]
    fn messages_beyond_the_limit_are_dropped() {
        let mut queue = AlertQueue::default();
        for number in 0..=MAXIMUM_QUEUED_ALERTS {
            queue.push(message(&number.to_string()));
        }

        assert_eq!(queue.pending_count(), MAXIMUM_QUEUED_ALERTS);
        assert_eq!(front_title(&queue), Some("0"));
    }
}
