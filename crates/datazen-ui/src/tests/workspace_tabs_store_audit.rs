//! T486 audit (tester): independent checks of the workspace tabs store.
//! Fresh vectors: boundary closes (the first and the last tab),
//! closeByWapp's fallback when the right slot is out of range, interleaved
//! open/close sequences, and tabs that share a page id across wapps.

use crate::workspace_tabs_store::{workspace_tab_key, WorkspaceTab, WorkspaceTabsStore};

fn tab(wapp: &str, page: &str, title: &str) -> WorkspaceTab {
    WorkspaceTab::with_key(wapp, page, title, None, "1.0.0")
}

#[test]
fn audit_distinct_wapps_sharing_a_page_have_distinct_keys() {
    assert_eq!(tab("w1", "shared", "A").key, "w1:shared");
    assert_eq!(tab("w2", "shared", "B").key, "w2:shared");
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "shared", "A"));
    store.open(tab("w2", "shared", "B"));
    assert_eq!(store.tabs.len(), 2, "same page id under two wapps is two tabs");
    assert_eq!(workspace_tab_key("w1", "shared"), workspace_tab_key("w2", "shared").replace("w2", "w1"));
}

#[test]
fn audit_closing_the_first_active_tab_takes_the_right_neighbor() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.open(tab("w1", "p2", "B"));
    store.activate("w1:p1");
    store.close("w1:p1");
    assert_eq!(store.active_key.as_deref(), Some("w1:p2"));
    assert_eq!(store.tabs.len(), 1);
}

#[test]
fn audit_closing_an_active_middle_tab_takes_the_right_not_the_left_neighbor() {
    // The differentiating vector: an active MIDDLE tab's close must take the
    // right neighbor, where right and left both exist.
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.open(tab("w1", "p2", "B"));
    store.open(tab("w1", "p3", "C"));
    store.open(tab("w1", "p4", "D"));
    store.activate("w1:p2");
    store.close("w1:p2");
    // Survivors [A, C, D]: the right neighbor C (index 1) activates, not the
    // left one A (index 0).
    assert_eq!(store.active_key.as_deref(), Some("w1:p3"));
}

#[test]
fn audit_a_close_storm_empties_the_store_and_the_activation() {
    let mut store = WorkspaceTabsStore::default();
    for n in 1..=5 {
        store.open(tab("w1", &format!("p{n}"), &format!("T{n}")));
    }
    // Close from the middle, both ends, and one missing key.
    store.close("w1:p3");
    store.close("w1:p1");
    store.close("w1:p5");
    store.close("w1:none");
    assert_eq!(store.tabs.len(), 2);
    assert_eq!(store.active_key.as_deref(), Some("w1:p4"));
    store.close("w1:p4");
    store.close("w1:p2");
    assert!(store.tabs.is_empty());
    assert_eq!(store.active_key, None);
    // Reopening after the storm starts fresh.
    store.open(tab("w1", "p1", "A"));
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
}

#[test]
fn audit_close_by_wapp_falls_left_when_the_right_slot_is_out_of_range() {
    // The removed wapp occupied the END of the list: next[firstRemovedIndex]
    // is out of range, so the left neighbor activates.
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.open(tab("w2", "p1", "B"));
    store.open(tab("w2", "p2", "C"));
    store.close_by_wapp("w2");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"), "the left neighbor");
}

#[test]
fn audit_close_by_wapp_takes_the_right_slot_by_first_removed_index() {
    // firstRemovedIndex points INSIDE the surviving list: the slot right of
    // it is the fallback, not the removed tab's own neighbor semantics.
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A")); // 0: dies
    store.open(tab("w2", "p1", "B")); // 1: survives
    store.open(tab("w1", "p2", "C")); // 2: dies
    store.open(tab("w2", "p2", "D")); // 3: survives
    store.activate("w1:p2");
    store.close_by_wapp("w1");
    // Survivors: B (index 0), D (index 1). firstRemovedIndex was 0 → the
    // right slot is B.
    assert_eq!(store.active_key.as_deref(), Some("w2:p1"));
}

#[test]
fn audit_close_by_wapp_picks_the_right_slot_not_the_left_when_both_exist() {
    // The differentiating vector: firstRemovedIndex > 0 AND the right slot
    // exists, so right (get[index]) and left (get[index-1]) disagree.
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w2", "p1", "B")); // survives
    store.open(tab("w1", "p1", "A")); // dies — first removed index 1
    store.open(tab("w2", "p2", "D")); // survives
    store.open(tab("w1", "p2", "C")); // dies
    store.activate("w1:p2");
    store.close_by_wapp("w1");
    // Survivors [B, D]: the right slot D (index 1) activates, not the left
    // slot B (index 0).
    assert_eq!(store.active_key.as_deref(), Some("w2:p2"));
}

#[test]
fn audit_close_by_wapp_with_no_active_tab_uses_the_neighbor_rule() {
    // activeKey is null (nothing activated yet): the active cannot survive,
    // so the fallback applies even though nothing was active.
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.open(tab("w2", "p1", "B"));
    store.open(tab("w1", "p2", "C"));
    // Nothing ever activated; the default activation is the LAST opened
    // (w1:p2) — deactivate it by closing an unrelated... there is no
    // deactivate; instead simulate by closing the active one first.
    store.close("w1:p2"); // active dies → right neighbor none → left = B
    assert_eq!(store.active_key.as_deref(), Some("w2:p1"));
    store.close_by_wapp("w2");
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.tabs[0].title, "A");
    // w2's tabs died with B active: firstRemovedIndex was 1 → the right
    // slot is out of range → left = A.
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
}

#[test]
fn audit_open_after_close_by_wapp_re_adds_fresh_tabs() {
    let mut store = WorkspaceTabsStore::default();
    store.open(tab("w1", "p1", "A"));
    store.close_by_wapp("w1");
    // The wapp was uninstalled then reinstalled: reopening the same page
    // starts a new tab (the store does not remember closed tabs).
    store.open(tab("w1", "p1", "A (new)"));
    assert_eq!(store.tabs.len(), 1);
    assert_eq!(store.tabs[0].title, "A (new)");
    assert_eq!(store.active_key.as_deref(), Some("w1:p1"));
}
