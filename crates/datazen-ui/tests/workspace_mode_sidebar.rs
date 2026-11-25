//! T556: `WorkspaceModeSidebar` — mode items/active state, actions, update
//! reminder, collapse/expand and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use datazen_ui::i18n::t;
use datazen_ui::views::workspace_mode_sidebar::{
    build, UpdateProgressVm, UpdateReminderVm, WorkspaceMode, WorkspaceModeAction,
    WorkspaceModeActionFn, WorkspaceModeSidebarVm, NAV_DATABASES, NAV_DASHBOARD, NAV_EXTENSIONS,
    NAV_SETTINGS, NAV_UPDATE, NAV_WORKFLOW, NAV_WORKSPACE_PAGES, ROOT_KEY, SIDEBAR_TOGGLE,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Probe {
    vm: WorkspaceModeSidebarVm,
    actions: Rc<RefCell<Vec<WorkspaceModeAction>>>,
    dark: bool,
}

fn vm() -> WorkspaceModeSidebarVm {
    WorkspaceModeSidebarVm {
        workspace_mode: WorkspaceMode::Connections,
        sidebar_expanded: true,
        ..Default::default()
    }
}

fn scene(vm: WorkspaceModeSidebarVm, dark: bool) -> Probe {
    Probe { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(p: &Probe, _cx: &mut rune::Cx<Probe>) -> rune::El {
    let actions = p.actions.clone();
    let on_action: Option<WorkspaceModeActionFn<Probe>> =
        Some(Rc::new(move |_s, action| actions.borrow_mut().push(action)));
    let mut layer = Vec::new();
    build(&p.vm, &on_action, p.dark, &mut layer);
    rune::col().w_full().h_full().children(layer)
}

fn runtime(p: Probe, size: (f32, f32)) -> rune::Runtime<Probe> {
    let dark = p.dark;
    let mut rt = rune::Runtime::new(p, view, size, rune::Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &rune::Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click(rt: &mut rune::Runtime<Probe>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn mode_items_labels_and_actions() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    for key in [
        ROOT_KEY,
        NAV_DATABASES,
        NAV_WORKFLOW,
        NAV_DASHBOARD,
        NAV_WORKSPACE_PAGES,
        NAV_EXTENSIONS,
        NAV_SETTINGS,
        SIDEBAR_TOGGLE,
    ] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(key)), "{key} composed");
    }
    assert_eq!(element(&rt, NAV_DATABASES).label.as_deref(), Some(t("nav.databases")));
    assert_eq!(element(&rt, NAV_WORKFLOW).label.as_deref(), Some(t("nav.workflow")));
    assert_eq!(element(&rt, NAV_DASHBOARD).label.as_deref(), Some(t("nav.dashboard")));
    assert_eq!(element(&rt, NAV_WORKSPACE_PAGES).label.as_deref(), Some(t("nav.workspacePages")));
    assert_eq!(element(&rt, NAV_EXTENSIONS).label.as_deref(), Some(t("nav.extensions")));

    click(&mut rt, NAV_DATABASES);
    click(&mut rt, NAV_WORKFLOW);
    click(&mut rt, NAV_DASHBOARD);
    click(&mut rt, NAV_WORKSPACE_PAGES);
    click(&mut rt, NAV_EXTENSIONS);
    click(&mut rt, NAV_SETTINGS);
    click(&mut rt, SIDEBAR_TOGGLE);
    let actions = rt.state.actions.borrow().clone();
    assert_eq!(
        actions,
        vec![
            WorkspaceModeAction::SetMode(WorkspaceMode::Connections),
            WorkspaceModeAction::OpenWorkflow,
            WorkspaceModeAction::OpenDashboard,
            WorkspaceModeAction::SetMode(WorkspaceMode::Workspace),
            WorkspaceModeAction::SetMode(WorkspaceMode::Extension),
            WorkspaceModeAction::OpenSettings,
            WorkspaceModeAction::ToggleSidebar,
        ]
    );
}

#[test]
fn active_state_and_expanded_labels() {
    let mut state = vm();
    state.workspace_mode = WorkspaceMode::Dashboard;
    let rt = runtime(scene(state, false), MIN);
    // Active shortcode keeps its label even collapsed; inactive ones show labels in expanded mode.
    assert_eq!(element(&rt, NAV_DASHBOARD).label.as_deref(), Some(t("nav.dashboard")));
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("workspace-nav-dashboard-label")));

    let mut collapsed = vm();
    collapsed.sidebar_expanded = false;
    let rt = runtime(scene(collapsed, true), MIN);
    assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("workspace-nav-databases-label")), "collapsed hides labels");
    assert_eq!(element(&rt, SIDEBAR_TOGGLE).label.as_deref(), Some(t("connWin.expandSidebar")));
}

#[test]
fn update_reminder_label_states_and_actions() {
    let mut state = vm();
    state.update.available_version = Some("0.3.1".to_owned());
    assert!(state.update_visible());
    assert_eq!(state.update_label(), t("settings.updater.sidebarAvailable").replace("{version}", "0.3.1"));
    let mut rt = runtime(scene(state, true), DESIGN);
    assert_eq!(element(&rt, NAV_UPDATE).label.as_deref(), Some("更新 0.3.1"));
    click(&mut rt, NAV_UPDATE);
    assert_eq!(*rt.state.actions.borrow(), vec![WorkspaceModeAction::InstallUpdate]);

    let mut pending = vm();
    pending.update.pending_restart_version = Some("0.3.2".to_owned());
    assert_eq!(pending.update_label(), t("settings.updater.sidebarRestartReady").replace("{version}", "0.3.2"));
    let mut rt = runtime(scene(pending, false), MIN);
    click(&mut rt, NAV_UPDATE);
    assert_eq!(*rt.state.actions.borrow(), vec![WorkspaceModeAction::RestartToApply]);

    let mut installing = vm();
    installing.update.available_version = Some("0.3.1".to_owned());
    installing.update.is_installing = true;
    installing.update.progress = Some(UpdateProgressVm {
        phase_downloading: true,
        downloaded: 5,
        total: 10,
        ..Default::default()
    });
    assert_eq!(installing.update_label(), t("settings.updater.downloading").replace("{pct}", "50"));
    let rt = runtime(scene(installing, true), MIN);
    assert!(!element(&rt, NAV_UPDATE).focusable, "installing blocks repeat clicks");

    let mut failed = vm();
    failed.update.available_version = Some("0.3.1".to_owned());
    failed.update.error = Some("signature check failed".to_owned());
    let rt = runtime(scene(failed, false), MIN);
    assert_eq!(element(&rt, NAV_UPDATE).help.as_deref(), Some("signature check failed"));
}

#[test]
fn geometry_both_sizes_both_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut state = vm();
            state.update.available_version = Some("0.3.1".to_owned());
            let rt = runtime(scene(state, dark), (w, h));
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h} dark={dark}: {:?} {:?} horizontal", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{w}x{h} dark={dark}: {:?} {:?} vertical", el.key, el.rect);
            }
        }
    }
}
