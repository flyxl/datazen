//! Workspace tabs. A tab is either a table or a query. Titles may repeat.
//! Empty titles are rejected. Ids start at 1 and are never reused.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Table,
    Query,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: u32,
    pub kind: TabKind,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tabs {
    next_id: u32,
    tabs: Vec<Tab>,
    selected: Option<u32>,
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

impl Tabs {
    pub fn new() -> Self {
        Self { next_id: 0, tabs: Vec::new(), selected: None }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn selected(&self) -> Option<&Tab> {
        self.selected.and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
    }

    pub fn open(&mut self, kind: TabKind, title: &str) -> Option<u32> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.tabs.push(Tab { id, kind, title: title.to_owned() });
        self.selected = Some(id);
        Some(id)
    }

    /// Returns false when `id` is not open. The previous selection stays.
    pub fn select(&mut self, id: u32) -> bool {
        if !self.tabs.iter().any(|tab| tab.id == id) {
            return false;
        }
        self.selected = Some(id);
        true
    }

    /// Closing the selected tab selects the tab on its left, or the new first tab
    /// when the closed tab was first. Closing the last tab clears the selection.
    pub fn close(&mut self, id: u32) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        let was_selected = self.selected == Some(id);
        self.tabs.remove(index);
        if was_selected {
            self.selected = if self.tabs.is_empty() {
                None
            } else if index == 0 {
                Some(self.tabs[0].id)
            } else {
                Some(self.tabs[index - 1].id)
            };
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_two_tables_selects_the_second_and_closing_it_returns_to_the_first() {
        let mut tabs = Tabs::new();
        let first = tabs.open(TabKind::Table, "customers").unwrap();
        let second = tabs.open(TabKind::Table, "orders").unwrap();
        assert_eq!(tabs.tabs().len(), 2);
        assert_eq!(tabs.selected().map(|tab| tab.id), Some(second));
        assert!(tabs.close(second));
        assert_eq!(tabs.tabs().len(), 1);
        assert_eq!(tabs.selected().map(|tab| tab.id), Some(first));
        assert_eq!(tabs.selected().map(|tab| tab.kind), Some(TabKind::Table));
    }

    #[test]
    fn blank_titles_and_unknown_ids_do_not_change_the_open_set() {
        let mut tabs = Tabs::new();
        assert!(tabs.open(TabKind::Query, "  ").is_none());
        let id = tabs.open(TabKind::Query, " draft ").unwrap();
        assert_eq!(tabs.selected().unwrap().title, "draft");
        assert!(!tabs.select(id + 9));
        assert_eq!(tabs.selected().unwrap().id, id);
        assert!(!tabs.close(id + 9));
        assert_eq!(tabs.tabs().len(), 1);
    }
}
