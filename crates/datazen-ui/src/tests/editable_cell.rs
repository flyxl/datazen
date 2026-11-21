//! T278: the EditableCell shell, the binary editor, and the boolean
//! editor — coder vectors.

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

fn k() -> &'static rune_kit::Kit {
    crate::ui::kit::kit(true)
}

#[derive(Clone, Default)]
struct BinaryProbe {
    aria_label: String,
    error: bool,
}

fn binary_view(s: &BinaryProbe, _cx: &mut Cx<BinaryProbe>) -> El {
    let el = BinaryCellEditor::new(s.aria_label.clone(), s.error)
        .build(crate::ui::kit::kit(true));
    rune_kit::root(el, Vec::new())
}

fn binary_runtime(aria_label: &str, error: bool) -> Runtime<BinaryProbe> {
    let state = BinaryProbe {
        aria_label: aria_label.to_owned(),
        error,
    };
    let mut rt = Runtime::new(state, binary_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

#[derive(Clone, Default)]
struct ShellProbe {
    value: Value,
    data_type: String,
    panel_id: Option<String>,
}

fn shell_view(s: &ShellProbe, _cx: &mut Cx<ShellProbe>) -> El {
    let mut cell = EditableCell::<ShellProbe>::new(s.value.clone(), s.data_type.clone());
    if let Some(panel_id) = &s.panel_id {
        cell = cell.panel_id(panel_id.clone());
    }
    let el = cell
        .on_commit(|_s, _v| {})
        .on_cancel(|_s| {})
        .build(crate::ui::kit::kit(true));
    rune_kit::root(el, Vec::new())
}

fn shell_runtime(value: Value, data_type: &str, panel_id: Option<&str>) -> Runtime<ShellProbe> {
    let state = ShellProbe {
        value,
        data_type: data_type.to_owned(),
        panel_id: panel_id.map(str::to_owned),
    };
    let mut rt = Runtime::new(state, shell_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

// -- the kind derivation (React 37-45) --------------------------------------

#[test]
fn the_kind_derivation_pins_the_shell_inputs() {
    // A boolean column with a boolean value.
    let derived = EditableCellKind::of(&json!(true), "boolean");
    assert_eq!(derived.kind, CellEditorKind::Boolean);
    assert_eq!(derived.text, "true");

    // An integer column: the number kind.
    let derived = EditableCellKind::of(&json!(5), "integer");
    assert_eq!(derived.kind, CellEditorKind::Number);

    // A text column with a line break: the long-text kind.
    let derived = EditableCellKind::of(&json!("a\nb"), "text");
    assert_eq!(derived.kind, CellEditorKind::LongText);
    assert_eq!(derived.text, "a\nb");

    // A blob column: the binary kind (the editor that does not edit).
    let derived = EditableCellKind::of(&json!([1, 2, 3]), "blob");
    assert_eq!(derived.kind, CellEditorKind::Binary);

    // The shell's hasCandidates is always false: an enum column
    // without a candidate list falls to the enum kind by type alone.
    let derived = EditableCellKind::of(&json!("x"), "enum");
    assert_eq!(derived.kind, CellEditorKind::Enum);

    // A NULL value on a text column: the empty seed string.
    let derived = EditableCellKind::of(&Value::Null, "text");
    assert_eq!(derived.text, "");
}

#[test]
fn the_code_point_count_feeds_the_kind() {
    // A long text tips the long-text kind: the threshold is 200 code
    // points (INLINE_TEXT_MAX_CODE_POINTS).
    let long = "数".repeat(201);
    let derived = EditableCellKind::of(&json!(long), "text");
    assert_eq!(derived.kind, CellEditorKind::LongText);
    // The short one stays the plain text kind.
    let derived = EditableCellKind::of(&json!("数据库"), "text");
    assert_eq!(derived.kind, CellEditorKind::Text);
}

// -- the shell's seam ---------------------------------------------------------

#[test]
fn the_shell_mounts_the_host_seam_with_the_derivation() {
    let rt = shell_runtime(json!("v"), "text", Some("panel-q-1"));
    let seam = keyed(&rt, "editable-cell").expect("the seam mounts");
    let label = seam.label.clone().unwrap_or_default();
    assert!(label.contains("T297"), "the seam names its host task: {label}");
    assert!(label.contains("kind=text"), "the kind rides the label: {label}");
    assert!(label.contains("nullable=true"), "the shell's nullable is always true: {label}");
    assert!(label.contains("panel=panel-q-1"), "the panel id rides the label: {label}");
    assert_eq!(keyed(&rt, "editable-cell-seam-text").and_then(|e| e.text), Some("v".to_owned()));
}

// -- the binary editor --------------------------------------------------------

#[test]
fn the_binary_editor_renders_the_reason_with_its_status_role() {
    let rt = binary_runtime("列 binary", false);
    let row = keyed(&rt, "cell-editor-binary").expect("the status row mounts");
    assert_eq!(row.role, "status");
    assert_eq!(row.label, Some("列 binary".to_owned()));
    assert_eq!(
        keyed(&rt, "cell-editor-binary-text").and_then(|e| e.text),
        Some(crate::i18n::t("cellEditor.binaryNotEditable").to_owned())
    );
    assert!(!has_key(&rt, "cell-editor-binary-sr"), "no echo without an error");

    // The sr-only echo mounts only with the error flag.
    let rt = binary_runtime("列 binary", true);
    assert!(has_key(&rt, "cell-editor-binary-sr"), "the error's sr-only echo mounts");
}

// -- the boolean editor -------------------------------------------------------

#[derive(Clone, Default)]
struct BoolProbe {
    draft: String,
    nullable: bool,
    select_open: bool,
    picks: Vec<Option<Value>>,
    toggles: usize,
    closes: usize,
}

fn view(s: &BoolProbe, _cx: &mut Cx<BoolProbe>) -> El {
    let mut editor = BooleanCellEditor::<BoolProbe>::new(s.draft.clone(), s.nullable, "列 flag")
        .select_open(s.select_open);
    editor = editor
        .on_pick(|st, value| st.picks.push(value))
        .on_select_toggle(|st| {
            st.select_open = !st.select_open;
            st.toggles += 1;
        })
        .on_select_close(|st| st.closes += 1);
    let mut layer = Vec::new();
    let el = editor.build(crate::ui::kit::kit(true), &mut layer);
    rune_kit::root(el, layer)
}

fn mount(state: BoolProbe, view_fn: fn(&BoolProbe, &mut Cx<BoolProbe>) -> El, size: (f32, f32)) -> Runtime<BoolProbe> {
    let mut rt = Runtime::new(state, view_fn, size, env(true));
    rt.frame(0.0);
    rt
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

#[test]
fn the_not_null_branch_is_the_checkbox_and_a_press_picks_the_opposite() {
    // draft "true" → checked.
    let rt = mount(
        BoolProbe { draft: "true".to_owned(), nullable: false, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "cell-editor-boolean"), "the checkbox mounts");
    assert!(!has_key(&rt, "cell-editor-boolean-trigger"), "no select trigger");

    // A press toggles: onPick(false).
    let mut rt = mount(
        BoolProbe { draft: "true".to_owned(), nullable: false, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    click(&mut rt, "cell-editor-boolean");
    assert_eq!(rt.state.picks, vec![Some(json!(false))]);

    // draft "false" → unchecked; a press picks true.
    let mut rt = mount(
        BoolProbe { draft: "false".to_owned(), nullable: false, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    click(&mut rt, "cell-editor-boolean");
    assert_eq!(rt.state.picks, vec![Some(json!(true))]);
}

#[test]
fn the_nullable_branch_is_the_three_state_select() {
    // draft "true" → the select's trigger mounts (the kit's derived
    // trigger key) and no checkbox exists.
    let rt = mount(
        BoolProbe { draft: "true".to_owned(), nullable: true, select_open: true, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "cell-editor-boolean-trigger"), "the select trigger mounts");

    // The listbox's three options ride the layer.
    assert!(has_key(&rt, "option0"), "the NULL option");
    assert!(has_key(&rt, "option1"), "the true option");
    assert!(has_key(&rt, "option2"), "the false option");
}

#[test]
fn a_choice_through_the_select_writes_the_discrete_value() {
    // The journey: open the three-state select, then choose each of
    // its options in turn and read the picked values. The kit derives
    // the trigger key (-trigger) and the option rows' positional keys
    // (option0/1/2 in the list order: NULL, true, false).
    let mut rt = mount(
        BoolProbe { draft: "false".to_owned(), nullable: true, select_open: false, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    // The trigger press opens (the toggle channel).
    click(&mut rt, "cell-editor-boolean-trigger");
    assert_eq!(rt.state.toggles, 1);

    // With the listbox open, choose the NULL option: onPick(None).
    rt.state.select_open = true;
    rt.frame(1.0 / 60.0);
    click(&mut rt, "option0");
    assert_eq!(rt.state.picks, vec![None], "the NULL option picks SQL NULL");

    // The true option: onPick(Some(true)).
    click(&mut rt, "option1");
    assert_eq!(rt.state.picks, vec![None, Some(json!(true))]);

    // The false option: onPick(Some(false)).
    click(&mut rt, "option2");
    assert_eq!(rt.state.picks, vec![None, Some(json!(true)), Some(json!(false))]);
}

#[test]
fn the_selects_current_follows_the_draft() {
    // The trigger shows the current option's label; the label lives on
    // a child of the trigger, so match by trigger path (the mapping
    // row's own precedent).
    let rt = mount(
        BoolProbe { draft: "true".to_owned(), nullable: true, select_open: false, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    assert!(trigger_shows(&rt, "true"), "draft true shows the true option");

    // A draft that is neither true nor false (the NULL seed) shows the
    // NULL option.
    let rt = mount(
        BoolProbe { draft: String::new(), nullable: true, select_open: false, ..Default::default() },
        view,
        (960.0, 640.0),
    );
    assert!(trigger_shows(&rt, "NULL"), "an off-draft shows the NULL option");
}

fn trigger_shows(rt: &Runtime<BoolProbe>, label: &str) -> bool {
    let trigger = keyed(rt, "cell-editor-boolean-trigger").expect("the trigger is laid out");
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(label) && e.path.starts_with(&trigger.path))
}
