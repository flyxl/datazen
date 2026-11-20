//! T360: `windows/data-sync/RecordsetEditor.tsx` port — the pure update
//! chain (toggle, bound value/inclusive, order, tuple components, the
//! limit validation), the mounted editor's branches (tuple vs scalar,
//! the toggle, the limit row), and window-width geometry.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::data_sync::recordset_editor::{
    default_order, set_bound_inclusive, set_bound_value, set_limit, set_order_by,
    set_tuple_component, toggle_recordset, tuple_mode, SyncRecordsetEditor,
    SyncRecordsetEditorVm,
};
use crate::views::{
    BoundName, RecordsetHost, TransferRecordset, TransferRecordsetBound,
    TransferRecordsetTupleBound, TransferRecordsetTupleRange,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn recordset_with(
    order_by: Option<&str>,
    start: Option<(&str, bool)>,
    end: Option<(&str, bool)>,
) -> Option<TransferRecordset> {
    Some(TransferRecordset {
        order_by: order_by.map(|o| o.to_owned()),
        start: start.map(|(v, i)| TransferRecordsetBound {
            value: v.to_owned(),
            inclusive: i,
        }),
        end: end.map(|(v, i)| TransferRecordsetBound {
            value: v.to_owned(),
            inclusive: i,
        }),
        tuple_range: None,
        limit: None,
    })
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[derive(Default)]
struct Scene {
    primary_keys: Vec<String>,
    recordset: Option<TransferRecordset>,
    host: RecordsetHost,
    order_open: bool,
    applied: Vec<Option<TransferRecordset>>,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        SyncRecordsetEditor::<Scene>::new(SyncRecordsetEditorVm {
            primary_keys: scene.primary_keys.clone(),
            recordset: scene.recordset.clone(),
            order_select_open: scene.order_open,
            limit_error: false,
        })
        .on_change(|s, next| {
            s.recordset = next.clone();
            s.host.resync(&next, &s.primary_keys.clone());
            s.applied.push(next);
        })
        .on_toggle_order(|s| s.order_open = true)
        .on_close_order(|s| s.order_open = false)
        .bind_start(|s| &mut s.host.start_edit)
        .bind_end(|s| &mut s.host.end_edit)
        .bind_limit(|s| &mut s.host.limit_edit)
        .bind_tuple(|s, name, index| match name {
            BoundName::Start => &mut s.host.tuple_start_edits[index],
            BoundName::End => &mut s.host.tuple_end_edits[index],
        })
        .build(scene.dark),
    )
}

fn mount(mut scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    scene.host.resync(&scene.recordset, &scene.primary_keys);
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

/// A kit button's accessible label (the button element carries it).
fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---- the pure chain ----

#[test]
fn default_order_is_the_single_key_only() {
    assert_eq!(default_order(&keys(&["id"])), "id");
    assert_eq!(default_order(&keys(&["a", "b"])), "");
    assert_eq!(default_order(&keys(&[])), "");
}

#[test]
fn tuple_mode_follows_the_react_disjunction() {
    let tuple = Some(TransferRecordset {
        tuple_range: Some(TransferRecordsetTupleRange {
            columns: keys(&["a", "b"]),
            start: None,
            end: None,
        }),
        ..Default::default()
    });
    assert!(tuple_mode(&keys(&["a", "b"]), tuple.as_ref()));
    // Composite key + no scalar fields at all → tuple mode.
    assert!(tuple_mode(&keys(&["a", "b"]), Some(&TransferRecordset::default())));
    // Single key never defaults to tuple.
    assert!(!tuple_mode(&keys(&["id"]), Some(&TransferRecordset::default())));
    // Any scalar field set on a composite key → scalar.
    let with_order = recordset_with(Some("a"), None, None);
    assert!(!tuple_mode(&keys(&["a", "b"]), with_order.as_ref()));
    let with_start = recordset_with(None, Some(("", true)), None);
    assert!(!tuple_mode(&keys(&["a", "b"]), with_start.as_ref()));
    // No recordset at all.
    assert!(!tuple_mode(&keys(&["a", "b"]), None));
}

#[test]
fn the_toggle_creates_the_right_empty_recordset() {
    // Composite key: a tuple range over the keys.
    let created = toggle_recordset(&keys(&["a", "b"]), None).unwrap();
    let range = created.tuple_range.expect("a tuple range");
    assert_eq!(range.columns, keys(&["a", "b"]));
    assert!(range.start.is_none() && range.end.is_none());
    // Single key: order by it.
    let created = toggle_recordset(&keys(&["id"]), None).unwrap();
    assert_eq!(created.order_by.as_deref(), Some("id"));
    // A recordset goes away entirely.
    assert!(toggle_recordset(&keys(&["id"]), recordset_with(Some("id"), None, None).as_ref()).is_none());
}

#[test]
fn a_bound_value_edit_deletes_on_empty_and_keeps_inclusive() {
    let base = recordset_with(Some("id"), Some(("10", false)), None);
    // A non-empty value keeps the bound's inclusive flag.
    let next = set_bound_value(base.as_ref(), BoundName::Start, "20");
    assert_eq!(next.start.as_ref().map(|b| (b.value.as_str(), b.inclusive)), Some(("20", false)));
    // An empty value deletes the bound.
    let next = set_bound_value(Some(&next), BoundName::Start, "");
    assert!(next.start.is_none());
    // A bound created from nothing defaults to inclusive.
    let next = set_bound_value(None, BoundName::End, "5");
    assert_eq!(next.end.as_ref().map(|b| (b.value.as_str(), b.inclusive)), Some(("5", true)));
}

#[test]
fn an_inclusive_edit_keeps_the_value() {
    let base = recordset_with(Some("id"), Some(("10", false)), None);
    let next = set_bound_inclusive(base.as_ref(), BoundName::Start, true);
    assert_eq!(next.start.as_ref().map(|b| (b.value.as_str(), b.inclusive)), Some(("10", true)));
}

#[test]
fn an_order_choice_clears_on_empty() {
    let base = recordset_with(Some("id"), None, None);
    let next = set_order_by(base.as_ref(), "");
    assert!(next.order_by.is_none());
    let next = set_order_by(Some(&next), "name");
    assert_eq!(next.order_by.as_deref(), Some("name"));
}

#[test]
fn a_tuple_component_pads_and_deletes_when_all_empty() {
    let base = TransferRecordset {
        tuple_range: Some(TransferRecordsetTupleRange {
            columns: keys(&["a", "b", "c"]),
            start: Some(TransferRecordsetTupleBound {
                values: vec!["1".to_owned()],
                inclusive: false,
            }),
            end: None,
        }),
        ..Default::default()
    };
    // Editing component 2 pads components 1 and 2 with ''.
    let next = set_tuple_component(&keys(&["a", "b", "c"]), Some(&base), BoundName::Start, 2, "9");
    let bound = next.tuple_range.as_ref().unwrap().start.as_ref().unwrap();
    assert_eq!(bound.values, vec!["1", "", "9"]);
    assert!(!bound.inclusive, "the existing inclusive flag survives");
    // The scalar trio is cleared.
    assert!(next.order_by.is_none() && next.start.is_none() && next.end.is_none());
    // All-empty components delete the bound, the range stays.
    let mut next2 = next.clone();
    next2.order_by = Some("x".to_owned());
    let next2 = set_tuple_component(&keys(&["a", "b", "c"]), Some(&next2), BoundName::Start, 0, "");
    let next2 = set_tuple_component(&keys(&["a", "b", "c"]), Some(&next2), BoundName::Start, 1, "");
    let next2 = set_tuple_component(&keys(&["a", "b", "c"]), Some(&next2), BoundName::Start, 2, "");
    let range = next2.tuple_range.as_ref().expect("the range stays");
    assert!(range.start.is_none(), "an all-empty bound is deleted");
}

#[test]
fn the_limit_accepts_whole_safe_positive_integers_only() {
    let base = recordset_with(Some("id"), None, None);
    let (next, error) = set_limit(base.as_ref(), "42");
    assert!(!error);
    assert_eq!(next.limit, Some(42.0));
    // An empty text clears the limit and the error.
    let (next, error) = set_limit(Some(&next), "");
    assert!(!error);
    assert!(next.limit.is_none());
    // Everything else keeps the model and raises the error.
    for bad in ["0", "-3", "1.5", "abc", "  ", "1e3", "9999999999999999999", "+1"] {
        let (next, error) = set_limit(base.as_ref(), bad);
        assert!(error, "{bad:?} must be rejected");
        assert_eq!(next.limit, None, "{bad:?} never writes the model");
    }
}

// ---- the mounted editor ----

#[test]
fn no_recordset_mounts_only_the_header() {
    let rt = mount(
        Scene {
            primary_keys: keys(&["id"]),
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-recordset"));
    assert!(has_key(&rt, "data-sync-recordset-toggle"));
    assert!(!has_key(&rt, "data-sync-recordset-editor"));
}

#[test]
fn a_single_key_recordset_mounts_the_scalar_editor() {
    let mut rt = mount(
        Scene {
            primary_keys: keys(&["id"]),
            recordset: recordset_with(Some("id"), None, None),
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-recordset-editor"));
    assert!(has_key(&rt, "data-sync-recordset-order-trigger"));
    assert!(has_key(&rt, "data-sync-recordset-start"));
    assert!(has_key(&rt, "data-sync-recordset-end"));
    assert!(has_key(&rt, "data-sync-recordset-limit"));
    assert!(!has_key(&rt, "data-sync-recordset-tuple-editor"));
    assert_eq!(
        label_of(&rt, "data-sync-recordset-toggle"),
        t("sync.recordsetDisable")
    );
    // The toggle drops the recordset entirely.
    click(&mut rt, "data-sync-recordset-toggle");
    assert_eq!(rt.state.applied.len(), 1);
    assert!(rt.state.applied[0].is_none());
}

#[test]
fn a_composite_key_mounts_the_tuple_editor() {
    let rt = mount(
        Scene {
            primary_keys: keys(&["a", "b"]),
            recordset: Some(TransferRecordset {
                tuple_range: Some(TransferRecordsetTupleRange {
                    columns: keys(&["a", "b"]),
                    start: Some(TransferRecordsetTupleBound {
                        values: vec!["1".to_owned(), "2".to_owned()],
                        inclusive: true,
                    }),
                    end: None,
                }),
                ..Default::default()
            }),
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-recordset-tuple-editor"));
    assert_eq!(text_of(&rt, "data-sync-recordset-tuple-order"), "a → b");
    assert!(has_key(&rt, "data-sync-recordset-start-0"));
    assert!(has_key(&rt, "data-sync-recordset-start-1"));
    // The end box has no bound: its inputs exist, its inclusive does not.
    assert!(has_key(&rt, "data-sync-recordset-end-0"));
    assert!(has_key(&rt, "data-sync-recordset-start-inclusive"));
    assert!(!has_key(&rt, "data-sync-recordset-end-inclusive"));
    assert!(!has_key(&rt, "data-sync-recordset-order-trigger"));
}

#[test]
fn zero_primary_keys_disable_the_toggle() {
    let mut rt = mount(
        Scene {
            primary_keys: Vec::new(),
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    click(&mut rt, "data-sync-recordset-toggle");
    assert!(rt.state.applied.is_empty(), "no keys: the toggle never fires");
}

#[test]
fn the_toggle_on_a_composite_key_creates_a_tuple_range() {
    let mut rt = mount(
        Scene {
            primary_keys: keys(&["a", "b"]),
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    click(&mut rt, "data-sync-recordset-toggle");
    let created = rt.state.applied[0].as_ref().expect("a recordset came back");
    assert!(created.tuple_range.is_some());
    assert_eq!(created.tuple_range.as_ref().unwrap().columns, keys(&["a", "b"]));
}

// ---- geometry ----

#[test]
fn the_editor_fits_both_window_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(
            Scene {
                primary_keys: keys(&["a", "b"]),
                recordset: Some(TransferRecordset {
                    tuple_range: Some(TransferRecordsetTupleRange {
                        columns: keys(&["a", "b"]),
                        start: Some(TransferRecordsetTupleBound {
                            values: vec!["1".to_owned(), "2".to_owned()],
                            inclusive: true,
                        }),
                        end: None,
                    }),
                    ..Default::default()
                }),
                ..Scene::default()
            },
            (w, h),
        );
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync-recordset") {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {outside:?}");
    }
}

#[test]
fn the_light_theme_mounts_the_same_tree() {
    let rt = mount(
        Scene {
            primary_keys: keys(&["id"]),
            recordset: recordset_with(Some("id"), None, None),
            dark: false,
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-recordset-editor"));
    assert!(has_key(&rt, "data-sync-recordset-limit"));
}
