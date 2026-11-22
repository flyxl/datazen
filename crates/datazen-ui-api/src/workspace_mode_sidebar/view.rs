use super::types::*;

pub const SIDEBAR_BASE: &str =
    "flex h-full shrink-0 flex-col self-stretch border-r border-edge bg-surface-alt";
pub const BUTTON_BASE: &str = "flex h-10 w-full items-center text-xs transition-colors";
pub const BUTTON_EXPANDED: &str = "justify-start gap-2.5 px-3";
pub const BUTTON_COLLAPSED: &str = "justify-center";
pub const BUTTON_ACTIVE: &str = "bg-accent/20 text-accent";
pub const BUTTON_INACTIVE: &str = "text-fg-secondary hover:bg-surface-raised hover:text-fg";
pub const UPDATE_BUTTON_BASE: &str =
    "flex h-10 w-full items-center text-xs transition-colors disabled:cursor-wait";
pub const UPDATE_BUTTON_TONE: &str = " text-accent hover:bg-accent/10";

fn button_class(expanded: bool, active: bool) -> String {
    let mut class = BUTTON_BASE.to_owned();
    class.push(' ');
    class.push_str(if expanded {
        BUTTON_EXPANDED
    } else {
        BUTTON_COLLAPSED
    });
    class.push(' ');
    class.push_str(if active {
        BUTTON_ACTIVE
    } else {
        BUTTON_INACTIVE
    });
    class
}

fn update_button_class(expanded: bool) -> String {
    format!(
        "{}{} {}{}",
        UPDATE_BUTTON_BASE,
        "",
        if expanded {
            BUTTON_EXPANDED
        } else {
            BUTTON_COLLAPSED
        },
        UPDATE_BUTTON_TONE
    )
}

pub fn progress_label(state: &UpdateState, t: &dyn Fn(&str) -> String) -> String {
    match state.progress {
        Some(progress) if progress.phase == UpdateProgressPhase::Downloading => {
            match progress.total.filter(|total| *total > 0) {
                Some(total) => {
                    let raw = (progress.downloaded as f64 / total as f64) * 100.0;
                    let pct = raw.round().min(100.0) as u64;
                    t("settings.updater.downloading").replace("{pct}", &pct.to_string())
                }
                None => t("settings.updater.downloadingIndeterminate"),
            }
        }
        Some(progress) if progress.phase == UpdateProgressPhase::Installing => {
            t("settings.updater.installing")
        }
        _ => t("settings.updater.sidebarInstalling"),
    }
}

pub fn update_label(state: &UpdateState, t: &dyn Fn(&str) -> String) -> String {
    if state.is_installing {
        return progress_label(state, t);
    }
    if let Some(pending) = &state.pending_restart_version {
        return t("settings.updater.sidebarRestartReady").replace("{version}", pending);
    }
    let fallback = state
        .available_version
        .as_deref()
        .or(state.pending_restart_version.as_deref())
        .unwrap_or("");
    t("settings.updater.sidebarAvailable").replace("{version}", fallback)
}

pub fn progress_ring(progress: Option<&UpdateProgress>) -> ProgressRing {
    let size = 16.0;
    let stroke = 2.0;
    let radius = (size - stroke) / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;
    let percent = progress.and_then(|state| {
        if state.phase == UpdateProgressPhase::Downloading {
            state.total.filter(|total| *total > 0).map(|total| {
                ((state.downloaded as f64 / total as f64) * 100.0)
                    .max(0.0)
                    .min(100.0)
            })
        } else {
            None
        }
    });
    let dash_offset = match percent {
        Some(pct) => circumference * (1.0 - pct / 100.0),
        None => circumference * 0.75,
    };
    ProgressRing {
        size,
        stroke,
        radius,
        circumference,
        percent,
        spin: percent.is_none(),
        dash_offset,
    }
}

pub fn build_update_view(
    state: &UpdateState,
    expanded: bool,
    t: &dyn Fn(&str) -> String,
) -> Option<UpdateButtonView> {
    if state.available_version.is_none() && state.pending_restart_version.is_none() {
        return None;
    }
    let label = update_label(state, t);
    let title = state.error.clone().unwrap_or_else(|| label.clone());
    let icon_kind = if state.is_installing {
        UpdateIconKind::Spinner
    } else if state.pending_restart_version.is_some() {
        UpdateIconKind::Restart
    } else {
        UpdateIconKind::Download
    };
    Some(UpdateButtonView {
        test_id: NavButtonId::Update,
        class: update_button_class(expanded),
        aria_label: label,
        title,
        disabled: state.is_installing,
        action: if state.pending_restart_version.is_some() {
            UpdateAction::Restart
        } else {
            UpdateAction::Install
        },
        expanded,
        show_badge: !state.is_installing,
        ring: if state.is_installing {
            Some(progress_ring(state.progress.as_ref()))
        } else {
            None
        },
        icon_kind,
    })
}

fn nav_button(
    id: NavButtonId,
    icon_id: &str,
    label: String,
    active: bool,
    expanded: bool,
    action: NavAction,
) -> NavButton {
    NavButton {
        id,
        class: button_class(expanded, active),
        icon_id: icon_id.to_owned(),
        label,
        active,
        expanded,
        action,
        test_id: id.as_str().to_owned(),
    }
}

pub fn build_sidebar_view(
    mode: WorkspaceMode,
    expanded: bool,
    update_state: &UpdateState,
    t: &dyn Fn(&str) -> String,
) -> SidebarView {
    let class = format!(
        "{}{}",
        SIDEBAR_BASE,
        if expanded { " w-28" } else { " w-10" }
    );
    let nav_buttons = vec![
        nav_button(
            NavButtonId::Databases,
            "nav.databases",
            t("nav.databases"),
            mode == WorkspaceMode::Connections,
            expanded,
            NavAction::SetWorkspaceMode(WorkspaceMode::Connections),
        ),
        nav_button(
            NavButtonId::Workflow,
            "action.workflow",
            t("nav.workflow"),
            mode == WorkspaceMode::Workflow,
            expanded,
            NavAction::OpenWorkflow,
        ),
        nav_button(
            NavButtonId::Dashboard,
            "action.dashboard",
            t("nav.dashboard"),
            mode == WorkspaceMode::Dashboard,
            expanded,
            NavAction::OpenDashboard,
        ),
        nav_button(
            NavButtonId::WorkspacePages,
            "nav.workspacePages",
            t("nav.workspacePages"),
            mode == WorkspaceMode::Workspace,
            expanded,
            NavAction::SetWorkspaceMode(WorkspaceMode::Workspace),
        ),
        nav_button(
            NavButtonId::Extensions,
            "nav.extensions",
            t("nav.extensions"),
            mode == WorkspaceMode::Extension,
            expanded,
            NavAction::SetWorkspaceMode(WorkspaceMode::Extension),
        ),
    ];
    let settings_button = nav_button(
        NavButtonId::Settings,
        "nav.settings",
        t("nav.settings"),
        false,
        expanded,
        NavAction::OpenSettings,
    );
    let toggle_label = if expanded {
        t("connWin.collapseSidebar")
    } else {
        t("connWin.expandSidebar")
    };
    let toggle_button = nav_button(
        NavButtonId::SidebarToggle,
        "nav.sidebarToggle",
        toggle_label,
        false,
        expanded,
        NavAction::ToggleSidebar,
    );
    SidebarView {
        class,
        expanded,
        nav_buttons,
        update_button: build_update_view(update_state, expanded, t),
        settings_button,
        toggle_button,
    }
}
