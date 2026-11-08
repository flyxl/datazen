//! Dashboard 域路由（W8 刀3d-2；契约 §6.15 的 HTTP 薄绑定）——仪表板
//! CRUD、组件创建、运行历史、手工执行与 monitor 授权从 legacy IPC 命令
//! 轨收敛进 MethodMap。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `listDashboards` | `GET /api/v1/dashboards` | — | 200 | — → `Dashboard[]` |
//! | `getDashboard` | `GET /api/v1/dashboards/{id}` | — | 200 | — → `Dashboard` |
//! | `saveDashboard` | `PUT /api/v1/dashboards/{id}` | ✓ | 200 | `Dashboard & SubmitRequest` → `Dashboard`（存储后回读） |
//! | `deleteDashboard` | `DELETE /api/v1/dashboards/{id}` | — | 204 | —（未知 id → 404，桌面 `NotFound` 语义） |
//! | `setDashboardRefreshPaused` | `PATCH /api/v1/dashboards/{id}/refresh-paused` | ✓ | 204 | `{ paused } & SubmitRequest` → — |
//! | `findDashboardWorkflowRefs` | `GET /api/v1/dashboards/workflow-refs?workflowId=` | — | 200 | — → `DashboardWorkflowRef[]` |
//! | `createWidgetFromSql` | `POST /api/v1/dashboards/{id}/widgets/from-sql` | ✓ | 200 | `CreateWidgetFromSqlParams & SubmitRequest` → `CreateWidgetResult` |
//! | `createWidgetFromWorkflow` | `POST /api/v1/dashboards/{id}/widgets/from-workflow` | ✓ | 200 | `CreateWidgetFromWorkflowParams & SubmitRequest` → `CreateWidgetResult` |
//! | `updateHiddenWidgetSql` | `POST /api/v1/dashboards/hidden-widget-sql` | ✓ | 204 | `UpdateHiddenWidgetSqlParams & SubmitRequest` → — |
//! | `runDashboardWidget` | `POST /api/v1/dashboards/{id}/widgets/{widgetId}/run` | ✓ | 200 | `SubmitRequest` → `WidgetRun`（同步应答；严格档） |
//! | `listWidgetRuns` | `GET /api/v1/dashboards/{id}/widgets/{widgetId}/runs?limit=` | — | 200 | — → `RunIndexEntry[]` |
//! | `getWidgetRun` | `GET /api/v1/dashboards/{id}/widgets/{widgetId}/runs/{runId}` | — | 200 | — → `WidgetRun` |
//! | `getMonitorScheduleGrant` | `GET /api/v1/dashboards/{id}/widgets/{widgetId}/schedule-grant` | — | 200 | — → `MonitorGrant \| null` |
//! | `authorizeMonitorSchedule` | `POST /api/v1/dashboards/{id}/widgets/{widgetId}/schedule-grant` | ✓ | 200 | `{ ttlSecs } & SubmitRequest` → `MonitorGrant` |
//! | `revokeMonitorSchedule` | `DELETE /api/v1/dashboards/{id}/widgets/{widgetId}/schedule-grant` | — | 204 | —（无授权单 = 幂等 no-op） |
//!
//! 域裁决（契约 §6.15 全文逐字对照）：
//!
//! - **归属 = 组织级**（CM-05）：v1 单组织部署，state 持有宿主配置组织
//!   的 scoped 件，每 handler 先过 `ensure_bound_organization`（aiHost /
//!   workflow 域同款闸——请求组织 ≠ 宿主配置组织 ⇒ 404 合并口径）。
//! - **桌面专属不进 HTTP 面**：`export_dashboard_with_dialog` /
//!   `import_dashboard_with_dialog` 是文件对话框语义（浏览器侧走产物
//!   上传/下载——§6.5 在账端点）；`webdriver_monitor_evidence` 是测试
//!   专用投影。`WEB_CAPABILITIES` 声明缺位，web 端如实报
//!   `CapabilityUnsupported`。
//! - **save 回读**：`saveDashboard` 应答存储后的形态（共享层
//!     `save_and_fetch`——归一化/时间戳权威后的回读，不是调用方发送的
//!   回声；桌面 IPC 同语义）。
//! - **执行是同步应答**（桌面 IPC 同语义；workflow execute / aiHost 同款
//!   先例）：`runDashboardWidget` 走共享调度引擎的 `tick_widget`
//!   （并发信号量 + 工作流锁），请求真主体经域级互斥下的身份槽注入
//!   （调度路径不走身份槽——授权单服务身份，见 `dashboard::scheduler`
//!   模块头）。受理面入限流严格档。
//! - **保存/删除/暂停后重载调度表**：共享引擎 `reload`（桌面
//!   `reload_from_store` 的 server 投影——无 tray 半边）。组件创建不
//!   重载（桌面同款：新组件默认 manual 刷新，进调度表的路径是下一次
//!   保存）。
//! - **monitor 授权面**：`authorize` 是完整校验链（桌面同链：状态闸 →
//!   工作流形状 → 只读 SQL → PostgreSQL/MySQL → 目标规范化）；`revoke`
//!   仅批准人；`get` 仅批准人可读（`grant owner required`，400——桌面
//!   `CommandError::Validation` 同语义，不是 404：调用方就是仪表板的
//!   编辑者，「有单但不是你的」是显式拒绝而不是存在性隐藏）。
//! - **令牌**：带体写面核 `SubmitRequest` 在场性与 `idempotencyKey`
//!   绑定（sessions 域同款；HMAC 轨 `saveUserAsset` 操作族）。DELETE
//!   面（delete / revoke）不带体，与 queryAssets / schemaDiff / workflow
//!   的 DELETE 现行口径一致。
//! - **错误 wire**：存储故障 → 503 `serviceUnavailable`；校验失败 →
//!   400 `invalidArgument`（文案与桌面 IPC `Validation` 同源）；未知
//!   id 读 → 404（防枚举固定文案）。

use std::sync::Arc;

use axum::extract::{Path, Query as QueryParams, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use datazen::dashboard::repository as dashboard_repository;
use datazen::dashboard::runs as dashboard_runs;
use datazen::dashboard::types::{Dashboard, ViewMode};
use datazen::shared::error::{ApiError, ApiErrorCode};

use crate::dashboard::creation::ServerWidgetCreationHost;
use crate::dashboard::grants as monitor_grants;
use crate::execution::resource_host::ServerResourceExecutionHost;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::{
    ServerConnections, ServerDashboardRuns, ServerDashboardStore, ServerMonitorGrants,
    ServerProfiles, ServerSettingsStore, ServerWorkflowStore,
};

/// 防枚举固定文案（跨组织/未知 id 同答）。
const NOT_VISIBLE_MESSAGE: &str = "The requested resource does not exist.";

/// Dashboard 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct DashboardState {
    pub dashboards: ServerDashboardStore,
    pub runs: ServerDashboardRuns,
    pub grants: ServerMonitorGrants,
    pub workflows: ServerWorkflowStore,
    /// 共享 registry（组件创建的定义读/写面）。
    pub registry: Arc<datazen::workflow::registry::WorkflowRegistry>,
    pub profiles: Arc<ServerProfiles>,
    pub connections: Arc<ServerConnections>,
    pub resource_host: Arc<ServerResourceExecutionHost>,
    pub settings: Arc<ServerSettingsStore>,
    /// 监控引擎（手工 tick + 调度面）。
    pub monitor: Arc<crate::dashboard::scheduler::ServerMonitorScheduler>,
    /// 手工 tick 的域级互斥（身份槽纪律；见 `dashboard::scheduler` 模块头）。
    pub manual_gate: Arc<tokio::sync::Mutex<()>>,
    /// 宿主配置的组织（v1 单组织；归属闸的比对值）。
    pub organization_id: String,
}

/// Dashboard 域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内）。
pub fn dashboard_router(state: DashboardState) -> Router {
    Router::new()
        .route("/api/v1/dashboards", get(handler_list_dashboards))
        .route(
            "/api/v1/dashboards/workflow-refs",
            get(handler_find_workflow_refs),
        )
        .route(
            "/api/v1/dashboards/hidden-widget-sql",
            post(handler_update_hidden_widget_sql),
        )
        .route(
            "/api/v1/dashboards/{id}",
            get(handler_get_dashboard)
                .put(handler_save_dashboard)
                .delete(handler_delete_dashboard),
        )
        .route(
            "/api/v1/dashboards/{id}/refresh-paused",
            axum::routing::patch(handler_set_refresh_paused),
        )
        .route(
            "/api/v1/dashboards/{id}/widgets/from-sql",
            post(handler_create_widget_from_sql),
        )
        .route(
            "/api/v1/dashboards/{id}/widgets/from-workflow",
            post(handler_create_widget_from_workflow),
        )
        .route(
            "/api/v1/dashboards/{id}/widgets/{widget_id}/run",
            post(handler_run_widget),
        )
        .route(
            "/api/v1/dashboards/{id}/widgets/{widget_id}/runs",
            get(handler_list_widget_runs),
        )
        .route(
            "/api/v1/dashboards/{id}/widgets/{widget_id}/runs/{run_id}",
            get(handler_get_widget_run),
        )
        .route(
            "/api/v1/dashboards/{id}/widgets/{widget_id}/schedule-grant",
            get(handler_get_schedule_grant)
                .post(handler_authorize_schedule)
                .delete(handler_revoke_schedule),
        )
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 归属闸（aiHost / workflow 域同款）：请求组织 ≠ 宿主配置组织 ⇒ 404
/// 合并口径。
fn ensure_bound_organization(
    state: &DashboardState,
    identity: &SessionIdentity,
) -> Result<(), Response> {
    if identity.ctx.organization_id.as_str() == state.organization_id {
        return Ok(());
    }
    Err(not_found_response())
}

/// 404 固定文案应答（未知/越组织同答，防枚举）。
fn not_found_response() -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

/// 令牌在场性 + `idempotencyKey` 绑定核对（sessions 域同款）。
fn ensure_submission_token(body: &Value) -> Result<(), Response> {
    let Some(token) = body.get("submissionToken") else {
        return Err(invalid_argument("submissionToken is required."));
    };
    let claimed = token.get("idempotencyKey").and_then(Value::as_str);
    let bound = body.get("idempotencyKey").and_then(Value::as_str);
    if let (Some(claimed), Some(bound)) = (claimed, bound) {
        if claimed != bound {
            return Err(invalid_argument(
                "submissionToken does not bind this request's idempotencyKey.",
            ));
        }
    }
    Ok(())
}

/// 存储故障 → 503（settings 域同款映射）。
fn store_unavailable(message: impl std::fmt::Display) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        format!("dashboard store: {message}"),
    ))
}

/// 保存/删除/暂停后的调度表重载（best-effort：失败不阻断应答，warn——
/// 引擎下一秒的 poll 仍按旧表跑，语义与桌面 `reload_from_store` 的
/// 非阻断口径一致）。
async fn reload_monitor(state: &DashboardState) {
    if let Err(error) = state.monitor.core().reload().await {
        tracing::warn!(%error, "monitor schedule reload failed");
    }
}

// ── 仪表板 CRUD ─────────────────────────────────────────────────────────────

/// `listDashboards`（GET /api/v1/dashboards）→ 200 `Dashboard[]`。
async fn handler_list_dashboards(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = state.dashboards.scope();
    match dashboard_repository::list_dashboards(&scope, &state.dashboards) {
        Ok(dashboards) => (StatusCode::OK, Json(dashboards)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `getDashboard`（GET /api/v1/dashboards/{id}）→ 200 `Dashboard`。
async fn handler_get_dashboard(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path(dashboard_id): Path<String>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = state.dashboards.scope();
    match dashboard_repository::get_dashboard(&scope, &state.dashboards, &dashboard_id) {
        Ok(dashboard) => (StatusCode::OK, Json(dashboard)).into_response(),
        Err(dashboard_repository::DashboardRepositoryError::NotFound(_)) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `saveDashboard`（PUT /api/v1/dashboards/{id}）→ 200 `Dashboard`
/// （存储后回读——归一化/时间戳权威后的形态）。体 = `Dashboard`（id 与
/// 路径一致，不一致 400 明拒）。
async fn handler_save_dashboard(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path(dashboard_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let dashboard: Dashboard = match serde_json::from_value(body) {
        Ok(dashboard) => dashboard,
        Err(source) => return invalid_argument(format!("saveDashboard: {source}")),
    };
    if dashboard.id != dashboard_id {
        return invalid_argument(format!(
            "saveDashboard: body id '{}' does not match the route id '{dashboard_id}'.",
            dashboard.id
        ));
    }
    let scope = state.dashboards.scope();
    match dashboard_repository::save_and_fetch(&scope, &state.dashboards, dashboard) {
        Ok(saved) => {
            reload_monitor(&state).await;
            (StatusCode::OK, Json(saved)).into_response()
        }
        Err(dashboard_repository::DashboardRepositoryError::NotFound(_)) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `deleteDashboard`（DELETE /api/v1/dashboards/{id}）→ 204。未知 id →
/// 404（桌面 `delete_dashboard` 的 `NotFound` 语义——与 workflow 域的
/// 幂等 no-op 不同，这是桌面行为的事实投影）。
async fn handler_delete_dashboard(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path(dashboard_id): Path<String>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = state.dashboards.scope();
    match dashboard_repository::delete_dashboard(&scope, &state.dashboards, &dashboard_id) {
        Ok(()) => {
            reload_monitor(&state).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(dashboard_repository::DashboardRepositoryError::NotFound(_)) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `setDashboardRefreshPaused`（PATCH /api/v1/dashboards/{id}/refresh-paused）
/// → 204。体 `{ paused }`。
async fn handler_set_refresh_paused(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path(dashboard_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let Some(paused) = body.get("paused").and_then(Value::as_bool) else {
        return invalid_argument("setDashboardRefreshPaused: paused is required.");
    };
    let scope = state.dashboards.scope();
    match dashboard_repository::set_dashboard_refresh_paused(
        &scope,
        &state.dashboards,
        &dashboard_id,
        paused,
    ) {
        Ok(()) => {
            reload_monitor(&state).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(dashboard_repository::DashboardRepositoryError::NotFound(_)) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `findDashboardWorkflowRefs`（GET /api/v1/dashboards/workflow-refs
/// ?workflowId=）→ 200 `DashboardWorkflowRef[]`（工作流删除闸的同源
/// 查询）。
async fn handler_find_workflow_refs(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<WorkflowRefsParams>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let Some(workflow_id) = params.workflow_id else {
        return invalid_argument("findDashboardWorkflowRefs: workflowId is required.");
    };
    let scope = state.dashboards.scope();
    match dashboard_repository::find_workflow_refs(&scope, &state.dashboards, &workflow_id) {
        Ok(refs) => (StatusCode::OK, Json(refs)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRefsParams {
    workflow_id: Option<String>,
}

// ── 组件创建 ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWidgetFromSqlBody {
    connection_id: String,
    sql: String,
    #[serde(default)]
    title: Option<String>,
    view_mode: ViewMode,
    #[serde(default)]
    chart_config: Option<datazen::dashboard::types::ChartConfig>,
}

/// `createWidgetFromSql`（POST /api/v1/dashboards/{id}/widgets/from-sql）
/// → 200 `CreateWidgetResult`（共享用例：SQL → 隐藏工作流铸造 → widget
/// 行合成）。
async fn handler_create_widget_from_sql(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path(dashboard_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let payload: CreateWidgetFromSqlBody = match serde_json::from_value(body) {
        Ok(payload) => payload,
        Err(source) => return invalid_argument(format!("createWidgetFromSql: {source}")),
    };
    // 仪表板在场性先行（404 分家——共享用例的 Store 错误是文案型，
    // 防枚举口径由域路由持有）。
    let scope = state.dashboards.scope();
    if let Err(error) =
        dashboard_repository::get_dashboard(&scope, &state.dashboards, &dashboard_id)
    {
        return match error {
            dashboard_repository::DashboardRepositoryError::NotFound(_) => not_found_response(),
            other => store_unavailable(other),
        };
    }
    let host = ServerWidgetCreationHost {
        dashboards: state.dashboards.clone(),
        registry: state.registry.clone(),
    };
    match datazen::dashboard::create::create_widget_from_sql(
        &scope,
        &host,
        &dashboard_id,
        &payload.connection_id,
        &payload.sql,
        payload.title,
        payload.view_mode,
        payload.chart_config,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(datazen::dashboard::create::CreateWidgetError::Validation(message)) => {
            invalid_argument(format!("createWidgetFromSql: {message}"))
        }
        Err(error) => invalid_argument(format!("createWidgetFromSql: {error}")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWidgetFromWorkflowBody {
    workflow_id: String,
    #[serde(default)]
    title: Option<String>,
    view_mode: ViewMode,
    #[serde(default)]
    chart_config: Option<datazen::dashboard::types::ChartConfig>,
}

/// `createWidgetFromWorkflow`（POST /api/v1/dashboards/{id}/widgets/
/// from-workflow）→ 200 `CreateWidgetResult`。
async fn handler_create_widget_from_workflow(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path(dashboard_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let payload: CreateWidgetFromWorkflowBody = match serde_json::from_value(body) {
        Ok(payload) => payload,
        Err(source) => return invalid_argument(format!("createWidgetFromWorkflow: {source}")),
    };
    // 仪表板在场性先行（404 分家，同 from-sql）。
    let scope = state.dashboards.scope();
    if let Err(error) =
        dashboard_repository::get_dashboard(&scope, &state.dashboards, &dashboard_id)
    {
        return match error {
            dashboard_repository::DashboardRepositoryError::NotFound(_) => not_found_response(),
            other => store_unavailable(other),
        };
    }
    let host = ServerWidgetCreationHost {
        dashboards: state.dashboards.clone(),
        registry: state.registry.clone(),
    };
    match datazen::dashboard::create::create_widget_from_workflow(
        &scope,
        &host,
        &dashboard_id,
        &payload.workflow_id,
        payload.title,
        payload.view_mode,
        payload.chart_config,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(datazen::dashboard::create::CreateWidgetError::Validation(message)) => {
            invalid_argument(format!("createWidgetFromWorkflow: {message}"))
        }
        Err(error) => invalid_argument(format!("createWidgetFromWorkflow: {error}")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateHiddenWidgetSqlBody {
    workflow_id: String,
    connection_id: String,
    sql: String,
}

/// `updateHiddenWidgetSql`（POST /api/v1/dashboards/hidden-widget-sql）
/// → 204（隐藏工作流的 SQL 原地更新——键是 workflowId，不挂仪表板
/// 路径）。
async fn handler_update_hidden_widget_sql(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let payload: UpdateHiddenWidgetSqlBody = match serde_json::from_value(body) {
        Ok(payload) => payload,
        Err(source) => return invalid_argument(format!("updateHiddenWidgetSql: {source}")),
    };
    let host = ServerWidgetCreationHost {
        dashboards: state.dashboards.clone(),
        registry: state.registry.clone(),
    };
    let scope = state.dashboards.scope();
    match datazen::dashboard::create::update_hidden_workflow_sql(
        &scope,
        &host,
        &payload.workflow_id,
        &payload.connection_id,
        &payload.sql,
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(datazen::dashboard::create::CreateWidgetError::Validation(message)) => {
            invalid_argument(format!("updateHiddenWidgetSql: {message}"))
        }
        Err(error) => invalid_argument(format!("updateHiddenWidgetSql: {error}")),
    }
}

// ── 运行历史与手工执行 ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WidgetRunsParams {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

/// `listWidgetRuns`（GET …/widgets/{widgetId}/runs?limit=）→ 200
/// `RunIndexEntry[]`（`started_at` 降序，桌面同序）。
async fn handler_list_widget_runs(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path((dashboard_id, widget_id)): Path<(String, String)>,
    QueryParams(params): QueryParams<WidgetRunsParams>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = state.dashboards.scope();
    match dashboard_runs::list_run_index(
        &scope,
        &state.runs,
        &dashboard_id,
        &widget_id,
        params.limit,
    ) {
        Ok(index) => (StatusCode::OK, Json(index)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `getWidgetRun`（GET …/widgets/{widgetId}/runs/{runId}）→ 200
/// `WidgetRun`；未知/越组织/别的 widget 的运行 → 404 固定文案。
async fn handler_get_widget_run(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path((dashboard_id, widget_id, run_id)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = state.dashboards.scope();
    match dashboard_runs::get_run(&scope, &state.runs, &dashboard_id, &widget_id, &run_id) {
        Ok(run) => (StatusCode::OK, Json(run)).into_response(),
        Err(dashboard_runs::DashboardRunsError::NotFound(_)) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `runDashboardWidget`（POST …/widgets/{widgetId}/run）→ 200 `WidgetRun`
/// （同步应答）。走共享调度引擎的 `tick_widget`（并发信号量 + 工作流
/// 锁）；请求真主体经域级互斥下的身份槽注入（`dashboard::scheduler`
/// 模块头的身份槽纪律）。
async fn handler_run_widget(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path((dashboard_id, widget_id)): Path<(String, String)>,
    body: Option<Json<Value>>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    // 受理面令牌（体可缺席——`Option<Json>`；在场时核绑定）。
    if let Some(Json(body)) = body.as_ref() {
        if let Err(response) = ensure_submission_token(body) {
            return response;
        }
    }
    // 仪表板与组件在场性（404 固定文案；widget 归属仪表板核对）。
    let scope = state.dashboards.scope();
    let dashboard =
        match dashboard_repository::get_dashboard(&scope, &state.dashboards, &dashboard_id) {
            Ok(dashboard) => dashboard,
            Err(dashboard_repository::DashboardRepositoryError::NotFound(_)) => {
                return not_found_response()
            }
            Err(error) => return store_unavailable(error),
        };
    let Some(widget) = dashboard.widgets.iter().find(|w| w.id == widget_id) else {
        return not_found_response();
    };
    let widget = widget.clone();

    // 身份槽纪律：域级互斥内注入请求真主体 → 引擎 tick → 清空。
    let _guard = state.manual_gate.lock().await;
    state
        .monitor
        .host()
        .set_interactive_identity(identity.ctx.clone())
        .await;
    let outcome = state
        .monitor
        .core()
        .tick_widget(&dashboard_id, &widget)
        .await;
    state.monitor.host().clear_interactive_identity().await;
    drop(_guard);

    match outcome {
        Ok(run) => (StatusCode::OK, Json(run)).into_response(),
        // 引擎错误（执行编排/落库）→ 400 家族文案如实携带（桌面
        // `CommandError` 同族；未知连接/工作流缺失都走这里）。
        Err(error) => invalid_argument(format!("runDashboardWidget: {error}")),
    }
}

// ── monitor 授权面 ──────────────────────────────────────────────────────────

/// `getMonitorScheduleGrant`（GET …/schedule-grant）→ 200
/// `MonitorGrant | null`（JSON null = 无授权单；仅批准人可读）。
async fn handler_get_schedule_grant(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path((dashboard_id, widget_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    match monitor_grants::read(&state.grants, &identity.ctx, &dashboard_id, &widget_id) {
        Ok(grant) => (StatusCode::OK, Json(grant)).into_response(),
        Err(error) => invalid_argument(format!("getMonitorScheduleGrant: {error}")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorizeScheduleBody {
    #[serde(default = "default_ttl")]
    ttl_secs: u32,
}

fn default_ttl() -> u32 {
    3600
}

/// `authorizeMonitorSchedule`（POST …/schedule-grant）→ 200 `MonitorGrant`
/// （体 `{ ttlSecs }`；完整校验链见 `dashboard::grants::authorize`）。
async fn handler_authorize_schedule(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path((dashboard_id, widget_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let payload: AuthorizeScheduleBody = match serde_json::from_value(body) {
        Ok(payload) => payload,
        Err(source) => return invalid_argument(format!("authorizeMonitorSchedule: {source}")),
    };
    match monitor_grants::authorize(
        &state.dashboards,
        &state.workflows,
        &state.grants,
        &state.profiles,
        &state.connections,
        &state.resource_host,
        &identity.ctx,
        &dashboard_id,
        &widget_id,
        payload.ttl_secs,
    )
    .await
    {
        Ok(grant) => (StatusCode::OK, Json(grant)).into_response(),
        Err(monitor_grants::GrantFlowError::Validation(message)) => {
            invalid_argument(format!("authorizeMonitorSchedule: {message}"))
        }
    }
}

/// `revokeMonitorSchedule`（DELETE …/schedule-grant）→ 204（无授权单 =
/// 幂等 no-op；仅批准人可撤）。
async fn handler_revoke_schedule(
    State(state): State<DashboardState>,
    identity: SessionIdentity,
    Path((dashboard_id, widget_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    match monitor_grants::revoke(&state.grants, &identity.ctx, &dashboard_id, &widget_id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => invalid_argument(format!("revokeMonitorSchedule: {error}")),
    }
}
