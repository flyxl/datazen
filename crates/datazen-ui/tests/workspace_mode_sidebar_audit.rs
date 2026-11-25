//! Independent audit for T556: `WorkspaceModeSidebar` port.
//!
//! Cover progress-label phases and label precedence the coder does not drive,
//! and pin two React contracts: Math.round for download percent and truthy
//! version strings for update-button presence.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::workspace_mode_sidebar::{
    build, UpdateProgressVm, UpdateReminderVm, WorkspaceModeAction, WorkspaceModeActionFn,
    WorkspaceModeSidebarVm, NAV_DATABASES, NAV_UPDATE, ROOT_KEY, SIDEBAR_TOGGLE,
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
        sidebar_expanded: true,
        ..Default::default()
    }
}

fn view(p: &Probe, _cx: &mut Cx<Probe>) -> El {
    let actions = p.actions.clone();
    let on_action: Option<WorkspaceModeActionFn<Probe>> =
        Some(Rc::new(move |_s, action| actions.borrow_mut().push(action)));
    let mut layer = Vec::new();
    build(&p.vm, &on_action, p.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(vm: WorkspaceModeSidebarVm, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe { vm, actions: Rc::new(RefCell::new(Vec::new())), dark },
        view, size, Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn progress_label_covers_all_phases() {
    let mut s = vm();
    s.update.progress = Some(UpdateProgressVm { phase_downloading: true, downloaded: 5, total: 10, ..Default::default() });
    assert_eq!(s.progress_label(), t("settings.updater.downloading").replace("{pct}", "50"));
    s.update.progress = Some(UpdateProgressVm { phase_downloading: true, downloaded: 0, total: 0, ..Default::default() });
    assert_eq!(s.progress_label(), t("settings.updater.downloadingIndeterminate"));
    s.update.progress = Some(UpdateProgressVm { phase_installing: true, ..Default::default() });
    assert_eq!(s.progress_label(), t("settings.updater.installing"));
    assert_eq!(vm().progress_label(), t("settings.updater.sidebarInstalling"));
}

#[test]
fn update_label_precedence_and_error_title() {
    let mut s = vm();
    s.update.available_version = Some("1.0".to_owned());
    s.update.pending_restart_version = Some("2.0".to_owned());
    assert_eq!(s.update_label(), t("settings.updater.sidebarRestartReady").replace("{version}", "2.0"), "audit: pending restart wins over available");
    s.update.is_installing = true;
    s.update.progress = Some(UpdateProgressVm { phase_installing: true, ..Default::default() });
    assert_eq!(s.update_label(), t("settings.updater.installing"), "audit: installing wins over pending restart");
    s.update.error = Some("boom".to_owned());
    assert_eq!(s.update_title(), "boom", "audit: error overrides the label in the tooltip");
}

#[test]
fn collapsed_rail_geometry_and_labels() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut s = vm();
            s.sidebar_expanded = false;
            s.update.available_version = Some("1.0".to_owned());
            let rt = runtime(s, dark, (w, h));
            assert!(!has_key(&rt, &format!("{NAV_DATABASES}-label")), "audit: collapsed hides the mode label");
            assert!(!has_key(&rt, "workspace-nav-update-label"), "audit: collapsed hides the update label");
            assert_eq!(element(&rt, SIDEBAR_TOGGLE).label.as_deref(), Some(t("connWin.expandSidebar")));
            assert_eq!(element(&rt, ROOT_KEY).rect[2], 40.0, "audit: collapsed rail is 40pt");
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
            }
        }
    }
}

// ----- React parity assertions -----

#[test]
fn react_download_percent_is_rounded_not_truncated() {
    let mut s = vm();
    s.update.available_version = Some("1.0".to_owned());
    s.update.is_installing = true;
    s.update.progress = Some(UpdateProgressVm { phase_downloading: true, downloaded: 2, total: 3, ..Default::default() });
    let expected = t("settings.updater.downloading").replace("{pct}", "67");
    assert_eq!(
        s.progress_label(),
        expected,
        "DEFECT: React uses Math.round((downloaded/total)*100), not integer truncation (WorkspaceModeSidebar.tsx:114)"
    );
}

#[test]
fn react_empty_version_strings_do_not_show_the_update_button() {
    let mut s = vm();
    s.update.available_version = Some(String::new());
    assert!(
        !s.update_visible(),
        "DEFECT: React gates the reminder on truthy version strings (WorkspaceModeSidebar.tsx:181)"
    );
    let rt = runtime(s, true, MIN);
    assert!(!has_key(&rt, NAV_UPDATE), "DEFECT: an empty availableVersion must not mount the update button");
}
