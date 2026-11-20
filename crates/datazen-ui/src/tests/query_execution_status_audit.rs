//! T134 independent audit: the query execution status strip, fresh vectors.
//!
//! The coder tests answer one scene per rule; this audit probes from the
//! other side: the a11y surface (role/label/help), fresh numbers, a
//! long-elapsed run, a zero-row terminal state, three presses counted, the
//! inert button's accessible tooltip, and the strip's geometry under both
//! windows and themes. The state machine (`queryExecActions`) is out of
//! scope: the VM slice is the contract.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::query::query_execution_status::{
    CancelActionState, CancelCapability, QueryExecutionStatus, QueryExecutionStatusVm, QueryPhase,
};
use crate::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: QueryExecutionStatusVm,
    cancels: usize,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            // A fresh vector: a supported winding-down run with big numbers.
            vm: QueryExecutionStatusVm {
                phase: QueryPhase::CancelRequested,
                cancel_capability: CancelCapability::Supported,
                cancel_state: CancelActionState::Requested,
                elapsed_ms: Some(600_000),
                row_count: Some(0),
                affected_rows: None,
            },
            cancels: 0,
        }
    }
}

fn view(scene: &Scene, cx: &mut Cx<Scene>) -> El {
    let dark = cx.system_dark();
    let strip = QueryExecutionStatus::<Scene>::new()
        .vm(scene.vm.clone())
        .on_cancel(|s: &mut Scene| s.cancels += 1)
        .build(dark);
    col()
        .key("audit-page")
        .w_full()
        .h_full()
        .padding(24.0)
        .child(strip.unwrap_or_else(|| col().key("audit-empty").w_full().h(4.0)))
}

fn runtime(scene: Scene, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn has<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S: 'static>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

#[test]
fn the_strip_announces_itself_as_a_polite_status_region() {
    let rt = runtime(Scene::default(), DESIGN, true);
    let root = element(&rt, "query-execution-status");
    // React's role="status"; `aria-live` has no rune surface yet (the module
    // docs track the gap), so the role is the observable half.
    assert_eq!(root.role, "status");
}

#[test]
fn a_winding_down_run_paints_the_cancelling_label_and_its_minutes() {
    let rt = runtime(Scene::default(), DESIGN, true);
    assert_eq!(
        text_of(&rt, "query-execution-status-phase"),
        t("query.cancelling")
    );
    // Fresh numbers, no grouping: 600000 ms and a 0-row result.
    assert_eq!(text_of(&rt, "query-execution-status-elapsed"), "600000 ms");
    assert_eq!(
        text_of(&rt, "query-execution-status-rows"),
        format!("0 {}", t("common.rows"))
    );
    assert!(!has(&rt, "query-execution-status-affected"));
    // The stop button answers the phase, not the cancel state.
    assert_eq!(text_of(&rt, "label"), t("query.cancelling"));
    // Requested: the button is inert while the run winds down.
    assert!(!element(&rt, "editor-stop-button").focusable);
}

#[test]
fn a_terminal_state_paints_its_verdict_not_the_run_indicators() {
    let mut scene = Scene::default();
    scene.vm.phase = QueryPhase::Succeeded;
    // A terminal state never carries the spinner or the stop button, even
    // with every field filled in.
    scene.vm.cancel_state = CancelActionState::Available;
    let rt = runtime(scene, DESIGN, true);
    assert_eq!(
        text_of(&rt, "query-execution-status-phase"),
        t("common.success")
    );
    assert!(!has(&rt, "query-execution-status-spinner"));
    assert!(!has(&rt, "editor-stop-button"));
    assert!(!has(&rt, "query-execution-status-cancel-note"));

    // The failure verdicts share `common.failed`: unknown outcome is a
    // failure verdict too, and it is not the success text.
    let mut scene = Scene::default();
    scene.vm.phase = QueryPhase::OutcomeUnknown;
    scene.vm.elapsed_ms = None;
    scene.vm.row_count = None;
    let rt = runtime(scene, DESIGN, true);
    assert_eq!(
        text_of(&rt, "query-execution-status-phase"),
        t("common.failed")
    );
    assert_ne!(
        text_of(&rt, "query-execution-status-phase"),
        t("common.success")
    );
}

#[test]
fn three_presses_on_an_available_cancel_reach_the_host_three_times() {
    let mut scene = Scene::default();
    scene.vm.phase = QueryPhase::Running;
    scene.vm.cancel_state = CancelActionState::Available;
    let mut rt = runtime(scene, DESIGN, true);
    for _ in 0..3 {
        let (x, y) = rt.find("editor-stop-button").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.frame(0.0);
    }
    assert_eq!(rt.state.cancels, 3);
}

#[test]
fn the_inert_stop_button_carries_its_reason_as_the_tooltip() {
    for capability in [CancelCapability::Unknown, CancelCapability::Unsupported] {
        let mut scene = Scene::default();
        scene.vm.phase = QueryPhase::Running;
        scene.vm.cancel_capability = capability;
        scene.vm.cancel_state = CancelActionState::Unavailable;
        let rt = runtime(scene, DESIGN, true);
        let button = element(&rt, "editor-stop-button");
        assert!(!button.focusable, "{capability:?}");
        let expected = match capability {
            CancelCapability::Unknown => t("query.cancelUnknown"),
            _ => t("query.cancelUnavailable"),
        };
        // React's `title`: the reason the control is inert, announced as the
        // accessible label of the row.
        assert_eq!(button.label.as_deref(), Some(expected), "{capability:?}");
        // The note span repeats the same reason next to the button.
        assert_eq!(
            text_of(&rt, "query-execution-status-cancel-note"),
            expected,
            "{capability:?}"
        );
    }

    // A supported driver answers no tooltip at all.
    let mut scene = Scene::default();
    scene.vm.phase = QueryPhase::Running;
    let rt = runtime(scene, DESIGN, true);
    assert_eq!(element(&rt, "editor-stop-button").label, None);
}

#[test]
fn the_idle_null_is_none_for_every_dark_and_size() {
    for dark in [true, false] {
        let mut scene = Scene::default();
        scene.vm = QueryExecutionStatusVm::default();
        let rt = runtime(scene, MIN, dark);
        assert!(!has(&rt, "query-execution-status"));
        assert!(has(&rt, "audit-empty"));
    }
}

#[test]
fn the_strip_keeps_inside_the_window_in_both_themes_and_sizes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            // The widest strip: every field present, note included.
            let mut scene = Scene::default();
            scene.vm.phase = QueryPhase::Running;
            scene.vm.cancel_capability = CancelCapability::Unknown;
            scene.vm.cancel_state = CancelActionState::Unavailable;
            scene.vm.affected_rows = Some(1_000_000);
            let rt = runtime(scene, (w, h), dark);
            let outside: Vec<String> = rt
                .elements()
                .iter()
                .filter(|e| {
                    let r = e.rect;
                    !e.virtual_extent
                        && (r[0] < -0.5
                            || r[1] < -0.5
                            || r[0] + r[2] > w + 0.5
                            || r[1] + r[3] > h + 0.5)
                })
                .map(|e| format!("{:?}", e.key.clone().unwrap_or(e.path.clone())))
                .collect();
            assert!(
                outside.is_empty(),
                "dark={dark} {w}x{h}: out of window: {outside:?}"
            );
        }
    }
}
