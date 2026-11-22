use super::types::{NavAction, NavButtonId, RestartPrompt, UpdateAction, UpdateState};

pub fn dispatch_nav_action(id: NavButtonId) -> Option<NavAction> {
    match id {
        NavButtonId::Databases => Some(NavAction::SetWorkspaceMode(super::types::WorkspaceMode::Connections)),
        NavButtonId::Workflow => Some(NavAction::OpenWorkflow),
        NavButtonId::Dashboard => Some(NavAction::OpenDashboard),
        NavButtonId::WorkspacePages => Some(NavAction::SetWorkspaceMode(super::types::WorkspaceMode::Workspace)),
        NavButtonId::Extensions => Some(NavAction::SetWorkspaceMode(super::types::WorkspaceMode::Extension)),
        NavButtonId::Settings => Some(NavAction::OpenSettings),
        NavButtonId::SidebarToggle => Some(NavAction::ToggleSidebar),
        NavButtonId::Update => None,
    }
}

pub fn resolve_restart_prompt(
    state: &UpdateState,
    prompted_for: Option<&str>,
    t: &dyn Fn(&str) -> String,
) -> Option<RestartPrompt> {
    let pending = state.pending_restart_version.as_deref()?;
    if prompted_for == Some(pending) {
        return None;
    }
    Some(RestartPrompt {
        message: t("settings.updater.restartPrompt").replace("{version}", pending),
        title: t("settings.updater.title"),
        confirm_label: t("settings.updater.restartNow"),
        cancel_label: t("settings.updater.restartLater"),
        kind: "info".to_owned(),
    })
}

pub fn resolve_confirm_result(_prompt: &RestartPrompt, confirmed: bool) -> Option<UpdateAction> {
    if confirmed {
        Some(UpdateAction::Restart)
    } else {
        None
    }
}
