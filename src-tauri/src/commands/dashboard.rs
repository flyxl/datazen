//! Dashboard IPC: CRUD, widget run history, and manual widget execution.

use tauri::{AppHandle, State};

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::dashboard::export::{export_dashboard_json, import_dashboard, DashboardExportError};
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

fn map_export_error(err: DashboardExportError) -> CommandError {
    match err {
        DashboardExportError::Validation(msg) => CommandError::Validation(msg),
        DashboardExportError::Io(e) => CommandError::Io(e),
        DashboardExportError::Store(e) => map_store_error(e),
    }
}

pub(crate) async fn list_dashboards_impl(
    state: &AppState,
) -> Result<Vec<Dashboard>, CommandError> {
    let data_dir = state.store.data_dir();
    store_list_dashboards(data_dir)
        .map_err(map_store_error)
        .cmd_err("list_dashboards")
}

pub(crate) async fn get_dashboard_impl(
    state: &AppState,
    id: String,
) -> Result<Dashboard, CommandError> {
    tracing::debug!(%id, "get_dashboard");
    let data_dir = state.store.data_dir();
    store_get_dashboard(data_dir, &id)
        .map_err(map_store_error)
        .cmd_err("get_dashboard")
}

pub(crate) async fn save_dashboard_impl(
    state: &AppState,
    dashboard: Dashboard,
) -> Result<Dashboard, CommandError> {
    tracing::info!(id = %dashboard.id, "save_dashboard");
    let data_dir = state.store.data_dir();
    let id = dashboard.id.clone();
    store_save_dashboard(data_dir, dashboard)
        .map_err(map_store_error)
        .cmd_err("save_dashboard")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .map_err(map_store_error)
        .cmd_err("save_dashboard")?;
    store_get_dashboard(data_dir, &id)
        .map_err(map_store_error)
        .cmd_err("save_dashboard")
}

pub(crate) async fn delete_dashboard_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_dashboard");
    let data_dir = state.store.data_dir();
    store_delete_dashboard(data_dir, &id)
        .map_err(map_store_error)
        .cmd_err("delete_dashboard")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .map_err(map_store_error)
        .cmd_err("delete_dashboard")
}

pub(crate) fn get_monitor_paused_impl(state: &AppState) -> bool {
    state.monitor_engine.is_paused()
}

pub(crate) fn set_monitor_paused_impl(state: &AppState, paused: bool) {
    state.monitor_engine.set_paused(paused);
}

#[tauri::command]
pub async fn list_dashboards(state: State<'_, AppState>) -> Result<Vec<Dashboard>, CommandError> {
    list_dashboards_impl(&state).await
}

#[tauri::command]
pub async fn get_dashboard(
    state: State<'_, AppState>,
    id: String,
) -> Result<Dashboard, CommandError> {
    get_dashboard_impl(&state, id).await
}

#[tauri::command]
pub async fn save_dashboard(
    state: State<'_, AppState>,
    dashboard: Dashboard,
) -> Result<Dashboard, CommandError> {
    save_dashboard_impl(&state, dashboard).await
}

#[tauri::command]
pub async fn delete_dashboard(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    delete_dashboard_impl(&state, id).await
}

pub(crate) async fn list_widget_runs_impl(
    state: &AppState,
    dashboard_id: String,
    widget_id: String,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, CommandError> {
    let data_dir = state.store.data_dir();
    list_run_index(data_dir, &dashboard_id, &widget_id, limit)
        .map_err(map_runs_error)
        .cmd_err("list_widget_runs")
}

pub(crate) async fn get_widget_run_impl(
    state: &AppState,
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
pub async fn list_widget_runs(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, CommandError> {
    list_widget_runs_impl(&state, dashboard_id, widget_id, limit).await
}

#[tauri::command]
pub async fn get_widget_run(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    run_id: String,
) -> Result<WidgetRun, CommandError> {
    get_widget_run_impl(&state, dashboard_id, widget_id, run_id).await
}

pub(crate) async fn run_dashboard_widget_impl(
    state: &AppState,
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

    state
        .monitor_engine
        .tick_widget(&dashboard_id, &widget)
        .await
        .map_err(map_execute_error)
        .cmd_err("run_dashboard_widget")
}

#[tauri::command]
pub async fn run_dashboard_widget(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
) -> Result<WidgetRun, CommandError> {
    run_dashboard_widget_impl(&state, dashboard_id, widget_id).await
}

/// Native save dialog + single-file dashboard JSON export.
/// Returns `true` if exported, `false` if cancelled.
#[tauri::command]
pub async fn export_dashboard_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    dashboard_id: String,
    default_file_name: String,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let data_dir = state.store.data_dir();
    let dashboard = store_get_dashboard(data_dir, &dashboard_id).map_err(map_store_error)?;

    let picked = app
        .dialog()
        .file()
        .add_filter("DataZen Dashboard", &["json"])
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let dest = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;

    let json = export_dashboard_json(&dashboard).map_err(map_export_error)?;
    tokio::fs::write(&dest, json.as_bytes())
        .await
        .cmd_err("export_dashboard_with_dialog")?;
    tracing::info!(%dashboard_id, "export_dashboard_with_dialog OK");
    Ok(true)
}

/// Native open dialog + single-file dashboard JSON import.
/// Returns the imported dashboard if saved, `None` if cancelled.
#[tauri::command]
pub async fn import_dashboard_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Dashboard>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .add_filter("DataZen Dashboard", &["json"])
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(None);
    };
    let source = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;

    let bytes = tokio::fs::read(&source)
        .await
        .cmd_err("import_dashboard_with_dialog")?;
    let data_dir = state.store.data_dir().clone();
    let dashboard = tokio::task::spawn_blocking(move || import_dashboard(&data_dir, &bytes))
        .await
        .map_err(|e| CommandError::Internal(format!("import_dashboard_with_dialog task: {e}")))?
        .map_err(map_export_error)
        .cmd_err("import_dashboard_with_dialog")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .map_err(map_store_error)
        .cmd_err("import_dashboard_with_dialog")?;
    tracing::info!(id = %dashboard.id, "import_dashboard_with_dialog OK");
    Ok(Some(dashboard))
}

#[tauri::command]
pub fn get_monitor_paused(state: State<'_, AppState>) -> bool {
    get_monitor_paused_impl(&state)
}

#[tauri::command]
pub fn set_monitor_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) {
    set_monitor_paused_impl(&state, paused);
    crate::tray::sync_tray(&app);
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

    #[test]
    fn map_export_error_validation() {
        use crate::dashboard::export::DashboardExportError;

        let err = map_export_error(DashboardExportError::Validation("bad json".into()));
        assert!(matches!(err, CommandError::Validation(msg) if msg == "bad json"));
    }

    fn sample_dashboard(id: &str) -> Dashboard {
        use crate::dashboard::types::{
            AggregationType, ChartConfig, ChartSortBy, ChartType, DashboardLayout, WidgetLayout,
        };

        Dashboard {
            id: id.into(),
            name: "Test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![crate::dashboard::types::DashboardWidget {
                id: "w1".into(),
                title: "Widget".into(),
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
        }
    }

    #[tokio::test]
    async fn dashboard_crud_via_impl() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        assert!(list_dashboards_impl(&test.state).await.unwrap().is_empty());

        let dash = sample_dashboard("dash-1");
        save_dashboard_impl(&test.state, dash.clone())
            .await
            .unwrap();
        let loaded = get_dashboard_impl(&test.state, "dash-1".into())
            .await
            .unwrap();
        assert_eq!(loaded.name, "Test");

        assert!(!get_monitor_paused_impl(&test.state));
        set_monitor_paused_impl(&test.state, true);
        assert!(get_monitor_paused_impl(&test.state));

        delete_dashboard_impl(&test.state, "dash-1".into())
            .await
            .unwrap();
        assert!(get_dashboard_impl(&test.state, "dash-1".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn run_dashboard_widget_with_connected_config() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::with_tables().await;
        test.save_connection("widget-cfg").await;
        test.connect_config("widget-cfg").await;

        let mut dash = sample_dashboard("dash-run");
        dash.widgets[0].config_id = "widget-cfg".into();
        save_dashboard_impl(&test.state, dash).await.unwrap();

        let run = run_dashboard_widget_impl(&test.state, "dash-run".into(), "w1".into())
            .await
            .unwrap();
        assert!(!run.id.is_empty());

        let runs = list_widget_runs_impl(&test.state, "dash-run".into(), "w1".into(), 10)
            .await
            .unwrap();
        assert_eq!(runs.len(), 1);
        let fetched = get_widget_run_impl(
            &test.state,
            "dash-run".into(),
            "w1".into(),
            runs[0].id.clone(),
        )
        .await
        .unwrap();
        assert_eq!(fetched.id, runs[0].id);
    }
}
