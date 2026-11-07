//! Schema Diff 域路由（W8 刀3b-2；契约 §6.13 的 HTTP 薄绑定）——表结构
//! 对比、计划准备与部署从 legacy IPC 命令轨收敛进 MethodMap。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `listSchemaDiffProfiles` | `GET /api/v1/schema-diff/profiles` | — | 200 | — → `SchemaDiffProfile[]` |
//! | `saveSchemaDiffProfile` | `PUT /api/v1/schema-diff/profiles` | ✓ | 204 | `SchemaDiffProfile & SubmitRequest` → — |
//! | `deleteSchemaDiffProfile` | `DELETE /api/v1/schema-diff/profiles/{id}` | ✓ | 204 | — |
//! | `compareTableSchemas` | `POST /api/v1/schema-diff/comparisons` | — | 200 | 对比文档体 → 对比结果（JSON） |
//! | `prepareSchemaDiffPlan` | `POST /api/v1/schema-diff/plans` | ✓ | 202 | 准备文档体 → `SchemaDiffJobAccepted` |
//! | `prepareSchemaUnifiedPlan` | `POST /api/v1/schema-diff/unified-plans` | ✓ | 202 | 统一准备文档体 → `SchemaDiffJobAccepted` |
//! | `executeSchemaDiffDeploy` | `POST /api/v1/schema-diff/deploys` | ✓ | 202 | 部署文档体 → `SchemaDiffJobAccepted` |
//! | `listSchemaDiffJobs` | `GET /api/v1/schema-diff/jobs?states=&limit=` | — | 200 | — → `JobView[]` |
//! | `getSchemaDiffJobDetails` | `GET /api/v1/schema-diff/jobs/{id}` | — | 200 | — → `SchemaDiffJobDetails` |
//! | `cancelSchemaDiffDeploy` | `POST /api/v1/schema-diff/jobs/{id}/cancel` | ✓ | 200 | — → `{ cancelled }` |
//! | `verifySchemaDiffJobRecovery` | `POST /api/v1/schema-diff/jobs/{id}/recovery-verification` | ✓ | 200 | `{ targetDbSessionId }` → `SchemaDiffJobDetails` |
//!
//! 域裁决（契约 §6.13）：
//!
//! - **令牌**：写面（save/delete profile、plans/unified-plans/deploys、
//!   cancel、recovery-verification）带 `SubmitRequest`，handler 核在场性 +
//!   `idempotencyKey` 绑定（query_assets 域同款；HMAC 轨——配置类操作）。
//!   compare 是 POST 读（对比文档体，契约 §6.7 的 POST 读同款裁决），
//!   不带令牌。
//! - **归属 = organization**（profiles 是「连接对」级别的组织资产，桌面
//!   单租户全体共享的服务端收敛；v12 表同口径）：scoped repo 按认证链
//!   事实逐请求装配（INV-01）。
//! - **会话可见性**：越组织/未知会话 → 404 固定文案（sessions 域防枚举
//!   裁决；job 的读回/取消对越组织行天然落空）。
//! - **job 受理 = 先持久化后取资源**（202 回执先于任何 DDL 执行；桌面
//!   IPC 同语义——受理回执后窗口期派发）。
//! - **错误 wire**：job 仓储 NotFound→404；校验→400 `invalidArgument`；
//!   冲突（幂等/选择版本漂移）→409 `contextConflict`；存储→503。
//! - **写面体先令牌后 DTO**：`Json<Value>` 先核令牌再反序列化强类型
//!   （query_assets 域同款——令牌字段不在业务 DTO 里）。

use std::sync::Arc;

use axum::extract::{Path, Query as QueryParams, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use datazen::jobs::domain::job::JobState;
use datazen::migration::schema_diff::api as schema_diff_api;
use datazen::migration::schema_diff::job::PrepareRequest;
use datazen::migration::schema_diff::profile::SchemaDiffProfile;
use datazen::migration::schema_diff::types::ColumnTypeOverride;
use datazen::migration::schema_diff::SchemaDiffProfileRepository as _;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::scope::OrganizationScope;

use crate::http::domains::table_data::not_visible;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::schema_diff::endpoints::{EndpointError, SessionEndpoints};
use crate::schema_diff::jobs::{SchemaDiffJobError, SchemaDiffJobs};
use crate::schema_diff::{ServerComparisonPreviewHost, ServerSchemaDiffAdapters};
use crate::storage::schema_diff_profiles::{
    connection_exists, ServerSchemaDiffProfiles as Profiles,
};
use crate::storage::AdminStore;

/// Schema Diff 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct SchemaDiffState {
    /// 管理库（profiles / 迁移历史；迁移由 bootstrap 负责）。
    pub store: Arc<AdminStore>,
    /// profile 仓储（v12 表）。
    pub profiles: Arc<Profiles>,
    /// Job 服务（JobHost + 进程级 PlanRepository + backend 依赖）。
    pub jobs: SchemaDiffJobs,
    /// 端点解析器（compare/recovery 与 Job 服务共享同一批实例）。
    pub endpoints: SessionEndpoints,
    /// 对比预览端口（无状态；DDL 渲染与全类型读取在共享层）。
    pub comparison_preview: Arc<ServerComparisonPreviewHost>,
    /// sync 适配器端口（懒注册；与 data_transfer 同一实例语义）。
    pub adapters: Arc<ServerSchemaDiffAdapters>,
}

/// Schema Diff 域 Router 片段（merge 进 `build_router_with_auth` 的
/// 认证链 L2–L6 之内）。
pub fn schema_diff_router(state: SchemaDiffState) -> Router {
    Router::new()
        .route(
            "/api/v1/schema-diff/profiles",
            get(handler_list_profiles).put(handler_save_profile),
        )
        .route(
            "/api/v1/schema-diff/profiles/{id}",
            delete(handler_delete_profile),
        )
        .route(
            "/api/v1/schema-diff/comparisons",
            post(handler_compare_tables),
        )
        .route("/api/v1/schema-diff/plans", post(handler_prepare_plan))
        .route(
            "/api/v1/schema-diff/unified-plans",
            post(handler_prepare_unified_plan),
        )
        .route("/api/v1/schema-diff/deploys", post(handler_deploy))
        .route("/api/v1/schema-diff/jobs", get(handler_list_jobs))
        .route("/api/v1/schema-diff/jobs/{id}", get(handler_job_details))
        .route(
            "/api/v1/schema-diff/jobs/{id}/cancel",
            post(handler_cancel_job),
        )
        .route(
            "/api/v1/schema-diff/jobs/{id}/recovery-verification",
            post(handler_verify_recovery),
        )
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 令牌在场性 + `idempotencyKey` 绑定核对（query_assets 域同款）。
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

/// 令牌核对后的强类型反序列化（未知字段——含令牌——被业务 DTO 忽略）。
fn parse_body<T: serde::de::DeserializeOwned>(body: Value, command: &str) -> Result<T, Response> {
    serde_json::from_value(body).map_err(|source| invalid_argument(format!("{command}: {source}")))
}

fn profile_id_of(body: &Value) -> String {
    body.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn scope_of(identity: &SessionIdentity) -> OrganizationScope {
    OrganizationScope {
        organization_id: identity.ctx.organization_id.clone(),
    }
}

fn not_found(message: impl Into<String>) -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, message))
}

fn profile_store_error(
    error: datazen::migration::schema_diff::SchemaDiffProfileRepositoryError,
) -> Response {
    match error {
        datazen::migration::schema_diff::SchemaDiffProfileRepositoryError::NotFound(entity) => {
            not_found(entity)
        }
        other => api_error_response(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            format!("schema diff profile store: {other}"),
        )),
    }
}

fn endpoint_error(error: EndpointError) -> Response {
    match error {
        EndpointError::NotVisible => not_visible(),
        EndpointError::ProfileUnavailable => not_found("Connection profile unavailable"),
        EndpointError::Unavailable(detail) => {
            api_error_response(ApiError::new(ApiErrorCode::ServiceUnavailable, detail))
        }
    }
}

fn job_error(error: SchemaDiffJobError) -> Response {
    match error {
        SchemaDiffJobError::NotFound(entity) => not_found(entity),
        SchemaDiffJobError::Validation(detail) => invalid_argument(detail),
        SchemaDiffJobError::Conflict(detail) => {
            api_error_response(ApiError::new(ApiErrorCode::ContextConflict, detail))
        }
        SchemaDiffJobError::Storage => api_error_response(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            "Schema Diff job storage is unavailable",
        )),
    }
}

// ── profiles ────────────────────────────────────────────────────────────────

/// `listSchemaDiffProfiles`（GET）→ 200 `SchemaDiffProfile[]`。
async fn handler_list_profiles(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
) -> Response {
    let scope = scope_of(&identity);
    match state.profiles.list(&scope).await {
        Ok(profiles) => (StatusCode::OK, Json(profiles)).into_response(),
        Err(error) => profile_store_error(error),
    }
}

/// `saveSchemaDiffProfile`（PUT）→ 204（upsert；桌面 `save_schema_diff_profile`
/// 同语义：校验 → 连接存在性 → 服务端 `updated_at`）。
async fn handler_save_profile(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    // `SchemaDiffProfile` 是 `deny_unknown_fields` 词汇（既有 IPC 契约）：
    // 令牌两字段核对后剥掉，再反序列化业务体。时间戳服务端权威——
    // `createdAt` 缺席时保留既有值（update）或铸造（create），`updatedAt`
    // 恒为本请求时刻（桌面 save 同语义）。
    let profile_id = profile_id_of(&body);
    let provided_created_at = body.get("createdAt").cloned();
    let existing_created_at = match state.profiles.list(&scope_of(&identity)).await {
        Ok(list) => list
            .into_iter()
            .find(|existing| existing.id == profile_id)
            .map(|existing| existing.created_at),
        Err(error) => return profile_store_error(error),
    };
    let now = chrono::Utc::now().to_rfc3339();
    let mut payload = body;
    if let Some(object) = payload.as_object_mut() {
        object.remove("submissionToken");
        object.remove("idempotencyKey");
        let created_at = provided_created_at
            .or_else(|| existing_created_at.map(|at| serde_json::json!(at.to_rfc3339())))
            .unwrap_or_else(|| serde_json::json!(now));
        object.insert("createdAt".into(), created_at);
        object.insert("updatedAt".into(), serde_json::json!(now));
    }
    let mut profile: SchemaDiffProfile = match parse_body(payload, "saveSchemaDiffProfile") {
        Ok(profile) => profile,
        Err(response) => return response,
    };
    if let Err(message) = profile.validate() {
        return invalid_argument(message);
    }
    let scope = scope_of(&identity);
    // 连接存在性（桌面同款：源/目标连接任一不在目录 → 校验拒绝）。
    for connection_id in [&profile.source_connection_id, &profile.target_connection_id] {
        match connection_exists(&state.store, &scope, connection_id).await {
            Ok(true) => {}
            Ok(false) => {
                return invalid_argument("schema diff profile source connection no longer exists")
            }
            Err(error) => return profile_store_error(error),
        }
    }
    match state.profiles.save(&scope, profile).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => profile_store_error(error),
    }
}

/// `deleteSchemaDiffProfile`（DELETE）→ 204（未知 id = 幂等 no-op，桌面
/// delete 同语义）。
async fn handler_delete_profile(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Path(profile_id): Path<String>,
) -> Response {
    let scope = scope_of(&identity);
    match state.profiles.delete(&scope, &profile_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => profile_store_error(error),
    }
}

// ── compare（POST 读：对比文档体） ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompareBody {
    source_db_session_id: String,
    target_db_session_id: String,
    source_table_name: String,
    target_table_name: String,
    #[serde(default)]
    source_schema: Option<String>,
    #[serde(default)]
    target_schema: Option<String>,
}

/// `compareTableSchemas`（POST /comparisons）→ 200 对比结果。单表对比是
/// 同步读（不建 job——桌面 IPC 同语义）；会话可见性闸在端点解析内。
async fn handler_compare_tables(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Json(body): Json<CompareBody>,
) -> Response {
    let source = match state
        .endpoints
        .resolve(
            &identity.ctx,
            &body.source_db_session_id,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => return endpoint_error(error),
    };
    let target = match state
        .endpoints
        .resolve(
            &identity.ctx,
            &body.target_db_session_id,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => return endpoint_error(error),
    };
    let comparison = schema_diff_api::compare_table_schemas(
        &source.endpoint(),
        &target.endpoint(),
        &body.source_table_name,
        &body.target_table_name,
        body.source_schema.as_deref(),
        body.target_schema.as_deref(),
        state.adapters.as_ref(),
        state.comparison_preview.as_ref(),
    )
    .await;
    match comparison {
        Ok(comparison) => match serde_json::to_value(comparison) {
            Ok(value) => (StatusCode::OK, Json(value)).into_response(),
            Err(error) => api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("compareTableSchemas serialization: {error}"),
            )),
        },
        Err(error) => api_error_response(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            format!("compareTableSchemas: {error}"),
        )),
    }
}

// ── plans / deploys（job 受理） ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreparePlanBody {
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    #[serde(default)]
    target_table_names: Option<Vec<String>>,
    #[serde(default)]
    target_only_table_names: Option<Vec<String>>,
    #[serde(default)]
    source_schema: Option<String>,
    #[serde(default)]
    target_schema: Option<String>,
    allow_destructive: bool,
    #[serde(default)]
    include_indexes: Option<bool>,
    #[serde(default)]
    type_overrides: Option<Vec<ColumnTypeOverride>>,
}

fn prepare_request_of(body: &PreparePlanBody) -> PrepareRequest {
    PrepareRequest::Table {
        source_db_session_id: body.source_db_session_id.clone(),
        target_db_session_id: body.target_db_session_id.clone(),
        table_names: body.table_names.clone(),
        target_table_names: body
            .target_table_names
            .clone()
            .unwrap_or_else(|| body.table_names.clone()),
        target_only_table_names: body.target_only_table_names.clone().unwrap_or_default(),
        source_schema: body.source_schema.clone(),
        target_schema: body.target_schema.clone(),
        allow_destructive: body.allow_destructive,
        include_indexes: body.include_indexes,
        type_overrides: body.type_overrides.clone().unwrap_or_default(),
    }
}

/// `prepareSchemaDiffPlan`（POST /plans）→ 202 `SchemaDiffJobAccepted`。
async fn handler_prepare_plan(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let body: PreparePlanBody = match parse_body(body, "prepareSchemaDiffPlan") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state
        .jobs
        .run_prepare_job(&identity.ctx, prepare_request_of(&body))
        .await
    {
        Ok(accepted) => (StatusCode::ACCEPTED, Json(accepted)).into_response(),
        Err(error) => job_error(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnifiedPlanBody {
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    #[serde(default)]
    target_table_names: Option<Vec<String>>,
    #[serde(default)]
    target_only_table_names: Option<Vec<String>>,
    #[serde(default)]
    source_objects: Option<Vec<datazen_driver_api::DatabaseObject>>,
    #[serde(default)]
    target_objects: Option<Vec<datazen_driver_api::DatabaseObject>>,
    #[serde(default)]
    source_schema: Option<String>,
    #[serde(default)]
    target_schema: Option<String>,
    allow_destructive: bool,
    #[serde(default)]
    include_indexes: Option<bool>,
    #[serde(default)]
    type_overrides: Option<Vec<ColumnTypeOverride>>,
}

/// `prepareSchemaUnifiedPlan`（POST /unified-plans）→ 202。
async fn handler_prepare_unified_plan(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let body: UnifiedPlanBody = match parse_body(body, "prepareSchemaUnifiedPlan") {
        Ok(body) => body,
        Err(response) => return response,
    };
    let request = PrepareRequest::Unified {
        source_db_session_id: body.source_db_session_id.clone(),
        target_db_session_id: body.target_db_session_id.clone(),
        table_names: body.table_names.clone(),
        target_table_names: body
            .target_table_names
            .clone()
            .unwrap_or_else(|| body.table_names.clone()),
        target_only_table_names: body.target_only_table_names.clone().unwrap_or_default(),
        source_objects: body.source_objects.clone().unwrap_or_default(),
        target_objects: body.target_objects.clone().unwrap_or_default(),
        source_schema: body.source_schema.clone(),
        target_schema: body.target_schema.clone(),
        allow_destructive: body.allow_destructive,
        include_indexes: body.include_indexes,
        type_overrides: body.type_overrides.clone().unwrap_or_default(),
    };
    match state.jobs.run_prepare_job(&identity.ctx, request).await {
        Ok(accepted) => (StatusCode::ACCEPTED, Json(accepted)).into_response(),
        Err(error) => job_error(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeployBody {
    target_db_session_id: String,
    /// 直接携带计划正文的兼容路径（桌面 D1 同款：先注册进
    /// PlanRepository 再走 apply Job）。
    #[serde(default)]
    plan: Option<datazen::migration::schema_diff::types::SchemaDiffPlan>,
    #[serde(default)]
    use_transaction: Option<bool>,
    #[serde(default)]
    require_rollback: Option<bool>,
    #[serde(default)]
    confirm_destructive: Option<String>,
    #[serde(default)]
    target_database: Option<String>,
    #[serde(default)]
    target_schema: Option<String>,
    #[serde(default)]
    profile: Option<datazen::history::MigrationProfileRef>,
    #[serde(default)]
    plan_id: Option<String>,
    #[serde(default)]
    selection_revision: Option<u64>,
}

/// `executeSchemaDiffDeploy`（POST /deploys）→ 202。受理先于任何 DDL 执行
/// （先持久化后取资源）。
async fn handler_deploy(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let body: DeployBody = match parse_body(body, "executeSchemaDiffDeploy") {
        Ok(body) => body,
        Err(response) => return response,
    };
    let (plan_id, selection_revision) = match (body.plan_id.clone(), body.selection_revision) {
        (Some(plan_id), Some(selection_revision)) => (plan_id, selection_revision),
        _ => {
            let Some(plan) = body.plan.clone() else {
                return invalid_argument(
                    "executeSchemaDiffDeploy requires planId+selectionRevision or a plan body.",
                );
            };
            match state
                .jobs
                .register_plan_for_apply(
                    &state.endpoints,
                    &identity.ctx,
                    plan,
                    &body.target_db_session_id,
                )
                .await
            {
                Ok(pair) => pair,
                Err(error) => return job_error(error),
            }
        }
    };
    let apply_request = datazen::migration::schema_diff::job::ApplyRequest {
        target_db_session_id: body.target_db_session_id.clone(),
        use_transaction: body.use_transaction.unwrap_or(true),
        require_rollback: body.require_rollback.unwrap_or(false),
        confirm_destructive: body.confirm_destructive.clone(),
        job_id: None,
        target_database: body.target_database.clone(),
        target_schema: body.target_schema.clone(),
        profile: body
            .profile
            .clone()
            .map(|profile| (profile.id, profile.revision)),
    };
    match state
        .jobs
        .run_apply_job(&identity.ctx, &plan_id, selection_revision, apply_request)
        .await
    {
        Ok(accepted) => (StatusCode::ACCEPTED, Json(accepted)).into_response(),
        Err(error) => job_error(error),
    }
}

// ── jobs 读回 / 取消 / 恢复核验 ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobsListParams {
    #[serde(default)]
    states: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

fn parse_state(raw: &str) -> Option<JobState> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "queued" => Some(JobState::Queued),
        "running" => Some(JobState::Running),
        "succeeded" => Some(JobState::Succeeded),
        "failed" => Some(JobState::Failed),
        "cancelled" => Some(JobState::Cancelled),
        _ => None,
    }
}

/// `listSchemaDiffJobs`（GET /jobs?states=a,b&limit=）→ 200 `JobView[]`。
async fn handler_list_jobs(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    QueryParams(params): QueryParams<JobsListParams>,
) -> Response {
    let mut states = Vec::new();
    if let Some(raw) = params.states.as_deref() {
        for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match parse_state(part) {
                Some(state_name) => states.push(state_name),
                None => {
                    return invalid_argument(format!(
                        "unknown job state '{part}' (expected queued | running | succeeded | failed | cancelled)."
                    ))
                }
            }
        }
    }
    match state
        .jobs
        .list(
            &identity.ctx,
            if states.is_empty() {
                None
            } else {
                Some(states)
            },
            params.limit,
        )
        .await
    {
        Ok(views) => (StatusCode::OK, Json(views)).into_response(),
        Err(error) => job_error(error),
    }
}

/// `getSchemaDiffJobDetails`（GET /jobs/{id}）→ 200 `SchemaDiffJobDetails`。
async fn handler_job_details(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
) -> Response {
    match state.jobs.details(&identity.ctx, &job_id).await {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(error) => job_error(error),
    }
}

/// `cancelSchemaDiffDeploy`（POST /jobs/{id}/cancel）→ 200 `{ cancelled }`。
async fn handler_cancel_job(
    State(state): State<SchemaDiffState>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryBody {
    target_db_session_id: String,
}

/// `verifySchemaDiffJobRecovery`（POST /jobs/{id}/recovery-verification）
/// → 200 `SchemaDiffJobDetails`。
async fn handler_verify_recovery(
    State(state): State<SchemaDiffState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let body: RecoveryBody = match parse_body(body, "verifySchemaDiffJobRecovery") {
        Ok(body) => body,
        Err(response) => return response,
    };
    match state
        .jobs
        .verify_recovery(
            &state.endpoints,
            &identity.ctx,
            &job_id,
            &body.target_db_session_id,
        )
        .await
    {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(error) => job_error(error),
    }
}
