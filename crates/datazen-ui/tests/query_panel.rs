//! T520: `QueryPanel` — pane routing, execute/cancel, results-host mount,
//! context inference, session banner, hosted dialogs and rect geometry at both
//! window sizes in both themes. Every command is a recorded [`QueryPanelAction`];
//! no fake IPC, store or timer. Cases are grouped to keep the test binary small.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::query_panel::{
    build, pane_key, resolve_focused_pane_id, CancelAvailability, MessageDialogVm, MessageKind,
    PanelSessionState, QueryPanelAction, QueryPanelActionFn, QueryPanelVm, EDITOR_KEY, EXECUTE_KEY,
    MESSAGE_DIALOG_KEY, MESSAGE_OK_KEY, NEW_SESSION_KEY, RESULTS_HOST_KEY, ROOT_KEY,
    SESSION_BANNER_KEY, SIDEBAR_KEY, STOP_KEY,
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
        panel_id: "panel-test".to_owned(),
        focused_pane_id: Some("p2".to_owned()),
        database: Some("app".to_owned()),
        current_database: Some("app".to_owned()),
        current_schema: Some("public".to_owned()),
        sql: "SELECT 222222".to_owned(),
        cancel_capability: Some(true),
        execution_seq: 3,
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

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn pane_routing_and_root_regions() {
    assert_eq!(pane_key("panel-test", Some("p2")), "panel-test::p2");
    assert_eq!(pane_key("panel-test", None), "panel-test");
    assert_eq!(pane_key("panel-test", Some("")), "panel-test");
    assert_eq!(resolve_focused_pane_id(Some("p2")), "p2");
    assert_eq!(resolve_focused_pane_id(None), "");

    let rt = runtime(scene(vm(), true), DESIGN);
    for key in [ROOT_KEY, EDITOR_KEY, SIDEBAR_KEY, "query-panel-editor-column", EXECUTE_KEY] {
        assert!(has_key(&rt, key), "{key} is composed");
    }
    assert_eq!(element(&rt, ROOT_KEY).help.as_deref(), Some("seq=3;running=false"));
    assert!(!has_key(&rt, RESULTS_HOST_KEY), "no results host while idle");

    let mut running = vm();
    running.running = true;
    running.execution_id = Some("exec-1".to_owned());
    let rt = runtime(scene(running, false), MIN);
    assert!(has_key(&rt, RESULTS_HOST_KEY));
    assert_eq!(element(&rt, ROOT_KEY).help.as_deref(), Some("seq=3;running=true"));
}

#[test]
fn execute_cancel_journey() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    assert_eq!(element(&rt, EXECUTE_KEY).label.as_deref(), Some(t("query.execute")));
    click(&mut rt, EXECUTE_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![QueryPanelAction::Execute { pane: Some("p2".to_owned()) }]);

    let mut running = vm();
    running.running = true;
    running.execution_id = Some("exec-1".to_owned());
    let mut rt = runtime(scene(running, false), MIN);
    assert!(!has_key(&rt, EXECUTE_KEY), "execute is replaced by stop while running");
    assert_eq!(element(&rt, STOP_KEY).label.as_deref(), Some(t("query.stop")));
    click(&mut rt, STOP_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![QueryPanelAction::Cancel { pane: Some("p2".to_owned()) }]);

    let mut no_id = vm();
    no_id.running = true;
    assert_eq!(no_id.cancel_availability(), CancelAvailability::StopDisabled);
    assert!(!no_id.can_cancel());
    assert_eq!(element(&runtime(scene(no_id, true), MIN), STOP_KEY).label.as_deref(), Some(t("query.stop")));

    let mut unsupported = vm();
    unsupported.running = true;
    unsupported.execution_id = Some("e".to_owned());
    unsupported.cancel_capability = Some(false);
    assert_eq!(element(&runtime(scene(unsupported, false), MIN), STOP_KEY).label.as_deref(), Some(t("query.cancelUnavailable")));
}

#[test]
fn context_inference_and_session_banner() {
    let mut state = vm();
    assert_eq!(state.selected_database().as_deref(), Some("app"));
    assert_eq!(state.selected_schema().as_deref(), Some("public"));
    state.namespace_confirmed = true;
    state.observed_database = Some("analytics".to_owned());
    state.observed_schema = Some("reporting".to_owned());
    assert_eq!(state.selected_database().as_deref(), Some("analytics"));
    assert_eq!(state.selected_schema().as_deref(), Some("reporting"));
    state.namespace_confirmed = false;
    state.is_path_hierarchy = true;
    state.current_database = Some("warehouse".to_owned());
    assert_eq!(state.selected_database().as_deref(), Some("warehouse"));
    state.database = Some("   ".to_owned());
    state.current_database = None;
    assert_eq!(state.selected_database(), None);

    let mut disconnected = vm();
    disconnected.session_state = PanelSessionState::Lost;
    assert!(disconnected.session_disconnected());
    let rt = runtime(scene(disconnected.clone(), true), MIN);
    assert_eq!(element(&rt, NEW_SESSION_KEY).label.as_deref(), Some(t("query.session.openNew")));
    assert!(has_key(&rt, SESSION_BANNER_KEY));
    let mut rt = runtime(scene(disconnected, false), MIN);
    click(&mut rt, NEW_SESSION_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![QueryPanelAction::OpenNewSession]);
    assert!(!has_key(&runtime(scene(vm(), true), MIN), SESSION_BANNER_KEY));
}

#[test]
fn hosted_dialogs_and_geometry() {
    let mut state = vm();
    state.show_message("query.postgresDoubleQuoteHint", MessageKind::Error);
    assert!(state.message.open);
    let mut rt = runtime(scene(state, true), DESIGN);
    assert!(has_key(&rt, MESSAGE_DIALOG_KEY));
    assert_eq!(element(&rt, "result-message-text-label").text.as_deref(), Some("query.postgresDoubleQuoteHint"));
    click(&mut rt, MESSAGE_OK_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![QueryPanelAction::CloseMessage]);

    let mut success = vm();
    success.show_message(t("query.refreshCompletionDone"), MessageKind::Success);
    assert_eq!(element(&runtime(scene(success, false), MIN), MESSAGE_DIALOG_KEY).label.as_deref(), Some(t("common.success")));
    let mut closed = vm();
    closed.show_message("x", MessageKind::Error);
    closed.close_message();
    assert!(!has_key(&runtime(scene(closed, true), MIN), MESSAGE_DIALOG_KEY));

    let mut dangerous = vm();
    dangerous.dangerous_confirm_open = true;
    let mut rt = runtime(scene(dangerous, true), DESIGN);
    click(&mut rt, "query-panel-dangerous-confirm-confirm");
    assert!(rt.state.actions.borrow().contains(&QueryPanelAction::ConfirmDangerous));

    let mut strategy = vm();
    strategy.strategy_ask_open = true;
    let mut rt = runtime(scene(strategy, true), DESIGN);
    click(&mut rt, "query-panel-strategy-entire");
    assert!(rt.state.actions.borrow().contains(&QueryPanelAction::ChooseStrategy { entire_script: true }));

    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut state = vm();
            state.running = true;
            state.execution_id = Some("exec-1".to_owned());
            state.session_state = PanelSessionState::Lost;
            state.message = MessageDialogVm {
                open: true,
                kind: MessageKind::Error,
                text: "query.postgresDoubleQuoteHint".to_owned(),
            };
            let rt = runtime(scene(state, dark), (width, height));
            for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, w, h] = element.rect;
                assert!(x >= -0.5 && x + w <= width + 0.5, "{width}x{height} dark={dark}: {:?} {:?} horizontal", element.key, element.rect);
                assert!(y >= -0.5 && y + h <= height + 0.5, "{width}x{height} dark={dark}: {:?} {:?} vertical", element.key, element.rect);
            }
        }
    }
}
