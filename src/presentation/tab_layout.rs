use std::time::Duration;

use crate::application::recommendation_view::IA_WIDGET_ID;
use crate::domain::widget::{WidgetId, WidgetRecord};
use crate::presentation::app_state::{TabView, WidgetEntry};

pub const IA_TAB_NAME: &str = "IA";
const IA_INTERVAL: Duration = Duration::from_secs(3600);

pub struct WidgetPlacement {
    pub id: WidgetId,
    pub interval: Duration,
    pub max_items: Option<usize>,
}

pub struct TabPlacement {
    pub name: String,
    pub widget_ids: Vec<WidgetId>,
}

pub fn build_tab_views(
    widgets: &[WidgetPlacement],
    tabs: &[TabPlacement],
    ia_record: Option<WidgetRecord>,
) -> (Vec<WidgetEntry>, Vec<TabView>) {
    let mut entries: Vec<WidgetEntry> = widgets.iter().map(loading_entry).collect();
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
        tab_views.push(TabView {
            name: IA_TAB_NAME.to_string(),
            entry_indexes: vec![entries.len() - 1],
        });
    }
    (entries, tab_views)
}

pub fn startup_notice(warnings: &[String]) -> Option<String> {
    warnings.first().cloned()
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

    fn widget(id: &str) -> WidgetPlacement {
        WidgetPlacement {
            id: WidgetId::new(id),
            interval: Duration::from_secs(30),
            max_items: None,
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

    #[test]
    fn ia_gets_its_own_trailing_tab_only_when_enabled() {
        let widgets = [widget("a"), widget("b")];
        let tabs = [tab("Principal", &["a", "b"])];

        let (_, without_ia) = build_tab_views(&widgets, &tabs, None);
        assert_eq!(without_ia.len(), 1);

        let (entries, with_ia) = build_tab_views(&widgets, &tabs, Some(WidgetRecord::loading()));
        assert_eq!(with_ia.len(), 2);
        assert_eq!(with_ia[1].name, IA_TAB_NAME);
        assert_eq!(with_ia[1].entry_indexes, vec![2]);
        assert_eq!(entries[2].id.as_str(), IA_WIDGET_ID);
    }

    #[test]
    fn entry_indexes_follow_config_order() {
        let widgets = [widget("a"), widget("b"), widget("c")];
        let tabs = [tab("Principal", &["c", "a"])];

        let (entries, tab_views) = build_tab_views(&widgets, &tabs, None);

        assert_eq!(ids(&entries), vec!["a", "b", "c"]);
        assert_eq!(tab_views[0].entry_indexes, vec![0, 2]);
    }

    #[test]
    fn tab_without_widgets_is_a_view_with_no_entries() {
        let widgets = [widget("a")];
        let tabs = [tab("Vazia", &[]), tab("Principal", &["a"])];

        let (_, tab_views) = build_tab_views(&widgets, &tabs, None);

        assert_eq!(
            tab_views[0],
            TabView {
                name: "Vazia".to_string(),
                entry_indexes: vec![],
            }
        );
        assert_eq!(tab_views[1].entry_indexes, vec![0]);
    }

    #[test]
    fn config_warnings_become_the_first_notice() {
        let warnings = vec!["primeiro".to_string(), "segundo".to_string()];

        assert_eq!(startup_notice(&warnings), Some("primeiro".to_string()));
        assert_eq!(startup_notice(&[]), None);
    }
}
