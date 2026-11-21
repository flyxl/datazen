//! T274 independent audit: fresh vectors for the three stores — the
//! binder's replacement and double-dispose semantics, the focus
//! writer's cross-tab and re-activation edges, and the key space's
//! round-trip and ordering invariants.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::metadata_identity_binding::MetadataIdentityBinding;
use crate::pane_focus::sync_pane_focus;
use crate::pane_keys::{
    pane_id_of_pane_key, pane_key, pane_keys_of_panel, panel_id_of_pane_key,
    resolve_focused_pane_id, DEFAULT_PANE_ID,
};

/// The binder accepts a replacement, and a token dispose runs only
/// once (the second dispose is a no-op, exactly as re-firing the
/// React closure is).
#[test]
fn a_replacement_binder_takes_over_and_double_dispose_is_inert() {
    let calls = Rc::new(RefCell::new(Vec::<String>::new()));
    let first = {
        let calls = Rc::clone(&calls);
        Rc::new(move |session: &str, _: &str, _: Option<&str>| {
            calls.borrow_mut().push(format!("first:{session}"));
        })
    };
    let second = {
        let calls = Rc::clone(&calls);
        Rc::new(move |session: &str, _: &str, _: Option<&str>| {
            calls.borrow_mut().push(format!("second:{session}"));
        })
    };

    let mut binding = MetadataIdentityBinding::new();
    let first_token = binding.register(first);
    binding.unregister(&first_token);
    // Double dispose: the binder is already gone, nothing restores
    // over the second.
    binding.unregister(&first_token);
    assert!(!binding.has_binder());

    let second_token = binding.register(second);
    binding.bind_session_metadata_identity("s1", "c", Some("sch"));
    assert_eq!(*calls.borrow(), vec!["second:s1".to_owned()]);
    binding.unregister(&second_token);
    assert!(!binding.has_binder());
    binding.bind_session_metadata_identity("s2", "c", None);
    assert_eq!(calls.borrow().len(), 1);
}

/// The focus map's write path keeps an untouched tab's entry while a
/// dead panel drops, and the active panel's id itself follows JS
/// truthiness (an empty string is no active tab).
#[test]
fn focus_edges_and_active_id_truthiness() {
    let prev: HashMap<String, String> = [("p1", "a"), ("p2", "b")]
        .iter()
        .map(|(p, pane)| ((*p).to_string(), (*pane).to_string()))
        .collect();

    // A write whose panel is not in the live set: the entry is
    // written anyway (React has no liveness gate on the write half —
    // only the copy loop drops dead ones).
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p2"), Some(("p3", Some("z"))));
    assert_eq!(patch.focused_pane_id_by_panel.get("p3").map(String::as_str), Some("z"));
    assert_eq!(patch.focused_pane_id.as_deref(), Some("b"), "the mirror still reads the live active tab");

    // An empty active id is falsy: no active tab, no mirror.
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some(""), None);
    assert_eq!(patch.active_panel_id, None);
    assert_eq!(patch.focused_pane_id, None);

    // The focus write to the ACTIVE panel is what flips the mirror in
    // the same patch.
    let patch = sync_pane_focus(&prev, &["p1", "p2"], Some("p2"), Some(("p2", Some("c"))));
    assert_eq!(patch.focused_pane_id.as_deref(), Some("c"));
    assert_eq!(patch.focused_pane_id_by_panel.get("p1").map(String::as_str), Some("a"));
}

/// The key space round-trips: every key the builder builds decomposes
/// back to its owner and pane, and the default pane is exactly the
/// bare id.
#[test]
fn every_built_key_round_trips() {
    for (panel, pane) in [("panel-q-1", None), ("panel-q-1", Some("s1")), ("p", Some("a::b"))] {
        let key = pane_key(panel, pane);
        assert_eq!(panel_id_of_pane_key(&key), panel);
        let expected_pane = pane.unwrap_or(DEFAULT_PANE_ID);
        assert_eq!(pane_id_of_pane_key(&key), expected_pane);
        assert_eq!(
            resolve_focused_pane_id(pane),
            expected_pane,
            "the focused-pane resolution agrees with the decomposition"
        );
    }
}

/// `pane_keys_of_panel` returns the panel's OWN keys only, and the
/// caller's order is the output order (a JS Map's iteration).
#[test]
fn pane_keys_of_panel_filters_strictly() {
    let keys = vec!["panel-a", "panel-ab", "panel-a::s1", "panel-b", "panel-a::s2"];
    let out = pane_keys_of_panel(keys, "panel-a");
    assert_eq!(
        out,
        vec![
            "panel-a".to_owned(),
            "panel-a::s1".to_owned(),
            "panel-a::s2".to_owned(),
        ],
        "panel-ab is a different panel, not a prefix match"
    );
}
