//! T486: the workspace tabs store — the stable key, idempotent open, the
//! close neighbor rule (right first, then left, then none), activate's
//! membership gate, and closeByWapp's surviving-active branch.

use datazen_ui::workspace_tabs_store::{workspace_tab_key, WorkspaceTab, WorkspaceTabsStore};

fn tab(wapp: &str, page: &str, title: &str) -> WorkspaceTab {
    WorkspaceTab::with_key(wapp, page, title, None, "1.0.0")
}

#[test]
fn the_key_is_the_wapp_and_page_joined_by_a_colon() {
    assert_eq!(workspace_tab_key("w1", "p2"), "w1:p2");
    // The parts keep their spelling — no escaping, no trimming.
    assert_eq!(workspace_tab_key("app-a", "page:weird"), "app-a:page:weird");
    assert_eq!(workspace_tab_key("", ""), ":");
}

#[test]
fn open_appends_and_activates() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.open(tab("w1", "p2", "Second"));
    assert_eq!(store.tabs.len(), 2);
    assert_eq!(store.active_key.as_deref(), Some("w1:p2"));
}

#[test]
fn open_is_idempotent_per_key_and_refreshes_in_place() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.open(tab("w1", "p2", "Second"));
    // Re-open the FIRST tab: order kept, metadata refreshed, it activates.
    let updated = WorkspaceTab::with_key("w1", "p1", "First (v2)", Some("icon".into()), "2.0.0");
    store.open(updated);
    assert_eq!(store.tabs.len(), 2, "no duplicate tab");
    assert_eq!(store.tabs[0].title, "First (v2)");
    assert_eq!(store.tabs[0].version, "2.0.0");
    assert_eq!(store.tabs[0].icon.as_deref(), Some("icon"));
    assert_eq!(store.tabs[1].title, "Second", "order is kept");
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
}

#[test]
fn closing_an_inactive_tab_keeps_the_activation() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.open(tab("w1", "p2", "Second"));
    store.close("w1:p1");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.active_key.as_deref(), Some("w1:p2"));
}

#[test]
fn closing_the_active_tab_takes_the_right_neighbor() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.open(tab("w1", "p2", "Second"));
    store.open(tab("w1", "p3", "Third"));
    // The closed tab must BE the active one for the neighbor rule to fire;
    // without this activation the close takes the inactive path.
    store.activate("w1:p2");
    store.close("w1:p2");
    assert_eq!(store.active_key.as_deref(), Some("w1:p3"), "right neighbor, not the left one");
}

#[test]
fn closing_the_last_active_tab_takes_the_left_neighbor() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.open(tab("w1", "p2", "Second"));
    store.open(tab("w1", "p3", "Third"));
    store.close("w1:p3");
    assert_eq!(store.active_key.as_deref(), Some("w1:p2"), "left neighbor");
}

#[test]
fn closing_the_only_tab_leaves_no_active_tab() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.close("w1:p1");
    assert!(store.tabs.is_empty());
    assert_eq!(store.active_key, None);
}

#[test]
fn closing_a_missing_key_is_a_no_op() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.close("no-such-key");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
}

#[test]
fn activate_only_moves_to_a_key_that_exists() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "First"));
    store.open(tab("w1", "p2", "Second"));
    store.activate("w1:p1");
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
    store.activate("w1:missing");
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"), "a missing key does not move it");
}

#[test]
fn close_by_wapp_keeps_a_foreign_active_tab_untouched() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.open(tab("w2", "p1", "B"));
    store.open(tab("w1", "p2", "C"));
    // Active is w2's: closing w1's tabs leaves it active.
    store.close_by_wapp("w1");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.tabs[0].title, "B");
    assert_eq!(store.active_key.as_deref(), Some("w2:p1"));
}

#[test]
fn close_by_wapp_follows_the_neighbor_rule_when_the_active_tab_dies() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.open(tab("w1", "p2", "B"));
    store.open(tab("w2", "p1", "C"));
    store.open(tab("w1", "p3", "D"));
    // Active is w1:p3 (D); removing all of w1 leaves C at index 1 — the
    // slot right of the first removed index (0) is C.
    store.close_by_wapp("w1");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.tabs[0].title, "C");
    assert_eq!(store.active_key.as_deref(), Some("w2:p1"));
}

#[test]
fn close_by_wapp_without_any_tab_of_that_app_is_a_no_op() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.close_by_wapp("w9");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
}

#[test]
fn close_by_wapp_emptying_the_store_leaves_no_active_tab() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.close_by_wapp("w1");
    assert!(store.tabs.is_empty());
    assert_eq!(store.active_key, None);
}
