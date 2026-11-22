use super::*;
use crate::WorkspaceMode;

fn t(key: &str) -> String {
    format!("T:{key}")
}

fn state() -> UpdateState {
    UpdateState::default()
}

#[test]
fn surface_gap_and_literals_are_pinned() {
    assert_eq!(WORKSPACE_MODE_SIDEBAR_SURFACE.len(), 8);
    assert_eq!(WORKSPACE_MODE_SIDEBAR_GAP.len(), 6);
    for (mode, literal) in [
        (WorkspaceMode::Connections, "connections"),
        (WorkspaceMode::Workflow, "workflow"),
        (WorkspaceMode::Dashboard, "dashboard"),
        (WorkspaceMode::Workspace, "workspace"),
        (WorkspaceMode::Extension, "extension"),
    ] {
        assert_eq!(mode.as_str(), literal);
    }
    assert_eq!(NavButtonId::Update.as_str(), "workspace-nav-update");
    assert_eq!(NavButtonId::SidebarToggle.as_str(), "workspace-sidebar-toggle");
}

#[test]
fn the_five_nav_buttons_keep_their_order_icon_and_action() {
    let view = build_sidebar_view(WorkspaceMode::Dashboard, false, &state(), &t);
    assert_eq!(view.nav_buttons.len(), 5);
    let expected = [
        (NavButtonId::Databases, "nav.databases", false),
        (NavButtonId::Workflow, "action.workflow", false),
        (NavButtonId::Dashboard, "action.dashboard", true),
        (NavButtonId::WorkspacePages, "nav.workspacePages", false),
        (NavButtonId::Extensions, "nav.extensions", false),
    ];
    for (button, (id, icon, active)) in view.nav_buttons.iter().zip(expected) {
        assert_eq!(button.id, id);
        assert_eq!(button.icon_id, icon);
        assert_eq!(button.active, active);
        assert_eq!(button.test_id, id.as_str());
        assert_eq!(button.expanded, false);
        assert_eq!(button.action, dispatch_nav_action(id).unwrap_or(NavAction::ToggleSidebar));
    }
}

#[test]
fn button_and_sidebar_classes_follow_expansion_and_active_state() {
    let collapsed = build_sidebar_view(WorkspaceMode::Connections, false, &state(), &t);
    assert_eq!(collapsed.class, format!("{SIDEBAR_BASE} w-10"));
    assert_eq!(
        collapsed.nav_buttons[0].class,
        format!("{BUTTON_BASE} {BUTTON_COLLAPSED} {BUTTON_ACTIVE}")
    );
    let expanded = build_sidebar_view(WorkspaceMode::Connections, true, &state(), &t);
    assert_eq!(expanded.class, format!("{SIDEBAR_BASE} w-28"));
    assert_eq!(
        expanded.nav_buttons[1].class,
        format!("{BUTTON_BASE} {BUTTON_EXPANDED} {BUTTON_INACTIVE}")
    );
    assert_eq!(expanded.settings_button.id, NavButtonId::Settings);
    assert_eq!(expanded.settings_button.icon_id, "nav.settings");
    assert_eq!(expanded.settings_button.action, NavAction::OpenSettings);
}

#[test]
fn the_toggle_button_uses_the_opposite_label_when_expanded() {
    let collapsed = build_sidebar_view(WorkspaceMode::Workspace, false, &state(), &t);
    assert_eq!(collapsed.toggle_button.label, "T:connWin.expandSidebar");
    assert_eq!(collapsed.toggle_button.action, NavAction::ToggleSidebar);
    let expanded = build_sidebar_view(WorkspaceMode::Workspace, true, &state(), &t);
    assert_eq!(expanded.toggle_button.label, "T:connWin.collapseSidebar");
}

#[test]
fn update_button_is_absent_until_a_version_exists() {
    let view = build_sidebar_view(WorkspaceMode::Connections, false, &state(), &t);
    assert_eq!(view.update_button, None);
    assert_eq!(dispatch_nav_action(NavButtonId::Update), None);
}

#[test]
fn available_and_pending_versions_pick_the_right_label_and_action() {
    let mut update = state();
    update.available_version = Some("1.2.0".to_owned());
    let view = build_sidebar_view(WorkspaceMode::Connections, true, &update, &t);
    let button = match view.update_button {
        Some(value) => value,
        None => panic!("an available version must show the button"),
    };
    assert_eq!(button.aria_label, "T:settings.updater.sidebarAvailable");
    assert_eq!(button.title, button.aria_label);
    assert!(!button.disabled);
    assert_eq!(button.action, UpdateAction::Install);
    assert_eq!(button.icon_kind, UpdateIconKind::Download);
    assert!(button.show_badge);
    assert!(button.expanded);
    assert!(button.class.contains(BUTTON_EXPANDED));

    update.pending_restart_version = Some("1.2.0".to_owned());
    let button = match build_update_view(&update, false, &t) {
        Some(value) => value,
        None => panic!("a pending restart must show the button"),
    };
    assert_eq!(button.aria_label, "T:settings.updater.sidebarRestartReady");
    assert_eq!(button.action, UpdateAction::Restart);
    assert_eq!(button.icon_kind, UpdateIconKind::Restart);
    assert!(button.class.contains(BUTTON_COLLAPSED));
}

#[test]
fn errors_override_the_title_only() {
    let mut update = state();
    update.available_version = Some("2.0.0".to_owned());
    update.error = Some("offline".to_owned());
    let button = match build_update_view(&update, false, &t) {
        Some(value) => value,
        None => panic!("expected a button"),
    };
    assert_eq!(button.title, "offline");
    assert_eq!(button.aria_label, "T:settings.updater.sidebarAvailable");
}

#[test]
fn progress_labels_cover_download_install_and_idle() {
    let mut update = state();
    update.progress = Some(UpdateProgress {
        phase: UpdateProgressPhase::Downloading,
        downloaded: 3,
        total: Some(4),
        ..Default::default()
    });
    assert_eq!(progress_label(&update, &t), "T:settings.updater.downloading");
    update.progress = Some(UpdateProgress {
        phase: UpdateProgressPhase::Downloading,
        downloaded: 3,
        total: None,
        ..Default::default()
    });
    assert_eq!(
        progress_label(&update, &t),
        "T:settings.updater.downloadingIndeterminate"
    );
    update.progress = Some(UpdateProgress {
        phase: UpdateProgressPhase::Installing,
        ..Default::default()
    });
    assert_eq!(progress_label(&update, &t), "T:settings.updater.installing");
    assert_eq!(progress_label(&state(), &t), "T:settings.updater.sidebarInstalling");
}

#[test]
fn an_installing_button_disables_clicks_and_shows_the_ring() {
    let mut update = state();
    update.pending_restart_version = Some("3.0.0".to_owned());
    update.is_installing = true;
    update.progress = Some(UpdateProgress {
        phase: UpdateProgressPhase::Downloading,
        downloaded: 1,
        total: Some(2),
        ..Default::default()
    });
    let button = match build_update_view(&update, false, &t) {
        Some(value) => value,
        None => panic!("expected a button"),
    };
    assert!(button.disabled);
    assert!(!button.show_badge);
    assert_eq!(button.aria_label, "T:settings.updater.downloading");
    assert_eq!(button.icon_kind, UpdateIconKind::Spinner);
    let ring = match button.ring {
        Some(value) => value,
        None => panic!("an installing button must show the ring"),
    };
    assert_eq!(ring.size, 16.0);
    assert_eq!(ring.stroke, 2.0);
    assert_eq!(ring.radius, 7.0);
    assert_eq!(ring.percent, Some(50.0));
    assert!(!ring.spin);

    let indeterminate = progress_ring(Some(&UpdateProgress::default()));
    assert_eq!(indeterminate.percent, None);
    assert!(indeterminate.spin);
    assert_eq!(indeterminate.dash_offset, indeterminate.circumference * 0.75);
}

#[test]
fn a_restart_prompt_is_raised_once_per_version() {
    let mut update = state();
    update.pending_restart_version = Some("4.0.0".to_owned());
    let prompt = match resolve_restart_prompt(&update, None, &t) {
        Some(value) => value,
        None => panic!("a new pending version must prompt"),
    };
    assert_eq!(prompt.title, "T:settings.updater.title");
    assert_eq!(prompt.confirm_label, "T:settings.updater.restartNow");
    assert_eq!(prompt.cancel_label, "T:settings.updater.restartLater");
    assert_eq!(prompt.kind, "info");
    assert_eq!(resolve_restart_prompt(&update, Some("4.0.0"), &t), None);
    assert_eq!(resolve_confirm_result(&prompt, true), Some(UpdateAction::Restart));
    assert_eq!(resolve_confirm_result(&prompt, false), None);
    assert_eq!(resolve_restart_prompt(&state(), None, &t), None);
}

#[test]
fn crate_root_reexports_the_sidebar_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::SidebarView>();
    resolves::<crate::NavButton>();
    resolves::<crate::WorkspaceMode>();
    assert_eq!(crate::WORKSPACE_MODE_SIDEBAR_SURFACE.len(), 8);
    assert_eq!(crate::NavButtonId::Settings.as_str(), "workspace-nav-settings");
}
