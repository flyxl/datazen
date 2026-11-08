//! Data Sync 域路由（W8 刀3e-3 + 刀3e-4b；契约 §6.16 的 HTTP 薄绑定）——
//! 任务/档案资产、配对分类、迁移运行历史读面、sync job 读回/取消，与
//! 执行面九方法（inspect / revalidate / compare / prepare / apply /
//! execute / SQL 预览 / 冲突检查 / 恢复核验）从 legacy IPC 命令轨收敛进
//! MethodMap。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `getSyncTasks` | `GET /api/v1/sync/tasks` | — | 200 | — → `SyncTask[]` |
//! | `deleteSyncTask` | `DELETE /api/v1/sync/tasks/{id}` | — | 204 | — |
//! | `getSyncProfiles` | `GET /api/v1/sync/profiles` | — | 200 | — → `SyncProfile[]` |
//! | `saveSyncProfile` | `PUT /api/v1/sync/profiles` | ✓ | 204 | `SyncProfile & SubmitRequest` → — |
//! | `deleteSyncProfile` | `DELETE /api/v1/sync/profiles/{id}` | — | 204 | — |
//! | `classifyDataSyncPair` | `POST /api/v1/sync/pairings` | — | 200 | `{ sourceDatabaseType, targetDatabaseType }` → `DataSyncPairingView` |
//! | `listMigrationRuns` | `GET /api/v1/migration-runs?operation=&status=&profileId=&connectionId=&offset=&limit=` | — | 200 | 过滤平铺查询参数 → `MigrationRunPage` |
//! | `getMigrationRun` | `GET /api/v1/migration-runs/{runId}` | — | 200 | — → `MigrationRunRecord`（miss → 404） |
//! | `listDataSyncJobs` | `GET /api/v1/sync/jobs` | — | 200 | — → `JobDetails[]` |
//! | `getDataSyncJob` | `GET /api/v1/sync/jobs/{id}` | — | 200 | — → `JobView` |
//! | `getDataSyncJobDetails` | `GET /api/v1/sync/jobs/{id}/details` | — | 200 | — → `JobDetails` |
//! | `getDataSyncJobPreview` | `GET /api/v1/sync/jobs/{id}/preview` | — | 200 | — → `SyncComparisonPreview` |
//! | `cancelDataSync` | `POST /api/v1/sync/jobs/{id}/cancel` | ✓ | 200 | `SubmitRequest` → `{ cancelled }` |
//! | `inspectDataSync` | `POST /api/v1/sync/inspections` | — | 200 | 检视请求 → `TableResult[]` |
//! | `revalidateDataSync` | `POST /api/v1/sync/revalidations` | — | 200 | 复核请求 → `{ staleTables }` |
//! | `compareDataSync` | `POST /api/v1/sync/comparisons` | — | 200 | 比较请求 → `SyncComparisonPreview` |
//! | `startDataSyncPrepareJob` | `POST /api/v1/sync/jobs/prepare` | — | 202 | 比较请求 → `JobView` |
//! | `startDataSyncApplyJob` | `POST /api/v1/sync/jobs/apply` | — | 202 | `{ sourceDbSessionId, targetDbSessionId, request }` → `JobView` |
//! | `executeDataSync` | `POST /api/v1/sync/executions` | — | 200 | `{ request, profile? }` → `DataSyncExecutionResponse` |
//! | `generateDataSyncSql` | `POST /api/v1/sync/sql-preview` | — | 200 | `{ planId, selection?, options }` → `SqlStatement[]` |
//! | `checkSyncConflicts` | `POST /api/v1/sync/tasks/{id}/conflicts` | — | 200 | — → `{ hasConflicts, conflicts }` |
//! | `verifyDataSyncRecovery` | `POST /api/v1/sync/jobs/{id}/recovery-verification` | — | 200 | 重授权请求 → `JobDetails` |
//!
//! 域裁决（契约 §6.16）：
//!
//! - **令牌**：带体写面（save profile、cancel）带 `SubmitRequest`，
//!   handler 核在场性 + `idempotencyKey` 绑定（schema_diff 域同款；
//!   HMAC 轨——配置类操作）。DELETE 面不设令牌（queryAssets /
//!   schemaDiff 先例：无体可携令牌，幂等删除由 L4 CSRF 护航）。
//!   pairings 是 POST 读（纯函数投影，契约 §6.7 的 POST 读同款裁决），
//!   不带令牌。
//! - **归属 = organization**（任务/档案是组织级资产，桌面单租户全体共享
//!   的服务端收敛；v15 表同口径）：scoped 存储按认证链事实逐请求装配
//!   （INV-01）。迁移运行历史（v12 表）同口径——Data Sync / Schema
//!   Diff / Data Transfer 三类运行共用，`operation` 区分。
//! - **统一 404**：单资源读（migration run / job）的 miss 与越组织不可
//!   区分（防枚举）；任务/档案 DELETE 幂等 204（跨边界与命中同应答）。
//! - **迁移运行历史路径**：`/api/v1/migration-runs` 顶层资源（三类运行
//!   的共享历史，schemaDiff 写面已随刀3b 落账；读面按契约 §6.13 登记
//!   随 sync 域收口）。
//! - **job 读面对空而真实的状态工作**：受理面（prepare/apply）随刀3e-4
//!   落地，本刀读回空集 / 404 是诚实应答，不是 fail-closed 占位。
//! - **错误 wire**：job 仓储 NotFound→404；校验→400 `invalidArgument`；
//!   存储→503。
//! - **写面体先令牌后 DTO**：`Json<Value>` 先核令牌再反序列化强类型
//!   （schema_diff 域同款——令牌字段不在业务 DTO 里）。

use std::sync::Arc;

use axum::extract::{Path, Query as QueryParams, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use datazen::history::MigrationRunFilter;
use datazen::migration::data_sync::profile::SyncProfile;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::scope::OrganizationScope;

use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::migration_runs::ServerMigrationRuns;
use crate::storage::sync_assets::{ServerSyncProfiles, ServerSyncTasks};
use crate::storage::AdminStore;
use crate::sync::assets::{save_sync_profile, SyncAssetError};
use crate::sync::execution::{
    SyncExecution, SyncExecutionError, SyncInspectRequest, SyncPrepareRequest, SyncRecoveryRequest,
    SyncRevalidateRequest,
};
use crate::sync::jobs::{SyncJobError, SyncJobs};

/// Data Sync 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct SyncState {
    /// 管理库（v15 任务/档案 + v12 迁移历史；迁移由 bootstrap 负责）。
    pub store: Arc<AdminStore>,
    /// 任务存储（v15 `sync_tasks`）。
    pub tasks: Arc<ServerSyncTasks>,
    /// 档案存储（v15 `sync_profiles`）。
    pub profiles: Arc<ServerSyncProfiles>,
    /// 迁移运行历史（v12 `migration_run_history`；与 schema_diff 写面同源）。
    pub migration_runs: Arc<ServerMigrationRuns>,
    /// job 服务（JobHost + 计划注册表 + 运行态）。
    pub jobs: SyncJobs,
    /// 执行面服务（刀3e-4b：与 `jobs` 共享同一 JobHost/计划注册表/运行态）。
    pub execution: SyncExecution,
}

/// Data Sync 域 Router 片段（merge 进 `build_router_with_auth` 的
/// 认证链 L2–L6 之内）。
pub fn sync_router(state: SyncState) -> Router {
    Router::new()
        .route("/api/v1/sync/tasks", get(handler_list_tasks))
        .route("/api/v1/sync/tasks/{id}", delete(handler_delete_task))
        .route(
            "/api/v1/sync/profiles",
            get(handler_list_profiles).put(handler_save_profile),
        )
        .route("/api/v1/sync/profiles/{id}", delete(handler_delete_profile))
        .route("/api/v1/sync/pairings", post(handler_classify_pairing))
        .route("/api/v1/migration-runs", get(handler_list_migration_runs))
        .route(
            "/api/v1/migration-runs/{runId}",
            get(handler_get_migration_run),
        )
        .route("/api/v1/sync/jobs", get(handler_list_jobs))
        .route("/api/v1/sync/jobs/prepare", post(handler_prepare_job))
        .route("/api/v1/sync/jobs/apply", post(handler_apply_job))
        .route("/api/v1/sync/jobs/{id}", get(handler_get_job))
        .route("/api/v1/sync/jobs/{id}/details", get(handler_job_details))
        .route("/api/v1/sync/jobs/{id}/preview", get(handler_job_preview))
        .route("/api/v1/sync/jobs/{id}/cancel", post(handler_cancel_job))
        .route(
            "/api/v1/sync/jobs/{id}/recovery-verification",
            post(handler_verify_recovery),
        )
        .route("/api/v1/sync/inspections", post(handler_inspect))
        .route("/api/v1/sync/revalidations", post(handler_revalidate))
        .route("/api/v1/sync/comparisons", post(handler_compare))
        .route("/api/v1/sync/executions", post(handler_execute))
        .route("/api/v1/sync/sql-preview", post(handler_generate_sql))
        .route(
            "/api/v1/sync/tasks/{id}/conflicts",
            post(handler_check_conflicts),
        )
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 令牌在场性 + `idempotencyKey` 绑定核对（schema_diff 域同款）。
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

fn scope_of(identity: &SessionIdentity) -> OrganizationScope {
    OrganizationScope {
        organization_id: identity.ctx.organization_id.clone(),
    }
}

fn not_found(message: impl Into<String>) -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, message))
}

fn asset_store_unavailable(detail: impl std::fmt::Display) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        format!("sync asset store: {detail}"),
    ))
}

fn asset_error(error: SyncAssetError) -> Response {
    match error {
        SyncAssetError::Validation(message) => {
            api_error_response(ApiError::new(ApiErrorCode::InvalidArgument, message))
        }
        SyncAssetError::Unavailable(detail) => asset_store_unavailable(detail),
    }
}

fn job_error(error: SyncJobError) -> Response {
    match error {
        SyncJobError::NotFound(id) => not_found(format!("Data Sync job '{id}' was not found")),
        SyncJobError::Validation(message) => {
            api_error_response(ApiError::new(ApiErrorCode::InvalidArgument, message))
        }
        SyncJobError::Unavailable(detail) => {
            api_error_response(ApiError::new(ApiErrorCode::ServiceUnavailable, detail))
        }
    }
}

fn migration_run_store_unavailable(detail: impl std::fmt::Display) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        format!("migration run store: {detail}"),
    ))
}

// ── 任务（v15 `sync_tasks`）────────────────────────────────────────────────

/// `getSyncTasks`（GET /api/v1/sync/tasks）→ 200 `SyncTask[]`。
async fn handler_list_tasks(State(state): State<SyncState>, identity: SessionIdentity) -> Response {
    match state.tasks.list(&scope_of(&identity)).await {
        Ok(tasks) => (StatusCode::OK, Json(tasks)).into_response(),
        Err(error) => asset_store_unavailable(error),
    }
}

/// `deleteSyncTask`（DELETE /api/v1/sync/tasks/{id}）→ 204（幂等：跨边界
/// 与命中同应答——防枚举口径同 queryAssets 的 favorite 删除）。
async fn handler_delete_task(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(task_id): Path<String>,
) -> Response {
    match state.tasks.delete(&scope_of(&identity), &task_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => asset_store_unavailable(error),
    }
}

// ── 档案（v15 `sync_profiles`）─────────────────────────────────────────────

/// `getSyncProfiles`（GET /api/v1/sync/profiles）→ 200 `SyncProfile[]`。
async fn handler_list_profiles(
    State(state): State<SyncState>,
    identity: SessionIdentity,
) -> Response {
    match state.profiles.list(&scope_of(&identity)).await {
        Ok(profiles) => (StatusCode::OK, Json(profiles)).into_response(),
        Err(error) => asset_store_unavailable(error),
    }
}

/// 令牌核对后的强类型反序列化：先剥令牌字段再进 DTO（`SyncProfile` 是
/// `deny_unknown_fields`——令牌字段不在业务 DTO 里，schema_diff 域同款
/// 先令牌后 DTO 的次序）。
fn parse_profile_body(body: Value) -> Result<SyncProfile, Response> {
    let mut payload = body;
    if let Some(map) = payload.as_object_mut() {
        map.remove("submissionToken");
        map.remove("idempotencyKey");
    }
    serde_json::from_value(payload)
        .map_err(|source| invalid_argument(format!("saveSyncProfile: {source}")))
}

/// `saveSyncProfile`（PUT /api/v1/sync/profiles）→ 204。保存即覆盖
/// （桌面 IPC 同语义）；校验 → 连接在场性 → `updated_at` 推进 → 落行。
async fn handler_save_profile(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let profile = match parse_profile_body(body) {
        Ok(profile) => profile,
        Err(response) => return response,
    };
    match save_sync_profile(&state.store, &state.profiles, &scope_of(&identity), profile).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => asset_error(error),
    }
}

/// `deleteSyncProfile`（DELETE /api/v1/sync/profiles/{id}）→ 204（幂等）。
async fn handler_delete_profile(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(profile_id): Path<String>,
) -> Response {
    match state
        .profiles
        .delete(&scope_of(&identity), &profile_id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => asset_store_unavailable(error),
    }
}

// ── 配对分类（共享层纯函数）────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClassifyPairingBody {
    source_database_type: String,
    target_database_type: String,
}

/// `classifyDataSyncPair`（POST /api/v1/sync/pairings）→ 200
/// `DataSyncPairingView`。POST 读：类型化入参走文档体（schema_diff
/// compare 同款裁决），纯函数无副作用。手动解析对齐 400 口径（axum
/// 的 Json 拒收是 422，域错误 wire 统一 invalidArgument 400）。
async fn handler_classify_pairing(_state: State<SyncState>, Json(body): Json<Value>) -> Response {
    let body: ClassifyPairingBody = match serde_json::from_value(body) {
        Ok(body) => body,
        Err(source) => return invalid_argument(format!("classifyDataSyncPair: {source}")),
    };
    let view = datazen::migration::data_sync::classify_data_sync_pair(
        &body.source_database_type,
        &body.target_database_type,
    );
    (StatusCode::OK, Json(view)).into_response()
}

// ── 迁移运行历史（v12 `migration_run_history`；三类运行共用）──────────────

/// 桌面 `list_migration_runs` 同款缺省与 clamp。
const DEFAULT_RUN_LIMIT: u64 = 25;
const MAX_RUN_LIMIT: u64 = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrationRunListParams {
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    connection_id: Option<String>,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    limit: Option<u64>,
}

/// `listMigrationRuns`（GET /api/v1/migration-runs）→ 200
/// `MigrationRunPage`。过滤平铺为查询参数（契约 §6.12 历史 page 同款）；
/// `limit` clamp 1..=100（桌面同款）。
async fn handler_list_migration_runs(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<MigrationRunListParams>,
) -> Response {
    let filter = MigrationRunFilter {
        operation: params.operation,
        status: params.status,
        profile_id: params.profile_id,
        connection_id: params.connection_id,
    };
    let offset = params.offset.unwrap_or(0);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_RUN_LIMIT)
        .clamp(1, MAX_RUN_LIMIT);
    let page = state
        .migration_runs
        .list(identity.ctx.organization_id.as_str(), filter, offset, limit)
        .await;
    match page {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(error) => migration_run_store_unavailable(error),
    }
}

/// `getMigrationRun`（GET /api/v1/migration-runs/{runId}）→ 200
/// `MigrationRunRecord`；miss / 越组织 → 404（统一 404，防枚举）。
async fn handler_get_migration_run(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(run_id): Path<String>,
) -> Response {
    let record = state
        .migration_runs
        .get(identity.ctx.organization_id.as_str(), &run_id)
        .await;
    match record {
        Ok(Some(record)) => (StatusCode::OK, Json(record)).into_response(),
        Ok(None) => not_found("Migration run was not found"),
        Err(error) => migration_run_store_unavailable(error),
    }
}

// ── sync job 读回（JobHost；kind 过滤在服务层）────────────────────────────

/// `listDataSyncJobs`（GET /api/v1/sync/jobs）→ 200 `JobDetails[]`（桌面
/// `list_data_sync_jobs` 逐 job 折 details 同款——UI 要进度与阶段）。
async fn handler_list_jobs(State(state): State<SyncState>, identity: SessionIdentity) -> Response {
    match state.jobs.list(&identity.ctx).await {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(error) => job_error(error),
    }
}

/// `getDataSyncJob`（GET /api/v1/sync/jobs/{id}）→ 200 `JobView`；未知 /
/// 他类 job → 404。
async fn handler_get_job(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
) -> Response {
    match state.jobs.get(&identity.ctx, &job_id).await {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(error) => job_error(error),
    }
}

/// `getDataSyncJobDetails`（GET /api/v1/sync/jobs/{id}/details）→ 200
/// `JobDetails`。
async fn handler_job_details(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
) -> Response {
    match state.jobs.details(&identity.ctx, &job_id).await {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(error) => job_error(error),
    }
}

/// `getDataSyncJobPreview`（GET /api/v1/sync/jobs/{id}/preview）→ 200
/// `SyncComparisonPreview`（plan_store 的行级复核投影；受理面随刀3e-4）。
async fn handler_job_preview(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
) -> Response {
    match state
        .jobs
        .preview(&identity.ctx, &scope_of(&identity), &job_id)
        .await
    {
        Ok(preview) => (StatusCode::OK, Json(preview)).into_response(),
        Err(error) => job_error(error),
    }
}

/// `cancelDataSync`（POST /api/v1/sync/jobs/{id}/cancel）→ 200
/// `{ cancelled }`（未知 / 他类 → false 幂等 no-op；桌面 `cancel_data_sync`
/// 减预受理取消记忆——服务端 id 受理时铸造）。
async fn handler_cancel_job(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    match state.jobs.cancel(&identity.ctx, &job_id).await {
        Ok(cancelled) => (
            StatusCode::OK,
            Json(serde_json::json!({ "cancelled": cancelled })),
        )
            .into_response(),
        Err(error) => job_error(error),
    }
}

// ── 执行面（W8 刀3e-4b；服务层见 sync/execution.rs）────────────────────────

/// 执行面错误映射（服务层桶 → wire 码：NotFound→404 / Validation→400 /
/// Storage→503）。
fn execution_error(error: SyncExecutionError) -> Response {
    match error {
        SyncExecutionError::NotFound(id) => {
            not_found(format!("Data Sync resource '{id}' was not found"))
        }
        SyncExecutionError::Validation(message) => {
            api_error_response(ApiError::new(ApiErrorCode::InvalidArgument, message))
        }
        SyncExecutionError::Storage(detail) => api_error_response(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            format!("Data Sync execution is unavailable: {detail}"),
        )),
    }
}

/// `Json<Value>` → 强类型（手动解析对齐 400 口径——axum Json 拒收是 422，
/// 域错误 wire 统一 invalidArgument；pairings 先例）。
fn parse_body<T: serde::de::DeserializeOwned>(body: Value, method: &str) -> Result<T, Response> {
    serde_json::from_value(body).map_err(|source| invalid_argument(format!("{method}: {source}")))
}

/// `inspectDataSync`（POST /api/v1/sync/inspections）→ 200 `TableResult[]`
/// （POST 读裁决：类型化入参走文档体，§6.7 同款；无副作用）。
async fn handler_inspect(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: SyncInspectRequest = match parse_body(body, "inspectDataSync") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state.execution.inspect(&identity.ctx, &body).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `revalidateDataSync`（POST /api/v1/sync/revalidations）→ 200
/// `{ staleTables }`（重跑 inspect 门闸的结构/主键漂移投影）。
async fn handler_revalidate(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: SyncRevalidateRequest = match parse_body(body, "revalidateDataSync") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state.execution.revalidate(&identity.ctx, &body).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `compareDataSync`（POST /api/v1/sync/comparisons）→ 200
/// `SyncComparisonPreview`（同步语义：durable prepare Job 冻结 ChangeSet 后
/// 读复核投影——桌面 `compare_data_sync` 同链）。
async fn handler_compare(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: SyncPrepareRequest = match parse_body(body, "compareDataSync") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state.execution.compare(&identity.ctx, &body).await {
        Ok(preview) => (StatusCode::OK, Json(preview)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `startDataSyncPrepareJob`（POST /api/v1/sync/jobs/prepare）→ 202
/// `JobView`（受理回执先于任何读取返回；重放同幂等键返回既有 Job）。
async fn handler_prepare_job(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: SyncPrepareRequest = match parse_body(body, "startDataSyncPrepareJob") {
        Ok(body) => body,
        Err(response) => return response,
    };
    let job_id = body
        .job_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("data-sync-{}", uuid::Uuid::new_v4()));
    match state
        .execution
        .start_prepare(&identity.ctx, &body, job_id)
        .await
    {
        Ok(view) => (StatusCode::ACCEPTED, Json(view)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `startDataSyncApplyJob` 的请求体（桌面 IPC 命令参数同构）。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyJobBody {
    source_db_session_id: String,
    target_db_session_id: String,
    request: datazen::migration::data_sync::plan_store::SyncRunRequest,
}

/// `startDataSyncApplyJob`（POST /api/v1/sync/jobs/apply）→ 202 `JobView`
/// （对确切的已复核 ChangeSet 受理；先持久化后取资源）。
async fn handler_apply_job(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: ApplyJobBody = match parse_body(body, "startDataSyncApplyJob") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state
        .execution
        .start_apply(
            &identity.ctx,
            &body.source_db_session_id,
            &body.target_db_session_id,
            body.request,
        )
        .await
    {
        Ok(view) => (StatusCode::ACCEPTED, Json(view)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `executeDataSync` 的请求体（`profile` 是可选的运行历史引用——桌面
/// `execute_data_sync` 命令的 `profile?` 参数同形）。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteBody {
    request: datazen::migration::data_sync::plan_store::SyncRunRequest,
    #[serde(default)]
    profile: Option<datazen::history::MigrationProfileRef>,
}

/// `executeDataSync`（POST /api/v1/sync/executions）→ 200
/// `DataSyncExecutionResponse`（apply 的同步投影 + v12 运行历史两段落账；
/// partially-applied / unknown 是带证据的成功应答，不是 4xx）。
async fn handler_execute(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: ExecuteBody = match parse_body(body, "executeDataSync") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state
        .execution
        .execute(&identity.ctx, body.request, body.profile.as_ref())
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `generateDataSyncSql` 的请求体（选区缺省 = 全量评审选区的缺省形状）。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SqlPreviewBody {
    plan_id: String,
    #[serde(default)]
    selection: Option<datazen::migration::data_sync::plan_store::SyncRunSelection>,
    options: datazen::migration::data_sync::SyncOptions,
}

/// `generateDataSyncSql`（POST /api/v1/sync/sql-preview）→ 200
/// `SqlStatement[]`（行级分页源 + IPC 上限；执行路径用同一分页源，从不
/// 累积这个预览向量）。
async fn handler_generate_sql(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: SqlPreviewBody = match parse_body(body, "generateDataSyncSql") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state
        .execution
        .generate_sql(
            &identity.ctx,
            &body.plan_id,
            body.selection.unwrap_or_default(),
            body.options,
        )
        .await
    {
        Ok(statements) => (StatusCode::OK, Json(statements)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `checkSyncConflicts`（POST /api/v1/sync/tasks/{id}/conflicts）→ 200
/// `{ hasConflicts, conflicts }`（持久化任务按 connectionId 重开专用会话，
/// 源侧行数与冻结计数比对；未知任务 → 404）。
async fn handler_check_conflicts(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(task_id): Path<String>,
) -> Response {
    match state
        .execution
        .check_conflicts(&identity.ctx, &task_id)
        .await
    {
        Ok(conflicts) => (StatusCode::OK, Json(conflicts)).into_response(),
        Err(error) => execution_error(error),
    }
}

/// `verifyDataSyncRecovery` 的请求体（`jobId` 在路径上——与 cancel 同款
/// 资源寻址）。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryBody {
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: String,
    target_database: String,
    #[serde(default)]
    source_schema: Option<String>,
    #[serde(default)]
    target_schema: Option<String>,
}

/// `verifyDataSyncRecovery`（POST /api/v1/sync/jobs/{id}/recovery-verification）
/// → 200 `JobDetails`（显式只读核验：端点重新授权 + 证据记录，永不续写；
/// schemaDiff 恢复核验同款裁决）。
async fn handler_verify_recovery(
    State(state): State<SyncState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let body: RecoveryBody = match parse_body(body, "verifyDataSyncRecovery") {
        Ok(body) => body,
        Err(response) => return response,
    };
    let request = SyncRecoveryRequest {
        job_id,
        source_db_session_id: body.source_db_session_id,
        target_db_session_id: body.target_db_session_id,
        source_database: body.source_database,
        target_database: body.target_database,
        source_schema: body.source_schema,
        target_schema: body.target_schema,
    };
    match state
        .execution
        .verify_recovery(&identity.ctx, &request)
        .await
    {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(error) => execution_error(error),
    }
}
