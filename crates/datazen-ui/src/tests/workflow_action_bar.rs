//! T019: the `WorkflowEditorActionBar` port — buttons, order, i18n labels,
//! the Save disabled state, the callback paths and the bar's geometry
//! (top border, `px-6 py-3`, right-aligned `gap-3`).

use rune::{col, el, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::workflow_editor_action_bar;

#[derive(Default)]
struct BarState {
    save_disabled: bool,
    saves: usize,
    cancels: usize,
}

fn bar_view(state: &BarState, _cx: &mut Cx<BarState>) -> El {
    // The page stacks the edit content above the bar: a growing spacer
    // places the bar at the pane's bottom the same way.
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(el().grow(1.0))
        .child(workflow_editor_action_bar::<BarState>(
            "workflow-save",
            state.save_disabled,
            |s: &mut BarState| s.saves += 1,
            |s: &mut BarState| s.cancels += 1,
            true,
        ))
}

fn probe(disabled: bool) -> Runtime<BarState> {
    let mut rt = Runtime::new(
        BarState {
            save_disabled: disabled,
            ..BarState::default()
        },
        bar_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<BarState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<BarState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<BarState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn both_buttons_exist_come_in_react_order_and_speak_i18n() {
    let rt = probe(false);
    let cancel = element(&rt, "workflow-editor-cancel");
    let save = element(&rt, "workflow-save");
    assert_eq!(cancel.role, "button", "Cancel is a button");
    assert_eq!(save.role, "button", "Save is a button");
    assert_eq!(
        cancel.label.as_deref(),
        Some(t("common.cancel")),
        "Cancel reads the zh-CN table, not a hard-coded label"
    );
    assert_eq!(
        save.label.as_deref(),
        Some(t("common.save")),
        "Save reads the zh-CN table, not a hard-coded label"
    );
    // React order: Cancel first, Save second — Cancel sits left of Save.
    assert!(
        cancel.rect[0] < save.rect[0],
        "Cancel at {:?} must precede Save at {:?}",
        cancel.rect,
        save.rect
    );
    // The two buttons share the row (same band) and gap-3 = 12pt apart.
    assert!((cancel.rect[1] - save.rect[1]).abs() < 0.5);
    assert!(
        (save.rect[0] - (cancel.rect[0] + cancel.rect[2]) - 12.0).abs() < 0.5,
        "gap-3 between the buttons: cancel {:?}, save {:?}",
        cancel.rect,
        save.rect
    );
    // ExtraLarge is the React md button's h-9 = 36pt.
    assert!(
        (save.rect[3] - 36.0).abs() < 0.5 && (cancel.rect[3] - 36.0).abs() < 0.5,
        "both buttons are 36pt tall: cancel {:?}, save {:?}",
        cancel.rect,
        save.rect
    );
}

#[test]
fn the_bar_keeps_reacts_geometry_top_border_box_and_alignment() {
    let rt = probe(false);
    let bar = element(&rt, "workflow-editor-action-bar");
    let save = element(&rt, "workflow-save");
    // px-6 py-3 around a 36pt button: 12 + 36 + 12 = 60pt tall.
    assert!(
        (bar.rect[3] - 60.0).abs() < 0.5,
        "px-6 py-3 around a 36pt button: {:?}",
        bar.rect
    );
    // w_full inside the view's 16pt padding: 960 - 32 = 928pt wide.
    assert!(
        (bar.rect[2] - 928.0).abs() < 0.5,
        "the bar spans its parent: {:?}",
        bar.rect
    );
    // justify-end + px-6: Save's right edge is 24pt inside the bar.
    assert!(
        (bar.rect[0] + bar.rect[2] - 24.0 - (save.rect[0] + save.rect[2])).abs() < 0.5,
        "Save is right-aligned at px-6: bar {:?}, save {:?}",
        bar.rect,
        save.rect
    );
    // The border-t separates the bar from the content above it: the row
    // starts 12pt (py-3) below the bar's top edge.
    let inner = element(&rt, "workflow-editor-action-bar-row");
    assert!(
        (inner.rect[1] - bar.rect[1] - 12.0).abs() < 0.5,
        "py-3 above the row: bar {:?}, row {:?}",
        bar.rect,
        inner.rect
    );
    // The bar is the last child of its column: its bottom edge is the
    // column's, 16pt above the window's bottom.
    assert!(
        (640.0 - 16.0 - (bar.rect[1] + bar.rect[3])).abs() < 0.5,
        "the bar hugs the bottom of the pane: {:?}",
        bar.rect
    );
}

#[test]
fn save_and_cancel_presses_reach_their_callbacks() {
    let mut rt = probe(false);
    click(&mut rt, "workflow-save");
    assert_eq!(rt.state.saves, 1, "the Save press ran on_save");
    assert_eq!(rt.state.cancels, 0);
    click(&mut rt, "workflow-editor-cancel");
    assert_eq!(rt.state.cancels, 1, "the Cancel press ran on_cancel");
    assert_eq!(rt.state.saves, 1);
}

#[test]
fn a_disabled_save_is_dimmed_inert_and_not_a_tab_stop() {
    let mut rt = probe(true);
    let save = element(&rt, "workflow-save");
    let cancel = element(&rt, "workflow-editor-cancel");
    assert!(!save.focusable, "a disabled Save must not be focusable");
    assert!(
        save.opacity < 0.99,
        "the kit dims a disabled button (opacity {})",
        save.opacity
    );
    assert!(cancel.focusable, "Cancel stays enabled");

    // The press is refused: nothing moves.
    click(&mut rt, "workflow-save");
    assert_eq!(rt.state.saves, 0, "a disabled Save press does nothing");
    click(&mut rt, "workflow-editor-cancel");
    assert_eq!(rt.state.cancels, 1, "Cancel still works beside it");

    // The enabled bar answers the same press through the same key.
    let mut rt = probe(false);
    click(&mut rt, "workflow-save");
    assert_eq!(rt.state.saves, 1);
    assert!(element(&rt, "workflow-save").focusable);
}

/// The bar stays inside the window at both DoD sizes (the page mounts it as
/// a bottom bar, so 960×640 is the tightest real case).
#[test]
fn the_bar_fits_both_window_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(
            BarState::default(),
            bar_view,
            (w, h),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        for key in [
            "workflow-editor-action-bar",
            "workflow-editor-action-bar-row",
            "workflow-editor-cancel",
            "workflow-save",
        ] {
            let e = element(&rt, key);
            assert!(
                e.rect[0] >= -0.5 && e.rect[0] + e.rect[2] <= w + 0.5,
                "{key} out of {w}x{h}: {:?}",
                e.rect
            );
        }
    }
}

/// The bar's slot-shaped API: a different save key lands on the Save button
/// (React's `saveTestId`).
#[test]
fn the_save_key_slot_renames_the_save_button() {
    let custom_view = |state: &BarState, _cx: &mut Cx<BarState>| {
        col()
            .w_full()
            .h_full()
            .padding(16.0)
            .child(el().grow(1.0))
            .child(workflow_editor_action_bar::<BarState>(
                "workflow-yaml-save",
                state.save_disabled,
                |s: &mut BarState| s.saves += 1,
                |s: &mut BarState| s.cancels += 1,
                true,
            ))
    };
    let mut rt = Runtime::new(
        BarState::default(),
        custom_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, "workflow-yaml-save"), "the slot key took");
    assert!(has_key(&rt, "workflow-editor-cancel"), "Cancel keeps its key");
    assert!(!has_key(&rt, "workflow-save"));
}

/// The bar mounts exactly its own four keyed parts — the root, the inner
/// row and the two buttons — with no stray keyed children.
#[test]
fn the_bar_has_no_sibling_elements_beside_its_parts() {
    let rt = probe(false);
    let keys: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with("workflow-editor"))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), keys.len(), "duplicate bar keys: {keys:?}");
    for expected in [
        "workflow-editor-action-bar",
        "workflow-editor-action-bar-row",
        "workflow-editor-cancel",
        "workflow-save",
    ] {
        assert!(keys.contains(&expected.to_owned()), "missing {expected}");
    }
    assert_eq!(keys.len(), 4, "the bar is exactly its four parts: {keys:?}");
}
