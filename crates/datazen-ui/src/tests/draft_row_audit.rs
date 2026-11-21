//! T257 independent audit: fresh vectors, none shared with the coder
//! file. The angles here: the unwired closed toggle in light mode, two
//! draft rows in one view keeping their own markers and callbacks, a
//! widths list shorter than the column list (the tail defaults), the
//! `js_string` round-trip for the written families (a decimal, a
//! non-Latin string, a bool, an array, a null), a window-wide geometry
//! sweep, the discard button's tab-ring opt-out, an editing target that
//! names another draft (nothing edits), and a plan pointed at an
//! unwritten column (the DEFAULT placeholder ignores it).

use std::collections::HashMap;

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use crate::components::data_table::cell_renderer::CellPresentationVm;
use crate::components::data_table::detail_panel_toggle::DetailPanelToggle;
use crate::components::data_table::draft_row::{DraftCellEditing, DraftRow};
use crate::components::data_table::table_header::Column;
use crate::i18n::t;
use crate::table_data_connection_state::PendingInsert;

const W: f32 = 1000.0;
const H: f32 = 400.0;
const START: f32 = 200.0;
const HEIGHT: f32 = 28.0;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

fn audit_columns() -> Vec<Column> {
    vec![
        Column::new("a-qty", "qty"),
        Column::new("a-who", "who"),
        Column::new("a-ok", "ok"),
        Column::new("a-tags", "tags"),
        Column::new("a-ts", "ts"),
        Column::new("a-blank", "blank"),
    ]
}

/// The audit row: five written families and one unwritten column.
fn audit_draft() -> PendingInsert {
    let mut values = HashMap::new();
    values.insert("qty".to_owned(), json!(3.5));
    values.insert("who".to_owned(), json!("李雷"));
    values.insert("ok".to_owned(), json!(true));
    values.insert("tags".to_owned(), json!([1, 2]));
    values.insert("ts".to_owned(), json!(null));
    PendingInsert {
        draft_id: "audit-1".to_owned(),
        values,
    }
}

#[derive(Default)]
struct Scene {
    draft: Option<PendingInsert>,
    second: Option<PendingInsert>,
    columns: Vec<Column>,
    widths: Option<Vec<f32>>,
    editing: Option<DraftCellEditing>,
    plan_column: Option<String>,
    activates: Vec<String>,
    removes: Vec<String>,
    edit_starts: Vec<(String, String)>,
    dark: bool,
}

fn view_one(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let draft = scene.draft.clone().expect("the audit scene carries a draft");
    let mut plans = HashMap::new();
    if let Some(plan_column) = scene.plan_column.clone() {
        plans.insert(plan_column, CellPresentationVm::text("audit-plan", "计划外文本"));
    }
    col()
        .key("audit-host")
        .w_full()
        .h_full()
        .child(
            DraftRow::<Scene>::new("draft-row-5", draft, START, HEIGHT)
                .columns(scene.columns.clone())
                .column_widths(scene.widths.clone())
                .editing(scene.editing.clone())
                .presentation_plans(plans)
                .on_draft_row_activate(|s: &mut Scene, id| s.activates.push(id.to_owned()))
                .on_draft_row_remove(|s: &mut Scene, id| s.removes.push(id.to_owned()))
                .on_draft_cell_edit_start(|s: &mut Scene, id, column| {
                    s.edit_starts.push((id.to_owned(), column.to_owned()))
                })
                .build(scene.dark),
        )
}

fn view_two(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let first = scene.draft.clone().expect("row zero");
    let second = scene.second.clone().expect("row one");
    let columns = scene.columns.clone();
    col()
        .key("audit-host")
        .w_full()
        .h_full()
        .child(
            DraftRow::<Scene>::new("draft-row-0", first, 0.0, HEIGHT)
                .columns(columns.clone())
                .on_draft_row_activate(|s: &mut Scene, id| s.activates.push(id.to_owned()))
                .on_draft_row_remove(|s: &mut Scene, id| s.removes.push(id.to_owned()))
                .on_draft_cell_edit_start(|s: &mut Scene, id, column| {
                    s.edit_starts.push((id.to_owned(), column.to_owned()))
                })
                .build(scene.dark),
        )
        .child(
            DraftRow::<Scene>::new("draft-row-1", second, 100.0, HEIGHT)
                .columns(columns)
                .on_draft_row_activate(|s: &mut Scene, id| s.activates.push(id.to_owned()))
                .on_draft_row_remove(|s: &mut Scene, id| s.removes.push(id.to_owned()))
                .on_draft_cell_edit_start(|s: &mut Scene, id, column| {
                    s.edit_starts.push((id.to_owned(), column.to_owned()))
                })
                .build(scene.dark),
        )
}

fn mount(scene: Scene, two: bool) -> Runtime<Scene> {
    let dark = scene.dark;
    let view = if two { view_two } else { view_one };
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

#[test]
fn an_unwired_closed_toggle_mounts_and_presses_quietly_in_light_mode() {
    let mut rt = {
        let scene = Scene { dark: false, ..Default::default() };
        let dark = scene.dark;
        let mut rt = Runtime::new(
            scene,
            |scene: &Scene, _cx: &mut Cx<Scene>| {
                col().w_full().h_full().child(
                    DetailPanelToggle::<Scene>::new("audit-toggle", false).build(scene.dark),
                )
            },
            (W, H),
            env(dark),
        );
        rt.frame(0.0);
        rt
    };
    assert!(has_key(&rt, "audit-toggle"));
    assert_eq!(label_of(&rt, "audit-toggle"), t("detail.show"));
    assert_eq!(maybe(&rt, "audit-toggle").unwrap().role, "button");
    assert_eq!(maybe(&rt, "audit-toggle").unwrap().fill.a, 0.0);
    click(&mut rt, "audit-toggle");
    assert!(rt.state.removes.is_empty() && rt.state.activates.is_empty());
}

#[test]
fn two_draft_rows_keep_their_own_markers_and_callbacks() {
    let mut values0 = HashMap::new();
    values0.insert("qty".to_owned(), json!(1));
    let mut values1 = HashMap::new();
    values1.insert("qty".to_owned(), json!(2));
    let scene = Scene {
        draft: Some(PendingInsert {
            draft_id: "a0".to_owned(),
            values: values0,
        }),
        second: Some(PendingInsert {
            draft_id: "a1".to_owned(),
            values: values1,
        }),
        columns: vec![Column::new("a-qty", "qty")],
        dark: true,
        ..Default::default()
    };
    let mut rt = mount(scene, true);

    // Each row mounts its own markers.
    assert!(has_key(&rt, "draft-row-0") && has_key(&rt, "draft-row-1"));
    assert_eq!(text_of(&rt, "draft-row-0-cell-a-qty-value"), "1");
    assert_eq!(text_of(&rt, "draft-row-1-cell-a-qty-value"), "2");
    assert_eq!(label_of(&rt, "draft-row-0"), t("tableData.draftRowUnsaved"));
    assert_eq!(text_of(&rt, "draft-row-1-badge"), t("tableData.draftRowBadge"));

    // Discarding the second row carries the second draft's id only.
    click(&mut rt, "draft-row-1-remove");
    assert_eq!(rt.state.removes, vec!["a1".to_owned()]);
    assert!(rt.state.activates.is_empty());
    // Its cell values are untouched by the discard.
    assert_eq!(text_of(&rt, "draft-row-0-cell-a-qty-value"), "1");
}

#[test]
fn a_widths_list_shorter_than_the_columns_defaults_the_tail() {
    let scene = Scene {
        draft: Some(audit_draft()),
        columns: audit_columns(),
        widths: Some(vec![100.0]),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene, false);

    assert_eq!(maybe(&rt, "draft-row-5-cell-a-qty").unwrap().rect[2], 100.0);
    for key in ["a-who", "a-ok", "a-tags", "a-ts", "a-blank"] {
        assert_eq!(
            maybe(&rt, &format!("draft-row-5-cell-{key}")).unwrap().rect[2],
            160.0,
            "the width after the list ends defaults to 160"
        );
    }
}

#[test]
fn written_values_round_trip_as_js_strings() {
    let scene = Scene {
        draft: Some(audit_draft()),
        columns: audit_columns(),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene, false);

    // React's renderer prints the JS spelling of each written family.
    assert_eq!(text_of(&rt, "draft-row-5-cell-a-qty-value"), "3.5");
    assert_eq!(text_of(&rt, "draft-row-5-cell-a-who-value"), "李雷");
    assert_eq!(text_of(&rt, "draft-row-5-cell-a-ok-value"), "true");
    assert_eq!(text_of(&rt, "draft-row-5-cell-a-tags-value"), "[1,2]");
    // The explicit NULL: the renderer's own NULL marker.
    assert_eq!(text_of(&rt, "draft-row-5-cell-a-ts-value"), "NULL");
}

#[test]
fn the_row_stays_inside_the_window_and_starts_at_the_left_edge() {
    let scene = Scene {
        draft: Some(audit_draft()),
        columns: audit_columns(),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene, false);

    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else { continue };
        if !key.starts_with("draft-row-5") {
            continue;
        }
        let [x, _y, w, _h] = e.rect;
        if x < -0.5 || x + w > W + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(outside.is_empty(), "{outside:?} escape the {W}x{H} window");
    // React 82: `absolute left-0`.
    assert_eq!(maybe(&rt, "draft-row-5").unwrap().rect[0], 0.0);
    assert_eq!(maybe(&rt, "draft-row-5").unwrap().rect[1], START);
    assert_eq!(maybe(&rt, "draft-row-5").unwrap().rect[3], HEIGHT);
}

#[test]
fn the_discard_button_stays_out_of_the_tab_ring() {
    // React 95: `tabIndex={-1}` — the discard button is press-only.
    let scene = Scene {
        draft: Some(audit_draft()),
        columns: audit_columns(),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene, false);

    assert!(
        !maybe(&rt, "draft-row-5-remove").unwrap().focusable,
        "the discard button is out of the tab ring"
    );
}

#[test]
fn an_editing_target_on_another_draft_edits_nothing_here() {
    // React 113–114: the editing cell is `draftId === draft.draftId &&
    // column === col.name` — a target that names a different draft must
    // not open an editor on this row.
    let scene = Scene {
        draft: Some(audit_draft()),
        columns: audit_columns(),
        editing: Some(DraftCellEditing {
            draft_id: "audit-2".to_owned(),
            column: "qty".to_owned(),
        }),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene, false);

    let editing_keys = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref().is_some_and(|k| k.ends_with("-editing")))
        .count();
    assert_eq!(editing_keys, 0, "no stand-in mounts for a foreign draft");
    assert_eq!(text_of(&rt, "draft-row-5-cell-a-qty-value"), "3.5");
}

#[test]
fn a_plan_pointed_at_an_unwritten_column_is_ignored() {
    // React 30–35: "Only a **written** cell consults its plan" — the
    // plan map may carry the unwritten column's entry, and the DEFAULT
    // placeholder still shows the pack text.
    let scene = Scene {
        draft: Some(audit_draft()),
        columns: audit_columns(),
        plan_column: Some("blank".to_owned()),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene, false);

    assert_eq!(
        text_of(&rt, "draft-row-5-cell-a-blank-default"),
        t("tableData.insertDefault")
    );
    assert!(!has_key(&rt, "draft-row-5-cell-a-blank-value"));
}
