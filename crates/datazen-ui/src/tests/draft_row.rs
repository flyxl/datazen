//! T257: the DetailPanelToggle and the DraftRow — coder tests.
//!
//! The toggle: both states mount with their titles and roles, the open
//! one paints the accent wash while the closed one stays clear, a press
//! flips through the callback, and an unwired one still mounts.
//!
//! The draft row: the markers and labels (the unsaved row label, the
//! badge, the discard glyph and its label), the geometry (the absolute
//! top, the height, the full width), the three write-intent states
//! (written / written-null / unwritten-default), the presentation plan
//! consulted only by a written cell, the discard press that removes
//! without activating, the cell press that activates exactly once, the
//! double press that starts the editor (with the two bubbled activation
//! clicks React also fires), the editing stand-in's seed, the 160
//! default width, and the grid roles only while selection is enabled.

use std::collections::HashMap;

use rune::{col, row, Cx, El, Env, Runtime};
use serde_json::json;

use crate::components::data_table::cell_renderer::CellPresentationVm;
use crate::components::data_table::detail_panel_toggle::DetailPanelToggle;
use crate::components::data_table::draft_row::{DraftCellEditing, DraftRow};
use crate::components::data_table::table_header::Column;
use crate::i18n::t;
use crate::table_data_connection_state::PendingInsert;

const W: f32 = 800.0;
const H: f32 = 400.0;
const ROW_START: f32 = 120.0;
const ROW_HEIGHT: f32 = 32.0;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

fn column(id: &str, name: &str, ty: Option<&str>) -> Column {
    let mut c = Column::new(id, name);
    if let Some(ty) = ty {
        c = c.ty(ty);
    }
    c
}

/// The row spec and the callback records, one scene per test.
#[derive(Default)]
struct Scene {
    draft: Option<PendingInsert>,
    columns: Vec<Column>,
    widths: Option<Vec<f32>>,
    editing: Option<DraftCellEditing>,
    selection: bool,
    plan_column: Option<String>,
    toggles: Vec<bool>,
    activates: Vec<String>,
    removes: Vec<String>,
    edit_starts: Vec<(String, String)>,
    dark: bool,
}

impl Scene {
    /// The coder vectors' row: three written states across four columns.
    fn row() -> Self {
        let mut values = HashMap::new();
        values.insert("id".to_owned(), json!(42));
        values.insert("ts".to_owned(), json!(null));
        values.insert("memo".to_owned(), json!("hello"));
        Self {
            draft: Some(PendingInsert {
                draft_id: "d7".to_owned(),
                values,
            }),
            columns: vec![
                column("c-id", "id", Some("int")),
                column("c-name", "name", None),
                column("c-ts", "ts", Some("timestamp")),
                column("c-memo", "memo", Some("text")),
            ],
            dark: true,
            ..Self::default()
        }
    }
}

fn view_toggles(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    row()
        .key("toggle-host")
        .gap(8.0)
        .child(
            DetailPanelToggle::<Scene>::new("toggle-open", true)
                .on_toggle(|s: &mut Scene| s.toggles.push(true))
                .build(scene.dark),
        )
        .child(
            DetailPanelToggle::<Scene>::new("toggle-closed", false)
                .on_toggle(|s: &mut Scene| s.toggles.push(false))
                .build(scene.dark),
        )
}

fn view_row(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let draft = scene.draft.clone().expect("the row scene carries a draft");
    let mut plans = HashMap::new();
    if let Some(plan_column) = scene.plan_column.clone() {
        plans.insert(plan_column, CellPresentationVm::text("plan", "呈现·文本"));
    }
    col()
        .key("row-host")
        .w_full()
        .h_full()
        .child(
            DraftRow::<Scene>::new("draft-row-2", draft, ROW_START, ROW_HEIGHT)
                .columns(scene.columns.clone())
                .column_widths(scene.widths.clone())
                .editing(scene.editing.clone())
                .cell_selection_enabled(scene.selection)
                .presentation_plans(plans)
                .on_draft_row_activate(|s: &mut Scene, id| s.activates.push(id.to_owned()))
                .on_draft_row_remove(|s: &mut Scene, id| s.removes.push(id.to_owned()))
                .on_draft_cell_edit_start(|s: &mut Scene, id, column| {
                    s.edit_starts.push((id.to_owned(), column.to_owned()))
                })
                .build(scene.dark),
        )
}

/// No callbacks at all: React's every handler is optional, so an
/// unwired row must still mount.
fn view_row_plain(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let draft = scene.draft.clone().expect("the row scene carries a draft");
    col()
        .key("row-host")
        .w_full()
        .h_full()
        .child(
            DraftRow::<Scene>::new("draft-row-2", draft, ROW_START, ROW_HEIGHT)
                .columns(scene.columns.clone())
                .build(scene.dark),
        )
}

fn mount_toggles(scene: Scene) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view_toggles, (W, H), env(dark));
    rt.frame(0.0);
    rt
}

fn mount_row(scene: Scene, plain: bool) -> Runtime<Scene> {
    let dark = scene.dark;
    let view = if plain { view_row_plain } else { view_row };
    let mut rt = Runtime::new(scene, view, (W, H), env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// React's `onDoubleClick`: the second press of a pair, the first one
/// having already bubbled its click.
fn double_click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }
}

// ---------------------------------------------------------------------------
// DetailPanelToggle (React 10–27)
// ---------------------------------------------------------------------------

#[test]
fn both_toggle_states_mount_with_their_titles_and_button_roles() {
    let rt = mount_toggles(Scene { dark: true, ..Default::default() });

    assert!(has_key(&rt, "toggle-open"), "the open toggle mounts");
    assert!(has_key(&rt, "toggle-closed"), "the closed toggle mounts");
    // React 17: `title={open ? t('detail.hide') : t('detail.show')}`.
    assert_eq!(label_of(&rt, "toggle-open"), t("detail.hide"));
    assert_eq!(label_of(&rt, "toggle-closed"), t("detail.show"));
    assert_eq!(maybe(&rt, "toggle-open").unwrap().role, "button");
    assert_eq!(maybe(&rt, "toggle-closed").unwrap().role, "button");
}

#[test]
fn the_open_toggle_paints_the_accent_wash_the_closed_one_stays_clear() {
    let rt = mount_toggles(Scene { dark: true, ..Default::default() });

    // React 20–22: `bg-accent/15` vs. no background at all.
    assert!(maybe(&rt, "toggle-open").unwrap().fill.a > 0.0, "the open wash paints");
    assert_eq!(maybe(&rt, "toggle-closed").unwrap().fill.a, 0.0);
}

#[test]
fn a_press_on_either_state_flips_through_the_callback() {
    let mut rt = mount_toggles(Scene { dark: true, ..Default::default() });

    click(&mut rt, "toggle-open");
    assert_eq!(rt.state.toggles, vec![true]);

    click(&mut rt, "toggle-closed");
    assert_eq!(rt.state.toggles, vec![true, false]);
}

#[test]
fn an_unwired_toggle_mounts_and_presses_without_a_callback() {
    // React's `onToggle` is the only prop besides `open`; without it the
    // button still renders and a press just does nothing.
    let mut rt = {
        let scene = Scene { dark: false, ..Default::default() };
        let dark = scene.dark;
        let mut rt = Runtime::new(
            scene,
            |scene: &Scene, _cx: &mut Cx<Scene>| {
                col().w_full().h_full().child(
                    DetailPanelToggle::<Scene>::new("toggle-plain", false).build(scene.dark),
                )
            },
            (W, H),
            env(dark),
        );
        rt.frame(0.0);
        rt
    };
    assert!(has_key(&rt, "toggle-plain"));
    click(&mut rt, "toggle-plain");
    assert!(rt.state.toggles.is_empty());
}

// ---------------------------------------------------------------------------
// DraftRow (React 52–166)
// ---------------------------------------------------------------------------

#[test]
fn the_row_mounts_its_markers_and_labels() {
    let rt = mount_row(Scene::row(), false);

    assert!(has_key(&rt, "draft-row-2"), "the root mounts with its testid key");
    // React 81: `aria-label={unsavedLabel}`.
    assert_eq!(label_of(&rt, "draft-row-2"), t("tableData.draftRowUnsaved"));
    // React 90–92: the badge.
    assert_eq!(text_of(&rt, "draft-row-2-badge"), t("tableData.draftRowBadge"));
    // React 93–106: the `×` glyph and the button's label.
    assert_eq!(text_of(&rt, "draft-row-2-remove-glyph"), "×");
    assert_eq!(label_of(&rt, "draft-row-2-remove"), t("tableData.removeDraftRow"));
}

#[test]
fn the_row_sits_at_its_start_with_its_height_and_the_full_width() {
    let rt = mount_row(Scene::row(), false);

    let [x, y, w, h] = maybe(&rt, "draft-row-2").unwrap().rect;
    assert_eq!((x, y, h), (0.0, ROW_START, ROW_HEIGHT), "absolute left-0, style.top, style.height");
    assert_eq!(w, W, "w_full inside the host");
}

#[test]
fn the_three_write_intent_states_render_their_own_contents() {
    let rt = mount_row(Scene::row(), false);

    // Written with a value: the grid's own renderer.
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-id-value"), "42");
    // Written with an explicit NULL: the renderer's own NULL marker.
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-ts-value"), "NULL");
    // Not written: the DEFAULT placeholder.
    assert!(has_key(&rt, "draft-row-2-cell-c-name-default"));
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-name-default"), t("tableData.insertDefault"));
}

#[test]
fn a_written_cell_consults_its_presentation_plan_the_default_never_does() {
    // React 30–35: only a **written** cell consults its plan; the DEFAULT
    // placeholder never sees one.
    let mut scene = Scene::row();
    // A plan for the unwritten column exists too — it must be ignored.
    scene.plan_column = Some("name".to_owned());
    let rt = mount_row(scene, false);

    assert_eq!(text_of(&rt, "draft-row-2-cell-c-name-default"), t("tableData.insertDefault"));

    let mut scene = Scene::row();
    scene.plan_column = Some("memo".to_owned());
    let rt = mount_row(scene, false);
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-memo-value"), "呈现·文本");
}

#[test]
fn pressing_the_discard_button_removes_the_draft_without_activating_it() {
    // React 100–103: `stopPropagation` — the row's own onClick never
    // sees the discard press. Rune's deepest-handler hit-testing gives
    // the same split without an explicit stop.
    let mut rt = mount_row(Scene::row(), false);

    click(&mut rt, "draft-row-2-remove");
    assert_eq!(rt.state.removes, vec!["d7".to_owned()]);
    assert!(rt.state.activates.is_empty(), "the discard press must not activate the row");
}

#[test]
fn a_press_on_a_cell_activates_the_draft_exactly_once() {
    // React: the cell has no onClick of its own, so a click bubbles to
    // the row's `onClick={() => onDraftRowActivate?.(draft.draftId)}`.
    let mut rt = mount_row(Scene::row(), false);

    click(&mut rt, "draft-row-2-cell-c-id");
    assert_eq!(rt.state.activates, vec!["d7".to_owned()]);

    // A press on the badge (the lead cell's text, no handler of its own)
    // reaches the row's handler the same way.
    click(&mut rt, "draft-row-2-badge");
    assert_eq!(rt.state.activates.len(), 2);
}

#[test]
fn a_double_press_on_a_cell_starts_the_editor_on_that_column() {
    // React 124–127: `onDoubleClick` with `stopPropagation` — the
    // dblclick starts the editor, while the two underlying clicks still
    // bubbled (two activations), exactly the pair Rune runs.
    let mut rt = mount_row(Scene::row(), false);

    // A single press must not open the editor.
    click(&mut rt, "draft-row-2-cell-c-memo");
    assert!(rt.state.edit_starts.is_empty());

    double_click(&mut rt, "draft-row-2-cell-c-memo");
    assert_eq!(
        rt.state.edit_starts,
        vec![("d7".to_owned(), "memo".to_owned())]
    );
    // 3 activations in total: the first single click's one, plus the two
    // presses of the pair — React's two bubbled clicks under its dblclick.
    assert_eq!(rt.state.activates.len(), 3, "the single click plus the pair's two presses");
}

#[test]
fn the_editing_cell_mounts_the_stand_in_seed() {
    // The T278 boundary: while a cell is being edited the port shows the
    // editor's seed — the raw value's text, empty for unwritten and NULL.
    let mut scene = Scene::row();
    scene.editing = Some(DraftCellEditing {
        draft_id: "d7".to_owned(),
        column: "memo".to_owned(),
    });
    let rt = mount_row(scene, false);

    assert!(has_key(&rt, "draft-row-2-cell-c-memo-editing"));
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-memo-editing"), "hello");
    // The other cells are untouched by the edit.
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-id-value"), "42");
}

#[test]
fn the_editing_seed_is_empty_for_an_unwritten_column() {
    // React 131: `value={value ?? null}` — the unwritten cell's editor
    // starts from null, i.e. an empty seed.
    let mut scene = Scene::row();
    scene.editing = Some(DraftCellEditing {
        draft_id: "d7".to_owned(),
        column: "name".to_owned(),
    });
    let rt = mount_row(scene, false);

    assert!(has_key(&rt, "draft-row-2-cell-c-name-editing"));
    assert_eq!(text_of(&rt, "draft-row-2-cell-c-name-editing"), "");
}

#[test]
fn column_widths_default_to_160_and_the_list_wins_where_given() {
    // React 115: `columnWidths?.[colIdx] ?? 160`.
    let rt = mount_row(Scene::row(), false);
    for key in [
        "draft-row-2-cell-c-id",
        "draft-row-2-cell-c-name",
        "draft-row-2-cell-c-ts",
        "draft-row-2-cell-c-memo",
    ] {
        assert_eq!(maybe(&rt, key).unwrap().rect[2], 160.0, "{key} defaults to 160");
    }

    let mut scene = Scene::row();
    scene.widths = Some(vec![64.0, 96.0]);
    let rt = mount_row(scene, false);
    assert_eq!(maybe(&rt, "draft-row-2-cell-c-id").unwrap().rect[2], 64.0);
    assert_eq!(maybe(&rt, "draft-row-2-cell-c-name").unwrap().rect[2], 96.0);
    assert_eq!(maybe(&rt, "draft-row-2-cell-c-ts").unwrap().rect[2], 160.0);
}

#[test]
fn grid_roles_appear_only_while_cell_selection_is_enabled() {
    // React 80/87: `role={cellSelectionEnabled ? 'row' : undefined}` and
    // the same gate on the lead cell's `rowheader`.
    let mut scene = Scene::row();
    scene.selection = true;
    let rt = mount_row(scene, false);
    assert_eq!(maybe(&rt, "draft-row-2").unwrap().role, "row");
    assert_eq!(maybe(&rt, "draft-row-2-lead").unwrap().role, "rowheader");

    let rt = mount_row(Scene::row(), false);
    assert_ne!(maybe(&rt, "draft-row-2").unwrap().role, "row");
    assert_ne!(maybe(&rt, "draft-row-2-lead").unwrap().role, "rowheader");
}
