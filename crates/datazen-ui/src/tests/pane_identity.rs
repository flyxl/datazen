//! T274: the metadata-identity binder seam, the pane-focus writer, and
//! the pane key space — coder vectors.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::metadata_identity_binding::MetadataIdentityBinding;
use crate::pane_focus::sync_pane_focus;
use crate::pane_keys::{
    is_pane_key_of_panel, pane_args, pane_id_of_pane_key, pane_key, pane_keys_of_panel,
    panel_id_of_pane_key, resolve_focused_pane_id, DEFAULT_PANE_ID,
};

// -- metadataIdentityBinding.ts -------------------------------------------

#[test]
fn the_binder_is_inert_until_registered() {
    let mut binding = MetadataIdentityBinding::new();
    assert!(!binding.has_binder());
    let seen = Rc::new(RefCell::new(Vec::<(String, String, Option<String>)>::new()));
    let recorder = {
        let seen = Rc::clone(&seen);
        Rc::new(move |session: &str, connection: &str, scope: Option<&str>| {
            seen.borrow_mut()
                .push((session.to_owned(), connection.to_owned(), scope.map(str::to_owned)));
        })
    };
    // Inert: the announce lands nowhere.
    binding.bind_session_metadata_identity("s1", "c1", None);
    assert!(seen.borrow().is_empty());

    let registration = binding.register(recorder);
    assert!(binding.has_binder());
    binding.bind_session_metadata_identity("s1", "c1", None);
    binding.bind_session_metadata_identity("s2", "c2", Some("public"));
    assert_eq!(
        *seen.borrow(),
        vec![
            ("s1".to_owned(), "c1".to_owned(), None),
            ("s2".to_owned(), "c2".to_owned(), Some("public".to_owned())),
        ]
    );

    // The disposer restores the void.
    binding.unregister(&registration);
    assert!(!binding.has_binder());
    binding.bind_session_metadata_identity("s3", "c3", None);
    assert_eq!(seen.borrow().len(), 2, "inert again after the disposer");
}

#[test]
fn a_replaced_binder_is_not_uninstalled_by_the_old_token() {
    let binding = &mut MetadataIdentityBinding::new();
    let first = Rc::new(|_: &str, _: &str, _: Option<&str>| {});
    let second = Rc::new(|_: &str, _: &str, _: Option<&str>| {});

    let first_token = binding.register(first);
    let _second_token = binding.register(second);
    // The old disposer fires, but the binder it names is no longer
    // installed — the void is NOT restored.
    binding.unregister(&first_token);
    assert!(binding.has_binder(), "a replaced binder survives the old token");
}

// -- paneFocus.ts ------------------------------------------------------------

fn focus_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(panel, pane)| ((*panel).to_owned(), (*pane).to_owned())).collect()
}

#[test]
fn dead_panels_drop_and_the_mirror_follows_the_active_tab() {
    let prev = focus_map(&[("p1", "a"), ("p2", "b"), ("p3", "c")]);
    // p3 is gone; the active tab is p1.
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p1"), None);
    assert_eq!(patch.focused_pane_id_by_panel, focus_map(&[("p1", "a"), ("p2", "b")]));
    assert_eq!(patch.focused_pane_id.as_deref(), Some("a"), "the mirror reads the active tab's entry");
    assert_eq!(patch.active_panel_id.as_deref(), Some("p1"));

    // The active tab with no entry: the mirror is null.
    let sparse = focus_map(&[("p1", "a")]);
    let patch = sync_pane_focus(&sparse, &["p1", "p2"], Some("p2"), None);
    assert_eq!(patch.focused_pane_id.as_deref(), None, "p2 has no entry, so the mirror is null");

    // No active tab: the mirror is null whatever the map holds.
    let patch = sync_pane_focus(&prev, &["p1", "p2"], None, None);
    assert_eq!(patch.focused_pane_id, None);
    assert_eq!(patch.active_panel_id, None);
}

#[test]
fn a_focus_write_applies_and_a_null_pane_removes_the_entry() {
    let prev = focus_map(&[("p1", "a"), ("p2", "b")]);
    // A pane focus on p2's second pane.
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p2"), Some(("p2", Some("c"))));
    assert_eq!(patch.focused_pane_id_by_panel, focus_map(&[("p1", "a"), ("p2", "c")]));
    assert_eq!(patch.focused_pane_id.as_deref(), Some("c"));

    // A null pane means the default pane: the entry is removed rather
    // than stored, so an unsplit tab is absent from the map.
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p2"), Some(("p2", None)));
    assert_eq!(patch.focused_pane_id_by_panel, focus_map(&[("p1", "a")]));
    assert_eq!(patch.focused_pane_id, None, "the mirror follows the removal");

    // An empty pane id is the same null (JS truthiness).
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p2"), Some(("p2", Some(""))));
    assert_eq!(patch.focused_pane_id_by_panel, focus_map(&[("p1", "a")]));
}

#[test]
fn focusing_one_tab_never_steals_another_tab_entry() {
    // p1's entry survives a write to p2, and vice versa (the React
    // doc's own guarantee).
    let prev = focus_map(&[("p1", "a"), ("p2", "b")]);
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p2"), Some(("p2", Some("c"))));
    assert_eq!(patch.focused_pane_id_by_panel.get("p1").map(String::as_str), Some("a"));
}

// -- paneKeys.ts -------------------------------------------------------------

#[test]
fn the_default_pane_keeps_the_bare_panel_id() {
    assert_eq!(pane_key("panel-q-1", None), "panel-q-1");
    assert_eq!(pane_key("panel-q-1", Some(DEFAULT_PANE_ID)), "panel-q-1");
    assert_eq!(pane_key("panel-q-1", Some("split-2")), "panel-q-1::split-2");
}

#[test]
fn composite_keys_decompose_back_to_their_owners() {
    assert_eq!(panel_id_of_pane_key("panel-q-1"), "panel-q-1");
    assert_eq!(panel_id_of_pane_key("panel-q-1::split-2"), "panel-q-1");
    assert_eq!(pane_id_of_pane_key("panel-q-1"), DEFAULT_PANE_ID);
    assert_eq!(pane_id_of_pane_key("panel-q-1::split-2"), "split-2");
    // The separator finds its first occurrence only.
    assert_eq!(panel_id_of_pane_key("p::a::b"), "p");
    assert_eq!(pane_id_of_pane_key("p::a::b"), "a::b");
}

#[test]
fn panel_membership_covers_every_pane() {
    assert!(is_pane_key_of_panel("panel-q-1", "panel-q-1"));
    assert!(is_pane_key_of_panel("panel-q-1::split-2", "panel-q-1"));
    assert!(!is_pane_key_of_panel("panel-q-2::split-2", "panel-q-1"));
    // The panel id cannot contain the separator, but a foreign pane
    // id that embeds one still belongs to its own panel.
    assert!(!is_pane_key_of_panel("panel-q-1", "panel-q-1::split-2"));
}

#[test]
fn pane_keys_of_panel_keeps_the_callers_map_order() {
    let keys = vec!["panel-b", "panel-a::s1", "panel-a", "panel-a::s2", "panel-c"];
    assert_eq!(
        pane_keys_of_panel(keys, "panel-a"),
        vec![
            "panel-a::s1".to_owned(),
            "panel-a".to_owned(),
            "panel-a::s2".to_owned(),
        ],
        "every pane of the panel, in the order the map iterates"
    );
}

#[test]
fn pane_args_and_the_focused_pane_resolution() {
    // The un-split call appends NOTHING at all (not an explicit
    // undefined — the pre-split contract).
    assert!(pane_args(None).is_empty());
    assert!(pane_args(Some(DEFAULT_PANE_ID)).is_empty());
    assert_eq!(pane_args(Some("split-2")), vec!["split-2"]);

    // Nullish, not truthy: null/undefined resolve to the default pane.
    assert_eq!(resolve_focused_pane_id(None), DEFAULT_PANE_ID);
    assert_eq!(resolve_focused_pane_id(Some("split-2")), "split-2");
    assert_eq!(resolve_focused_pane_id(Some("")), "");
}
