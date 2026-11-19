//! T134: the query execution status strip — coder tests.
//! Mounted runtime probes over the view-model slice: the idle null, the
//! phase labels, the spinner/stop-button gating, the count fields, the
//! capability note and the cancel press path.

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
    show_cancel: bool,
    cancels: usize,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            vm: QueryExecutionStatusVm {
                phase: QueryPhase::Running,
                cancel_capability: CancelCapability::Supported,
                cancel_state: CancelActionState::Available,
                elapsed_ms: Some(128),
                row_count: Some(42),
                affected_rows: Some(3),
            },
            show_cancel: true,
            cancels: 0,
        }
    }
}

fn view(scene: &Scene, cx: &mut Cx<Scene>) -> El {
    let dark = cx.system_dark();
    let strip = QueryExecutionStatus::<Scene>::new()
        .vm(scene.vm.clone())
        .show_cancel(scene.show_cancel)
        .on_cancel(|s: &mut Scene| s.cancels += 1)
        .build(dark);
    col()
        .key("page")
        .w_full()
        .h_full()
        .padding(24.0)
        .child(strip.unwrap_or_else(|| col().key("empty").w_full().h(4.0)))
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
fn idle_renders_nothing() {
    let mut scene = Scene::default();
    scene.vm = QueryExecutionStatusVm::default(); // phase: Idle
    let rt = runtime(scene, DESIGN, true);
    assert!(!has(&rt, "query-execution-status"), "React's null return");
    assert!(has(&rt, "empty"), "the caller's null-arm placeholder shows");
}

#[test]
fn every_non_idle_phase_paints_its_label() {
    for (phase, key) in [
        (QueryPhase::Running, "query.running"),
        (QueryPhase::CancelRequested, "query.cancelling"),
        (QueryPhase::Cancelled, "query.cancelled"),
        (QueryPhase::Failed, "common.failed"),
        (QueryPhase::OutcomeUnknown, "common.failed"),
        (QueryPhase::Succeeded, "common.success"),
    ] {
        let mut scene = Scene::default();
        scene.vm.phase = phase;
        // Terminal phases with no counts: strip down to the label itself.
        scene.vm.elapsed_ms = None;
        scene.vm.row_count = None;
        scene.vm.affected_rows = None;
        scene.vm.cancel_capability = CancelCapability::Supported;
        let rt = runtime(scene, DESIGN, true);
        assert_eq!(
            text_of(&rt, "query-execution-status-phase"),
            t(key),
            "{phase:?}"
        );
    }
}

#[test]
fn only_the_active_phases_show_the_spinner_and_the_stop_button() {
    for phase in [QueryPhase::Running, QueryPhase::CancelRequested] {
        let mut scene = Scene::default();
        scene.vm.phase = phase;
        let rt = runtime(scene.clone(), DESIGN, true);
        assert!(has(&rt, "query-execution-status-spinner"), "{phase:?}");
        assert!(has(&rt, "editor-stop-button"), "{phase:?}");
    }
    for phase in [
        QueryPhase::Succeeded,
        QueryPhase::Failed,
        QueryPhase::Cancelled,
        QueryPhase::OutcomeUnknown,
    ] {
        let mut scene = Scene::default();
        scene.vm.phase = phase;
        scene.vm.cancel_state = CancelActionState::Unavailable;
        let rt = runtime(scene, DESIGN, true);
        assert!(!has(&rt, "query-execution-status-spinner"), "{phase:?}");
        assert!(!has(&rt, "editor-stop-button"), "{phase:?}");
        assert!(!has(&rt, "query-execution-status-cancel-note"), "{phase:?}");
    }
}

#[test]
fn the_count_fields_render_with_their_units() {
    let rt = runtime(Scene::default(), DESIGN, true);
    assert_eq!(text_of(&rt, "query-execution-status-elapsed"), "128 ms");
    assert_eq!(
        text_of(&rt, "query-execution-status-rows"),
        format!("42 {}", t("common.rows"))
    );
    assert_eq!(
        text_of(&rt, "query-execution-status-affected"),
        format!("3 {}", t("query.affectedRows"))
    );

    // A run that reported nothing paints no count fields at all.
    let mut scene = Scene::default();
    scene.vm.elapsed_ms = None;
    scene.vm.row_count = None;
    scene.vm.affected_rows = None;
    let rt = runtime(scene, DESIGN, true);
    assert!(!has(&rt, "query-execution-status-elapsed"));
    assert!(!has(&rt, "query-execution-status-rows"));
    assert!(!has(&rt, "query-execution-status-affected"));
}

#[test]
fn the_stop_button_answers_only_when_the_cancel_is_available() {
    // available: enabled and pressable — the press reaches the host.
    let mut rt = runtime(Scene::default(), DESIGN, true);
    assert!(element(&rt, "editor-stop-button").focusable);
    let (x, y) = rt.find("editor-stop-button").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.cancels, 1, "the press reached the host");

    // cancel_requested: the strip is winding down — inert.
    let mut scene = Scene::default();
    scene.vm.phase = QueryPhase::CancelRequested;
    scene.vm.cancel_state = CancelActionState::Requested;
    let rt = runtime(scene, DESIGN, true);
    assert!(!element(&rt, "editor-stop-button").focusable);

    // unavailable / failed cancels: inert the same way.
    for state in [CancelActionState::Unavailable, CancelActionState::Failed] {
        let mut scene = Scene::default();
        scene.vm.cancel_state = state;
        let rt = runtime(scene, DESIGN, true);
        assert!(
            !element(&rt, "editor-stop-button").focusable,
            "{state:?}"
        );
    }
}

#[test]
fn the_stop_button_label_follows_the_phase() {
    let rt = runtime(Scene::default(), DESIGN, true);
    assert_eq!(text_of(&rt, "label"), t("query.stop"));

    let mut scene = Scene::default();
    scene.vm.phase = QueryPhase::CancelRequested;
    scene.vm.cancel_state = CancelActionState::Requested;
    let rt = runtime(scene, DESIGN, true);
    assert_eq!(text_of(&rt, "label"), t("query.cancelling"));
}

#[test]
fn show_cancel_false_hides_the_button_but_keeps_the_run_visible() {
    let mut scene = Scene::default();
    scene.show_cancel = false;
    let rt = runtime(scene, DESIGN, true);
    assert!(has(&rt, "query-execution-status-spinner"));
    assert!(has(&rt, "query-execution-status-phase"));
    assert!(!has(&rt, "editor-stop-button"));
}

#[test]
fn an_unsupported_or_unknown_driver_paints_the_honest_note() {
    for capability in [CancelCapability::Unknown, CancelCapability::Unsupported] {
        let mut scene = Scene::default();
        scene.vm.cancel_capability = capability;
        // The React VM derivation answers 'unavailable' for a driver that
        // cannot cancel (an `available` state implies a supported driver).
        scene.vm.cancel_state = CancelActionState::Unavailable;
        let rt = runtime(scene, DESIGN, true);
        let expected = match capability {
            CancelCapability::Unknown => t("query.cancelUnknown"),
            _ => t("query.cancelUnavailable"),
        };
        assert_eq!(
            text_of(&rt, "query-execution-status-cancel-note"),
            expected,
            "{capability:?}"
        );
        // The button still shows (active), inert.
        assert!(!element(&rt, "editor-stop-button").focusable);
    }

    // A supported driver paints no note.
    let rt = runtime(Scene::default(), DESIGN, true);
    assert!(!has(&rt, "query-execution-status-cancel-note"));
}

#[test]
fn the_strip_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let rt = runtime(Scene::default(), (w, h), dark);
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
