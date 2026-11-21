mod actions;
mod labels;
mod singleton;

pub use actions::{
    connection_payload, connection_window_plan, dashboard_window_plan, docs_window_plan,
    emit_menu_action_plan, settings_window_plan, workflow_window_plan, ConnectionWindowOpts,
    ConnectionWindowPlan, DashboardWindowPlan, DocsWindowPlan, MenuActionPlan, WINDOW_MANAGER_GAP,
};
pub use labels::{
    is_main_shell_menu_id, PostOnboardingSample, MAIN_SHELL_MENU_IDS, PENDING_CONNECTION_KEY,
    POST_ONBOARDING_SAMPLE_KEY, WINDOW_CAPABILITY_LABEL_SAMPLES,
};
pub use singleton::{
    backup_window_options, data_sync_window_options, data_transfer_window_options,
    schema_diff_window_options, BackupMode, BackupPrefill, SingletonWindowPlan,
};
