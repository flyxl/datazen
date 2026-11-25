//! T023-S3: `RecordsetEditor` — the enable switch, the single-bound
//! editor (order column + start/end bounds + limit), and the
//! multi-primary-key tuple editor (`ColumnMappingEditor.tsx:195-440`).
//!
//! Probes the rendered element tree (keys, single `kit-root` mount,
//! geometry), drives the real kit controls through pointer/keyboard
//! journeys (enable toggle, order select, bound/limit inputs via paste
//! + Enter, inclusive checkboxes), checks the pure update logic
//! against the React source, and checks the editor mounted inside the
//! S2 shell. Every assertion observes the runtime tree or the probe's
//! recorded patches, not the component's own report.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::i18n::{has, t};
use datazen_ui::views::{
    apply_bound_update, apply_tuple_bound_update, default_order, effective_order, enable_recordset,
    format_limit, order_options, parse_limit, BoundName, ColumnMappingEditor,
    ColumnMappingEditorState, ColumnMappingTablePatch, RecordsetBoundPatch, RecordsetEditor,
    RecordsetEditorVm, RecordsetHost, RecordsetPatch, RecordsetTupleBoundPatch,
    TransferColumnMapping, TransferRecordset, TransferRecordsetBound, TransferRecordsetTupleBound,
    TransferRecordsetTupleRange, TransferTableVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn bound(value: &str) -> TransferRecordsetBound {
    TransferRecordsetBound {
        value: value.to_owned(),
        inclusive: true,
    }
}

fn tuple_bound(values: &[&str]) -> TransferRecordsetTupleBound {
    TransferRecordsetTupleBound {
        values: values.iter().map(|v| v.to_string()).collect(),
        inclusive: true,
    }
}

fn single_recordset(order_by: Option<&str>) -> TransferRecordset {
    TransferRecordset {
        order_by: order_by.map(str::to_owned),
        ..TransferRecordset::default()
    }
}

// ---------------------------------------------------------------------------
// Standalone probe
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Probe {
    columns: Vec<String>,
    types: Vec<(String, String)>,
    pks: Vec<String>,
    recordset: Option<TransferRecordset>,
    dark: bool,
    disabled: bool,
    host: RecordsetHost,
    patches: Vec<Option<TransferRecordset>>,
}

fn seeded() -> Probe {
    let mut probe = Probe {
        columns: vec!["id".to_owned(), "name".to_owned(), "email".to_owned()],
        types: vec![
            ("id".to_owned(), "INTEGER".to_owned()),
            ("name".to_owned(), "TEXT".to_owned()),
            ("email".to_owned(), "TEXT".to_owned()),
        ],
        pks: vec!["id".to_owned()],
        recordset: Some(single_recordset(Some("id"))),
        dark: true,
        disabled: false,
        host: RecordsetHost::default(),
        patches: Vec::new(),
    };
    probe.host.resync(&probe.recordset, &probe.pks);
    probe
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let vm = RecordsetEditorVm {
        columns: s.columns.clone(),
        source_column_types: s.types.clone(),
        primary_keys: s.pks.clone(),
        recordset: s.recordset.clone(),
        order_select_open: s.host.order_open,
        disabled: s.disabled,
    };
    RecordsetEditor::<Probe>::new(vm)
        .start_edit(&s.host.start_edit)
        .end_edit(&s.host.end_edit)
        .tuple_start_edits(&s.host.tuple_start_edits)
        .tuple_end_edits(&s.host.tuple_end_edits)
        .limit_edit(&s.host.limit_edit)
        .on_change(|st: &mut Probe, next: Option<TransferRecordset>| {
            let pks = st.pks.clone();
            st.recordset = next.clone();
            st.host.resync(&st.recordset, &pks);
            st.patches.push(next);
        })
        .on_start_edit(|st: &mut Probe| &mut st.host.start_edit)
        .on_end_edit(|st: &mut Probe| &mut st.host.end_edit)
        .on_tuple_start_edit(|st: &mut Probe, index: usize| &mut st.host.tuple_start_edits[index])
        .on_tuple_end_edit(|st: &mut Probe, index: usize| &mut st.host.tuple_end_edits[index])
        .on_limit_edit(|st: &mut Probe| &mut st.host.limit_edit)
        .on_toggle_order(|st: &mut Probe| {
            st.host.order_open = !st.host.order_open;
        })
        .on_close_order(|st: &mut Probe| {
            st.host.order_open = false;
        })
        .build(s.dark)
}

fn unwired_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let vm = RecordsetEditorVm {
        columns: s.columns.clone(),
        source_column_types: s.types.clone(),
        primary_keys: s.pks.clone(),
        recordset: s.recordset.clone(),
        order_select_open: false,
        disabled: s.disabled,
    };
    RecordsetEditor::<Probe>::new(vm).build(s.dark)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_unwired(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, unwired_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn kit_roots<S>(rt: &Runtime<S>) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some("kit-root"))
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn type_into<S>(rt: &mut Runtime<S>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.settle_all();
    rt.frame(0.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---------------------------------------------------------------------------
// Pure logic
// ---------------------------------------------------------------------------

#[test]
fn the_i18n_keys_the_editor_asks_for_resolve() {
    for key in [
        "transfer.mapping.recordset",
        "transfer.mapping.recordsetHint",
        "transfer.mapping.noRecordset",
        "transfer.mapping.recordsetOrder",
        "transfer.mapping.recordsetOrderRequired",
        "transfer.mapping.recordsetTextCollationHint",
        "transfer.mapping.recordsetStart",
        "transfer.mapping.recordsetEnd",
        "transfer.mapping.recordsetUnbounded",
        "transfer.mapping.recordsetInclusive",
        "transfer.mapping.recordsetLimit",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
    }
}

#[test]
fn enabling_picks_tuple_mode_for_multi_key_tables() {
    // No primary key: no automatic order.
    assert_eq!(
        enable_recordset(&[]),
        TransferRecordset {
            order_by: None,
            ..TransferRecordset::default()
        }
    );
    // One key: the single-key default order.
    assert_eq!(
        enable_recordset(&["id".to_owned()]),
        TransferRecordset {
            order_by: Some("id".to_owned()),
            ..TransferRecordset::default()
        }
    );
    // Two keys: tuple mode, columns in declared order, no bounds.
    assert_eq!(
        enable_recordset(&["id".to_owned(), "ts".to_owned()]),
        TransferRecordset {
            tuple_range: Some(TransferRecordsetTupleRange {
                columns: vec!["id".to_owned(), "ts".to_owned()],
                start: None,
                end: None,
            }),
            ..TransferRecordset::default()
        }
    );
}

#[test]
fn the_effective_order_falls_back_to_the_single_key() {
    assert_eq!(default_order(&[]), None);
    assert_eq!(default_order(&["id".to_owned()]), Some("id".to_owned()));
    assert_eq!(
        default_order(&["id".to_owned(), "ts".to_owned()]),
        None,
        "two keys have no automatic order"
    );
    let rs = single_recordset(Some("name"));
    assert_eq!(effective_order(&rs, &["id".to_owned()]), "name");
    let rs = single_recordset(None);
    assert_eq!(effective_order(&rs, &["id".to_owned()]), "id");
    assert_eq!(effective_order(&rs, &[]), "");
    assert_eq!(
        effective_order(&rs, &["id".to_owned(), "ts".to_owned()]),
        ""
    );
}

#[test]
fn order_options_label_columns_with_their_types() {
    let options = order_options(
        &["id".to_owned(), "name".to_owned()],
        &[("id".to_owned(), "INTEGER".to_owned())],
        "pick one",
    );
    assert_eq!(options[0].value, "");
    assert_eq!(options[0].label, "pick one");
    assert_eq!(options[1].value, "id");
    assert_eq!(options[1].label, "id (INTEGER)");
    assert_eq!(options[2].value, "name");
    assert_eq!(options[2].label, "name", "no known type: plain name");
}

#[test]
fn a_bound_update_sets_and_clears() {
    let rs = single_recordset(Some("id"));
    // Setting a value creates the bound, inclusive by default.
    let next = apply_bound_update(
        &rs,
        BoundName::Start,
        RecordsetBoundPatch {
            value: Some("100".to_owned()),
            inclusive: None,
        },
    );
    assert_eq!(next.start.as_ref(), Some(&bound("100")));
    assert_eq!(next.end, None, "the other bound is untouched");
    assert_eq!(next.order_by.as_deref(), Some("id"));

    // A blank value drops the bound.
    let next = apply_bound_update(
        &next,
        BoundName::Start,
        RecordsetBoundPatch {
            value: Some("   ".to_owned()),
            inclusive: None,
        },
    );
    assert_eq!(next.start, None);

    // Toggling inclusive keeps the value.
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    let next = apply_bound_update(
        &rs,
        BoundName::Start,
        RecordsetBoundPatch {
            value: None,
            inclusive: Some(false),
        },
    );
    assert_eq!(
        next.start,
        Some(TransferRecordsetBound {
            value: "100".to_owned(),
            inclusive: false,
        })
    );
}

#[test]
fn a_tuple_bound_update_switches_to_tuple_mode() {
    let pks = vec!["id".to_owned(), "ts".to_owned()];
    let mut rs = enable_recordset(&pks);
    rs.order_by = Some("id".to_owned());
    rs.start = Some(bound("9"));
    // A non-blank value clears the scalar order and bounds.
    let next = apply_tuple_bound_update(
        &rs,
        &pks,
        BoundName::Start,
        RecordsetTupleBoundPatch {
            values: Some(vec!["7".to_owned(), String::new()]),
            inclusive: None,
        },
    );
    assert_eq!(next.order_by, None);
    assert_eq!(next.start, None);
    assert_eq!(next.end, None);
    assert_eq!(
        next.tuple_range.as_ref().map(|r| r.columns.clone()),
        Some(pks.clone())
    );
    assert_eq!(
        next.tuple_range.as_ref().and_then(|r| r.start.clone()),
        Some(tuple_bound(&["7", ""]))
    );

    // All-blank values drop the edited bound and keep the other one,
    // without touching the scalar fields.
    let mut rs = next.clone();
    rs.tuple_range.as_mut().unwrap().end = Some(tuple_bound(&["8", "z"]));
    let next = apply_tuple_bound_update(
        &rs,
        &pks,
        BoundName::Start,
        RecordsetTupleBoundPatch {
            values: Some(vec![String::new(), "  ".to_owned()]),
            inclusive: None,
        },
    );
    assert_eq!(
        next.tuple_range.as_ref().and_then(|r| r.start.clone()),
        None
    );
    assert_eq!(
        next.tuple_range.as_ref().and_then(|r| r.end.clone()),
        Some(tuple_bound(&["8", "z"])),
        "the other bound survives the blank-out"
    );

    // Toggling inclusive keeps the values.
    let next = apply_tuple_bound_update(
        &rs,
        &pks,
        BoundName::End,
        RecordsetTupleBoundPatch {
            values: None,
            inclusive: Some(false),
        },
    );
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.end.as_ref())
            .map(|b| b.inclusive),
        Some(false)
    );
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.end.as_ref())
            .map(|b| b.values.clone()),
        Some(vec!["8".to_owned(), "z".to_owned()])
    );
}

#[test]
fn the_recordset_patch_spreads_like_react_update() {
    let mut rs = single_recordset(Some("id"));
    RecordsetPatch {
        order_by: Some(None),
        limit: Some(Some(250.0)),
        ..RecordsetPatch::default()
    }
    .apply_to(&mut rs);
    assert_eq!(rs.order_by, None, "Some(None) clears the order");
    assert_eq!(rs.limit, Some(250.0));
    assert_eq!(rs.start, None, "untouched fields stay");
}

#[test]
fn limit_parsing_mirrors_number_conversion() {
    assert_eq!(parse_limit(""), None);
    assert_eq!(parse_limit("   "), None);
    assert_eq!(parse_limit("100"), Some(100.0));
    assert_eq!(parse_limit(" 42 "), Some(42.0));
    assert!(parse_limit("abc").map(|v| v.is_nan()).unwrap_or(false));
    assert_eq!(format_limit(None), "");
    assert_eq!(format_limit(Some(100.0)), "100");
    assert_eq!(format_limit(Some(2.5)), "2.5");
    assert_eq!(format_limit(Some(f64::NAN)), "NaN");
}

// ---------------------------------------------------------------------------
// Standalone rendering
// ---------------------------------------------------------------------------

#[test]
fn the_disabled_editor_shows_the_empty_state() {
    let mut probe = seeded();
    probe.recordset = None;
    probe.host.resync(&probe.recordset, &probe.pks);
    let rt = runtime(probe, DESIGN);
    for key in [
        "data-transfer-recordset",
        "data-transfer-recordset-enable",
        "data-transfer-recordset-empty",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    assert!(!has_key(&rt, "data-transfer-recordset-editor"));
    assert_eq!(
        element(&rt, "data-transfer-recordset-empty")
            .text
            .as_deref(),
        Some(t("transfer.mapping.noRecordset"))
    );
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn the_single_editor_renders_the_react_keys() {
    let rt = runtime(seeded(), DESIGN);
    for key in [
        "data-transfer-recordset",
        "data-transfer-recordset-enable",
        "data-transfer-recordset-editor",
        "data-transfer-recordset-order-trigger",
        "data-transfer-recordset-start",
        "data-transfer-recordset-end",
        "data-transfer-recordset-limit",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    // The order is set, so there is no order error and no inclusive
    // checkbox yet — both bounds are still unbounded.
    assert!(!has_key(&rt, "data-transfer-recordset-order-error"));
    assert!(!has_key(&rt, "data-transfer-recordset-start-inclusive"));
    assert!(!has_key(&rt, "data-transfer-recordset-end-inclusive"));
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn the_order_error_shows_only_without_an_order() {
    let mut probe = seeded();
    probe.recordset = Some(single_recordset(None));
    probe.pks = Vec::new();
    probe.host.resync(&probe.recordset, &probe.pks);
    let rt = runtime(probe, DESIGN);
    assert!(has_key(&rt, "data-transfer-recordset-order-error"));
    assert_eq!(
        element(&rt, "data-transfer-recordset-order-error")
            .text
            .as_deref(),
        Some(t("transfer.mapping.recordsetOrderRequired"))
    );
}

#[test]
fn the_tuple_editor_renders_one_input_per_key() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    probe.recordset = Some(enable_recordset(&probe.pks));
    probe.host.resync(&probe.recordset, &probe.pks);
    let rt = runtime(probe, DESIGN);
    for key in [
        "data-transfer-recordset-tuple-editor",
        "data-transfer-recordset-tuple-collation-hint",
        "data-transfer-recordset-tuple-start-0",
        "data-transfer-recordset-tuple-start-1",
        "data-transfer-recordset-tuple-end-0",
        "data-transfer-recordset-tuple-end-1",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    // Tuple mode has no order select and no order error: the kit
    // select only exposes its trigger key, so assert on that.
    assert!(!has_key(&rt, "data-transfer-recordset-order-trigger"));
    assert!(!has_key(&rt, "data-transfer-recordset-order-error"));
    assert_eq!(
        element(&rt, "data-transfer-recordset-tuple-order")
            .text
            .as_deref(),
        Some(format!("{}: id → created_at", t("transfer.mapping.recordsetOrder")).as_str())
    );
    assert_eq!(kit_roots(&rt), 1);
}

// ---------------------------------------------------------------------------
// Standalone journeys
// ---------------------------------------------------------------------------

#[test]
fn the_enable_toggle_commits_the_react_default_and_clears() {
    // Single key: enabling commits { orderBy: <the key> }.
    let mut probe = seeded();
    probe.recordset = None;
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].as_ref().map(|r| r.order_by.clone()),
        Some(Some("id".to_owned()))
    );
    assert!(has_key(&rt, "data-transfer-recordset-editor"));

    // Disabling clears the whole recordset.
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 2);
    assert_eq!(rt.state.patches[1], None);
    assert!(has_key(&rt, "data-transfer-recordset-empty"));
}

#[test]
fn the_enable_toggle_starts_tuple_mode_for_multi_key_tables() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    probe.recordset = None;
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("enabling commits Some");
    assert_eq!(
        next.tuple_range.as_ref().map(|r| r.columns.clone()),
        Some(vec!["id".to_owned(), "created_at".to_owned()])
    );
    assert_eq!(next.order_by, None);
    assert!(has_key(&rt, "data-transfer-recordset-tuple-editor"));
}

#[test]
fn choosing_an_order_commits_and_closes_the_listbox() {
    let mut rt = runtime(seeded(), DESIGN);
    assert!(!rt.state.host.order_open);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    assert!(rt.state.host.order_open, "the trigger opens the listbox");
    assert!(has_key(&rt, "option2"), "the listbox opened");
    // Options: 0 = required, 1 = id, 2 = name, 3 = email.
    click(&mut rt, "option2");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("choosing commits Some");
    assert_eq!(next.order_by.as_deref(), Some("name"));
    assert!(
        !rt.state.host.order_open,
        "choosing closes the listbox (React closes its own)"
    );
    assert!(!has_key(&rt, "option2"), "the listbox is gone");
}

#[test]
fn choosing_the_empty_order_option_clears_the_order() {
    // No primary key: there is no single-key default for the cleared
    // order to fall back to, so the order error appears (React's
    // `orderBy = recordset?.orderBy ?? defaultOrder ?? ''`).
    let mut probe = seeded();
    probe.pks = Vec::new();
    probe.recordset = Some(single_recordset(Some("name")));
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    click(&mut rt, "option0");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("choosing commits Some");
    assert_eq!(next.order_by, None);
    assert!(has_key(&rt, "data-transfer-recordset-order-error"));
}

#[test]
fn pressing_escape_closes_the_order_listbox() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    assert!(rt.state.host.order_open);
    press(&mut rt, Key::Escape);
    assert!(!rt.state.host.order_open, "Escape closes via on_close");
    assert!(rt.state.patches.is_empty());
}

#[test]
fn typing_a_bound_and_pressing_enter_commits_it() {
    let mut rt = runtime(seeded(), DESIGN);
    type_into(&mut rt, "data-transfer-recordset-start", "100");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("submit commits Some");
    assert_eq!(next.start.as_ref(), Some(&bound("100")));
    assert_eq!(
        rt.state.host.start_edit.display().0.as_str(),
        "100",
        "the committed model re-seeds the buffer"
    );
    // The bound now exists, so its inclusive checkbox appears.
    assert!(has_key(&rt, "data-transfer-recordset-start-inclusive"));
}

#[test]
fn clearing_a_bound_drops_it() {
    let mut probe = seeded();
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    assert!(has_key(&rt, "data-transfer-recordset-start-inclusive"));
    click(&mut rt, "data-transfer-recordset-start");
    press(&mut rt, Key::End);
    for _ in 0..3 {
        press(&mut rt, Key::Backspace);
    }
    assert_eq!(rt.state.host.start_edit.display().0.as_str(), "");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("submit commits Some");
    assert_eq!(next.start, None, "a blank value drops the bound");
    assert!(!has_key(&rt, "data-transfer-recordset-start-inclusive"));
}

#[test]
fn the_inclusive_checkbox_flips_only_its_bound() {
    let mut probe = seeded();
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    rs.end = Some(bound("200"));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-start-inclusive");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("toggle commits Some");
    assert_eq!(
        next.start.as_ref().map(|b| b.inclusive),
        Some(false),
        "only the start bound flips"
    );
    assert_eq!(next.start.as_ref().map(|b| b.value.as_str()), Some("100"));
    assert_eq!(
        next.end.as_ref().map(|b| b.inclusive),
        Some(true),
        "the end bound is untouched"
    );
}

#[test]
fn typing_a_limit_commits_it_and_blank_clears_it() {
    let mut rt = runtime(seeded(), DESIGN);
    type_into(&mut rt, "data-transfer-recordset-limit", "500");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("submit commits Some");
    assert_eq!(next.limit, Some(500.0));
    assert_eq!(
        rt.state.host.limit_edit.display().0.as_str(),
        "500",
        "the committed model re-seeds the buffer"
    );

    // A non-numeric limit stores NaN, exactly as React's Number().
    type_into(&mut rt, "data-transfer-recordset-limit", "abc");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 2);
    let next = rt.state.patches[1].clone().expect("submit commits Some");
    assert!(next.limit.map(|v| v.is_nan()).unwrap_or(false));

    // Blank clears it again.
    click(&mut rt, "data-transfer-recordset-limit");
    press(&mut rt, Key::End);
    for _ in 0..6 {
        press(&mut rt, Key::Backspace);
    }
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 3);
    let next = rt.state.patches[2].clone().expect("submit commits Some");
    assert_eq!(next.limit, None);
}

#[test]
fn typing_a_tuple_cell_commits_only_that_cell() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    probe.recordset = Some(enable_recordset(&probe.pks));
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    type_into(
        &mut rt,
        "data-transfer-recordset-tuple-start-1",
        "2026-01-01",
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("submit commits Some");
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.start.clone())
            .map(|b| b.values),
        Some(vec![String::new(), "2026-01-01".to_owned()]),
        "only the edited cell is filled; the other stays blank"
    );
    // The bound now exists, so its inclusive checkbox appears.
    assert!(has_key(
        &rt,
        "data-transfer-recordset-tuple-start-inclusive"
    ));
}

#[test]
fn blanking_every_tuple_cell_drops_the_bound() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    let mut rs = enable_recordset(&probe.pks);
    rs.tuple_range.as_mut().unwrap().start = Some(tuple_bound(&["7", "x"]));
    rs.tuple_range.as_mut().unwrap().end = Some(tuple_bound(&["8", "z"]));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    // Blank the first start cell; the second still holds "x".
    click(&mut rt, "data-transfer-recordset-tuple-start-0");
    press(&mut rt, Key::End);
    press(&mut rt, Key::Backspace);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("submit commits Some");
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.start.clone())
            .map(|b| b.values),
        Some(vec![String::new(), "x".to_owned()]),
        "one blank cell does not drop the bound"
    );
    // Blank the second start cell too: the bound drops, the end stays.
    click(&mut rt, "data-transfer-recordset-tuple-start-1");
    press(&mut rt, Key::End);
    press(&mut rt, Key::Backspace);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 2);
    let next = rt.state.patches[1].clone().expect("submit commits Some");
    assert_eq!(
        next.tuple_range.as_ref().and_then(|r| r.start.clone()),
        None,
        "all-blank values drop the edited bound"
    );
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.end.clone())
            .map(|b| b.values),
        Some(vec!["8".to_owned(), "z".to_owned()]),
        "the other bound survives"
    );
}

#[test]
fn the_tuple_inclusive_checkbox_flips_only_its_bound() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    let mut rs = enable_recordset(&probe.pks);
    rs.tuple_range.as_mut().unwrap().start = Some(tuple_bound(&["7", "x"]));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-tuple-start-inclusive");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("toggle commits Some");
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.start.as_ref())
            .map(|b| b.inclusive),
        Some(false)
    );
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.start.as_ref())
            .map(|b| b.values.clone()),
        Some(vec!["7".to_owned(), "x".to_owned()]),
        "the values survive the flip"
    );
}

#[test]
fn a_disabled_editor_commits_nothing() {
    let mut probe = seeded();
    probe.disabled = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-enable");
    click(&mut rt, "data-transfer-recordset-start");
    rt.paste("should-not-commit");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    click(&mut rt, "data-transfer-recordset-limit");
    rt.paste("500");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.patches.is_empty(),
        "disabled commits nothing: {:?}",
        rt.state.patches
    );
    assert_eq!(
        rt.state.recordset.as_ref().and_then(|r| r.start.clone()),
        None
    );
    assert_eq!(rt.state.recordset.as_ref().and_then(|r| r.limit), None);
}

#[test]
fn the_unwired_editor_renders_read_only() {
    let rt = runtime_unwired(seeded(), DESIGN);
    // The model values are there as plain text under the inputs' keys.
    assert!(has_key(&rt, "data-transfer-recordset"));
    assert!(has_key(&rt, "data-transfer-recordset-order-trigger"));
    assert!(has_key(&rt, "data-transfer-recordset-start"));
    assert!(has_key(&rt, "data-transfer-recordset-limit"));
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn the_editor_stays_inside_both_window_sizes_in_both_themes() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut probe = seeded();
            probe.dark = dark;
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "recordset editor");
            // Header above the editor body, no overlap.
            let header = element(&rt, "data-transfer-recordset-header");
            let body = element(&rt, "data-transfer-recordset-editor");
            assert!(header.rect[1] + header.rect[3] <= body.rect[1] + 0.5);

            // The taller tuple editor fits too.
            let mut probe = seeded();
            probe.dark = dark;
            probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
            probe.recordset = Some(enable_recordset(&probe.pks));
            probe.host.resync(&probe.recordset, &probe.pks);
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "tuple editor");
        }
    }
}

// ---------------------------------------------------------------------------
// Mounted inside the S2 shell
// ---------------------------------------------------------------------------

fn mapping(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
    }
}

fn shell_table() -> TransferTableVm {
    TransferTableVm {
        source_table: "users".to_owned(),
        target_table: "users".to_owned(),
        status: datazen_ui::views::TransferMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: vec![mapping("id", "id", false), mapping("name", "name", false)],
        source_columns: Some(vec!["id".to_owned(), "name".to_owned(), "email".to_owned()]),
        target_columns: Some(vec!["id".to_owned(), "name".to_owned()]),
        source_column_types: vec![
            ("id".to_owned(), "INTEGER".to_owned()),
            ("name".to_owned(), "TEXT".to_owned()),
            ("email".to_owned(), "TEXT".to_owned()),
        ],
        source_primary_keys: Some(vec!["id".to_owned()]),
        recordset: Some(single_recordset(Some("id"))),
    }
}

#[derive(Clone)]
struct ShellProbe {
    editor: ColumnMappingEditorState,
    dark: bool,
    patches: Vec<ColumnMappingTablePatch>,
}

fn shell_seeded() -> ShellProbe {
    ShellProbe {
        editor: ColumnMappingEditorState::new(shell_table(), false, false),
        dark: true,
        patches: Vec::new(),
    }
}

fn shell_view(s: &ShellProbe, _cx: &mut Cx<ShellProbe>) -> El {
    let mut layer = Vec::new();
    let el = ColumnMappingEditor::<ShellProbe>::new(&s.editor)
        .editor_mut(|st: &mut ShellProbe| &mut st.editor)
        .on_change(|st: &mut ShellProbe, patch| {
            st.editor.apply_table_patch(&patch);
            st.patches.push(patch);
        })
        .build(s.dark, &mut layer);
    rune_kit::root(el, layer)
}

fn shell_runtime(probe: ShellProbe, size: (f32, f32)) -> Runtime<ShellProbe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, shell_view, size, env(dark));
    rt.frame(0.0);
    rt
}

#[test]
fn the_shell_mounts_the_recordset_editor_once() {
    let rt = shell_runtime(shell_seeded(), DESIGN);
    for key in [
        "data-transfer-column-editor",
        "data-transfer-recordset",
        "data-transfer-recordset-enable",
        "data-transfer-recordset-editor",
        "data-transfer-recordset-order-trigger",
        "data-transfer-recordset-start",
        "data-transfer-recordset-limit",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    assert_eq!(kit_roots(&rt), 1, "one root for the whole editor");
}

#[test]
fn the_shell_enable_toggle_commits_a_table_patch() {
    let mut probe = shell_seeded();
    probe.editor.table.recordset = None;
    probe.editor.resync_recordset();
    let mut rt = shell_runtime(probe, DESIGN);
    assert!(has_key(&rt, "data-transfer-recordset-empty"));
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].recordset,
        Some(Some(single_recordset(Some("id")))),
        "the toggle commits {{ recordset }} as a table patch"
    );
    assert_eq!(
        rt.state.editor.table.recordset,
        Some(single_recordset(Some("id")))
    );
    assert!(has_key(&rt, "data-transfer-recordset-editor"));
}

#[test]
fn a_shell_bound_commit_flows_up_as_a_table_patch() {
    let mut rt = shell_runtime(shell_seeded(), DESIGN);
    type_into(&mut rt, "data-transfer-recordset-start", "100");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .recordset
        .clone()
        .expect("bound commit carries a recordset patch")
        .expect("the recordset is Some");
    assert_eq!(next.start.as_ref(), Some(&bound("100")));
    assert_eq!(
        rt.state
            .editor
            .table
            .recordset
            .as_ref()
            .and_then(|r| r.start.clone()),
        Some(bound("100")),
        "the patch landed in the table"
    );
    assert_eq!(
        rt.state
            .editor
            .recordset_host
            .start_edit
            .display()
            .0
            .as_str(),
        "100",
        "the shell re-seeded the recordset buffer"
    );
}

#[test]
fn the_shell_with_an_enabled_recordset_stays_inside_both_windows() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut probe = shell_seeded();
            probe.dark = dark;
            let rt = shell_runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "shell with recordset");
        }
    }
}
