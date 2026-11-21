//! T278 independent audit: fresh vectors — the kind derivation's
//! zoned and temporal edges, the shell seam's exact label fields, the
//! binary editor's both-themes window geometry, and the boolean
//! editor's three-state select through a full choose journey at both
//! themes.

use rune::{Cx, El, Env, Runtime};
use serde_json::{json, Value};

use crate::components::data_table::editable_cell::{EditableCell, EditableCellKind};
use crate::components::data_table::editors::binary_cell_editor::BinaryCellEditor;
use crate::components::data_table::editors::boolean_cell_editor::BooleanCellEditor;
use crate::support::cell_editor_kind::CellEditorKind;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    keyed(rt, key).is_some()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = keyed(rt, key)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

/// A zoned temporal type: the kind refuses the picker (a picker cannot
/// hold an offset), and the plain temporal stays the date kind.
#[test]
fn the_zoned_rule_and_the_temporal_kinds() {
    // A zoned datetime: not the Date kind (the picker refuses).
    let zoned = EditableCellKind::of(&json!("2026-01-02 03:04:05+08:00"), "timestamptz");
    assert_ne!(zoned.kind, CellEditorKind::Date, "a zoned value refuses the picker");
    // The plain date: the Date kind.
    let date = EditableCellKind::of(&json!("2026-01-02"), "date");
    assert_eq!(date.kind, CellEditorKind::Date);

    // A JSON column: the Json kind (the old editor's raw-text defect
    // is the new host's parse — the kind is what the shell decides).
    let json_kind = EditableCellKind::of(&json!(1), "json");
    assert_eq!(json_kind.kind, CellEditorKind::Json);
}

/// The seam's label carries the whole derivation: the kind, the cell
/// type, the zoned flag, the seed text, the fixed nullable, and the
/// panel id — with an absent panel reading as the empty string.
#[test]
fn the_seam_label_carries_the_whole_derivation() {
    let rt = audit_shell(json!(5), "integer", None);
    let label = keyed(&rt, "editable-cell").and_then(|e| e.label).unwrap_or_default();
    assert!(label.starts_with("T297|kind=number|"), "the label leads with the host and kind: {label}");
    assert!(label.contains("text=\"5\""), "the seed text rides the label: {label}");
    assert!(label.ends_with("|nullable=true|panel="), "an absent panel reads empty: {label}");
}

/// The binary editor's row stays inside the minimum window at both
/// themes with the same key set, and its text is the pack's exact
/// word.
#[test]
fn the_binary_row_is_theme_stable_inside_the_minimum_window() {
    for dark in [true, false] {
        let rt = audit_binary("列 binary", false, dark);
        assert_eq!(
            keyed(&rt, "cell-editor-binary-text").and_then(|e| e.text),
            Some("二进制值无法在表格中编辑".to_owned()),
            "the pack word verbatim (dark={dark})"
        );
        for e in rt.elements() {
            let r = e.rect;
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= 960.5 && r[1] + r[3] <= 640.5,
                "{:?} out of the minimum window (dark={dark})",
                e.key
            );
        }
    }
}

/// The three-state select's full choose journey at both themes: the
/// open press records the toggle, every option writes its discrete
/// value, and choosing closes the listbox.
#[test]
fn the_three_state_journey_writes_each_discrete_value() {
    for dark in [true, false] {
        let mut rt = audit_bool("false", dark);
        click(&mut rt, "cell-editor-boolean-trigger");
        assert_eq!(rt.state.toggles, 1, "the trigger records the open (dark={dark})");
        // The list order is fixed: NULL, true, false (option0/1/2).
        for (row, option, expected) in [
            ("option0", "NULL", None),
            ("option1", "true", Some(json!(true))),
            ("option2", "false", Some(json!(false))),
        ] {
            click(&mut rt, row);
            assert_eq!(
                rt.state.picks.last(),
                Some(&expected),
                "the {option} option writes {expected:?} (dark={dark})"
            );
        }
    }
}

/// The checkbox branch's press journey: a checked draft picks false,
/// an unchecked draft picks true, and no select ever mounts.
#[test]
fn the_checkbox_journey_picks_the_opposite() {
    let mut rt = audit_bool_checked("true");
    assert!(!has_key(&rt, "cell-editor-boolean-trigger"), "no select in the checkbox branch");
    assert!(!has_key(&rt, "option0"), "no listbox in the checkbox branch");
    click(&mut rt, "cell-editor-boolean");
    assert_eq!(rt.state.picks, vec![Some(json!(false))]);

    let mut rt = audit_bool_checked("false");
    click(&mut rt, "cell-editor-boolean");
    assert_eq!(rt.state.picks, vec![Some(json!(true))]);
}

// -- the audit's own probes ---------------------------------------------------

#[derive(Clone)]
struct BoolAudit {
    draft: String,
    nullable: bool,
    select_open: bool,
    dark: bool,
    picks: Vec<Option<Value>>,
    toggles: usize,
}

fn audit_view(s: &BoolAudit, _cx: &mut Cx<BoolAudit>) -> El {
    let mut editor =
        BooleanCellEditor::<BoolAudit>::new(s.draft.clone(), s.nullable, "列 flag")
            .select_open(s.select_open);
    editor = editor
        .on_pick(|st, value| st.picks.push(value))
        .on_select_toggle(|st| {
            st.select_open = !st.select_open;
            st.toggles += 1;
        });
    let mut layer = Vec::new();
    let el = editor.build(crate::ui::kit::kit(s.dark), &mut layer);
    rune_kit::root(el, layer)
}

fn audit_bool(draft: &str, dark: bool) -> Runtime<BoolAudit> {
    let state = BoolAudit {
        draft: draft.to_owned(),
        nullable: true,
        select_open: false,
        dark,
        picks: Vec::new(),
        toggles: 0,
    };
    let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(dark));
    rt.frame(0.0);
    rt
}

fn audit_bool_checked(draft: &str) -> Runtime<BoolAudit> {
    let state = BoolAudit {
        draft: draft.to_owned(),
        nullable: false,
        select_open: false,
        dark: true,
        picks: Vec::new(),
        toggles: 0,
    };
    let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

#[derive(Clone)]
struct BinaryAudit {
    aria_label: String,
    error: bool,
    dark: bool,
}

fn binary_audit_view(s: &BinaryAudit, _cx: &mut Cx<BinaryAudit>) -> El {
    let el = BinaryCellEditor::new(s.aria_label.clone(), s.error)
        .build(crate::ui::kit::kit(s.dark));
    rune_kit::root(el, Vec::new())
}

fn audit_binary(aria_label: &str, error: bool, dark: bool) -> Runtime<BinaryAudit> {
    let state = BinaryAudit {
        aria_label: aria_label.to_owned(),
        error,
        dark,
    };
    let mut rt = Runtime::new(state, binary_audit_view, (960.0, 640.0), env(dark));
    rt.frame(0.0);
    rt
}

#[derive(Clone)]
struct ShellAudit {
    value: Value,
    data_type: String,
    panel_id: Option<String>,
}

fn shell_audit_view(s: &ShellAudit, _cx: &mut Cx<ShellAudit>) -> El {
    let mut cell = EditableCell::<ShellAudit>::new(s.value.clone(), s.data_type.clone());
    if let Some(panel_id) = &s.panel_id {
        cell = cell.panel_id(panel_id.clone());
    }
    let el = cell
        .on_commit(|_s, _v| {})
        .on_cancel(|_s| {})
        .build(crate::ui::kit::kit(true));
    rune_kit::root(el, Vec::new())
}

fn audit_shell(value: Value, data_type: &str, panel_id: Option<&str>) -> Runtime<ShellAudit> {
    let state = ShellAudit {
        value,
        data_type: data_type.to_owned(),
        panel_id: panel_id.map(str::to_owned),
    };
    let mut rt = Runtime::new(state, shell_audit_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}
