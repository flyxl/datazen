//! Independent audit for T018 (the AddToDashboardDialog Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/dashboard/AddToDashboardDialog.tsx` and its
//! React test `__tests__/AddToDashboardDialog.test.tsx`: nothing while
//! closed, the list load gating the confirm button, the first dashboard
//! preselected, `onConfirm('d1')` from the picker, the create-new branch and
//! `onConfirm('new', 'My Panel')`, the blank-name default, and the empty-list
//! straight-to-create path. Element evidence comes from
//! `Runtime::elements()` (runtime layout), never from a value the component
//! reports about itself; presses go through real pointer events.
//!
//! The state-machine transitions are additionally mutation-proved
//! out-of-tree (the scratch crate, runbook §3.4); this file pins the same
//! behaviour at the runtime level plus the callback wiring.

use rune::{Cx, El, Env, Runtime, TextEdit};

use crate::views::dashboard::add_to_dashboard_dialog::{
    AddToDashboardDialog, AddToDashboardState, AddToDashboardTarget, DashboardOptionVm,
};

fn board(id: &str, name: &str) -> DashboardOptionVm {
    DashboardOptionVm { id: id.to_owned(), name: name.to_owned() }
}

fn boards() -> Vec<DashboardOptionVm> {
    vec![board("d1", "Board A"), board("d2", "Board B")]
}

#[derive(Clone)]
struct AuditState {
    dialog: AddToDashboardState,
    dark: bool,
    closes: usize,
    confirms: Vec<AddToDashboardTarget>,
    selects: Vec<String>,
    begins_new: usize,
    backs: usize,
}

impl AuditState {
    fn closed() -> Self {
        Self {
            dialog: AddToDashboardState::default(),
            dark: true,
            closes: 0,
            confirms: Vec::new(),
            selects: Vec::new(),
            begins_new: 0,
            backs: 0,
        }
    }

    fn loading() -> Self {
        let mut scene = Self::closed();
        scene.dialog.begin_open();
        scene
    }

    fn with_list() -> Self {
        let mut scene = Self::loading();
        assert!(scene.dialog.deliver_list(boards(), scene.dialog.request_epoch));
        scene
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    AddToDashboardDialog::<AuditState>::new("add-to-dashboard", &s.dialog)
        .on_close(|s: &mut AuditState| {
            s.closes += 1;
            s.dialog.close();
        })
        .on_select(|s: &mut AuditState, id: String| {
            s.selects.push(id.clone());
            s.dialog.select(&id);
        })
        .on_begin_new(|s: &mut AuditState| {
            s.begins_new += 1;
            s.dialog.begin_create_new();
        })
        .on_back_existing(|s: &mut AuditState| {
            s.backs += 1;
            s.dialog.back_to_existing();
        })
        .on_confirm(|s: &mut AuditState, target: AddToDashboardTarget| {
            s.confirms.push(target);
            s.dialog.close();
        })
        .on_edit_name(|s: &mut AuditState| &mut s.dialog.new_name)
        .build(s.dark, &mut layer);
    // The layer children ride the root so elements() sees them.
    let mut root = rune::col().key("probe");
    for child in layer {
        root = root.child(child);
    }
    root
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<AuditState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

// ---- the state machine (React's useState + the open effect) --------------

#[test]
fn begin_opens_the_load_and_prefills_the_default_name() {
    let mut state = AddToDashboardState::default();
    state.begin_open();
    assert!(state.open);
    assert!(state.loading, "the open effect starts the spinner");
    assert!(!state.creating_new, "the effect leaves the create branch");
    assert_eq!(state.new_name.text, "新看板", "t('dashboard.defaultName')");
    assert_eq!(state.request_epoch, state.open_epoch, "the load is the open's epoch");
    // Reopen bumps the token and resets the same three.
    state.creating_new = true;
    let first_epoch = state.open_epoch;
    state.begin_open();
    assert_eq!(state.open_epoch, first_epoch + 1);
    assert!(!state.creating_new);
    assert!(state.loading);
}

#[test]
fn a_delivery_preselects_the_first_dashboard() {
    let mut state = AddToDashboardState::default();
    state.begin_open();
    let token = state.request_epoch;
    assert!(state.deliver_list(boards(), token));
    assert!(!state.loading);
    assert_eq!(state.selection, "d1", "React setSelection(list[0]!.id)");
    assert_eq!(state.dashboards.len(), 2);
}

#[test]
fn an_empty_list_goes_straight_to_create_new() {
    let mut state = AddToDashboardState::default();
    state.begin_open();
    assert!(state.deliver_list(Vec::new(), state.request_epoch));
    assert!(state.creating_new, "no dashboards -> creatingNew");
    assert_eq!(state.selection, "");
    assert!(!state.loading);
}

#[test]
fn a_stale_or_closed_delivery_is_dropped() {
    // React's `cancelled` flag: the effect cleanup on close/reopen.
    let mut state = AddToDashboardState::default();
    state.begin_open();
    let stale = state.request_epoch;
    state.begin_open(); // a second open supersedes the first load
    assert!(!state.deliver_list(boards(), stale), "the superseded load lands nothing");
    assert!(state.dashboards.is_empty());
    assert!(state.loading, "the current load still pending");

    let current = state.request_epoch;
    state.close();
    assert!(!state.deliver_list(boards(), current), "a delivery after close lands nothing");
    assert!(state.dashboards.is_empty());
}

#[test]
fn back_to_existing_needs_a_list_and_resets_the_selection() {
    let mut state = AddToDashboardState::default();
    state.begin_open();
    state.deliver_list(boards(), state.request_epoch);
    state.select("d2"); // NOT the first — so a reset is observable
    state.begin_create_new();
    state.back_to_existing();
    assert!(!state.creating_new);
    assert_eq!(state.selection, "d1", "React setSelection(dashboards[0]!.id)");
    // With no list the link cannot leave the create branch.
    let mut empty = AddToDashboardState::default();
    empty.begin_open();
    empty.deliver_list(Vec::new(), empty.request_epoch);
    empty.back_to_existing();
    assert!(empty.creating_new, "nothing to select -> stay creating");
}

#[test]
fn can_confirm_is_reacts_disabled_expression() {
    let mut state = AddToDashboardState::default();
    state.begin_open();
    assert!(!state.can_confirm(), "loading disables");

    state.deliver_list(boards(), state.request_epoch);
    assert!(state.can_confirm(), "a selection enables");

    state.begin_create_new();
    // React prefills the default name at open, so the create branch starts
    // ENABLED — only a cleared input disables the button.
    assert!(state.can_confirm(), "the prefilled default name enables");
    state.new_name = TextEdit::new(String::new());
    assert!(!state.can_confirm(), "a cleared name disables");
    state.new_name = TextEdit::new("   ".to_owned());
    assert!(!state.can_confirm(), "whitespace-only name disables");
    state.new_name = TextEdit::new("My Panel".to_owned());
    assert!(state.can_confirm(), "a name enables");

    state.selection = String::new();
    state.back_to_existing(); // restores the first id
    assert!(state.can_confirm());

    // The defensive third clause: the picker branch with no selection (a
    // state only a partial host delivery could produce) still disables.
    state.creating_new = false;
    state.loading = false;
    state.selection = String::new();
    assert!(!state.can_confirm(), "no selection disables");
}

#[test]
fn confirm_carries_both_payload_shapes() {
    let mut state = AddToDashboardState::default();
    state.begin_open();
    state.deliver_list(boards(), state.request_epoch);
    assert_eq!(
        state.confirm(),
        AddToDashboardTarget::Existing { id: "d1".to_owned() },
        "React onConfirm(selection)"
    );

    state.begin_create_new();
    state.new_name = TextEdit::new("  My Panel  ".to_owned());
    assert_eq!(
        state.confirm(),
        AddToDashboardTarget::New { name: "My Panel".to_owned() },
        "React trims and forwards"
    );

    state.new_name = TextEdit::new(String::new());
    assert_eq!(
        state.confirm(),
        AddToDashboardTarget::New { name: "新看板".to_owned() },
        "blank falls back to t('dashboard.defaultName')"
    );
}

// ---- runtime evidence (element tree, presses) ------------------------------

#[test]
fn closed_paints_nothing_at_all() {
    // React test 1: container.innerHTML === ''.
    let rt = rt(AuditState::closed());
    assert!(maybe(&rt, "add-to-dashboard").is_none(), "no panel while closed");
    assert!(maybe(&rt, "add-to-dashboard-confirm").is_none());
    assert!(maybe(&rt, "add-to-dashboard-dialog").is_none());
}

#[test]
fn the_load_line_shows_while_list_dashboards_is_pending() {
    let rt = rt(AuditState::loading());
    assert!(has(&rt, "add-to-dashboard-loading"), "the spinner line");
    assert!(has(&rt, "add-to-dashboard"), "the panel itself shows");
    assert!(!has(&rt, "dashboard-target-select"), "no picker while loading");
    // And the confirm button is disabled: rt.find still locates it.
    let confirm = maybe(&rt, "add-to-dashboard-confirm").expect("the button is laid out");
    assert!(!confirm.focusable, "a disabled kit button is not focusable");
}

#[test]
fn the_picker_lists_dashboards_and_confirms_the_first() {
    // React test 2: the list renders and the confirm fires with 'd1'.
    let mut rt = rt(AuditState::with_list());
    assert!(has(&rt, "add-to-dashboard-dialog"), "the body root");
    assert!(has(&rt, "dashboard-target-select"), "the picker trigger");
    assert!(has(&rt, "dashboard-target-new"), "the create-new link");

    press(&mut rt, "add-to-dashboard-confirm");
    let scene = rt.state;
    assert_eq!(
        scene.confirms,
        vec![AddToDashboardTarget::Existing { id: "d1".to_owned() }]
    );
    assert!(!scene.dialog.open, "confirming closes (the host's onConfirm did)");
}

#[test]
fn the_create_branch_confirms_a_new_panel_by_name() {
    // React test 3: create-new -> type a name -> onConfirm('new', 'My Panel').
    let mut scene = AuditState::with_list();
    scene.dialog.begin_create_new();
    scene.dialog.new_name = TextEdit::new("My Panel".to_owned());
    let mut rt = rt(scene);

    assert!(has(&rt, "dashboard-new-panel-name"), "the name field");
    assert!(has(&rt, "add-to-dashboard-select-existing"), "the back link (list non-empty)");
    assert!(!has(&rt, "dashboard-target-select"), "the picker is gone");

    press(&mut rt, "add-to-dashboard-confirm");
    assert_eq!(
        rt.state.confirms,
        vec![AddToDashboardTarget::New { name: "My Panel".to_owned() }]
    );
}

#[test]
fn the_create_branch_link_presses_reach_the_state() {
    // The two accent links and the cancel button wire through real presses.
    let mut rt = rt(AuditState::with_list());
    press(&mut rt, "dashboard-target-new");
    assert_eq!(rt.state.begins_new, 1);
    assert!(rt.state.dialog.creating_new);

    press(&mut rt, "add-to-dashboard-select-existing");
    assert_eq!(rt.state.backs, 1);
    assert!(!rt.state.dialog.creating_new);
    assert_eq!(rt.state.dialog.selection, "d1");

    press(&mut rt, "add-to-dashboard-cancel");
    assert_eq!(rt.state.closes, 1);
    assert!(!rt.state.dialog.open);
}

#[test]
fn an_empty_list_renders_the_create_branch_without_a_back_link() {
    // (Both runtimes are built before the `rt` binding shadows the
    // module-level constructor.)
    let mut cleared = scene_cleared_name();
    assert!(cleared.dialog.deliver_list(Vec::new(), cleared.dialog.request_epoch));
    let cleared_rt = rt(cleared);

    let mut scene = AuditState::loading();
    assert!(scene.dialog.deliver_list(Vec::new(), scene.dialog.request_epoch));
    let rt = rt(scene);

    assert!(has(&rt, "dashboard-new-panel-name"));
    assert!(
        !has(&rt, "add-to-dashboard-select-existing"),
        "React {{dashboards.length > 0}} hides the link"
    );
    // The prefilled default name keeps confirm enabled in the create
    // branch (React prefills `t('dashboard.defaultName')` at open); a
    // cleared input is what disables it.
    let confirm = maybe(&rt, "add-to-dashboard-confirm").expect("laid out");
    assert!(confirm.focusable, "the prefilled default name enables");
    let confirm = maybe(&cleared_rt, "add-to-dashboard-confirm").expect("laid out");
    assert!(!confirm.focusable, "a cleared input disables");
}

fn scene_cleared_name() -> AuditState {
    let mut scene = AuditState::closed();
    scene.dialog.begin_open();
    scene.dialog.new_name = TextEdit::new(String::new());
    scene
}

#[test]
fn the_panel_fits_the_960_window_in_both_themes() {
    for dark in [true, false] {
        let mut scene = AuditState::with_list();
        scene.dark = dark;
        let rt = rt(scene);
        let panel = maybe(&rt, "add-to-dashboard").expect("the panel is keyed");
        let [x, y, w, h] = panel.rect;
        assert!(x >= -0.5 && y >= -0.5, "on-screen at {x},{y} (dark={dark})");
        assert!(x + w <= 960.5 && y + h <= 640.5, "inside the window (dark={dark})");
    }
}
