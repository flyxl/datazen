//! Independent audit for T520: `QueryPanel` port.
//!
//! Two kinds of coverage the coder does not drive: (a) pure helper branches and
//! geometry with every dialog mounted, and (b) exact React-parity assertions for
//! the result-message dialog, strategy ask and confirmed-namespace context.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query_panel::{
    build, CancelAvailability, MessageKind, PanelSessionState, QueryPanelAction,
    QueryPanelActionFn, QueryPanelVm, MESSAGE_DIALOG_KEY, MESSAGE_OK_KEY, ROOT_KEY, STOP_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: QueryPanelVm,
    actions: Rc<RefCell<Vec<QueryPanelAction>>>,
    dark: bool,
}

fn vm() -> QueryPanelVm {
    QueryPanelVm {
        panel_id: "panel-audit".to_owned(),
        focused_pane_id: Some("p2".to_owned()),
        database: Some("app".to_owned()),
        current_database: Some("app".to_owned()),
        current_schema: Some("public".to_owned()),
        cancel_capability: Some(true),
        ..Default::default()
    }
}

fn scene(vm: QueryPanelVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let on_action: Option<QueryPanelActionFn<Scene>> =
        Some(Rc::new(move |_st, action| actions.borrow_mut().push(action)));
    build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(vm: QueryPanelVm, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let scene = scene(vm, dark);
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

// -------------------------------------------------------------------------
// (a) Passing branch-coverage tests — used as mutation canaries.
// -------------------------------------------------------------------------

#[test]
fn cancel_availability_covers_execute_stop_and_capability_disabled() {
    let mut s = vm();
    assert_eq!(s.cancel_availability(), CancelAvailability::Execute, "audit: idle shows Execute");

    s.running = true;
    assert_eq!(s.cancel_availability(), CancelAvailability::StopDisabled, "audit: running without an execution id disables Stop");

    s.execution_id = Some("exec-1".to_owned());
    assert_eq!(s.cancel_availability(), CancelAvailability::Stop, "audit: running with id and capability shows Stop");

    s.cancel_capability = None;
    assert_eq!(s.cancel_availability(), CancelAvailability::StopDisabled, "audit: unknown cancel capability disables Stop");

    s.cancel_capability = Some(false);
    assert_eq!(s.cancel_availability(), CancelAvailability::StopDisabled, "audit: unsupported cancel capability disables Stop");

    s.cancel_capability = Some(true);
    let rt = runtime(s.clone(), false, MIN);
    assert_eq!(element(&rt, STOP_KEY).label.as_deref(), Some(t("query.stop")), "audit: enabled Stop uses query.stop label");
}

#[test]
fn results_idle_tracks_all_five_sources() {
    let mut s = vm();
    assert!(s.results_idle(), "audit: a fresh panel is idle");
    s.running = true;
    assert!(!s.results_idle(), "audit: running clears idle");
    s.running = false;
    s.error = "x".to_owned();
    assert!(!s.results_idle(), "audit: an error clears idle");
    s.error.clear();
    s.show_explain = true;
    assert!(!s.results_idle(), "audit: explain clears idle");
    s.show_explain = false;
    s.diagnosis_visible = true;
    assert!(!s.results_idle(), "audit: diagnosis clears idle");
    s.diagnosis_visible = false;
    s.results_count = 1;
    assert!(!s.results_idle(), "audit: a result row clears idle");
}

#[test]
fn message_dialog_open_close_and_replace() {
    let mut s = vm();
    s.show_message("first", MessageKind::Error);
    s.show_message("second", MessageKind::Success);
    assert_eq!(s.message.text, "second", "audit: a new message replaces the previous text");
    assert_eq!(s.message.kind, MessageKind::Success, "audit: a new message replaces the previous kind");
    assert_eq!(element(&runtime(s.clone(), true, MIN), MESSAGE_DIALOG_KEY).label.as_deref(), Some(t("common.success")));
    s.close_message();
    assert!(!s.message.open, "audit: close_message closes the dialog");
    assert_eq!(s.message.text, "second", "audit: close_message keeps the text");
    assert!(!has_key(&runtime(s, false, DESIGN), MESSAGE_DIALOG_KEY));
}

#[test]
fn session_disconnect_and_switch_confirm_variants() {
    assert!(!PanelSessionState::Active.is_disconnected());
    assert!(PanelSessionState::Lost.is_disconnected());
    assert!(PanelSessionState::Closed.is_disconnected());
    let mut s = vm();
    assert!(!s.needs_switch_confirm(), "audit: no transaction needs no switch confirm");
    s.in_transaction = true;
    assert!(s.needs_switch_confirm(), "audit: an active transaction needs a switch confirm");
}

#[test]
fn geometry_with_every_dialog_mounted_at_both_sizes_and_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut s = vm();
            s.running = true;
            s.execution_id = Some("exec-1".to_owned());
            s.session_state = PanelSessionState::Closed;
            s.message = crate::views::query_panel::MessageDialogVm {
                open: true,
                kind: MessageKind::Error,
                text: "a fairly long result message that must remain inside the dialog".to_owned(),
            };
            s.dangerous_confirm_open = true;
            s.switch_tx_confirm_open = true;
            s.strategy_ask_open = true;
            let rt = runtime(s, dark, (w, h));
            for key in [ROOT_KEY, MESSAGE_DIALOG_KEY, MESSAGE_OK_KEY] {
                assert!(has_key(&rt, key), "audit: {key} laid out at {w}x{h} dark={dark}");
            }
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
            }
        }
    }
}

// -------------------------------------------------------------------------
// (b) Exact React-parity assertions — these FAIL on the current port.
// -------------------------------------------------------------------------

#[test]
fn react_error_message_title_is_common_error_not_common_failed() {
    let mut s = vm();
    s.show_message("boom", MessageKind::Error);
    let rt = runtime(s, true, DESIGN);
    assert_eq!(
        element(&rt, MESSAGE_DIALOG_KEY).label.as_deref(),
        Some(t("common.error")),
        "DEFECT: React ResultMessageDialog uses common.error for an error title, not common.failed"
    );
}

#[test]
fn react_error_message_has_a_copy_action() {
    let mut s = vm();
    s.show_message("boom", MessageKind::Error);
    let rt = runtime(s, true, DESIGN);
    let copy = element(&rt, "result-message-copy");
    assert_eq!(copy.help.as_deref(), Some(t("common.copy")), "DEFECT: React renders result-message-copy with common.copy tooltip for errors");
}

#[test]
fn react_success_message_has_no_copy_action() {
    let mut s = vm();
    s.show_message("done", MessageKind::Success);
    let rt = runtime(s, true, DESIGN);
    assert!(!has_key(&rt, "result-message-copy"), "audit: React hides the copy action for success messages");
}

#[test]
fn react_strategy_ask_uses_execution_strategy_labels() {
    let mut s = vm();
    s.strategy_ask_open = true;
    let rt = runtime(s, true, DESIGN);
    assert_eq!(
        element(&rt, "query-panel-strategy-current").label.as_deref(),
        Some(t("query.executionStrategy.runCurrent")),
        "DEFECT: React strategy ask uses query.executionStrategy.runCurrent, not a history-scope label"
    );
    assert_eq!(
        element(&rt, "query-panel-strategy-entire").label.as_deref(),
        Some(t("query.executionStrategy.runEntire")),
        "DEFECT: React strategy ask uses query.executionStrategy.runEntire, not a history-scope label"
    );
}

#[test]
fn react_confirmed_namespace_without_database_keeps_null() {
    let mut s = vm();
    s.namespace_confirmed = true;
    s.observed_database = None;
    s.observed_schema = Some("reporting".to_owned());
    assert_eq!(
        s.selected_database(),
        None,
        "DEFECT: React returns observedNamespace.database (null) with no bound/current fallback"
    );
}
