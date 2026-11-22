mod events;
mod types;
mod view;

#[cfg(test)]
mod tests;

pub use events::{dispatch_nav_action, resolve_confirm_result, resolve_restart_prompt};
pub use types::{
    NavAction, NavButton, NavButtonId, ProgressRing, RestartPrompt, SidebarView, UpdateAction,
    UpdateButtonView, UpdateIconKind, UpdateProgress, UpdateProgressPhase, UpdateState,
    WORKSPACE_MODE_SIDEBAR_GAP, WORKSPACE_MODE_SIDEBAR_SURFACE,
};
pub use view::{
    build_sidebar_view, build_update_view, progress_label, progress_ring, update_label, BUTTON_ACTIVE,
    BUTTON_BASE, BUTTON_COLLAPSED, BUTTON_EXPANDED, BUTTON_INACTIVE, SIDEBAR_BASE,
};
