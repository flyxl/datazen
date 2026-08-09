//! Dashboard IPC: CRUD, widget run history, and manual widget execution.

use tauri::State;

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::dashboard::execute::execute_widget_once;
use crate::dashboard::runs::{get_run, list_run_index, DashboardRunsError, RunIndexEntry};
use crate::dashboard::store::{
    delete_dashboard as store_delete_dashboard, get_dashboard as store_get_dashboard,
    list_dashboards as store_list_dashboards, save_dashboard as store_save_dashboard,
    DashboardStoreError,
};
use crate::dashboard::types::{Dashboard, WidgetRun};

fn map_store_error(err: DashboardStoreError) -> CommandError {
    match err {
        DashboardStoreError::NotFound(id) => CommandError::NotFound(id),
        DashboardStoreError::Io(e) => CommandError::Io(e),
        DashboardStoreError::Parse(msg) => CommandError::Validation(msg),
    }
}

fn map_runs_error(err: DashboardRunsError) -> CommandError {
    match err {
        DashboardRunsError::NotFound(id) => CommandError::NotFound(id),
        DashboardRunsError::Io(e) => CommandError::Io(e),
        DashboardRunsError::Parse(msg) => CommandError::Validation(msg),
    }
}

fn map_execute_error(err: crate::dashboard::execute::DashboardExecuteError) -> CommandError {
    match err {
        crate::dashboard::execute::DashboardExecuteError::Connection(e) => CommandError::Connection(e),
        crate::dashboard::execute::DashboardExecuteError::Driver(e) => CommandError::Driver(e),
        crate::dashboard::execute::DashboardExecuteError::Runs(e) => map_runs_error(e),
    }
}

#[tauri::command]
pub async fn list_dashboards(state: State<'_, AppState>) -> Result<Vec<Dashboard>, CommandError> {
    let data_dir = state.store.data_dir();
    store_list_dashboards(data_dir)
        .map_err(map_store_error)
        .cmd_err("list_dashboards")
}

#[tauri::command]
pub async fn get_dashboard(
    state: State<'_, AppState>,
    id: String,
) -> Result<Dashboard, CommandError> {
    tracing::debug!(%id, "get_dashboard");
    let data_dir = state.store.data_dir();
    store_get_dashboard(data_dir, &id)
        .map_err(map_store_error)
        .cmd_err("get_dashboard")
}

#[tauri::command]
pub async fn save_dashboard(
    state: State<'_, AppState>,
    dashboard: Dashboard,
) -> Result<Dashboard, CommandError> {
    tracing::info!(id = %dashboard.id, "save_dashboard");
    let data_dir = state.store.data_dir();
    let id = dashboard.id.clone();
    store_save_dashboard(data_dir, dashboard)
        .map_err(map_store_error)
        .cmd_err("save_dashboard")?;
    store_get_dashboard(data_dir, &id)
        .map_err(map_store_error)
        .cmd_err("save_dashboard")
}

#[tauri::command]
pub async fn delete_dashboard(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_dashboard");
    let data_dir = state.store.data_dir();
    store_delete_dashboard(data_dir, &id)
        .map_err(map_store_error)
        .cmd_err("delete_dashboard")
}

#[tauri::command]
pub async fn list_widget_runs(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, CommandError> {
    let data_dir = state.store.data_dir();
    list_run_index(data_dir, &dashboard_id, &widget_id, limit)
        .map_err(map_runs_error)
        .cmd_err("list_widget_runs")
}

#[tauri::command]
pub async fn get_widget_run(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    run_id: String,
) -> Result<WidgetRun, CommandError> {
    let data_dir = state.store.data_dir();
    get_run(data_dir, &dashboard_id, &widget_id, &run_id)
        .map_err(map_runs_error)
        .cmd_err("get_widget_run")
}

#[tauri::command]
pub async fn run_dashboard_widget(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
) -> Result<WidgetRun, CommandError> {
    tracing::info!(%dashboard_id, %widget_id, "run_dashboard_widget");
    let data_dir = state.store.data_dir();
    let dashboard = store_get_dashboard(data_dir, &dashboard_id).map_err(map_store_error)?;
    let widget = dashboard
        .widgets
        .iter()
        .find(|w| w.id == widget_id)
        .ok_or_else(|| CommandError::NotFound(format!("widget {widget_id}")))?
        .clone();

    let settings = state.store.get_settings().await;
    execute_widget_once(
        &state.connection_manager,
        data_dir,
        &settings,
        &dashboard_id,
        &widget,
    )
    .await
    .map_err(map_execute_error)
    .cmd_err("run_dashboard_widget")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dashboard::store::get_dashboard as store_get_dashboard;

    #[test]
    fn unknown_dashboard_maps_to_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = store_get_dashboard(dir.path(), "missing-id").unwrap_err();
        let cmd_err = map_store_error(err);
        assert!(matches!(cmd_err, CommandError::NotFound(msg) if msg == "missing-id"));
    }

    #[test]
    fn unknown_widget_run_maps_to_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = get_run(dir.path(), "d1", "w1", "run-x").unwrap_err();
        let cmd_err = map_runs_error(err);
        assert!(matches!(cmd_err, CommandError::NotFound(msg) if msg == "run-x"));
    }

    #[test]
    fn unknown_widget_maps_to_not_found() {
        use crate::dashboard::store::save_dashboard;
        use crate::dashboard::types::{
            AggregationType, ChartConfig, ChartSortBy, ChartType, Dashboard, DashboardLayout,
            WidgetLayout,
        };

        let dir = tempfile::tempdir().unwrap();
        let dashboard = Dashboard {
            id: "d1".into(),
            name: "Test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![crate::dashboard::types::DashboardWidget {
                id: "w-known".into(),
                title: "Known".into(),
                config_id: "cfg-1".into(),
                sql: "SELECT 1".into(),
                chart_config: ChartConfig {
                    chart_type: ChartType::Line,
                    x_axis: None,
                    y_axes: vec![],
                    group_by: None,
                    aggregation: AggregationType::None,
                    sort_by: ChartSortBy::None,
                    show_legend: true,
                    show_grid: true,
                    show_values: false,
                    color_scheme: "default".into(),
                },
                layout: WidgetLayout {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 3,
                },
                refresh_sec: 60,
                alert: None,
                enabled: true,
            }],
            enabled: true,
        };
        save_dashboard(dir.path(), dashboard.clone()).unwrap();

        let widget_id = "w-missing".to_string();
        let err = dashboard
            .widgets
            .iter()
            .find(|w| w.id == widget_id)
            .ok_or_else(|| CommandError::NotFound(format!("widget {widget_id}")))
            .unwrap_err();
        assert!(matches!(err, CommandError::NotFound(msg) if msg == "widget w-missing"));
    }
}
