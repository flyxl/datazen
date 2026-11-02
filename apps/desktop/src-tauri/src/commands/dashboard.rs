//! Dashboard IPC: CRUD, widget run history, manual execution, and widget creation.
//!
//! B2 仪表盘刀（WR-A2 同刀法）：`*_impl` 转发层已删除，接线逻辑直接住在
//! `#[tauri::command]` 处理函数里；形态中立的「存储后回读」组合下沉到
//! `datazen::dashboard::repository::save_and_fetch`。命令壳只保留
//! state→repository 的装配与错误映射，IPC 契约（命令名、参数、错误文案）零变化。

use tauri::{AppHandle, State};

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::dashboard::create::CreateWidgetResult;
use crate::dashboard::export::{export_dashboard_json, import_dashboard};
use crate::platform::ipc::require_application_services;
use datazen::dashboard::repository::{
    delete_dashboard as store_delete_dashboard, find_workflow_refs,
    get_dashboard as store_get_dashboard, list_dashboards as store_list_dashboards, save_and_fetch,
    set_dashboard_refresh_paused as store_set_refresh_paused,
};
use datazen::dashboard::runs::{get_run, list_run_index};
use datazen::dashboard::types::{ChartConfig, Dashboard, RunIndexEntry, ViewMode, WidgetRun};

#[tauri::command]
pub async fn list_dashboards(state: State<'_, AppState>) -> Result<Vec<Dashboard>, CommandError> {
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard.clone();
    store_list_dashboards(&scope, &*app_db).cmd_err("list_dashboards")
}

#[tauri::command]
pub async fn get_dashboard(
    state: State<'_, AppState>,
    id: String,
) -> Result<Dashboard, CommandError> {
    tracing::debug!(%id, "get_dashboard");
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard.clone();
    store_get_dashboard(&scope, &*app_db, &id).cmd_err("get_dashboard")
}

#[tauri::command]
pub async fn save_dashboard(
    state: State<'_, AppState>,
    dashboard: Dashboard,
) -> Result<Dashboard, CommandError> {
    tracing::info!(id = %dashboard.id, "save_dashboard");
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard.clone();
    let saved = save_and_fetch(&scope, &*app_db, dashboard).cmd_err("save_dashboard")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("save_dashboard")?;
    Ok(saved)
}

#[tauri::command]
pub async fn delete_dashboard(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_dashboard");
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard.clone();
    store_delete_dashboard(&scope, &*app_db, &id).cmd_err("delete_dashboard")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("delete_dashboard")
}

#[tauri::command]
pub async fn set_dashboard_refresh_paused(
    state: State<'_, AppState>,
    id: String,
    paused: bool,
) -> Result<(), CommandError> {
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard.clone();
    store_set_refresh_paused(&scope, &*app_db, &id, paused)
        .cmd_err("set_dashboard_refresh_paused")?;
    state
        .monitor_engine
        .reload_from_store()
        .await
        .cmd_err("set_dashboard_refresh_paused")
}

#[tauri::command]
pub fn find_dashboard_workflow_refs(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<Vec<datazen::dashboard::types::DashboardWorkflowRef>, CommandError> {
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard.clone();
    find_workflow_refs(&scope, &*app_db, &workflow_id).cmd_err("find_dashboard_workflow_refs")
}

#[tauri::command]
pub async fn list_widget_runs(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, CommandError> {
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard_runs.clone();
    list_run_index(&scope, &*app_db, &dashboard_id, &widget_id, limit).cmd_err("list_widget_runs")
}

#[tauri::command]
pub async fn get_widget_run(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    run_id: String,
) -> Result<WidgetRun, CommandError> {
    let scope = state.organization_scope()?;
    let app_db = require_application_services(&state)?.dashboard_runs.clone();
    get_run(&scope, &*app_db, &dashboard_id, &widget_id, &run_id).cmd_err("get_widget_run")
}

#[tauri::command]
pub async fn run_dashboard_widget(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
) -> Result<WidgetRun, CommandError> {
    tracing::info!(%dashboard_id, %widget_id, "run_dashboard_widget");
    let scope = state.organization_scope()?;
    // scope-gate: 手工执行（1 处宿主残留）——`monitor_engine` 的 tick 路径持有
    // `AppDb` 具体类型，属 §5.1.3 形态专属清单，不进中立层。
    let app_db = state.store.app_db();
    let dashboard = store_get_dashboard(&scope, &*app_db, &dashboard_id)?;
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
    let scope = state.organization_scope()?;
    let app_db = state.store.app_db();
    crate::dashboard::create::create_widget_from_sql(
        &scope,
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
    let scope = state.organization_scope()?;
    let app_db = state.store.app_db();
    crate::dashboard::create::update_hidden_workflow_sql(
        &scope,
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
    let scope = state.organization_scope()?;
    let app_db = state.store.app_db();
    crate::dashboard::create::create_widget_from_workflow(
        &scope,
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
    let scope = state.organization_scope()?;
    // 导出/导入走桌面文件对话框与 `AppDb` 具体类型（§5.1.3 形态专属清单），
    // 不进中立层；因此这里读的是宿主 store，而不是聚合的仪表盘端口。
    let app_db = state.store.app_db();
    let dashboard = store_get_dashboard(&scope, &*app_db, &dashboard_id)?;

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
        export_dashboard_json(&*app_db, &dashboard).cmd_err("export_dashboard_with_dialog")?;
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
    let scope = state.organization_scope()?;
    let app_db = state.store.app_db();
    let dashboard = tokio::task::spawn_blocking(move || import_dashboard(&scope, &*app_db, &bytes))
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
    use super::command_test_host::host_command_app;
    use super::*;

    /// 仪表盘命令族的宿主残留必须**有名字**。
    ///
    /// P3 把 8 处仓储调用搬到了聚合（仪表盘 6 + 运行历史 2）；剩下 6 处是
    /// 两类形态专属用途，保留在宿主 store 上：
    /// - 组件创建（3 处）：`WidgetCreationHost` 的实现要读 `AppDb::get_workflow`
    ///   这个**具体**记录接口，与 `WorkflowRegistry` 一起才会成型；
    /// - 导入/导出（2 处）：走桌面文件对话框，且 `export_dashboard_json` /
    ///   `import_dashboard` 的签名就是 `&AppDb`；
    /// - 手工执行（1 处）：`monitor_engine` 的重载路径读的是宿主库。
    ///
    /// 这条断言的作用不是「少几个数字」，而是让任何人想再动这里时必须先
    /// 说明自己搬的是哪一类——数字变了而理由没变，就是偷偷把形态专属
    /// 的东西塞进了中立层。
    ///
    /// 注意：本模块必须保持在文件中部（monitor 授权命令之前）——断言按
    /// 首个 `#[cfg(test)]` 切分生产半边，授权命令里的 store 访问不计入。
    #[test]
    fn the_command_shell_reads_the_aggregate_and_names_the_host_residue() {
        let source = include_str!("dashboard.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert_eq!(
            production
                .matches("require_application_services(&state)")
                .count(),
            8,
            "仪表盘 6 处 + 运行历史 2 处都应从聚合取端口"
        );
        assert_eq!(
            production.matches("state.store").count(),
            6,
            "宿主残留只允许组件创建(3)/导入导出(2)/手工执行(1)"
        );
    }

    use crate::store::{AppDb, WorkflowRecord, WorkflowVisibility};
    use chrono::Utc;
    use datazen::dashboard::types::{
        DashboardLayout, DashboardWidget, RefreshMode, RefreshPolicy, WidgetLayout,
    };

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
        let scope = crate::platform::local_organization_scope();
        let err = store_get_dashboard(&scope, &*db, "missing-id").unwrap_err();
        let cmd_err: CommandError = err.into();
        assert!(matches!(cmd_err, CommandError::NotFound(msg) if msg == "missing-id"));
    }

    /// CRUD 适配器烟测（C4）：真实命令入口 → 聚合端口 → 模块仓储规则 → 宿主 SQLite。
    /// 深度行为（归一化、组织隔离）在 `modules/src/dashboard/domain/repository.rs` 测。
    #[tokio::test]
    async fn dashboard_crud_roundtrip() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        let app = host_command_app(test);
        assert!(list_dashboards(app.state()).await.unwrap().is_empty());

        seed_workflow(&app.keep.store.app_db(), "wf-1");
        let dash = sample_dashboard("dash-1", "wf-1");
        let saved = save_dashboard(app.state(), dash.clone()).await.unwrap();
        assert_eq!(saved.id, "dash-1");
        let loaded = get_dashboard(app.state(), "dash-1".into()).await.unwrap();
        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded, saved);

        set_dashboard_refresh_paused(app.state(), "dash-1".into(), true)
            .await
            .unwrap();
        let paused = get_dashboard(app.state(), "dash-1".into()).await.unwrap();
        assert!(paused.refresh_paused);

        delete_dashboard(app.state(), "dash-1".into())
            .await
            .unwrap();
        assert!(get_dashboard(app.state(), "dash-1".into()).await.is_err());
    }

    #[tokio::test]
    async fn run_dashboard_widget_with_workflow() {
        use crate::testing::app_state::TestAppState;
        use datazen::workflow::model::{WorkflowDefinition, WorkflowStep};

        let test = TestAppState::with_tables_and_resource_provider().await;
        test.save_connection("widget-cfg").await;
        test.connect_config("widget-cfg").await;

        let workflow = WorkflowDefinition {
            database: None,
            id: "wf-run".into(),
            name: "Run".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
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
            .save_workflow(&test.state.organization_scope().unwrap(), &workflow)
            .await
            .unwrap();

        let dash = sample_dashboard("dash-run", "wf-run");
        let app = host_command_app(test);
        save_dashboard(app.state(), dash).await.unwrap();

        let run = run_dashboard_widget(app.state(), "dash-run".into(), "w1".into())
            .await
            .unwrap();
        assert!(!run.id.is_empty());
        assert_eq!(run.status, datazen::dashboard::types::WidgetRunStatus::Ok);

        let runs = list_widget_runs(app.state(), "dash-run".into(), "w1".into(), 10)
            .await
            .unwrap();
        assert_eq!(runs.len(), 1);
    }
}

// 测试宿主（cfg(test)）：声明必须留在 `mod tests` 之后——上面的源码守卫按
// 首个 `#[cfg(test)]` 切分生产半边。
#[cfg(test)]
#[path = "dashboard_command_test_host.rs"]
pub(crate) mod command_test_host;

#[tauri::command]
pub async fn authorize_monitor_widget_schedule(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    ttl_secs: u32,
) -> Result<datazen::monitor::domain::MonitorGrant, CommandError> {
    crate::monitor::grants::authorize(&state, &dashboard_id, &widget_id, ttl_secs)
        .await
        .map_err(CommandError::Validation)
}
#[tauri::command]
pub async fn revoke_monitor_widget_schedule(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
) -> Result<(), CommandError> {
    crate::monitor::grants::revoke(&state, &dashboard_id, &widget_id)
        .map_err(CommandError::Validation)
}
#[tauri::command]
pub async fn get_monitor_widget_schedule_grant(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
) -> Result<Option<datazen::monitor::domain::MonitorGrant>, CommandError> {
    let call = state
        .platform_require()
        .map_err(|_| CommandError::Validation("platform unavailable".into()))?
        .begin_call();
    let grant = state
        .store
        .app_db()
        .widget_monitor_grant(&dashboard_id, &widget_id)
        .map_err(|_| CommandError::Validation("grant lookup failed".into()))?;
    if grant
        .as_ref()
        .is_some_and(|grant| grant.approved_by != call.context().principal_id.as_str())
    {
        return Err(CommandError::Validation("grant owner required".into()));
    }
    Ok(grant)
}

/// Test-only projection: never exposes the service context, scopes, SQL or errors.
#[cfg(feature = "webdriver")]
#[tauri::command]
pub async fn webdriver_monitor_evidence(
    state: State<'_, AppState>,
    dashboard_id: String,
    widget_id: String,
    tick: bool,
) -> Result<serde_json::Value, CommandError> {
    use datazen::jobs::domain::job::JobFilter;
    let grant =
        get_monitor_widget_schedule_grant(state.clone(), dashboard_id.clone(), widget_id.clone())
            .await?
            .ok_or_else(|| CommandError::Validation("grant required".into()))?;
    // Reading an existing receipt after revoke is allowed only for its approver.
    let mut tick_accepted = false;
    if tick {
        let scope = state.organization_scope()?;
        let dashboard = store_get_dashboard(&scope, &*state.store.app_db(), &dashboard_id)
            .cmd_err("webdriver_monitor_evidence")?;
        let widget = dashboard
            .widgets
            .iter()
            .find(|widget| widget.id == widget_id)
            .ok_or_else(|| CommandError::Validation("widget missing".into()))?;
        tick_accepted = state
            .monitor_engine
            .webdriver_scheduled_tick(&dashboard_id, widget)
            .await
            .is_ok();
    }
    let records = state
        .desktop_job_host
        .list(
            &grant.context(),
            JobFilter {
                organization_id: grant.context().organization_id.clone(),
                states: Vec::new(),
                owner: None,
                after: None,
                limit: Some(100),
            },
        )
        .await
        .map_err(|_| CommandError::Validation("receipt lookup failed".into()))?;
    let mut receipts = Vec::new();
    for record in records {
        let occurrence = &record.definition.payload["monitorOccurrence"];
        if record.view.kind != "monitorOccurrence"
            || occurrence["dashboardId"].as_str() != Some(dashboard_id.as_str())
            || occurrence["widgetId"].as_str() != Some(widget_id.as_str())
            || occurrence["grantId"].as_str() != Some(grant.id.as_str())
        {
            continue;
        }
        let details = state
            .desktop_job_host
            .details(&grant.context(), record.view.job_id.clone())
            .await
            .map_err(|_| CommandError::Validation("receipt details failed".into()))?;
        receipts.push(serde_json::json!({
            "jobId": record.view.job_id, "state": record.view.state,
            "scheduledAt": occurrence["scheduledAt"],
            "results": details.domain_results,
        }));
    }
    let physical_dispatches = grant
        .scopes
        .iter()
        .map(|scope| {
            let id = scope.target.connection_id.as_str();
            let count = crate::platform::session_backend::webdriver_dispatch::count(id)
                .ok_or_else(|| {
                    CommandError::Validation("dedicated fixture counter unavailable".into())
                })?;
            Ok(serde_json::json!({ "connectionId": id, "count": count }))
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    Ok(
        serde_json::json!({ "tickAccepted": tick_accepted, "receipts": receipts,
        "physicalDispatches": physical_dispatches,
        "stage": datazen::monitor::application::evidence::stage(&dashboard_id, &widget_id) }),
    )
}
