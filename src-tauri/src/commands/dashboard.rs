//! Dashboard IPC: CRUD, widget run history, manual execution, and widget creation.

use tauri::{AppHandle, State};

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::dashboard::create::{
    create_widget_from_sql as create_widget_from_sql_impl,
    create_widget_from_workflow as create_widget_from_workflow_impl,
    update_hidden_workflow_sql as update_hidden_workflow_sql_impl, CreateWidgetResult,
};
use crate::dashboard::export::{export_dashboard_json, import_dashboard};
use crate::dashboard::runs::{get_run, list_run_index, RunIndexEntry};
use crate::dashboard::store::{
    delete_dashboard as store_delete_dashboard, find_workflow_refs,
    get_dashboard as store_get_dashboard, list_dashboards as store_list_dashboards,
    save_dashboard as store_save_dashboard,
    set_dashboard_refresh_paused as store_set_refresh_paused,
};
use crate::dashboard::types::{ChartConfig, Dashboard, DashboardWidget, ViewMode, WidgetRun};

pub(crate) async fn list_dashboards_impl(state: &AppState) -> Result<Vec<Dashboard>, CommandError> {
    let app_db = state.store.app_db();
    store_list_dashboards(&app_db).cmd_err("list_dashboards")
}

pub(crate) async fn get_dashboard_impl(
    state: &AppState,
    id: String,
) -> Result<Dashboard, CommandError> {
    tracing::debug!(%id, "get_dashboard");
    let app_db = state.store.app_db();
    store_get_dashboard(&app_db, &id).cmd_err("get_dashboard")
}

pub(crate) async fn save_dashboard_impl(
    state: &AppState,
    dashboard: Dashboard,
) -> Result<Dashboard, CommandError> {
    tracing::info!(id = %dashboard.id, "save_dashboard");
    let app_db = state.store.app_db();
    let id = dashboard.id.clone();
    store_save_dashboard(&app_db, dashboard).cmd_err("save_dashboard")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("save_dashboard")?;
    store_get_dashboard(&app_db, &id).cmd_err("save_dashboard")
}

pub(crate) async fn delete_dashboard_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_dashboard");
    let app_db = state.store.app_db();
    store_delete_dashboard(&app_db, &id).cmd_err("delete_dashboard")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("delete_dashboard")
}

pub(crate) async fn set_dashboard_refresh_paused_impl(
    state: &AppState,
    id: String,
    paused: bool,
) -> Result<(), CommandError> {
    let app_db = state.store.app_db();
    store_set_refresh_paused(&app_db, &id, paused).cmd_err("set_dashboard_refresh_paused")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("set_dashboard_refresh_paused")
}

pub(crate) fn find_dashboard_workflow_refs_impl(
    state: &AppState,
    workflow_id: String,
) -> Result<Vec<crate::dashboard::types::DashboardWorkflowRef>, CommandError> {
    let app_db = state.store.app_db();
    find_workflow_refs(&app_db, &workflow_id).cmd_err("find_dashboard_workflow_refs")
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
pub async fn delete_dashboard(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    delete_dashboard_impl(&state, id).await
}

#[tauri::command]
pub async fn set_dashboard_refresh_paused(
    state: State<'_, AppState>,
    id: String,
    paused: bool,
) -> Result<(), CommandError> {
    set_dashboard_refresh_paused_impl(&state, id, paused).await
}

#[tauri::command]
pub fn find_dashboard_workflow_refs(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<Vec<crate::dashboard::types::DashboardWorkflowRef>, CommandError> {
    find_dashboard_workflow_refs_impl(&state, workflow_id)
}

pub(crate) async fn list_widget_runs_impl(
    state: &AppState,
    dashboard_id: String,
    widget_id: String,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, CommandError> {
    let app_db = state.store.app_db();
    list_run_index(&app_db, &dashboard_id, &widget_id, limit).cmd_err("list_widget_runs")
}

pub(crate) async fn get_widget_run_impl(
    state: &AppState,
    dashboard_id: String,
    widget_id: String,
    run_id: String,
) -> Result<WidgetRun, CommandError> {
    let app_db = state.store.app_db();
    get_run(&app_db, &dashboard_id, &widget_id, &run_id).cmd_err("get_widget_run")
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
    let app_db = state.store.app_db();
    let dashboard = store_get_dashboard(&app_db, &dashboard_id)?;
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetFromSqlParams {
    pub dashboard_id: String,
    pub connection_id: String,
    pub sql: String,
    pub title: Option<String>,
    pub view_mode: ViewMode,
    pub chart_config: Option<ChartConfig>,
}

#[tauri::command]
pub async fn create_widget_from_sql(
    state: State<'_, AppState>,
    params: CreateWidgetFromSqlParams,
) -> Result<CreateWidgetResult, CommandError> {
    let app_db = state.store.app_db();
    create_widget_from_sql_impl(
        &app_db,
        &state.workflow_registry,
        &params.dashboard_id,
        &params.connection_id,
        &params.sql,
        params.title,
        params.view_mode,
        params.chart_config,
    )
    .await
    .cmd_err("create_widget_from_sql")
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetFromWorkflowParams {
    pub dashboard_id: String,
    pub workflow_id: String,
    pub title: Option<String>,
    pub view_mode: ViewMode,
    pub chart_config: Option<ChartConfig>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHiddenWidgetSqlParams {
    pub workflow_id: String,
    pub connection_id: String,
    pub sql: String,
}

#[tauri::command]
pub async fn update_hidden_widget_sql(
    state: State<'_, AppState>,
    params: UpdateHiddenWidgetSqlParams,
) -> Result<(), CommandError> {
    let app_db = state.store.app_db();
    update_hidden_workflow_sql_impl(
        &app_db,
        &state.workflow_registry,
        &params.workflow_id,
        &params.connection_id,
        &params.sql,
    )
    .await
    .cmd_err("update_hidden_widget_sql")
}

#[tauri::command]
pub async fn create_widget_from_workflow(
    state: State<'_, AppState>,
    params: CreateWidgetFromWorkflowParams,
) -> Result<CreateWidgetResult, CommandError> {
    let app_db = state.store.app_db();
    create_widget_from_workflow_impl(
        &app_db,
        &params.dashboard_id,
        &params.workflow_id,
        params.title,
        params.view_mode,
        params.chart_config,
    )
    .await
    .cmd_err("create_widget_from_workflow")
}

/// Native save dialog + single-file dashboard JSON export.
#[tauri::command]
pub async fn export_dashboard_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    dashboard_id: String,
    default_file_name: String,
) -> Result<bool, CommandError> {
    let app_db = state.store.app_db();
    let dashboard = store_get_dashboard(&app_db, &dashboard_id)?;

    let dest = super::dialog::save_file(
        &app,
        ("DataZen Dashboard".into(), vec!["json".into()]),
        default_file_name,
    )
    .await?;
    let Some(dest) = dest else {
        return Ok(false);
    };

    let json =
        export_dashboard_json(&app_db, &dashboard).cmd_err("export_dashboard_with_dialog")?;
    tokio::fs::write(&dest, json.as_bytes())
        .await
        .cmd_err("export_dashboard_with_dialog")?;
    tracing::info!(%dashboard_id, "export_dashboard_with_dialog OK");
    Ok(true)
}

/// Native open dialog + single-file dashboard JSON import.
#[tauri::command]
pub async fn import_dashboard_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Dashboard>, CommandError> {
    let source = super::dialog::open_file(
        &app,
        vec![("DataZen Dashboard".into(), vec!["json".into()])],
    )
    .await?;
    let Some(source) = source else {
        return Ok(None);
    };

    let bytes = tokio::fs::read(&source)
        .await
        .cmd_err("import_dashboard_with_dialog")?;
    let app_db = state.store.app_db();
    let dashboard = tokio::task::spawn_blocking(move || import_dashboard(&app_db, &bytes))
        .await
        .map_err(|e| CommandError::Internal(format!("import_dashboard_with_dialog task: {e}")))?
        .cmd_err("import_dashboard_with_dialog")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("import_dashboard_with_dialog")?;
    tracing::info!(id = %dashboard.id, "import_dashboard_with_dialog OK");
    Ok(Some(dashboard))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{DashboardLayout, RefreshMode, RefreshPolicy, WidgetLayout};
    use crate::store::{AppDb, WorkflowRecord, WorkflowVisibility};
    use chrono::Utc;

    fn sample_dashboard(id: &str, workflow_id: &str) -> Dashboard {
        Dashboard {
            id: id.into(),
            name: "Test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![DashboardWidget {
                id: "w1".into(),
                title: "Widget".into(),
                workflow_id: workflow_id.into(),
                view_mode: ViewMode::Chart,
                chart_config: None,
                layout: WidgetLayout {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 3,
                },
                refresh: RefreshPolicy {
                    mode: RefreshMode::Manual,
                    refresh_sec: None,
                },
                alert: None,
                enabled: true,
            }],
            enabled: true,
            refresh_paused: false,
        }
    }

    fn seed_workflow(db: &AppDb, id: &str) {
        db.upsert_workflow(&WorkflowRecord {
            id: id.into(),
            name: "WF".into(),
            description: String::new(),
            visibility: WorkflowVisibility::User,
            definition_yaml: format!(
                "id: {id}\nname: WF\nconnection: widget-cfg\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT 1 AS v\n"
            ),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
    }

    #[test]
    fn unknown_dashboard_maps_to_not_found() {
        let db = AppDb::open_in_memory().unwrap();
        let err = store_get_dashboard(&db, "missing-id").unwrap_err();
        let cmd_err: CommandError = err.into();
        assert!(matches!(cmd_err, CommandError::NotFound(msg) if msg == "missing-id"));
    }

    #[tokio::test]
    async fn dashboard_crud_via_impl() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        assert!(list_dashboards_impl(&test.state).await.unwrap().is_empty());

        seed_workflow(&test.state.store.app_db(), "wf-1");
        let dash = sample_dashboard("dash-1", "wf-1");
        save_dashboard_impl(&test.state, dash.clone())
            .await
            .unwrap();
        let loaded = get_dashboard_impl(&test.state, "dash-1".into())
            .await
            .unwrap();
        assert_eq!(loaded.name, "Test");

        set_dashboard_refresh_paused_impl(&test.state, "dash-1".into(), true)
            .await
            .unwrap();
        let paused = get_dashboard_impl(&test.state, "dash-1".into())
            .await
            .unwrap();
        assert!(paused.refresh_paused);

        delete_dashboard_impl(&test.state, "dash-1".into())
            .await
            .unwrap();
        assert!(get_dashboard_impl(&test.state, "dash-1".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn run_dashboard_widget_with_workflow() {
        use crate::testing::app_state::TestAppState;
        use crate::workflow::model::{WorkflowDefinition, WorkflowStep};

        let test = TestAppState::with_tables().await;
        test.save_connection("widget-cfg").await;
        test.connect_config("widget-cfg").await;

        let workflow = WorkflowDefinition {
            id: "wf-run".into(),
            name: "Run".into(),
            description: String::new(),
            version: None,
            author: None,
            variables: vec![],
            connection: Some("widget-cfg".into()),
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1 AS v".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        test.state
            .workflow_registry
            .save_workflow(&workflow)
            .await
            .unwrap();

        let dash = sample_dashboard("dash-run", "wf-run");
        save_dashboard_impl(&test.state, dash).await.unwrap();

        let run = run_dashboard_widget_impl(&test.state, "dash-run".into(), "w1".into())
            .await
            .unwrap();
        assert!(!run.id.is_empty());
        assert_eq!(run.status, crate::dashboard::types::WidgetRunStatus::Ok);

        let runs = list_widget_runs_impl(&test.state, "dash-run".into(), "w1".into(), 10)
            .await
            .unwrap();
        assert_eq!(runs.len(), 1);
    }
}
