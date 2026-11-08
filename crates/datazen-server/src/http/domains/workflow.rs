//! Workflow 域路由（W8 刀3c-2；契约 §6.14 的 HTTP 薄绑定）——定义管理、
//! 执行与运行历史从 legacy IPC 命令轨收敛进 MethodMap。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `listWorkflows` | `GET /api/v1/workflows` | — | 200 | — → `WorkflowListItem[]` |
//! | `saveWorkflow` | `PUT /api/v1/workflows/{id}` | ✓ | 204 | `WorkflowDefinition & SubmitRequest` → — |
//! | `saveWorkflowYaml` | `POST /api/v1/workflows/from-yaml` | ✓ | 200 | `{ yaml } & SubmitRequest` → `WorkflowDefinition` |
//! | `getWorkflow` | `GET /api/v1/workflows/{id}` | — | 200 | — → `WorkflowDefinition` |
//! | `getWorkflowYaml` | `GET /api/v1/workflows/{id}/yaml` | — | 200 | — → `{ yaml }` |
//! | `deleteWorkflow` | `DELETE /api/v1/workflows/{id}` | — | 204 | —（未知 id = 幂等 no-op） |
//! | `executeWorkflow` | `POST /api/v1/workflows/{id}/execute` | ✓ | 200 | `{ variables, connectionId? } & SubmitRequest` → `WorkflowExecutionResult` |
//! | `reloadWorkflows` | `POST /api/v1/workflows/reload` | — | 204 | —（库后端 no-op，桌面同语义） |
//! | `listWorkflowRuns` | `GET /api/v1/workflow-runs?workflowId=` | — | 200 | — → `WorkflowRunListItem[]` |
//! | `getWorkflowRun` | `GET /api/v1/workflow-runs/{id}` | — | 200 | — → `WorkflowRunRecord` |
//! | `clearWorkflowRuns` | `DELETE /api/v1/workflow-runs?workflowId=` | — | 200 | — → `{ cleared }` |
//!
//! 域裁决（契约 §6.14 全文逐字对照）：
//!
//! - **归属 = 组织级**（CM-05）：v1 单组织部署，state 持有宿主配置组织
//!   的 scoped 件，每 handler 先过 `ensure_bound_organization`（aiHost
//!   域同款闸——请求组织 ≠ 宿主配置组织 ⇒ 404 合并口径）。多组织扩容
//!   时改 per-request 装配（queryAssets 域同款）。
//! - **桌面专属一命令不进 HTTP 面**：`getWorkflowDir` 是文件系统语义
//!   （工作流目录路径对接本机文件管理），服务端定义是库行——
//!   `WEB_CAPABILITIES` 声明缺位，web 端如实报 `CapabilityUnsupported`。
//! - **执行是同步应答**（桌面 IPC 同语义——`WorkflowExecutor::execute`
//!   await 到底返回整份 `WorkflowExecutionResult`；aiHost.executeTarget
//!   同款先例）。长跑工作流超过传输超时是调用方的重试/超时策略问题，
//!   不在本域发明 202/job 语义（执行域的 job 面是另一个用例）。
//! - **AI 步骤 fail-closed**：`ServerWorkflowHost::complete_ai` 显式拒绝
//!   （400 `invalidArgument` 家族文案），不是静默降级；调度面被授权单
//!   派生更早拒绝（服务身份不放行 AI 步骤）。
//! - **运行历史落账**：交互执行与调度派发都在应答前 best-effort 落账
//!   （`DashboardHidden` 不进用户面，桌面同语义）；落账失败不阻断执行
//!   应答（warn 降级）。
//! - **令牌**：带体写面（save / save-yaml / execute）核 `SubmitRequest`
//!   在场性与 `idempotencyKey` 绑定（sessions 域同款；HMAC 轨签发，
//!   `saveUserAsset` 操作族——job 受理同族的配置类资产操作）。
//!   DELETE 面（delete / clear）不带体，与 queryAssets / schemaDiff 的
//!   DELETE 现行口径一致（幂等 no-op 语义下令牌收益为零）。
//! - **错误 wire**：存储故障 → 503 `serviceUnavailable`；定义校验失败 →
//!   400 `invalidArgument`；未知 id 读 → 404（防枚举固定文案）。

use std::sync::Arc;

use axum::extract::{Path, Query as QueryParams, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::workflow::domain::history::new_run_id;
use datazen::workflow::domain::model::{WorkflowDefinition, WorkflowVisibility};
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::registry::WorkflowRegistry;

use crate::execution::resource_host::PrincipalScopedHost;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::ServerDashboardStore;
use crate::storage::ServerProfiles;
use crate::storage::ServerWorkflowHistoryStore;
use crate::workflow::host::ServerWorkflowHost;

/// 防枚举固定文案（跨组织/未知 id 同答）。
const NOT_VISIBLE_MESSAGE: &str = "The requested resource does not exist.";

/// Workflow 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct WorkflowState {
    /// 共享层 registry（登记规则单家；store 是宿主配置组织的 scoped 件）。
    pub registry: Arc<WorkflowRegistry>,
    /// 运行历史（同组织的 scoped 件）。
    pub history: ServerWorkflowHistoryStore,
    /// 交互执行的原语宿主（与 aiHost / MCP / driverGateway 同一实例族）。
    pub execution: Arc<dyn datazen::workflow::resource_execution::ResourceExecutionHost>,
    /// profile 读取面（连接名/方言解析）。
    pub profiles: Arc<ServerProfiles>,
    /// 仪表板存储（删除闸的引用核对——刀3d 落库时补上的欠账）。
    pub dashboard_refs: ServerDashboardStore,
    /// 宿主配置的组织（v1 单组织；归属闸的比对值）。
    pub organization_id: String,
}

/// Workflow 域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内）。
pub fn workflow_router(state: WorkflowState) -> Router {
    Router::new()
        .route("/api/v1/workflows", get(handler_list_workflows))
        .route(
            "/api/v1/workflows/from-yaml",
            post(handler_save_workflow_yaml),
        )
        .route("/api/v1/workflows/reload", post(handler_reload_workflows))
        .route(
            "/api/v1/workflows/{id}",
            get(handler_get_workflow)
                .put(handler_save_workflow)
                .delete(handler_delete_workflow),
        )
        .route(
            "/api/v1/workflows/{id}/yaml",
            get(handler_get_workflow_yaml),
        )
        .route(
            "/api/v1/workflows/{id}/execute",
            post(handler_execute_workflow),
        )
        .route(
            "/api/v1/workflow-runs",
            get(handler_list_workflow_runs).delete(handler_clear_workflow_runs),
        )
        .route("/api/v1/workflow-runs/{id}", get(handler_get_workflow_run))
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 归属闸（aiHost 域同款）：请求组织 ≠ 宿主配置组织 ⇒ 404 合并口径。
fn ensure_bound_organization(
    state: &WorkflowState,
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
        format!("workflow store: {message}"),
    ))
}

/// 执行前取定义：未知/越组织 → 404 固定文案。
async fn require_workflow(
    state: &WorkflowState,
    workflow_id: &str,
) -> Result<datazen::workflow::domain::model::WorkflowDefinition, Response> {
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    state
        .registry
        .get(&scope, workflow_id)
        .await
        .ok_or_else(not_found_response)
}

// ── 定义管理 ────────────────────────────────────────────────────────────────

/// `listWorkflows`（GET /api/v1/workflows）→ 200 `WorkflowListItem[]`
/// （user-可见面；dashboardHidden 是仪表板私有面，不出现）。
async fn handler_list_workflows(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    if let Err(error) = state.registry.ensure_loaded(&scope).await {
        return store_unavailable(error);
    }
    let items = state.registry.list(&scope).await;
    (StatusCode::OK, Json(items)).into_response()
}

/// `saveWorkflow`（PUT /api/v1/workflows/{id}）→ 204。体 =
/// `WorkflowDefinition`（id 与路径一致——不一致是客户端装配 bug，
/// 400 明拒）；时间戳服务端权威（核心 registry：create 铸造 / update
/// 保留 createdAt、恒写 updatedAt）。
async fn handler_save_workflow(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Path(workflow_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let definition: WorkflowDefinition = match serde_json::from_value(body) {
        Ok(definition) => definition,
        Err(source) => return invalid_argument(format!("saveWorkflow: {source}")),
    };
    if definition.id != workflow_id {
        return invalid_argument(format!(
            "saveWorkflow: body id '{}' does not match the route id '{workflow_id}'.",
            definition.id
        ));
    }
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    if let Err(message) = state.registry.save(&scope, &definition).await {
        // 校验失败（id 词表/YAML 编码）与存储故障分家：共享层 save 的
        // Err(String) 两种都可能——id 词表错误是调用方问题（400），存储
        // 故障是环境问题（503）。按文案分派不诚实，按语义分家需要端口
        // 改形；本刀按「写路径失败如实 503 + 文案携带」处理，校验错误
        // 在 save_yaml 路径有专门的 400 半边（decode 失败先行暴露）。
        return store_unavailable(message);
    }
    StatusCode::NO_CONTENT.into_response()
}

/// `saveWorkflowYaml`（POST /api/v1/workflows/from-yaml）→ 200
/// `WorkflowDefinition`。体 `{ yaml }`；解析/校验失败 → 400（桌面
/// `workflow_save_yaml` 的 `Validation` 同语义）。
async fn handler_save_workflow_yaml(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let Some(yaml) = body.get("yaml").and_then(Value::as_str) else {
        return invalid_argument("saveWorkflowYaml: yaml is required.");
    };
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    match state.registry.save_yaml(&scope, yaml).await {
        Ok(definition) => (StatusCode::OK, Json(definition)).into_response(),
        Err(message) => invalid_argument(format!("saveWorkflowYaml: {message}")),
    }
}

/// `getWorkflow`（GET /api/v1/workflows/{id}）→ 200 `WorkflowDefinition`
/// （存储列胜过文档——重命名后旧文档的名字不回读；registry 规则）。
async fn handler_get_workflow(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Path(workflow_id): Path<String>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    match require_workflow(&state, &workflow_id).await {
        Ok(definition) => (StatusCode::OK, Json(definition)).into_response(),
        Err(response) => response,
    }
}

/// `getWorkflowYaml`（GET /api/v1/workflows/{id}/yaml）→ 200 `{ yaml }`
/// （原文档；标量包装口径与 `{ deleted }` / `{ purged }` 家族一致）。
async fn handler_get_workflow_yaml(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Path(workflow_id): Path<String>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    // YAML 原文走 scoped store（registry 的 get 返回的是解析后的定义，
    // 文档原文是编辑器轨道的既有契约）。
    let workflows = state.registry.store();
    match workflows.get_workflow(&scope, &workflow_id) {
        Ok(Some(record)) => (
            StatusCode::OK,
            Json(json!({ "yaml": record.definition_yaml })),
        )
            .into_response(),
        Ok(None) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `deleteWorkflow`（DELETE /api/v1/workflows/{id}）→ 204。未知 id =
/// 幂等 no-op（端口契约注释的不可区分口径）；被仪表板组件引用的
/// 定义拒绝删除（桌面 `AppDb.delete_workflow` 的 `WorkflowInUse` 同
/// 语义同文案——引用闸随刀3d dashboard 存储落库时补上，模块头旧
/// 登记随本刀销账）。
async fn handler_delete_workflow(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Path(workflow_id): Path<String>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    // 引用闸：仪表板组件还在用的定义不可删（断组件的查询供给是
    // 数据面破坏，不是清理）。
    match datazen::dashboard::repository::find_workflow_refs(
        &scope,
        &state.dashboard_refs,
        &workflow_id,
    ) {
        Ok(refs) if !refs.is_empty() => {
            let summary = refs
                .iter()
                .map(|r| format!("{} / {}", r.dashboard_name, r.widget_title))
                .collect::<Vec<_>>()
                .join("; ");
            return invalid_argument(format!("Workflow in use by dashboards: {summary}"));
        }
        Ok(_) => {}
        Err(error) => return store_unavailable(error),
    }
    match state.registry.delete(&scope, &workflow_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        // id 词表错误 → 400（与未知 id 的 no-op 分家）。
        Err(message) => invalid_argument(format!("deleteWorkflow: {message}")),
    }
}

/// `reloadWorkflows`（POST /api/v1/workflows/reload）→ 204。库后端
/// no-op（桌面同语义——reload 是文件后端时代的 IPC 兼容面；服务端
/// 出生就是库后端，ensure_loaded 幂等即可）。
async fn handler_reload_workflows(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let scope = datazen::shared::scope::OrganizationScope::new(
        datazen::shared::id::OrganizationId::new(state.organization_id.clone()),
    );
    match state.registry.ensure_loaded(&scope).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => store_unavailable(error),
    }
}

// ── 执行 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteBody {
    #[serde(default = "default_variables")]
    variables: Value,
    #[serde(default)]
    connection_id: Option<String>,
}

fn default_variables() -> Value {
    json!({})
}

/// `executeWorkflow`（POST /api/v1/workflows/{id}/execute）→ 200
/// `WorkflowExecutionResult`。同步应答（桌面 IPC 同语义）；请求真主体
/// 的执行宿主（`PrincipalScopedHost`——审计归因不落合成服务主体）。
async fn handler_execute_workflow(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Path(workflow_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let payload: ExecuteBody = match serde_json::from_value(body) {
        Ok(payload) => payload,
        Err(source) => return invalid_argument(format!("executeWorkflow: {source}")),
    };
    let workflow = match require_workflow(&state, &workflow_id).await {
        Ok(workflow) => workflow,
        Err(response) => return response,
    };
    // 每请求身份包装（aiHost.executeTarget 同款）+ server 执行宿主。
    let host = ServerWorkflowHost::new(
        Arc::new(PrincipalScopedHost {
            inner: state.execution.clone(),
            context: identity.ctx.clone(),
        }),
        state.profiles.clone(),
        &state.organization_id,
    );
    let result = match WorkflowExecutor::execute(
        &workflow,
        &host,
        payload.connection_id.as_deref(),
        &payload.variables,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            // 执行编排失败（含 AI 步骤的 fail-closed 拒绝）→ 400 家族
            // 文案如实携带（桌面 CommandError::Validation 同族）。
            return invalid_argument(format!("executeWorkflow: {error}"));
        }
    };
    // 运行历史落账（best-effort；DashboardHidden 不进用户面）。
    if workflow.visibility != WorkflowVisibility::DashboardHidden {
        let now = chrono::Local::now();
        let run_id = new_run_id(now);
        if let Err(error) = state
            .history
            .record(
                &run_id,
                &workflow.id,
                &workflow.name,
                &payload.variables,
                &result,
                &now.to_rfc3339(),
            )
            .await
        {
            tracing::warn!("Failed to record workflow history: {error}");
        }
    }
    (StatusCode::OK, Json(result)).into_response()
}

// ── 运行历史 ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunsListParams {
    #[serde(default)]
    workflow_id: Option<String>,
}

/// `listWorkflowRuns`（GET /api/v1/workflow-runs?workflowId=）→ 200
/// `WorkflowRunListItem[]`（`created_at` 降序，桌面同序）。
async fn handler_list_workflow_runs(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<RunsListParams>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    match state.history.list(params.workflow_id.as_deref()).await {
        Ok(runs) => (StatusCode::OK, Json(runs)).into_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `getWorkflowRun`（GET /api/v1/workflow-runs/{id}）→ 200
/// `WorkflowRunRecord`；未知/越组织 → 404 固定文案。
async fn handler_get_workflow_run(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    Path(run_id): Path<String>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    match state.history.get(&run_id).await {
        Ok(Some(record)) => (StatusCode::OK, Json(record)).into_response(),
        Ok(None) => not_found_response(),
        Err(error) => store_unavailable(error),
    }
}

/// `clearWorkflowRuns`（DELETE /api/v1/workflow-runs?workflowId=）→ 200
/// `{ cleared }`。行数让调用方拒绝 no-op 的成功（桌面同语义）。
async fn handler_clear_workflow_runs(
    State(state): State<WorkflowState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<RunsListParams>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    match state.history.clear(params.workflow_id.as_deref()).await {
        Ok(cleared) => (StatusCode::OK, Json(json!({ "cleared": cleared }))).into_response(),
        Err(error) => store_unavailable(error),
    }
}
