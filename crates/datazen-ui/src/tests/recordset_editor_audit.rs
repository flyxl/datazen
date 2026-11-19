//! T023-S3 independent audit: `RecordsetEditor` fresh vectors the coder
//! tests do not cover (`ColumnMappingEditor.tsx:195-440`).
//!
//! Pure-logic boundaries (enable/order/limit parsing and formatting,
//! bound and tuple-bound updates, patch spreading, host re-seeding),
//! standalone journeys (enable/disable round trip, order clearing with
//! 1 vs 0 vs 2+ primary keys, disabled controls, 3-key tuples,
//! uncommitted-draft loss, whitespace handling, the exiting-node
//! lookup question), and shell wiring (disable flowing up as
//! `Some(None)`, tuple mounting in the shell, patch re-seeding).
//! Every assertion observes the runtime tree or the probe's recorded
//! patches, not the component's own report.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::i18n::t;
use crate::views::{
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
// Standalone probe (own copy of the coder's scaffolding)
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

/// Filtered lookup: exiting nodes count as gone (the coder's helper).
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

/// Unfiltered lookup, exactly as the S2 helpers do it: an exiting node
/// still answers. Used only to demonstrate the S2-helper misread risk.
fn raw_has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key))
}

fn raw_is_exiting<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.exiting && e.key.as_deref() == Some(key))
}

fn kit_roots<S>(rt: &Runtime<S>) -> usize {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting && e.key.as_deref() == Some("kit-root"))
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

/// Clear a field's buffer the way a user does: focus, End, Backspace.
fn clear_field<S>(rt: &mut Runtime<S>, key: &str, len: usize) {
    click(rt, key);
    press(rt, Key::End);
    for _ in 0..len {
        press(rt, Key::Backspace);
    }
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.clone().unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---------------------------------------------------------------------------
// Pure logic: enable / order boundaries
// ---------------------------------------------------------------------------

#[test]
fn enabling_with_three_keys_starts_tuple_mode_with_all_three() {
    let pks = vec!["id".to_owned(), "a".to_owned(), "b".to_owned()];
    assert_eq!(
        enable_recordset(&pks),
        TransferRecordset {
            tuple_range: Some(TransferRecordsetTupleRange {
                columns: pks,
                start: None,
                end: None,
            }),
            ..TransferRecordset::default()
        }
    );
    assert_eq!(default_order(&["id".to_owned(), "a".to_owned(), "b".to_owned()]), None);
}

#[test]
fn order_options_with_no_columns_is_just_the_required_option() {
    let options = order_options(&[], &[], "pick one");
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].value, "");
    assert_eq!(options[0].label, "pick one");
    // The required option is first even when columns exist.
    let options = order_options(
        &["id".to_owned()],
        &[("id".to_owned(), "INTEGER".to_owned())],
        "pick one",
    );
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].value, "");
    assert_eq!(options[1].label, "id (INTEGER)");
}

#[test]
fn clearing_the_order_falls_back_to_the_single_key_only() {
    // React: `orderBy = recordset?.orderBy ?? defaultOrder ?? ''`.
    // Choosing the empty option writes orderBy: undefined, so the
    // single-key default takes over again.
    let mut rs = single_recordset(Some("name"));
    RecordsetPatch {
        order_by: Some(None),
        ..RecordsetPatch::default()
    }
    .apply_to(&mut rs);
    assert_eq!(rs.order_by, None);
    assert_eq!(effective_order(&rs, &["id".to_owned()]), "id");
    assert_eq!(effective_order(&rs, &[]), "");
    assert_eq!(
        effective_order(&rs, &["id".to_owned(), "ts".to_owned()]),
        ""
    );
    // A stored empty string is NOT nullish in React either, so it does
    // not fall back — the order error is observable even with one key
    // for a directly-constructed model (unreachable through the UI,
    // where clearing writes None, not "").
    let rs = single_recordset(Some(""));
    assert_eq!(effective_order(&rs, &["id".to_owned()]), "");
}

// ---------------------------------------------------------------------------
// Pure logic: limit parsing / formatting boundaries
// ---------------------------------------------------------------------------

#[test]
fn limit_parsing_boundaries() {
    assert_eq!(parse_limit("0"), Some(0.0));
    assert_eq!(parse_limit("-5"), Some(-5.0));
    assert_eq!(parse_limit("1e3"), Some(1000.0));
    assert_eq!(parse_limit("1.5"), Some(1.5));
    assert_eq!(parse_limit("5."), Some(5.0));
    assert_eq!(parse_limit(".5"), Some(0.5));
    assert_eq!(parse_limit("NaN").map(|v| v.is_nan()), Some(true));
    assert_eq!(parse_limit("Infinity").map(|v| v.is_infinite()), Some(true));
    assert_eq!(parse_limit("  42  "), Some(42.0));
    assert_eq!(parse_limit(""), None);
    assert_eq!(parse_limit("   "), None);
    // Rust's f64 parser rejects JS Number's hex/binary/octal literals,
    // so these land on NaN where JS Number("0x10") is 16. The limit
    // input is type=number, which never produces such text; recorded
    // as a known, non-blocking divergence from "mirrors exactly".
    assert!(parse_limit("0x10").map(|v| v.is_nan()).unwrap_or(false));
    assert!(parse_limit("0b101").map(|v| v.is_nan()).unwrap_or(false));
    assert!(parse_limit("abc").map(|v| v.is_nan()).unwrap_or(false));
}

#[test]
fn limit_formatting_boundaries() {
    assert_eq!(format_limit(None), "");
    assert_eq!(format_limit(Some(0.0)), "0");
    assert_eq!(format_limit(Some(-3.0)), "-3");
    assert_eq!(format_limit(Some(100.0)), "100");
    assert_eq!(format_limit(Some(2.5)), "2.5");
    assert_eq!(format_limit(Some(-0.5)), "-0.5");
    assert_eq!(format_limit(Some(f64::NAN)), "NaN");
    // Known divergences from JS String(limit), recorded not fixed:
    // Rust prints infinity as "inf" (JS: "Infinity"), and integers
    // beyond i64 saturate through `as i64` (JS prints the full
    // digits of 1e20). Limits are user-typed small numbers, so both
    // are non-blocking.
    assert_eq!(format_limit(Some(f64::INFINITY)), "inf");
    assert_eq!(format_limit(Some(1e20)), "9223372036854775807");
    assert_eq!(format_limit(Some(1e19)), "9223372036854775807");
}

// ---------------------------------------------------------------------------
// Pure logic: bound / tuple-bound updates
// ---------------------------------------------------------------------------

#[test]
fn an_end_bound_update_sets_and_clears_at_the_pure_level() {
    // The End arm of apply_bound_update: no coder or audit test had
    // called it before this one (it was an uncovered line).
    let rs = single_recordset(Some("id"));
    let next = apply_bound_update(
        &rs,
        BoundName::End,
        RecordsetBoundPatch {
            value: Some("200".to_owned()),
            inclusive: None,
        },
    );
    assert_eq!(next.end.as_ref(), Some(&bound("200")));
    assert_eq!(next.start, None, "the other bound is untouched");
    let next = apply_bound_update(
        &next,
        BoundName::End,
        RecordsetBoundPatch {
            value: Some("".to_owned()),
            inclusive: None,
        },
    );
    assert_eq!(next.end, None);
}

#[test]
fn clearing_a_bound_wins_over_an_inclusive_flip() {
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    // React merges the patch then drops the bound when the merged
    // value is blank — the inclusive flip goes down with it.
    let next = apply_bound_update(
        &rs,
        BoundName::Start,
        RecordsetBoundPatch {
            value: Some("   ".to_owned()),
            inclusive: Some(false),
        },
    );
    assert_eq!(next.start, None);
}

#[test]
fn an_inclusive_only_patch_on_an_absent_bound_creates_nothing() {
    let rs = single_recordset(Some("id"));
    // React merges over the { value: '', inclusive: true } default,
    // then drops the blank bound.
    let next = apply_bound_update(
        &rs,
        BoundName::Start,
        RecordsetBoundPatch {
            value: None,
            inclusive: Some(false),
        },
    );
    assert_eq!(next.start, None);
    assert_eq!(next.order_by.as_deref(), Some("id"));
}

#[test]
fn a_tuple_update_with_no_prior_range_creates_it_on_all_blank() {
    let pks = vec!["id".to_owned(), "ts".to_owned()];
    let rs = single_recordset(Some("id"));
    // React: all-blank still commits { tupleRange: { columns } }.
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
        next.tuple_range,
        Some(TransferRecordsetTupleRange {
            columns: pks,
            start: None,
            end: None,
        })
    );
    // ...and the scalar fields survive an all-blank tuple patch.
    assert_eq!(next.order_by.as_deref(), Some("id"));
}

#[test]
fn an_inclusive_only_tuple_patch_on_an_absent_bound_creates_nothing() {
    let pks = vec!["id".to_owned(), "ts".to_owned()];
    let rs = enable_recordset(&pks);
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
        next.tuple_range.as_ref().and_then(|r| r.end.clone()),
        None
    );
    assert_eq!(
        next.tuple_range.as_ref().map(|r| r.columns.clone()),
        Some(pks)
    );
}

#[test]
fn a_short_tuple_values_patch_is_kept_verbatim_like_react() {
    let pks = vec!["id".to_owned(), "ts".to_owned(), "n".to_owned()];
    let rs = enable_recordset(&pks);
    // React spreads the patch values array as-is; there is no padding.
    let next = apply_tuple_bound_update(
        &rs,
        &pks,
        BoundName::Start,
        RecordsetTupleBoundPatch {
            values: Some(vec!["7".to_owned()]),
            inclusive: None,
        },
    );
    assert_eq!(
        next.tuple_range
            .as_ref()
            .and_then(|r| r.start.clone())
            .map(|b| b.values),
        Some(vec!["7".to_owned()])
    );
}

#[test]
fn the_recordset_patch_clears_tuple_range_and_limit() {
    let pks = vec!["id".to_owned(), "ts".to_owned()];
    let mut rs = enable_recordset(&pks);
    rs.limit = Some(250.0);
    RecordsetPatch {
        tuple_range: Some(None),
        limit: Some(None),
        ..RecordsetPatch::default()
    }
    .apply_to(&mut rs);
    assert_eq!(rs.tuple_range, None);
    assert_eq!(rs.limit, None);
    // An empty patch leaves everything alone.
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("9"));
    RecordsetPatch::default().apply_to(&mut rs);
    assert_eq!(rs.start.as_ref(), Some(&bound("9")));
    assert_eq!(rs.order_by.as_deref(), Some("id"));
}

// ---------------------------------------------------------------------------
// Pure logic: host re-seeding
// ---------------------------------------------------------------------------

#[test]
fn the_host_resizes_tuple_buffers_when_the_keys_change() {
    let mut host = RecordsetHost::default();
    let pks2 = vec!["id".to_owned(), "ts".to_owned()];
    let mut rs = enable_recordset(&pks2);
    rs.tuple_range.as_mut().unwrap().start = Some(tuple_bound(&["7", "x"]));
    host.resync(&Some(rs), &pks2);
    assert_eq!(host.tuple_start_edits.len(), 2);
    assert_eq!(host.tuple_start_edits[0].display().0.as_str(), "7");
    assert_eq!(host.tuple_start_edits[1].display().0.as_str(), "x");

    // One key now: the stale second buffer is gone.
    let pks1 = vec!["id".to_owned()];
    host.resync(&Some(single_recordset(Some("id"))), &pks1);
    assert_eq!(host.tuple_start_edits.len(), 1);
    assert_eq!(host.tuple_end_edits.len(), 1);

    // Three keys, bound values shorter than the keys: pad with "".
    let pks3 = vec!["id".to_owned(), "ts".to_owned(), "n".to_owned()];
    let mut rs = enable_recordset(&pks3);
    rs.tuple_range.as_mut().unwrap().end = Some(tuple_bound(&["8"]));
    host.resync(&Some(rs), &pks3);
    assert_eq!(host.tuple_end_edits.len(), 3);
    assert_eq!(host.tuple_end_edits[0].display().0.as_str(), "8");
    assert_eq!(host.tuple_end_edits[1].display().0.as_str(), "");
    assert_eq!(host.tuple_end_edits[2].display().0.as_str(), "");

    // No recordset: every buffer clears.
    host.resync(&None, &pks3);
    assert_eq!(host.start_edit.display().0.as_str(), "");
    assert_eq!(host.limit_edit.display().0.as_str(), "");
    assert!(host
        .tuple_start_edits
        .iter()
        .all(|e| e.display().0.as_str().is_empty()));
}

// ---------------------------------------------------------------------------
// Standalone journeys
// ---------------------------------------------------------------------------

#[test]
fn the_enable_toggle_round_trips_off_and_on_again() {
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
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 2);
    assert_eq!(rt.state.patches[1], None);
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 3);
    assert_eq!(
        rt.state.patches[2].as_ref().map(|r| r.order_by.clone()),
        Some(Some("id".to_owned()))
    );
    assert!(has_key(&rt, "data-transfer-recordset-editor"));
}

#[test]
fn clearing_the_order_with_one_key_falls_back_to_that_key() {
    let mut probe = seeded();
    probe.recordset = Some(single_recordset(Some("name")));
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    click(&mut rt, "option0");
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("choosing commits Some");
    assert_eq!(next.order_by, None, "the empty option clears orderBy");
    assert_eq!(
        effective_order(&next, &["id".to_owned()]),
        "id",
        "the single-key default takes over"
    );
    assert!(
        !has_key(&rt, "data-transfer-recordset-order-error"),
        "no order error: the effective order is the single key"
    );
}

#[test]
fn clearing_the_order_with_two_keys_shows_the_order_error() {
    // Two keys, but a single-mode recordset (no tupleRange): the
    // single editor renders, and clearing the order has no default
    // to fall back to.
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "ts".to_owned()];
    probe.recordset = Some(single_recordset(Some("name")));
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    assert!(!has_key(&rt, "data-transfer-recordset-tuple-editor"));
    click(&mut rt, "data-transfer-recordset-order-trigger");
    click(&mut rt, "option0");
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].as_ref().and_then(|r| r.order_by.clone()),
        None
    );
    assert!(has_key(&rt, "data-transfer-recordset-order-error"));
}

#[test]
fn a_stored_empty_order_shows_the_error_even_with_one_key() {
    // Direct model state (unreachable through the UI, where clearing
    // writes None): "" is not nullish in React either.
    let mut probe = seeded();
    probe.recordset = Some(single_recordset(Some("")));
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
fn choosing_the_last_order_option_commits_that_column() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    // Options: 0 = required, 1 = id, 2 = name, 3 = email.
    click(&mut rt, "option3");
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0]
            .as_ref()
            .and_then(|r| r.order_by.clone()),
        Some("email".to_owned())
    );
    assert!(!rt.state.host.order_open);
}

#[test]
fn a_disabled_editor_opens_no_listbox_and_flips_no_checkbox() {
    let mut probe = seeded();
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    probe.recordset = Some(rs);
    probe.disabled = true;
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    assert!(
        !rt.state.host.order_open,
        "the disabled order select stays shut"
    );
    assert!(!has_key(&rt, "option1"), "no listbox opens while disabled");
    click(&mut rt, "data-transfer-recordset-start-inclusive");
    assert!(
        rt.state.patches.is_empty(),
        "the disabled inclusive checkbox commits nothing"
    );
    assert_eq!(
        rt.state
            .recordset
            .as_ref()
            .and_then(|r| r.start.as_ref())
            .map(|b| b.inclusive),
        Some(true)
    );
}

#[test]
fn typing_an_end_bound_and_pressing_enter_commits_it() {
    let mut rt = runtime(seeded(), DESIGN);
    type_into(&mut rt, "data-transfer-recordset-end", "200");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0].clone().expect("submit commits Some");
    assert_eq!(next.end.as_ref(), Some(&bound("200")));
    assert_eq!(next.start, None);
    assert!(has_key(&rt, "data-transfer-recordset-end-inclusive"));
}

#[test]
fn a_short_stored_tuple_bound_pushes_instead_of_indexing() {
    // Stored tuple values shorter than the keys can only arrive from
    // a directly-constructed model (the UI always keeps full-length
    // values). The submit path appends when the edited index is past
    // the stored length, so the value lands at the end, not at its
    // key position — React's `values[index] = ...` would instead grow
    // the array with a hole at the missing position. Recorded as a
    // known edge divergence, non-blocking.
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "a".to_owned(), "b".to_owned()];
    let mut rs = enable_recordset(&probe.pks);
    rs.tuple_range.as_mut().unwrap().start = Some(tuple_bound(&["7"]));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    type_into(&mut rt, "data-transfer-recordset-tuple-start-2", "z");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0]
            .as_ref()
            .and_then(|r| r.tuple_range.clone())
            .and_then(|r| r.start)
            .map(|b| b.values),
        Some(vec!["7".to_owned(), "z".to_owned()]),
    );
}

#[test]
fn a_bound_value_is_stored_verbatim_including_surrounding_spaces() {
    // React trims only for the blank check; the stored value keeps
    // its spaces.
    let mut rt = runtime(seeded(), DESIGN);
    type_into(&mut rt, "data-transfer-recordset-start", " 100 ");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0]
            .as_ref()
            .and_then(|r| r.start.clone())
            .map(|b| b.value),
        Some(" 100 ".to_owned())
    );
}

#[test]
fn a_zero_or_negative_limit_commits_like_react_number() {
    // React's min={1} is only an input attribute; Number("0") and
    // Number("-5") still commit. The port does not enforce a minimum
    // either.
    let mut rt = runtime(seeded(), DESIGN);
    type_into(&mut rt, "data-transfer-recordset-limit", "0");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].as_ref().and_then(|r| r.limit),
        Some(0.0)
    );
    clear_field(&mut rt, "data-transfer-recordset-limit", 1);
    rt.paste("-5");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 2);
    assert_eq!(
        rt.state.patches[1].as_ref().and_then(|r| r.limit),
        Some(-5.0)
    );
}

#[test]
fn the_tuple_editor_renders_three_keys_and_commits_the_end_cell() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "a".to_owned(), "b".to_owned()];
    probe.recordset = Some(enable_recordset(&probe.pks));
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    for key in [
        "data-transfer-recordset-tuple-start-0",
        "data-transfer-recordset-tuple-start-1",
        "data-transfer-recordset-tuple-start-2",
        "data-transfer-recordset-tuple-end-0",
        "data-transfer-recordset-tuple-end-1",
        "data-transfer-recordset-tuple-end-2",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    assert_eq!(
        element(&rt, "data-transfer-recordset-tuple-order")
            .text
            .as_deref(),
        Some(format!("{}: id → a → b", t("transfer.mapping.recordsetOrder")).as_str())
    );
    type_into(&mut rt, "data-transfer-recordset-tuple-end-2", "z");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0]
            .as_ref()
            .and_then(|r| r.tuple_range.clone())
            .and_then(|r| r.end)
            .map(|b| b.values),
        Some(vec![String::new(), String::new(), "z".to_owned()])
    );
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn an_uncommitted_tuple_draft_is_lost_when_another_cell_submits() {
    // Consequence of the documented Enter-submit deviation: React
    // commits every keystroke, so a draft in cell 0 is already in the
    // model when cell 1 commits; here the submit patch reads the
    // stored values, and cell 0's draft was never submitted.
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    probe.recordset = Some(enable_recordset(&probe.pks));
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-recordset-tuple-start-0");
    rt.paste("first");
    rt.settle_all();
    rt.frame(0.0);
    type_into(&mut rt, "data-transfer-recordset-tuple-start-1", "second");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0]
            .as_ref()
            .and_then(|r| r.tuple_range.clone())
            .and_then(|r| r.start)
            .map(|b| b.values),
        Some(vec![String::new(), "second".to_owned()]),
        "cell 0's uncommitted draft does not survive cell 1's submit"
    );
}

#[test]
fn a_cleared_bound_leaves_an_exiting_checkbox_the_filtered_lookup_hides() {
    let mut probe = seeded();
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let mut rt = runtime(probe, DESIGN);
    assert!(has_key(&rt, "data-transfer-recordset-start-inclusive"));
    clear_field(&mut rt, "data-transfer-recordset-start", 3);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].as_ref().and_then(|r| r.start.clone()),
        None
    );
    // The coder's filtered helper must not see the checkbox any more.
    assert!(!has_key(&rt, "data-transfer-recordset-start-inclusive"));
    // Report what the raw tree still holds at this point: if the
    // checkbox is still listed as exiting, an unfiltered lookup (the
    // S2 helper shape) would wrongly report it present.
    eprintln!(
        "AUDIT exiting checkbox still listed: raw_present={} exiting={}",
        raw_has_key(&rt, "data-transfer-recordset-start-inclusive"),
        raw_is_exiting(&rt, "data-transfer-recordset-start-inclusive"),
    );
    if raw_is_exiting(&rt, "data-transfer-recordset-start-inclusive") {
        assert!(
            raw_has_key(&rt, "data-transfer-recordset-start-inclusive"),
            "an unfiltered lookup sees the exiting checkbox the filtered one hides"
        );
    }
}

#[test]
fn a_closed_listbox_may_still_be_listed_exiting_for_raw_lookups() {
    // Same mechanism as the inclusive checkbox, on the order listbox:
    // after choosing, the filtered helper sees the listbox as gone,
    // but the raw tree may still list option2 as exiting — the exact
    // shape the S2 helpers read, so an S2-style `!has_key("option2")`
    // assertion here would be timing-sensitive.
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-recordset-order-trigger");
    assert!(has_key(&rt, "option2"), "the listbox opened");
    click(&mut rt, "option2");
    assert_eq!(rt.state.patches.len(), 1);
    assert!(!has_key(&rt, "option2"), "filtered: the listbox is gone");
    eprintln!(
        "AUDIT listbox option2 after choose: raw_present={} exiting={}",
        raw_has_key(&rt, "option2"),
        raw_is_exiting(&rt, "option2"),
    );
}

#[test]
fn the_unwired_tuple_editor_renders_values_as_plain_text() {
    let mut probe = seeded();
    probe.pks = vec!["id".to_owned(), "created_at".to_owned()];
    let mut rs = enable_recordset(&probe.pks);
    rs.tuple_range.as_mut().unwrap().start = Some(tuple_bound(&["7", "x"]));
    probe.recordset = Some(rs);
    probe.host.resync(&probe.recordset, &probe.pks);
    let rt = runtime_unwired(probe, DESIGN);
    assert!(has_key(&rt, "data-transfer-recordset-tuple-start-0"));
    assert_eq!(
        element(&rt, "data-transfer-recordset-tuple-start-0")
            .text
            .as_deref(),
        Some("7")
    );
    assert_eq!(
        element(&rt, "data-transfer-recordset-tuple-start-1")
            .text
            .as_deref(),
        Some("x")
    );
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn the_three_key_tuple_editor_fits_the_small_window_in_both_themes() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut probe = seeded();
            probe.dark = dark;
            probe.pks = vec!["id".to_owned(), "a".to_owned(), "b".to_owned()];
            probe.recordset = Some(enable_recordset(&probe.pks));
            probe.host.resync(&probe.recordset, &probe.pks);
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "three-key tuple editor");
            assert_eq!(kit_roots(&rt), 1);
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
        status: crate::views::TransferMappingStatus::Matched,
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
fn the_shell_disable_toggle_clears_the_recordset_as_none() {
    let mut rt = shell_runtime(shell_seeded(), DESIGN);
    assert!(has_key(&rt, "data-transfer-recordset-editor"));
    click(&mut rt, "data-transfer-recordset-enable");
    assert_eq!(rt.state.patches.len(), 1);
    assert_eq!(
        rt.state.patches[0].recordset,
        Some(None),
        "disabling commits {{ recordset: undefined }} as a table patch"
    );
    assert_eq!(rt.state.editor.table.recordset, None);
    assert!(has_key(&rt, "data-transfer-recordset-empty"));
    assert_eq!(
        rt.state.editor.recordset_host.start_edit.display().0.as_str(),
        "",
        "the shell re-seeded the cleared buffers"
    );
    assert_eq!(
        rt.state.editor.recordset_host.limit_edit.display().0.as_str(),
        ""
    );
}

#[test]
fn the_shell_mounts_the_tuple_editor_for_a_multi_key_table() {
    let mut probe = shell_seeded();
    probe.editor.table.source_primary_keys = Some(vec!["id".to_owned(), "created_at".to_owned()]);
    probe.editor.table.recordset = Some(enable_recordset(&[
        "id".to_owned(),
        "created_at".to_owned(),
    ]));
    probe.editor.resync_recordset();
    let mut rt = shell_runtime(probe, DESIGN);
    for key in [
        "data-transfer-recordset-tuple-editor",
        "data-transfer-recordset-tuple-start-0",
        "data-transfer-recordset-tuple-start-1",
        "data-transfer-recordset-tuple-end-0",
        "data-transfer-recordset-tuple-end-1",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out in the shell");
    }
    assert!(!has_key(&rt, "data-transfer-recordset-order-trigger"));
    type_into(&mut rt, "data-transfer-recordset-tuple-start-0", "7");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.patches.len(), 1);
    let next = rt.state.patches[0]
        .recordset
        .clone()
        .expect("tuple commit carries a recordset patch")
        .expect("the recordset is Some");
    assert_eq!(
        next.tuple_range
            .and_then(|r| r.start)
            .map(|b| b.values),
        Some(vec!["7".to_owned(), String::new()])
    );
}

#[test]
fn a_table_patch_with_a_cleared_recordset_reseeds_the_shell_host() {
    let mut probe = shell_seeded();
    let mut rs = single_recordset(Some("id"));
    rs.start = Some(bound("100"));
    rs.limit = Some(250.0);
    probe.editor.table.recordset = Some(rs);
    probe.editor.resync_recordset();
    assert_eq!(
        probe.editor.recordset_host.start_edit.display().0.as_str(),
        "100"
    );
    assert_eq!(
        probe.editor.recordset_host.limit_edit.display().0.as_str(),
        "250"
    );
    probe.editor.apply_table_patch(&ColumnMappingTablePatch {
        recordset: Some(None),
        ..ColumnMappingTablePatch::default()
    });
    assert_eq!(probe.editor.table.recordset, None);
    assert_eq!(
        probe.editor.recordset_host.start_edit.display().0.as_str(),
        "",
        "clearing re-seeds the start buffer to empty"
    );
    assert_eq!(
        probe.editor.recordset_host.limit_edit.display().0.as_str(),
        ""
    );
    // A patch without a recordset field leaves the model and the
    // buffers alone.
    let mut probe = shell_seeded();
    probe.editor.apply_table_patch(&ColumnMappingTablePatch {
        target_table: Some("other".to_owned()),
        ..ColumnMappingTablePatch::default()
    });
    assert_eq!(
        probe.editor.table.recordset,
        Some(single_recordset(Some("id")))
    );
    assert_eq!(
        probe.editor.recordset_host.limit_edit.display().0.as_str(),
        "",
        "no limit was stored, so the buffer stays empty"
    );
}
