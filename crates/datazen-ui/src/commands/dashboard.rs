//! Port of `packages/frontend/src/commands/dashboard.ts` (T237) — the
//! dashboard command family (W8 刀3d web 收口, contract §6.15).
//!
//! Thirteen of the fifteen React wrappers ride the backend-client
//! dashboard domain client (the [`DashboardMethod`] MethodMap — the
//! `ai::workflow` pattern); the body-carrying write faces mint the
//! `saveUserAsset` submission token (the same
//! [`AiSubmissionTokenVm`] vocabulary, unbound from sessions).
//! `exportWithDialog` / `importWithDialog` deliberately stay on the
//! legacy rail (contract §6.15's domain ruling: desktop file-dialog
//! semantics do not belong on the HTTP face).
//!
//! `runDashboardWidgetsOnce` (the create/import flows' run-everything-
//! enabled ripple) is [`run_widgets_once_requests`]: the requests it
//! would fire, one per enabled widget, each failure swallowed (React's
//! `.catch(() => undefined)` — the host drops the failed run's error).
//!
//! Known deltas: none behavioural — every method name and payload key
//! is the React source's verbatim.

use serde_json::{Value, json};

use crate::commands::ai::workflow::{AiSubmissionTokenVm, with_submission_token};

/// The Dashboard `MethodMap` slice — one variant per domain method;
/// `wire_name` is the transport call name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashboardMethod {
    GetMonitorScheduleGrant,
    AuthorizeMonitorSchedule,
    RevokeMonitorSchedule,
    ListDashboards,
    GetDashboard,
    SaveDashboard,
    DeleteDashboard,
    SetDashboardRefreshPaused,
    FindDashboardWorkflowRefs,
    ListWidgetRuns,
    GetWidgetRun,
    RunDashboardWidget,
    CreateWidgetFromSql,
    CreateWidgetFromWorkflow,
    UpdateHiddenWidgetSql,
}

impl DashboardMethod {
    /// The transport call name (`transport.call(name, payload)`).
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::GetMonitorScheduleGrant => "getMonitorScheduleGrant",
            Self::AuthorizeMonitorSchedule => "authorizeMonitorSchedule",
            Self::RevokeMonitorSchedule => "revokeMonitorSchedule",
            Self::ListDashboards => "listDashboards",
            Self::GetDashboard => "getDashboard",
            Self::SaveDashboard => "saveDashboard",
            Self::DeleteDashboard => "deleteDashboard",
            Self::SetDashboardRefreshPaused => "setDashboardRefreshPaused",
            Self::FindDashboardWorkflowRefs => "findDashboardWorkflowRefs",
            Self::ListWidgetRuns => "listWidgetRuns",
            Self::GetWidgetRun => "getWidgetRun",
            Self::RunDashboardWidget => "runDashboardWidget",
            Self::CreateWidgetFromSql => "createWidgetFromSql",
            Self::CreateWidgetFromWorkflow => "createWidgetFromWorkflow",
            Self::UpdateHiddenWidgetSql => "updateHiddenWidgetSql",
        }
    }
}

// ---- the monitor-schedule grant faces ---------------------------------------

/// `getMonitorScheduleGrant(dashboardId, widgetId)`.
pub fn get_monitor_schedule_grant_request(
    dashboard_id: &str,
    widget_id: &str,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::GetMonitorScheduleGrant,
        json!({ "dashboardId": dashboard_id, "widgetId": widget_id }),
    )
}

/// `authorizeMonitorSchedule(dashboardId, widgetId)`: the fixed one-hour
/// ttl plus the saveUserAsset token (the authorized write face).
pub fn authorize_monitor_schedule_request(
    dashboard_id: &str,
    widget_id: &str,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::AuthorizeMonitorSchedule,
        with_submission_token(
            json!({ "dashboardId": dashboard_id, "widgetId": widget_id, "ttlSecs": 3600 }),
            token,
        ),
    )
}

/// `revokeMonitorSchedule(dashboardId, widgetId)` (bodyless DELETE face,
/// no token).
pub fn revoke_monitor_schedule_request(
    dashboard_id: &str,
    widget_id: &str,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::RevokeMonitorSchedule,
        json!({ "dashboardId": dashboard_id, "widgetId": widget_id }),
    )
}

// ---- the dashboard CRUD faces -----------------------------------------------

/// `listDashboards()`.
pub fn list_dashboards_request() -> (DashboardMethod, Value) {
    (DashboardMethod::ListDashboards, json!({}))
}

/// `getDashboard(id)`.
pub fn get_dashboard_request(id: &str) -> (DashboardMethod, Value) {
    (DashboardMethod::GetDashboard, json!({ "id": id }))
}

/// `saveDashboard(dashboard)`: the `{ dashboard }` bag plus the token.
pub fn save_dashboard_request(
    dashboard: &Value,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::SaveDashboard,
        with_submission_token(json!({ "dashboard": dashboard }), token),
    )
}

/// `deleteDashboard(id)` (bodyless, no token).
pub fn delete_dashboard_request(id: &str) -> (DashboardMethod, Value) {
    (DashboardMethod::DeleteDashboard, json!({ "id": id }))
}

/// `setDashboardRefreshPaused(id, paused)` — a write face, token-carrying.
pub fn set_dashboard_refresh_paused_request(
    id: &str,
    paused: bool,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::SetDashboardRefreshPaused,
        with_submission_token(json!({ "id": id, "paused": paused }), token),
    )
}

/// `findDashboardWorkflowRefs(workflowId)`.
pub fn find_dashboard_workflow_refs_request(
    workflow_id: &str,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::FindDashboardWorkflowRefs,
        json!({ "workflowId": workflow_id }),
    )
}

// ---- the run faces ------------------------------------------------------------

/// `listWidgetRuns(dashboardId, widgetId, limit)`.
pub fn list_widget_runs_request(
    dashboard_id: &str,
    widget_id: &str,
    limit: u32,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::ListWidgetRuns,
        json!({ "dashboardId": dashboard_id, "widgetId": widget_id, "limit": limit }),
    )
}

/// `getWidgetRun(dashboardId, widgetId, runId)`.
pub fn get_widget_run_request(
    dashboard_id: &str,
    widget_id: &str,
    run_id: &str,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::GetWidgetRun,
        json!({ "dashboardId": dashboard_id, "widgetId": widget_id, "runId": run_id }),
    )
}

/// `runDashboardWidget(dashboardId, widgetId)`: the write face that
/// carries only the token.
pub fn run_dashboard_widget_request(
    dashboard_id: &str,
    widget_id: &str,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::RunDashboardWidget,
        with_submission_token(
            json!({ "dashboardId": dashboard_id, "widgetId": widget_id }),
            token,
        ),
    )
}

/// `createWidgetFromSql(params)`: the `{ dashboardId, params }` bag plus
/// the token; the caller runs the returned dashboard's enabled widgets
/// once ([`run_widgets_once_requests`]).
pub fn create_widget_from_sql_request(
    params: &Value,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::CreateWidgetFromSql,
        with_submission_token(
            json!({
                "dashboardId": params.get("dashboardId").cloned().unwrap_or(Value::Null),
                "params": params,
            }),
            token,
        ),
    )
}

/// `createWidgetFromWorkflow(params)`: the same bag shape.
pub fn create_widget_from_workflow_request(
    params: &Value,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::CreateWidgetFromWorkflow,
        with_submission_token(
            json!({
                "dashboardId": params.get("dashboardId").cloned().unwrap_or(Value::Null),
                "params": params,
            }),
            token,
        ),
    )
}

/// `updateHiddenWidgetSql(params)`: the `{ params }` bag plus the token.
pub fn update_hidden_widget_sql_request(
    params: &Value,
    token: &AiSubmissionTokenVm,
) -> (DashboardMethod, Value) {
    (
        DashboardMethod::UpdateHiddenWidgetSql,
        with_submission_token(json!({ "params": params }), token),
    )
}

// ---- the ripple + the legacy rail --------------------------------------------

/// One dashboard widget the ripple reads (the `DashboardWidget` fields
/// `runDashboardWidgetsOnce` filters on).
pub struct RippleWidget {
    pub id: String,
    pub enabled: bool,
}

/// `runDashboardWidgetsOnce(dashboard)`: the run requests for every
/// **enabled** widget of the dashboard the create/import flow just
/// produced — fire-and-forget, each failure swallowed (`.catch(() =>
/// undefined)`; the host drops a failed run's error and keeps going).
pub fn run_widgets_once_requests(
    dashboard_id: &str,
    widgets: &[RippleWidget],
    token: &AiSubmissionTokenVm,
) -> Vec<(DashboardMethod, Value)> {
    widgets
        .iter()
        .filter(|widget| widget.enabled)
        .map(|widget| run_dashboard_widget_request(dashboard_id, &widget.id, token))
        .collect()
}

/// One shaped legacy command (the two dialog faces stay on the legacy
/// rail).
#[derive(Clone, Debug, PartialEq)]
pub struct DashboardLegacyRequest {
    pub command: &'static str,
    pub value: Value,
}

/// `exportWithDialog(dashboardId, defaultFileName)` — merged IPC (native
/// save dialog + export), deliberately legacy (contract §6.15).
pub fn export_with_dialog_request(
    dashboard_id: &str,
    default_file_name: &str,
) -> DashboardLegacyRequest {
    DashboardLegacyRequest {
        command: "export_dashboard_with_dialog",
        value: json!({ "dashboardId": dashboard_id, "defaultFileName": default_file_name }),
    }
}

/// `importWithDialog()` — the native open dialog + import, legacy; the
/// answered dashboard's enabled widgets run once
/// ([`run_widgets_once_requests`]), a dismissed dialog answers null.
pub fn import_with_dialog_request() -> DashboardLegacyRequest {
    DashboardLegacyRequest { command: "import_dashboard_with_dialog", value: json!({}) }
}
