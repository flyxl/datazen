//! T237 audit: the dashboard command family, verified against the React
//! source re-read line by line (commands/dashboard.ts has no React test
//! of its own — every expectation below is re-derived from the wrapper
//! literals and contract §6.15's rail rulings).
//!
//! Tester-written: the coder's module is treated as a black box.

use serde_json::json;

use datazen_ui::commands::ai::workflow::AiSubmissionTokenVm;
use datazen_ui::commands::dashboard::{
    authorize_monitor_schedule_request, create_widget_from_sql_request,
    create_widget_from_workflow_request, delete_dashboard_request,
    export_with_dialog_request, find_dashboard_workflow_refs_request,
    get_dashboard_request, get_monitor_schedule_grant_request, get_widget_run_request,
    import_with_dialog_request, list_dashboards_request, list_widget_runs_request,
    revoke_monitor_schedule_request, run_dashboard_widget_request, run_widgets_once_requests,
    save_dashboard_request, set_dashboard_refresh_paused_request, update_hidden_widget_sql_request,
    DashboardMethod, RippleWidget,
};

fn token() -> AiSubmissionTokenVm {
    AiSubmissionTokenVm {
        idempotency_key: "key-1".to_owned(),
        expires_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

#[test]
fn the_wire_names_are_the_domain_client_methods() {
    use DashboardMethod as M;
    let cases = [
        (M::GetMonitorScheduleGrant, "getMonitorScheduleGrant"),
        (M::AuthorizeMonitorSchedule, "authorizeMonitorSchedule"),
        (M::RevokeMonitorSchedule, "revokeMonitorSchedule"),
        (M::ListDashboards, "listDashboards"),
        (M::GetDashboard, "getDashboard"),
        (M::SaveDashboard, "saveDashboard"),
        (M::DeleteDashboard, "deleteDashboard"),
        (M::SetDashboardRefreshPaused, "setDashboardRefreshPaused"),
        (M::FindDashboardWorkflowRefs, "findDashboardWorkflowRefs"),
        (M::ListWidgetRuns, "listWidgetRuns"),
        (M::GetWidgetRun, "getWidgetRun"),
        (M::RunDashboardWidget, "runDashboardWidget"),
        (M::CreateWidgetFromSql, "createWidgetFromSql"),
        (M::CreateWidgetFromWorkflow, "createWidgetFromWorkflow"),
        (M::UpdateHiddenWidgetSql, "updateHiddenWidgetSql"),
    ];
    for (method, name) in cases {
        assert_eq!(method.wire_name(), name);
    }
}

#[test]
fn the_monitor_faces_carry_their_exact_bags() {
    assert_eq!(
        get_monitor_schedule_grant_request("d-1", "w-1"),
        (
            DashboardMethod::GetMonitorScheduleGrant,
            json!({ "dashboardId": "d-1", "widgetId": "w-1" })
        )
    );
    // The authorize face: the fixed one-hour ttl plus the token.
    let (method, value) = authorize_monitor_schedule_request("d-1", "w-1", &token());
    assert_eq!(method, DashboardMethod::AuthorizeMonitorSchedule);
    assert_eq!(value["ttlSecs"], 3600);
    assert_eq!(value["submissionToken"]["idempotencyKey"], "key-1");
    assert_eq!(value["submissionToken"]["expiresAt"], "2026-01-01T00:00:00Z");
    // The revoke face: bodyless, no token.
    let (method, value) = revoke_monitor_schedule_request("d-1", "w-1");
    assert_eq!(method, DashboardMethod::RevokeMonitorSchedule);
    assert_eq!(value, json!({ "dashboardId": "d-1", "widgetId": "w-1" }));
    assert!(value.get("submissionToken").is_none());
}

#[test]
fn the_crud_faces_match_the_react_wrappers() {
    assert_eq!(list_dashboards_request(), (DashboardMethod::ListDashboards, json!({})));
    assert_eq!(
        get_dashboard_request("d-1"),
        (DashboardMethod::GetDashboard, json!({ "id": "d-1" }))
    );
    // saveDashboard: the { dashboard } bag plus the token.
    let (method, value) = save_dashboard_request(&json!({"id": "d-1"}), &token());
    assert_eq!(method, DashboardMethod::SaveDashboard);
    assert_eq!(value["dashboard"]["id"], "d-1");
    assert!(value.get("submissionToken").is_some());
    // deleteDashboard: bodyless.
    let (method, value) = delete_dashboard_request("d-1");
    assert_eq!(method, DashboardMethod::DeleteDashboard);
    assert_eq!(value, json!({ "id": "d-1" }));
    assert!(value.get("submissionToken").is_none());
    // setDashboardRefreshPaused: a write face, token-carrying.
    let (method, value) = set_dashboard_refresh_paused_request("d-1", true, &token());
    assert_eq!(method, DashboardMethod::SetDashboardRefreshPaused);
    assert_eq!(value["paused"], true);
    assert!(value.get("submissionToken").is_some());
    // findDashboardWorkflowRefs.
    assert_eq!(
        find_dashboard_workflow_refs_request("wf-1"),
        (DashboardMethod::FindDashboardWorkflowRefs, json!({ "workflowId": "wf-1" }))
    );
}

#[test]
fn the_run_faces_match_the_react_wrappers() {
    assert_eq!(
        list_widget_runs_request("d-1", "w-1", 25),
        (
            DashboardMethod::ListWidgetRuns,
            json!({ "dashboardId": "d-1", "widgetId": "w-1", "limit": 25 })
        )
    );
    assert_eq!(
        get_widget_run_request("d-1", "w-1", "run-9"),
        (
            DashboardMethod::GetWidgetRun,
            json!({ "dashboardId": "d-1", "widgetId": "w-1", "runId": "run-9" })
        )
    );
    let (method, value) = run_dashboard_widget_request("d-1", "w-1", &token());
    assert_eq!(method, DashboardMethod::RunDashboardWidget);
    assert_eq!(value["dashboardId"], "d-1");
    assert_eq!(value["widgetId"], "w-1");
    assert!(value.get("submissionToken").is_some());
}

#[test]
fn the_create_faces_wrap_params_and_dashboard_id() {
    let params = json!({ "dashboardId": "d-1", "sql": "SELECT 1", "title": "New widget" });
    let (method, value) = create_widget_from_sql_request(&params, &token());
    assert_eq!(method, DashboardMethod::CreateWidgetFromSql);
    assert_eq!(value["dashboardId"], "d-1", "the bag's dashboardId comes from the params");
    assert_eq!(value["params"]["sql"], "SELECT 1");
    assert!(value.get("submissionToken").is_some());

    let params = json!({ "dashboardId": "d-2", "workflowId": "wf-1" });
    let (method, value) = create_widget_from_workflow_request(&params, &token());
    assert_eq!(method, DashboardMethod::CreateWidgetFromWorkflow);
    assert_eq!(value["dashboardId"], "d-2");
    assert_eq!(value["params"]["workflowId"], "wf-1");

    let (method, value) = update_hidden_widget_sql_request(
        &json!({ "dashboardId": "d-1", "widgetId": "w-1", "sql": "SELECT 2" }),
        &token(),
    );
    assert_eq!(method, DashboardMethod::UpdateHiddenWidgetSql);
    assert_eq!(value["params"]["widgetId"], "w-1");
    assert!(value.get("submissionToken").is_some());
}

#[test]
fn the_ripple_runs_every_enabled_widget_once() {
    let widgets = vec![
        RippleWidget { id: "w-on".to_owned(), enabled: true },
        RippleWidget { id: "w-off".to_owned(), enabled: false },
        RippleWidget { id: "w-on2".to_owned(), enabled: true },
    ];
    let requests = run_widgets_once_requests("d-1", &widgets, &token());
    assert_eq!(requests.len(), 2, "the disabled widget never runs");
    assert_eq!(requests[0].0, DashboardMethod::RunDashboardWidget);
    assert_eq!(requests[0].1["widgetId"], "w-on");
    assert_eq!(requests[1].1["widgetId"], "w-on2");
    // Every request carries the token (the write face's mint).
    assert!(requests.iter().all(|(_, value)| value.get("submissionToken").is_some()));
    // An empty dashboard runs nothing.
    assert!(run_widgets_once_requests("d-1", &[], &token()).is_empty());
}

#[test]
fn the_dialog_faces_stay_on_the_legacy_rail() {
    // Contract §6.15's ruling: desktop file-dialog semantics never ride
    // the HTTP face — these two stay legacyInvoke.
    let export = export_with_dialog_request("d-1", "ops-dashboard.json");
    assert_eq!(export.command, "export_dashboard_with_dialog");
    assert_eq!(
        export.value,
        json!({ "dashboardId": "d-1", "defaultFileName": "ops-dashboard.json" })
    );
    let import = import_with_dialog_request();
    assert_eq!(import.command, "import_dashboard_with_dialog");
    assert_eq!(import.value, json!({}));
}
