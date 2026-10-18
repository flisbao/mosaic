use std::time::Duration;

use crate::application::recommendation_view::IA_WIDGET_ID;
use crate::domain::widget::{WidgetId, WidgetRecord};
use crate::presentation::app_state::{TabView, WidgetEntry};

pub const IA_TAB_NAME: &str = "IA";
pub const IA_WIDGET_SPAN: u16 = 1;
const IA_INTERVAL: Duration = Duration::from_secs(3600);
const WARNING_SEPARATOR: &str = " | ";

pub struct WidgetPlacement {
    pub id: WidgetId,
    pub interval: Duration,
    pub max_items: Option<usize>,
    pub span: u16,
}

pub struct TabPlacement {
    pub name: String,
    pub widget_ids: Vec<WidgetId>,
}

pub struct TabLayout {
    pub entries: Vec<WidgetEntry>,
    pub tabs: Vec<TabView>,
    pub spans: Vec<u16>,
}

pub fn build_tab_views(
    widgets: &[WidgetPlacement],
    tabs: &[TabPlacement],
    ia_record: Option<WidgetRecord>,
) -> TabLayout {
    let mut entries: Vec<WidgetEntry> = widgets.iter().map(loading_entry).collect();
    let mut spans: Vec<u16> = widgets.iter().map(|widget| widget.span).collect();
    let mut tab_views: Vec<TabView> = tabs
        .iter()
        .map(|tab| TabView {
            name: tab.name.clone(),
            entry_indexes: entry_indexes_of(&entries, &tab.widget_ids),
        })
        .collect();
    if let Some(record) = ia_record {
        entries.push(WidgetEntry {
            id: WidgetId::new(IA_WIDGET_ID),
            record,
            interval: IA_INTERVAL,
            max_items: None,
        });
        spans.push(IA_WIDGET_SPAN);
        tab_views.push(TabView {
            name: IA_TAB_NAME.to_string(),
            entry_indexes: vec![entries.len() - 1],
        });
    }
    TabLayout {
        entries,
        tabs: tab_views,
        spans,
    }
}

pub fn startup_notice(warnings: &[String]) -> Option<String> {
    if warnings.is_empty() {
        return None;
    }
    Some(warnings.join(WARNING_SEPARATOR))
}

fn loading_entry(placement: &WidgetPlacement) -> WidgetEntry {
    WidgetEntry {
        id: placement.id.clone(),
        record: WidgetRecord::loading(),
        interval: placement.interval,
        max_items: placement.max_items,
    }
}

fn entry_indexes_of(entries: &[WidgetEntry], widget_ids: &[WidgetId]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| widget_ids.contains(&entry.id))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget(id: &str, span: u16) -> WidgetPlacement {
        WidgetPlacement {
            id: WidgetId::new(id),
            interval: Duration::from_secs(30),
            max_items: None,
            span,
        }
    }

    fn tab(name: &str, widget_ids: &[&str]) -> TabPlacement {
        TabPlacement {
            name: name.to_string(),
            widget_ids: widget_ids.iter().map(|id| WidgetId::new(*id)).collect(),
        }
    }

    fn ids(entries: &[WidgetEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.id.as_str()).collect()
    }

    fn names(tabs: &[TabView]) -> Vec<&str> {
        tabs.iter().map(|tab| tab.name.as_str()).collect()
    }

    #[test]
    fn ia_gets_its_own_trailing_tab_only_when_enabled() {
        let widgets = [widget("a", 1), widget("b", 1)];
        let tabs = [tab("Principal", &["a", "b"])];

        let without_ia = build_tab_views(&widgets, &tabs, None);
        assert_eq!(names(&without_ia.tabs), vec!["Principal"]);

        let with_ia = build_tab_views(&widgets, &tabs, Some(WidgetRecord::loading()));
        assert_eq!(names(&with_ia.tabs), vec!["Principal", IA_TAB_NAME]);
        assert_eq!(with_ia.tabs[1].entry_indexes, vec![2]);
        assert_eq!(with_ia.entries[2].id.as_str(), IA_WIDGET_ID);
    }

    #[test]
    fn spans_follow_entries_and_ia_span_comes_last() {
        let widgets = [widget("a", 2), widget("b", 3)];
        let tabs = [tab("Principal", &["a", "b"])];

        let without_ia = build_tab_views(&widgets, &tabs, None);
        assert_eq!(without_ia.spans, vec![2, 3]);
        assert_eq!(without_ia.spans.len(), without_ia.entries.len());

        let with_ia = build_tab_views(&widgets, &tabs, Some(WidgetRecord::loading()));
        assert_eq!(with_ia.spans, vec![2, 3, IA_WIDGET_SPAN]);
        assert_eq!(with_ia.spans.len(), with_ia.entries.len());
    }

    #[test]
    fn entry_indexes_follow_config_order() {
        let widgets = [widget("a", 1), widget("b", 1), widget("c", 1)];
        let tabs = [tab("Principal", &["c", "a"])];

        let layout = build_tab_views(&widgets, &tabs, None);

        assert_eq!(ids(&layout.entries), vec!["a", "b", "c"]);
        assert_eq!(layout.tabs[0].entry_indexes, vec![0, 2]);
    }

    #[test]
    fn tab_without_widgets_is_a_view_with_no_entries() {
        let widgets = [widget("a", 1)];
        let tabs = [tab("Vazia", &[]), tab("Principal", &["a"])];

        let layout = build_tab_views(&widgets, &tabs, None);

        assert_eq!(
            layout.tabs[0],
            TabView {
                name: "Vazia".to_string(),
                entry_indexes: vec![],
            }
        );
        assert_eq!(layout.tabs[1].entry_indexes, vec![0]);
    }

    #[test]
    fn widget_id_missing_from_entries_is_ignored() {
        let widgets = [widget("a", 1)];
        let tabs = [tab("Principal", &["fantasma", "a"])];

        let layout = build_tab_views(&widgets, &tabs, None);

        assert_eq!(layout.tabs[0].entry_indexes, vec![0]);
    }

    #[test]
    fn widget_listed_twice_in_a_tab_appears_once() {
        let widgets = [widget("a", 1)];
        let tabs = [tab("Principal", &["a", "a"])];

        let layout = build_tab_views(&widgets, &tabs, None);

        assert_eq!(layout.tabs[0].entry_indexes, vec![0]);
    }

    #[test]
    fn widget_in_two_tabs_appears_in_both() {
        let widgets = [widget("a", 1), widget("b", 1)];
        let tabs = [tab("Principal", &["a"]), tab("Outra", &["a", "b"])];

        let layout = build_tab_views(&widgets, &tabs, None);

        assert_eq!(layout.tabs[0].entry_indexes, vec![0]);
        assert_eq!(layout.tabs[1].entry_indexes, vec![0, 1]);
    }

    #[test]
    fn widget_in_no_tab_stays_out_of_every_tab_view() {
        let widgets = [widget("a", 1), widget("b", 1)];
        let tabs = [tab("Principal", &["a"])];

        let layout = build_tab_views(&widgets, &tabs, None);

        assert_eq!(ids(&layout.entries), vec!["a", "b"]);
        assert!(layout
            .tabs
            .iter()
            .all(|tab| !tab.entry_indexes.contains(&1)));
    }

    #[test]
    fn config_tab_named_ia_collides_with_the_ia_tab() {
        let widgets = [widget("a", 1)];
        let tabs = [tab(IA_TAB_NAME, &["a"])];

        let layout = build_tab_views(&widgets, &tabs, Some(WidgetRecord::loading()));

        assert_eq!(names(&layout.tabs), vec![IA_TAB_NAME, IA_TAB_NAME]);
        assert_eq!(layout.tabs[0].entry_indexes, vec![0]);
        assert_eq!(layout.tabs[1].entry_indexes, vec![1]);
    }

    #[test]
    fn config_warnings_join_into_one_notice() {
        let warnings = vec!["primeiro".to_string(), "segundo".to_string()];

        assert_eq!(
            startup_notice(&warnings),
            Some("primeiro | segundo".to_string())
        );
        assert_eq!(startup_notice(&[]), None);
    }
}
