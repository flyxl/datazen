//! T232: the foreign-key navigation stack — coder tests.
//! The stack rules (the once-only jump, the dead-tab pruning, the cycle
//! rewind), the depth cap, the viewport record/peek/take trio, and the
//! reset.

use std::collections::HashSet;

use crate::fk_navigation_store::{
    FkNavJump, FkNavOrigin, FkNavigationStore, GridViewport, FK_NAV_MAX_DEPTH,
};

fn jump(panel_id: &str, nav_key: &str) -> FkNavJump {
    FkNavJump {
        panel_id: panel_id.to_owned(),
        nav_key: nav_key.to_owned(),
        origin: FkNavOrigin {
            source_panel_id: format!("src-{panel_id}"),
            source_table: "orders".to_owned(),
            local_columns: vec!["customer_id".to_owned()],
            target_table: "customers".to_owned(),
            target_columns: vec!["id".to_owned()],
        },
    }
}

fn ids(store: &FkNavigationStore) -> Vec<&str> {
    store.back().iter().map(|entry| entry.id.as_str()).collect()
}

fn panels(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

// ---- the push / depth cap ----------------------------------------------------

#[test]
fn push_mints_sequential_ids_and_clears_the_forward_stack() {
    let mut store = FkNavigationStore::new();
    let first = store.push(jump("tab-1", "k1"));
    assert_eq!(first.id, "fk-nav-1");
    assert_eq!(first.panel_id, "tab-1");
    assert_eq!(first.origin.source_panel_id, "src-tab-1");

    let second = store.push(jump("tab-2", "k2"));
    assert_eq!(second.id, "fk-nav-2");
    assert_eq!(store.back().len(), 2);
    assert_eq!(ids(&store), vec!["fk-nav-1", "fk-nav-2"]);

    // goBack then goForward leaves a forward entry; a fresh push clears it.
    let existing = panels(&["tab-1", "tab-2"]);
    store.go_back(&existing);
    assert_eq!(store.forward().len(), 1);
    store.push(jump("tab-3", "k3"));
    assert!(store.forward().is_empty());
    assert_eq!(store.back().len(), 2); // the back entry left, the new one in
}

#[test]
fn the_depth_cap_drops_the_oldest_and_marks_truncated() {
    let mut store = FkNavigationStore::new();
    for index in 0..FK_NAV_MAX_DEPTH {
        store.push(jump(&format!("tab-{index}"), &format!("k{index}")));
    }
    assert_eq!(store.back().len(), FK_NAV_MAX_DEPTH);
    assert!(!store.truncated());

    // The cap+1-th push drops the oldest entry.
    store.push(jump("tab-overflow", "kx"));
    assert_eq!(store.back().len(), FK_NAV_MAX_DEPTH);
    assert!(store.truncated());
    // The oldest (fk-nav-1) is gone; the newest is on top.
    assert!(store.back().iter().all(|entry| entry.id != "fk-nav-1"));
    assert_eq!(store.back()[0].id, "fk-nav-2");
    assert_eq!(store.back()[FK_NAV_MAX_DEPTH - 1].id, format!("fk-nav-{}", FK_NAV_MAX_DEPTH + 1));

    // The flag survives until reset (React never clears it elsewhere).
    store.go_back(&panels(&[]));
    assert!(store.truncated());
    store.reset();
    assert!(!store.truncated());
}

// ---- the back / forward walk -------------------------------------------------

#[test]
fn go_back_skips_and_drops_dead_entries() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    store.push(jump("tab-c", "k3"));

    // The top entry's tab is closed: it is skipped AND dropped.
    let existing = panels(&["tab-a", "tab-b"]);
    let entry = store.go_back(&existing).expect("a living entry below the dead top");
    assert_eq!(entry.panel_id, "tab-b");
    // The back activation target is the SOURCE tab of the entry.
    assert_eq!(entry.origin.source_panel_id, "src-tab-b");
    // The dead tab-c entry dropped out of the back stack.
    assert_eq!(ids(&store), vec!["fk-nav-1"]);
    // The stepped-over entry moved to the forward stack.
    assert_eq!(store.forward().iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["fk-nav-2"]);

    // A second goBack with only tab-a alive reaches the bottom.
    let existing = panels(&["tab-a"]);
    let entry = store.go_back(&existing).expect("the bottom entry");
    assert_eq!(entry.panel_id, "tab-a");
    assert!(store.back().is_empty());
    assert_eq!(store.forward().len(), 2);

    // Nothing left to go back to: None, and the stacks stay as they are.
    assert_eq!(store.go_back(&existing), None);
    assert!(store.back().is_empty());
    assert_eq!(store.forward().len(), 2);
}

#[test]
fn go_back_with_all_dead_tabs_empties_back_but_keeps_forward() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    // One step back first, so the forward stack is populated.
    let existing = panels(&["tab-a", "tab-b"]);
    store.go_back(&existing);
    assert_eq!(store.forward().len(), 1);

    // Now every back tab dies: the back stack empties, the forward
    // stack survives (React's `set({ back: [], forward })`).
    assert_eq!(store.go_back(&panels(&[])), None);
    assert!(store.back().is_empty());
    assert_eq!(store.forward().len(), 1);
}

#[test]
fn go_forward_replays_the_jump_and_clears_the_stack_to_the_entry() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    let existing = panels(&["tab-a", "tab-b"]);
    store.go_back(&existing);
    store.go_back(&existing);
    // The forward stack is [B, A] — the top is the last jump the user
    // backed out of, so forward re-enters A first (the browser's rule).
    assert_eq!(store.forward().len(), 2);

    // goForward activates the entry's own panel (the opened tab).
    let entry = store.go_forward(&existing).expect("the top forward entry");
    assert_eq!(entry.panel_id, "tab-a");
    assert_eq!(store.forward().len(), 1);
    assert_eq!(ids(&store), vec!["fk-nav-1"]);

    let entry = store.go_forward(&existing).expect("the older forward entry");
    assert_eq!(entry.panel_id, "tab-b");
    assert!(store.forward().is_empty());
    assert_eq!(ids(&store), vec!["fk-nav-1", "fk-nav-2"]);

    // Nothing left: None.
    assert_eq!(store.go_forward(&existing), None);
}

#[test]
fn go_forward_with_a_dead_tail_clears_both_stacks() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    // One step back: the back stack holds A, the forward holds B.
    let existing = panels(&["tab-a", "tab-b"]);
    store.go_back(&existing);
    assert_eq!(ids(&store), vec!["fk-nav-1"]);
    assert_eq!(store.forward().len(), 1);

    // Every tab dies meanwhile: React's dead-tail arm clears BOTH
    // stacks (back: [] too), not just the forward one.
    assert_eq!(store.go_forward(&panels(&[])), None);
    assert!(store.back().is_empty());
    assert!(store.forward().is_empty());
}

#[test]
fn go_forward_caps_the_back_stack_from_the_front_without_the_flag() {
    let mut store = FkNavigationStore::new();
    // A full back stack (32), then walk back all the way, then a forward
    // replay past the cap.
    let mut all = Vec::new();
    for index in 0..FK_NAV_MAX_DEPTH {
        store.push(jump(&format!("tab-{index}"), &format!("k{index}")));
        all.push(format!("tab-{index}"));
    }
    let existing: HashSet<String> = all.iter().map(|id| id.to_string()).collect();
    for _ in 0..FK_NAV_MAX_DEPTH {
        store.go_back(&existing);
    }
    assert_eq!(store.forward().len(), FK_NAV_MAX_DEPTH);
    assert!(!store.truncated());

    // One replay re-enters the newest left jump; the flag stays false
    // (React's own goForward arm never sets it).
    let replayed = store.go_forward(&existing).expect("the newest forward entry");
    assert_eq!(store.back().len(), 1);
    assert_eq!(store.back().last().map(|e| e.id.as_str()), Some(replayed.id.as_str()));
    assert!(!store.truncated());

    // The full walk lands exactly at the cap, in order, never past it.
    for _ in 1..FK_NAV_MAX_DEPTH {
        store.go_forward(&existing).expect("the walk continues");
    }
    assert_eq!(store.back().len(), FK_NAV_MAX_DEPTH);
    assert_eq!(store.back()[0].id, "fk-nav-1");
    assert!(store.forward().is_empty());
    assert!(!store.truncated());
}

// ---- the cycle rewind --------------------------------------------------------

#[test]
fn rewind_to_cuts_after_the_entry_and_clears_forward() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    store.push(jump("tab-c", "k3"));
    let existing = panels(&["tab-a", "tab-b", "tab-c"]);
    store.go_back(&existing);
    store.go_back(&existing);
    assert_eq!(store.forward().len(), 2);

    // Rewind to the middle entry: the back stack keeps through it, the
    // forward stack empties.
    store.rewind_to("fk-nav-1");
    assert_eq!(ids(&store), vec!["fk-nav-1"]);
    assert!(store.forward().is_empty());

    // An unknown id changes nothing.
    store.rewind_to("fk-nav-999");
    assert_eq!(ids(&store), vec!["fk-nav-1"]);
}

// ---- the viewports -----------------------------------------------------------

fn viewport(scroll_top: f64) -> GridViewport {
    GridViewport {
        scroll_top,
        anchor_row: Some(12),
        anchor_column: Some("id".to_owned()),
    }
}

#[test]
fn the_viewport_trio_records_peeks_and_takes() {
    let mut store = FkNavigationStore::new();

    // Nothing recorded yet.
    assert_eq!(store.peek_viewport("tab-1"), None);
    assert_eq!(store.take_viewport("tab-1"), None);

    // Record + peek keeps the snapshot.
    store.record_viewport("tab-1", viewport(100.0));
    assert_eq!(store.peek_viewport("tab-1"), Some(&viewport(100.0)));
    assert_eq!(store.peek_viewport("tab-1"), Some(&viewport(100.0)));
    // Another tab's peek misses.
    assert_eq!(store.peek_viewport("tab-2"), None);

    // Only the latest snapshot of a tab is kept.
    store.record_viewport("tab-1", viewport(250.0));
    assert_eq!(store.peek_viewport("tab-1"), Some(&viewport(250.0)));

    // Take consumes: a second take misses.
    assert_eq!(store.take_viewport("tab-1"), Some(viewport(250.0)));
    assert_eq!(store.take_viewport("tab-1"), None);
    assert_eq!(store.peek_viewport("tab-1"), None);
}

// ---- prune / reset -----------------------------------------------------------

#[test]
fn prune_drops_dead_entries_and_dead_viewports() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    store.push(jump("tab-c", "k3"));
    store.record_viewport("tab-a", viewport(10.0));
    store.record_viewport("tab-dead", viewport(20.0));
    let existing = panels(&["tab-a", "tab-b", "tab-c"]);
    store.go_back(&existing);
    store.go_back(&existing);

    // tab-b and tab-c die: their entries drop from both stacks, and the
    // dead viewport snapshot drops too.
    let living = panels(&["tab-a"]);
    store.prune(&living);
    assert_eq!(ids(&store), vec!["fk-nav-1"]);
    assert!(store.forward().is_empty());
    assert_eq!(store.peek_viewport("tab-a"), Some(&viewport(10.0)));
    assert_eq!(store.peek_viewport("tab-dead"), None);
}

#[test]
fn reset_forgets_everything_but_the_counter_survives() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    store.record_viewport("tab-a", viewport(1.0));
    store.go_back(&panels(&["tab-a", "tab-b"]));

    store.reset();
    assert!(store.back().is_empty());
    assert!(store.forward().is_empty());
    assert!(!store.truncated());
    assert_eq!(store.peek_viewport("tab-a"), None);

    // The counter survived: the next id continues the sequence (React's
    // module counter survives reset the same way).
    let next = store.push(jump("tab-c", "k3"));
    assert_eq!(next.id, "fk-nav-3");
}

// ---- find_entry_id_by_nav_key ------------------------------------------------

#[test]
fn find_by_nav_key_answers_the_newest_match() {
    let mut store = FkNavigationStore::new();
    store.push(jump("tab-a", "k1"));
    store.push(jump("tab-b", "k2"));
    // The cycle rule can stack the same key again after a rewind.
    store.rewind_to("fk-nav-1");
    store.push(jump("tab-c", "k1"));

    assert_eq!(store.find_entry_id_by_nav_key("k1"), Some("fk-nav-3"));
    assert_eq!(store.find_entry_id_by_nav_key("k2"), None);
    // An empty store answers None for everything.
    let mut fresh = FkNavigationStore::new();
    assert_eq!(fresh.find_entry_id_by_nav_key("k1"), None);
}
