//! T360 independent audit (tester): fresh vectors — the limit's boundary
//! integers, the toggle's destruction of the mounted editor, the
//! all-empty tuple deletion keeping the sibling bound, the limit error
//! paragraph, and a mixed scalar/tuple range starting scalar (the first
//! tuple edit clears the scalar trio on the mounted model).

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::data_sync::recordset_editor::{
    set_limit, set_tuple_component, SyncRecordsetEditor, SyncRecordsetEditorVm,
};
use datazen_ui::views::{
    BoundName, RecordsetHost, TransferRecordset, TransferRecordsetBound,
    TransferRecordsetTupleBound, TransferRecordsetTupleRange,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
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
    limit_error: bool,
    applied: Vec<Option<TransferRecordset>>,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        SyncRecordsetEditor::<Scene>::new(SyncRecordsetEditorVm {
            primary_keys: scene.primary_keys.clone(),
            recordset: scene.recordset.clone(),
            order_select_open: scene.order_open,
            limit_error: scene.limit_error,
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_limit_boundaries_match_the_js_gates() {
    let base = TransferRecordset {
        order_by: Some("id".to_owned()),
        ..Default::default()
    };
    // The smallest and the largest safe integers pass.
    for good in ["1", "9007199254740991"] {
        let (next, error) = set_limit(Some(&base), good);
        assert!(!error, "{good} passes the JS gates");
        assert!(next.limit.is_some(), "{good} writes the model");
    }
    // One past the largest safe integer is rejected.
    let (next, error) = set_limit(Some(&base), "9007199254740992");
    assert!(error);
    assert_eq!(next.limit, None, "the model is never written by a reject");
    // Leading zeros stay valid digits.
    let (next, error) = set_limit(Some(&base), "007");
    assert!(!error);
    assert_eq!(next.limit, Some(7.0));
}

#[test]
fn the_toggle_unmounts_the_whole_editor_body() {
    let mut rt = mount(
        Scene {
            primary_keys: keys(&["id"]),
            recordset: Some(TransferRecordset {
                order_by: Some("id".to_owned()),
                ..Default::default()
            }),
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-recordset-editor"));
    click(&mut rt, "data-sync-recordset-toggle");
    assert!(rt.state.applied[0].is_none());
    // The host model dropped: the next frame renders no editor body.
    rt.frame(1.0 / 60.0);
    assert!(
        !has_key(&rt, "data-sync-recordset-editor"),
        "the recordset gone, the body unmounts"
    );
}

#[test]
fn an_all_empty_tuple_deletion_keeps_the_sibling_bound() {
    let base = TransferRecordset {
        tuple_range: Some(TransferRecordsetTupleRange {
            columns: keys(&["a", "b"]),
            start: Some(TransferRecordsetTupleBound {
                values: vec!["1".to_owned(), "2".to_owned()],
                inclusive: true,
            }),
            end: Some(TransferRecordsetTupleBound {
                values: vec!["9".to_owned(), "9".to_owned()],
                inclusive: false,
            }),
        }),
        ..Default::default()
    };
    let keys2 = keys(&["a", "b"]);
    let next = set_tuple_component(&keys2, Some(&base), BoundName::Start, 0, "");
    let next = set_tuple_component(&keys2, Some(&next), BoundName::Start, 1, "");
    let range = next.tuple_range.as_ref().expect("the range stays");
    assert!(range.start.is_none(), "the all-empty start bound is deleted");
    assert_eq!(
        range.end.as_ref().map(|b| (b.values.clone(), b.inclusive)),
        Some((vec!["9".to_owned(), "9".to_owned()], false)),
        "the sibling end bound is untouched"
    );
}

#[test]
fn a_scalar_first_recordset_switches_to_tuple_mode_on_the_first_tuple_edit() {
    // Composite keys with a scalar recordset: typing one tuple component
    // clears the scalar trio (the React `updateTupleBound` chain).
    let base = TransferRecordset {
        order_by: Some("a".to_owned()),
        start: Some(TransferRecordsetBound {
            value: "10".to_owned(),
            inclusive: true,
        }),
        end: None,
        ..Default::default()
    };
    let next = set_tuple_component(&keys(&["a", "b"]), Some(&base), BoundName::Start, 0, "1");
    assert!(next.order_by.is_none(), "orderBy cleared");
    assert!(next.start.is_none(), "the scalar start cleared");
    let range = next.tuple_range.as_ref().expect("a tuple range took over");
    assert_eq!(range.start.as_ref().unwrap().values, vec!["1", ""]);
}

#[test]
fn the_limit_error_paragraph_mounts_from_the_host_flag() {
    let rt = mount(
        Scene {
            primary_keys: keys(&["id"]),
            recordset: Some(TransferRecordset {
                order_by: Some("id".to_owned()),
                ..Default::default()
            }),
            limit_error: true,
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-recordset-limit-error"));
    // Without the flag, no paragraph.
    let rt = mount(
        Scene {
            primary_keys: keys(&["id"]),
            recordset: Some(TransferRecordset {
                order_by: Some("id".to_owned()),
                ..Default::default()
            }),
            limit_error: false,
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(!has_key(&rt, "data-sync-recordset-limit-error"));
    // The text itself exists in the locale.
    assert!(!t("sync.recordsetLimitInvalid").is_empty());
}
