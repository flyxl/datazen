//! T232 independent audit: the foreign-key navigation stack, fresh
//! vectors.
//!
//! The coder tests cover one vector per rule; this audit attacks from
//! the other side: an interleaved walk with a dead tab buried mid-stack,
//! a composite-key origin travelling intact through push/go_back, the
//! viewport re-record round-trip, prune with two survivors out of
//! three, a rewind-to-top no-op, the truncated flag's persistence
//! through unrelated ops, and the empty-store edges.

use std::collections::HashSet;

use crate::fk_navigation_store::{
    FkNavJump, FkNavOrigin, FkNavigationStore, GridViewport, FK_NAV_MAX_DEPTH,
};

fn composite_jump(panel_id: &str, nav_key: &str, source_panel: &str) -> FkNavJump {
    FkNavJump {
        panel_id: panel_id.to_owned(),
        nav_key: nav_key.to_owned(),
        origin: FkNavOrigin {
            source_panel_id: source_panel.to_owned(),
            source_table: "order_items".to_owned(),
            local_columns: vec!["order_id".to_owned(), "product_id".to_owned()],
            target_table: "order_line".to_owned(),
            target_columns: vec!["order".to_owned(), "product".to_owned()],
        },
    }
}

fn simple_jump(panel_id: &str, nav_key: &str) -> FkNavJump {
    FkNavJump {
        panel_id: panel_id.to_owned(),
        nav_key: nav_key.to_owned(),
        origin: FkNavOrigin {
            source_panel_id: format!("src-of-{panel_id}"),
            source_table: "orders".to_owned(),
            local_columns: vec!["customer_id".to_owned()],
            target_table: "customers".to_owned(),
            target_columns: vec!["id".to_owned()],
        },
    }
}

fn live(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn a_composite_origin_travels_intact_through_push_and_go_back() {
    let mut store = FkNavigationStore::new();
    let created = store.push(composite_jump("tab-x", "orders.1|products.2", "tab-w"));
    assert_eq!(created.id, "fk-nav-1");

    // goBack answers the entry with the origin untouched — the pairing
    // order included.
    let entry = store.go_back(&live(&["tab-x"])).expect("the entry is alive");
    assert_eq!(entry.id, "fk-nav-1");
    assert_eq!(entry.origin.local_columns, vec!["order_id", "product_id"]);
    assert_eq!(entry.origin.target_columns, vec!["order", "product"]);
    assert_eq!(entry.origin.source_panel_id, "tab-w");
    // The back activation target is the source tab, not the opened one.
    assert_ne!(entry.origin.source_panel_id, entry.panel_id);
}

#[test]
fn an_interleaved_walk_with_a_dead_tab_buried_mid_stack() {
    let mut store = FkNavigationStore::new();
    for (panel, key) in [("p1", "a"), ("p2", "b"), ("p3", "c"), ("p4", "d")] {
        store.push(simple_jump(panel, key));
    }
    // The user walks back two, forward one.
    let all = live(&["p1", "p2", "p3", "p4"]);
    store.go_back(&all); // d leaves: back [a,b,c], forward [d]
    store.go_back(&all); // c leaves: back [a,b], forward [d,c]
    store.go_forward(&all); // c re-enters: back [a,b,c], forward [d]

    // p2's tab closes: the dead b sits buried UNDER the living c.
    let alive = live(&["p1", "p3", "p4"]);
    // The next goBack stops at the living top (c): the dead b below
    // it SURVIVES the walk — React's scan only passes entries above
    // the one it returns to.
    let entry = store.go_back(&alive).expect("the living c on top");
    assert_eq!(entry.panel_id, "p3");
    assert_eq!(
        store.back().iter().map(|e| e.nav_key.clone()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(
        store.forward().iter().map(|e| e.nav_key.clone()).collect::<Vec<_>>(),
        vec!["d", "c"]
    );

    // The goBack after that scans THROUGH the dead b: it is skipped
    // and dropped, and the walk lands on a.
    let entry = store.go_back(&alive).expect("the living a under the dead b");
    assert_eq!(entry.panel_id, "p1");
    assert!(store.back().is_empty());
    assert_eq!(
        store.forward().iter().map(|e| e.nav_key.clone()).collect::<Vec<_>>(),
        vec!["d", "c", "a"]
    );

    // Now p1's tab closes too: the forward top (a) is dead, so a
    // forward walk skips it and re-enters c — a drops from the stack.
    let only_c_d = live(&["p3", "p4"]);
    let entry = store.go_forward(&only_c_d).expect("the living c below the dead a");
    assert_eq!(entry.panel_id, "p3");
    assert!(store.forward().iter().all(|e| e.nav_key != "a"));
    assert_eq!(
        store.forward().iter().map(|e| e.nav_key.clone()).collect::<Vec<_>>(),
        vec!["d"]
    );
}

#[test]
fn the_viewport_re_record_round_trip_after_a_take() {
    let mut store = FkNavigationStore::new();
    let first = GridViewport {
        scroll_top: 120.0,
        anchor_row: Some(3),
        anchor_column: Some("name".to_owned()),
    };
    let second = GridViewport {
        scroll_top: 480.0,
        anchor_row: None,
        anchor_column: None,
    };

    store.record_viewport("tab-r", first.clone());
    // The take consumes…
    assert_eq!(store.take_viewport("tab-r"), Some(first));
    // …a second take misses…
    assert_eq!(store.take_viewport("tab-r"), None);
    // …and a re-entered tab records a fresh position.
    store.record_viewport("tab-r", second.clone());
    assert_eq!(store.peek_viewport("tab-r"), Some(&second));

    // No-anchor viewports (both anchors None) travel as themselves.
    assert_eq!(store.take_viewport("tab-r"), Some(second));
}

#[test]
fn prune_keeps_the_two_survivors_of_three() {
    let mut store = FkNavigationStore::new();
    for (panel, key) in [("q1", "x"), ("q2", "y"), ("q3", "z")] {
        store.push(simple_jump(panel, key));
    }
    store.record_viewport("q1", GridViewport { scroll_top: 1.0, anchor_row: None, anchor_column: None });
    store.record_viewport("q2", GridViewport { scroll_top: 2.0, anchor_row: Some(1), anchor_column: None });
    store.record_viewport("q3", GridViewport { scroll_top: 3.0, anchor_row: None, anchor_column: Some("c".to_owned()) });

    // The user walked back one first (forward holds the top entry).
    let all = live(&["q1", "q2", "q3"]);
    store.go_back(&all);

    // q3's tab closes: its entry drops from the forward stack, its
    // viewport drops, q1/q2 survive intact everywhere.
    let survivors = live(&["q1", "q2"]);
    store.prune(&survivors);
    assert_eq!(
        store.back().iter().map(|e| e.nav_key.clone()).collect::<Vec<_>>(),
        vec!["x", "y"]
    );
    assert!(store.forward().is_empty());
    assert_eq!(store.peek_viewport("q1").map(|v| v.scroll_top), Some(1.0));
    assert_eq!(store.peek_viewport("q2").map(|v| v.scroll_top), Some(2.0));
    assert_eq!(store.peek_viewport("q3"), None);
}

#[test]
fn rewind_to_the_top_entry_is_a_no_op_for_back_but_clears_forward() {
    let mut store = FkNavigationStore::new();
    store.push(simple_jump("t1", "a"));
    store.push(simple_jump("t2", "b"));
    store.go_back(&live(&["t1", "t2"])); // forward [b], back [a]

    // Rewinding to the top of the back stack keeps the back stack whole…
    store.rewind_to("fk-nav-1");
    assert_eq!(store.back().len(), 1);
    // …and still clears the forward stack.
    assert!(store.forward().is_empty());
}

#[test]
fn the_truncated_flag_survives_prune_and_rewind() {
    let mut store = FkNavigationStore::new();
    for index in 0..=FK_NAV_MAX_DEPTH {
        store.push(simple_jump(&format!("m{index}"), &format!("k{index}")));
    }
    assert!(store.truncated());

    // Unrelated operations never clear the flag; only reset does.
    store.rewind_to("fk-nav-1");
    assert!(store.truncated());
    store.prune(&live(&["m0", "m1"]));
    assert!(store.truncated());
    store.go_back(&live(&["m0"]));
    assert!(store.truncated());
    store.go_forward(&live(&["m0"]));
    assert!(store.truncated());

    store.reset();
    assert!(!store.truncated());
}

#[test]
fn the_empty_store_edges_answer_none_without_touching_anything() {
    let mut store = FkNavigationStore::new();
    assert_eq!(store.go_back(&live(&["anything"])), None);
    assert_eq!(store.go_forward(&live(&["anything"])), None);
    assert!(store.back().is_empty());
    assert!(store.forward().is_empty());

    // Rewind and prune on the empty store are no-ops too.
    store.rewind_to("fk-nav-1");
    store.prune(&live(&["anything"]));
    assert!(store.back().is_empty());
    assert_eq!(store.peek_viewport("anything"), None);
    assert_eq!(store.find_entry_id_by_nav_key("anything"), None);
}

#[test]
fn find_by_nav_key_answers_the_newest_after_a_cycle_re_push() {
    let mut store = FkNavigationStore::new();
    store.push(simple_jump("c1", "shared-key"));
    store.push(simple_jump("c2", "other"));
    // The cycle rule: the rewind cuts the detour, then the same target
    // is jumped again under the same key.
    store.rewind_to("fk-nav-1");
    store.push(simple_jump("c3", "shared-key"));

    assert_eq!(store.find_entry_id_by_nav_key("shared-key"), Some("fk-nav-3"));
    assert_eq!(store.find_entry_id_by_nav_key("other"), None);
    assert_eq!(store.find_entry_id_by_nav_key("missing"), None);
}
