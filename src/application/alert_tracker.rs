use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::widget::WidgetId;
use crate::domain::widget_content::Alert;

pub const MAXIMUM_NEW_ALERTS_PER_SNAPSHOT: usize = 5;
pub const MAXIMUM_REMEMBERED_IDS: usize = 1000;

#[derive(Default)]
struct SeenIds {
    order: VecDeque<String>,
    ids: HashSet<String>,
}

impl SeenIds {
    fn remember(&mut self, id: &str) -> bool {
        if !self.ids.insert(id.to_string()) {
            return false;
        }
        self.order.push_back(id.to_string());
        while self.order.len() > MAXIMUM_REMEMBERED_IDS {
            if let Some(oldest) = self.order.pop_front() {
                self.ids.remove(&oldest);
            }
        }
        true
    }
}

#[derive(Default)]
pub struct AlertTracker {
    widgets: HashMap<WidgetId, SeenIds>,
}

impl AlertTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh_alerts(&mut self, widget: &WidgetId, alerts: &[Alert]) -> Vec<Alert> {
        let is_first_snapshot = !self.widgets.contains_key(widget);
        let seen = self.widgets.entry(widget.clone()).or_default();
        let mut fresh = vec![];
        for alert in alerts {
            let is_new = seen.remember(&alert.id);
            if is_new && !is_first_snapshot && fresh.len() < MAXIMUM_NEW_ALERTS_PER_SNAPSHOT {
                fresh.push(alert.clone());
            }
        }
        fresh
    }

    pub fn remembered_count(&self, widget: &WidgetId) -> usize {
        self.widgets.get(widget).map_or(0, |seen| seen.order.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widget_content::AlertLevel;

    fn alert(id: &str) -> Alert {
        Alert {
            id: id.to_string(),
            title: format!("título {id}"),
            body: String::new(),
            level: AlertLevel::Info,
        }
    }

    fn alerts(ids: &[&str]) -> Vec<Alert> {
        ids.iter().map(|id| alert(id)).collect()
    }

    fn ids(alerts: &[Alert]) -> Vec<&str> {
        alerts.iter().map(|alert| alert.id.as_str()).collect()
    }

    fn widget(name: &str) -> WidgetId {
        WidgetId::new(name)
    }

    #[test]
    fn first_snapshot_is_silent_but_remembers_ids() {
        let mut tracker = AlertTracker::new();
        assert!(tracker
            .fresh_alerts(&widget("a"), &alerts(&["1", "2"]))
            .is_empty());
        assert!(tracker
            .fresh_alerts(&widget("a"), &alerts(&["1", "2"]))
            .is_empty());
    }

    #[test]
    fn new_ids_after_the_first_snapshot_are_returned_once() {
        let mut tracker = AlertTracker::new();
        tracker.fresh_alerts(&widget("a"), &alerts(&["1"]));
        let fresh = tracker.fresh_alerts(&widget("a"), &alerts(&["1", "2", "3"]));
        assert_eq!(ids(&fresh), vec!["2", "3"]);
    }

    #[test]
    fn same_id_is_not_returned_twice() {
        let mut tracker = AlertTracker::new();
        tracker.fresh_alerts(&widget("a"), &[]);
        assert_eq!(tracker.fresh_alerts(&widget("a"), &alerts(&["9"])).len(), 1);
        assert!(tracker
            .fresh_alerts(&widget("a"), &alerts(&["9"]))
            .is_empty());
    }

    #[test]
    fn duplicate_ids_in_one_snapshot_count_once() {
        let mut tracker = AlertTracker::new();
        tracker.fresh_alerts(&widget("a"), &[]);
        let fresh = tracker.fresh_alerts(&widget("a"), &alerts(&["9", "9", "9"]));
        assert_eq!(ids(&fresh), vec!["9"]);
    }

    #[test]
    fn at_most_five_new_alerts_per_snapshot() {
        let mut tracker = AlertTracker::new();
        tracker.fresh_alerts(&widget("a"), &[]);
        let burst = alerts(&["1", "2", "3", "4", "5", "6", "7"]);
        let fresh = tracker.fresh_alerts(&widget("a"), &burst);
        assert_eq!(ids(&fresh), vec!["1", "2", "3", "4", "5"]);
        assert!(tracker.fresh_alerts(&widget("a"), &burst).is_empty());
    }

    #[test]
    fn widgets_are_tracked_independently() {
        let mut tracker = AlertTracker::new();
        tracker.fresh_alerts(&widget("a"), &alerts(&["1"]));
        assert!(tracker
            .fresh_alerts(&widget("b"), &alerts(&["1"]))
            .is_empty());
        let fresh = tracker.fresh_alerts(&widget("b"), &alerts(&["1", "2"]));
        assert_eq!(ids(&fresh), vec!["2"]);
        assert_eq!(
            ids(&tracker.fresh_alerts(&widget("a"), &alerts(&["2"]))),
            vec!["2"]
        );
    }

    #[test]
    fn empty_alert_list_is_fine() {
        let mut tracker = AlertTracker::new();
        assert!(tracker.fresh_alerts(&widget("a"), &[]).is_empty());
        assert!(tracker.fresh_alerts(&widget("a"), &[]).is_empty());
    }

    #[test]
    fn remembered_ids_are_bounded() {
        let mut tracker = AlertTracker::new();
        tracker.fresh_alerts(&widget("a"), &[]);
        let flood: Vec<Alert> = (0..1500).map(|number| alert(&number.to_string())).collect();
        let fresh = tracker.fresh_alerts(&widget("a"), &flood);
        assert_eq!(fresh.len(), MAXIMUM_NEW_ALERTS_PER_SNAPSHOT);
        assert_eq!(
            tracker.remembered_count(&widget("a")),
            MAXIMUM_REMEMBERED_IDS
        );
    }
}
