//! connections 域路由（契约 §6.3 的 HTTP 薄绑定）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `listConnections` | `GET /api/v1/connections` | — | 200 | — → `ProfileView[]`（不分页） |
//! | `createConnection` | `POST /api/v1/connections` | ✓ | 201 | `ProfileDraft & SubmitRequest` → `ProfileView` |
//! | `updateConnection` | `PATCH /api/v1/connections/{id}` | ✓ | 200 | `{ expectedRevision, patch }` → `ProfileView` |
//! | `disableConnection` | `PATCH /api/v1/connections/{id}` | ✓ | 200 | `{ expectedRevision, enabled: false }` → `ProfileView` |
//!
//! 域裁决（契约 §6.3 全文逐字对照）：
//!
//! - **凭据 write-only（主线红线，connection-management §4.1）**：请求侧
//!   `ProfileDraft.credentials` / `ProfilePatch.credentials` 只交存储侧的
//!   `SecretRef` 通道（明文永不落库、永不回显）；响应侧一律是
//!   [`ProfileView`] 投影——`secretRef`、`networkRouteRef` 与全部敏感
//!   `publicOptions` 键在 application 投影处剔除（`ProfileView::from_record`），
//!   `credentialConfigured` 只表达布尔。handler **不触碰** `ConnectionConfig`
//!   表单词汇（那是 `ServerConnections`/catalog 面，§6.3 的形状是 Profile
//!   词汇——`ProfileView` / `ProfileDraft` / `expectedRevision`）。
//! - **PATCH 一条路由双服务**：顶层 `enabled: false` → disable 语义，
//!   否则 `{ expectedRevision, patch }` → update 语义（判别无歧义：
//!   `ProfilePatch = Partial<Omit<ProfileDraft,'driverId'>>` 没有 enabled
//!   字段）。disable 与 patch 同时在场时 disable 优先（契约注）。
//! - **expectedRevision 的 CAS**：update 走 `ProfileRepository::compare_and_set`
//!   （端口 `CasConflict` → 409 `configRevisionMismatch`——端口模块头的
//!   server host 映射裁决）；disable 的端口方法不带版本，`expectedRevision`
//!   的核对在 handler 侧补齐（先 get 比对再 disable，契约请求形状）。
//! - **幂等令牌（令牌列 ✓）**：解包先例照 `http::router::handler_jobs_submit`
//!   （`submissionToken` 接受字符串或 `{ idempotencyKey, expiresAt }`）；
//!   校验 = `SubmissionTokenIssuer::verify`（HMAC/过期/keyVersion）+ 身份
//!   交叉核对 + operation 白名单。签发面在 `router.rs` 的
//!   `handler_issue_token`（`createProfile` 已在账）。
//! - **组织隔离（CM-05）**：仓储按 `ctx` 组织限定；越组织/不存在的
//!   `{id}` 一律 404 固定文案（与 L6 的 `NOT_VISIBLE_MESSAGE` 同一条，
//!   防 ID 枚举）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;

use datazen::connection::{
    ProfileDraft, ProfilePatch, ProfileRepository, ProfileScope, ProfileView,
};
use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::submission::{
    SubmissionPresentation, SubmissionTokenIssuer, VerifiedSubmission,
};
use datazen::execution::SubmissionToken;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{ConfigRevision, ConnectionId, IdempotencyKey, Timestamp};

use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};

/// 连接域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct ConnectionsState {
    /// 连接档案仓储（`storage::profiles::ServerProfiles` 的端口面）。
    pub profiles: Arc<dyn ProfileRepository>,
    /// 幂等令牌签发/校验面——与 `router.rs` `DomainState.issuer` **同一
    /// 实例**（`handler_issue_token` 签出的令牌在这里核验）。
    pub issuer: Arc<dyn SubmissionTokenIssuer>,
}

/// connections 域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内；`/api/v1/connections/{id}` 的 GET/PATCH 已在
/// L6 登记形状）。
pub fn connections_router(state: ConnectionsState) -> Router {
    Router::new()
        .route(
            "/api/v1/connections",
            get(handler_list_connections).post(handler_create_connection),
        )
        .route(
            "/api/v1/connections/{id}",
            axum::routing::patch(handler_patch_connection),
        )
        .with_state(state)
}

// ── 幂等令牌（提交先例照 handler_jobs_submit）─────────────────────────────

/// 令牌载荷缺省过期（远未来；真实过期以 issuer 的签名载荷为准，
/// 与 handler_jobs_submit 的同款兜底）。
const DISTANT_FUTURE: &str = "9999-12-31T23:59:59Z";

/// 请求体 → 令牌出示（claimed 身份取自认证链事实；签名载荷与认证链
/// 不一致 → 服务判 Unauthenticated）。
fn presentation_of(body: &Value, ctx: &RequestContext) -> Result<SubmissionPresentation, Response> {
    let (idempotency_key, expires_at) = match body.get("submissionToken") {
        Some(Value::String(raw)) => (raw.clone(), DISTANT_FUTURE.to_string()),
        Some(Value::Object(map)) => (
            map.get("idempotencyKey")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            map.get("expiresAt")
                .and_then(Value::as_str)
                .unwrap_or(DISTANT_FUTURE)
                .to_string(),
        ),
        _ => return Err(invalid_argument("submissionToken is required.")),
    };
    Ok(SubmissionPresentation {
        token: SubmissionToken {
            idempotency_key: IdempotencyKey::new(idempotency_key),
            expires_at: Timestamp::new(expires_at),
        },
        claimed_organization_id: ctx.organization_id.clone(),
        claimed_principal_id: ctx.principal_id.clone(),
        claimed_client_instance_id: ctx.client_instance_id.clone(),
        claimed_delegation_id: ctx.delegation_id.clone(),
    })
}

/// 令牌校验三步（SubmissionService::submit_job 的同序裁决，本域消费面）：
/// 签名/过期 → 身份交叉核对 → operation 白名单（配置写入类只有
/// `createProfile`；`IdempotentOperation` 词表封闭，update/disable 复用
/// 同一配置写 operation——如需独立 operation 属协议升级，归主线裁决）。
async fn verify_config_write_token(
    state: &ConnectionsState,
    ctx: &RequestContext,
    body: &Value,
) -> Result<VerifiedSubmission, Response> {
    let presentation = presentation_of(body, ctx)?;
    let verified = match state.issuer.verify(&presentation).await {
        Ok(verified) => verified,
        Err(PortError::TokenInvalid) => {
            return Err(api_error_response(ApiError::new(
                ApiErrorCode::Unauthenticated,
                "Submission token is invalid, unknown-keyed, or expired.",
            )));
        }
        Err(source) => {
            return Err(api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("token verification failed: {source}"),
            )));
        }
    };
    if verified.organization_id != ctx.organization_id
        || verified.principal_id != ctx.principal_id
        || verified.client_instance_id != ctx.client_instance_id
    {
        return Err(api_error_response(ApiError::new(
            ApiErrorCode::Unauthenticated,
            "Submission identity does not match the authenticated context.",
        )));
    }
    if verified.operation != IdempotentOperation::CreateProfile {
        return Err(invalid_argument(
            "This endpoint only accepts createProfile tokens.",
        ));
    }
    Ok(verified)
}

/// 本域的 `PortError` → wire 映射。`CasConflict` → `configRevisionMismatch`
/// 是端口模块头的 server host 裁决（不落 `ContextConflict` 的通用映射）。
fn profile_error(error: PortError) -> ApiError {
    match error {
        PortError::CasConflict { entity, id } => ApiError::new(
            ApiErrorCode::ConfigRevisionMismatch,
            format!("{entity} {id} changed concurrently; re-read configRevision and retry."),
        ),
        PortError::NotFound(_) => ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE),
        PortError::BackendUnavailable(detail) | PortError::ProviderTimeout(detail) => {
            ApiError::new(ApiErrorCode::ServiceUnavailable, detail)
        }
        other => ApiError::new(ApiErrorCode::ServiceUnavailable, other.to_string()),
    }
}

/// 越组织/不存在的统一 404（防 ID 枚举；与 L6 同一条固定文案）。
fn not_visible() -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

/// `expectedRevision` 的 wire 形态是 Counter（十进制字符串，CM-01）；数字
/// 字面量也收（防御面）——与 jobs list 游标的字符串口径一致。
fn expected_revision_of(body: &Value) -> Option<u64> {
    match body.get("expectedRevision") {
        Some(Value::String(raw)) => raw.trim().parse::<u64>().ok(),
        Some(Value::Number(value)) => value.as_u64(),
        _ => None,
    }
}

// ── handlers ────────────────────────────────────────────────────────────────

/// `listConnections`（§6.3：GET /api/v1/connections，不分页）→ 200
/// `ProfileView[]`。净化视图：投影在 application 层（`ProfileView::from_record`），
/// 凭据与敏感选项不进响应（见模块头红线）。集合类路由不经 L6 id 寻址，
/// 可见性过滤在本 handler（组织 + enabled，镜像用例面 `list_connections`
/// 的双过滤）。
async fn handler_list_connections(
    State(state): State<ConnectionsState>,
    identity: SessionIdentity,
) -> Response {
    match state
        .profiles
        .list(&identity.ctx, ProfileScope::from_context(&identity.ctx))
        .await
    {
        Ok(records) => {
            let views: Vec<ProfileView> = records
                .iter()
                .filter(|record| {
                    record.organization_id == identity.ctx.organization_id && record.enabled
                })
                .map(ProfileView::from_record)
                .collect();
            Json(views).into_response()
        }
        Err(error) => api_error_response(profile_error(error)),
    }
}

/// `createConnection`（§6.3：POST /api/v1/connections，`ProfileDraft &
/// SubmitRequest`）→ 201 `ProfileView`。`connectionId` 由仓储铸造
/// （幂等重放同令牌返回既有行——`ServerProfiles::create` 的 creation_key
/// 语义），201 只表达「本次受理建立了配置」。
async fn handler_create_connection(
    State(state): State<ConnectionsState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let verified = match verify_config_write_token(&state, &identity.ctx, &body).await {
        Ok(verified) => verified,
        Err(response) => return response,
    };
    // `ProfileDraft` 不带 deny_unknown_fields：`submissionToken` 字段在
    // 反序列化处被忽略（信封与载荷同体，§6.3 的 `& SubmitRequest` 形状）。
    let draft: ProfileDraft = match serde_json::from_value(body) {
        Ok(draft) => draft,
        Err(source) => return invalid_argument(format!("draft: {source}")),
    };
    match state
        .profiles
        .create(&identity.ctx, draft, &verified.idempotency_key)
        .await
    {
        Ok(record) => {
            (StatusCode::CREATED, Json(ProfileView::from_record(&record))).into_response()
        }
        Err(error) => api_error_response(profile_error(error)),
    }
}

/// `updateConnection` / `disableConnection`（§6.3：PATCH
/// /api/v1/connections/{id}，一条路由双服务）→ 200 `ProfileView`。
async fn handler_patch_connection(
    State(state): State<ConnectionsState>,
    identity: SessionIdentity,
    Path(connection_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let verified = match verify_config_write_token(&state, &identity.ctx, &body).await {
        Ok(verified) => verified,
        Err(response) => return response,
    };
    let _ = verified.idempotency_key; // 令牌即受理键；CAS/禁用的幂等由仓储版本承担
    let connection_id = ConnectionId::new(connection_id);
    // 判别（契约 §6.3 注）：顶层 `enabled: false` 只属于 disable 语义。
    if matches!(body.get("enabled"), Some(Value::Bool(false))) {
        return disable(state, identity.ctx, connection_id, body).await;
    }
    update(state, identity.ctx, connection_id, body).await
}

/// disable 语义：`{ expectedRevision, enabled: false }`。先做可见性判定
/// （越组织/不存在 → 404 固定文案），再核对 `expectedRevision`（handler
/// 侧 CAS——端口 `disable` 不带版本，见模块头），最后翻 `enabled`。
async fn disable(
    state: ConnectionsState,
    ctx: RequestContext,
    connection_id: ConnectionId,
    body: Value,
) -> Response {
    let Some(expected) = expected_revision_of(&body) else {
        return invalid_argument("expectedRevision is required (decimal string counter).");
    };
    let record = match state.profiles.get(&ctx, connection_id.clone()).await {
        Ok(Some(record)) => record,
        Ok(None) => return not_visible(),
        Err(error) => return api_error_response(profile_error(error)),
    };
    if record.config_revision.counter().get() != expected {
        return api_error_response(ApiError::new(
            ApiErrorCode::ConfigRevisionMismatch,
            "The connection configuration changed concurrently; re-read configRevision and retry.",
        ));
    }
    match state.profiles.disable(&ctx, connection_id).await {
        Ok(record) => Json(ProfileView::from_record(&record)).into_response(),
        Err(error) => api_error_response(profile_error(error)),
    }
}

/// update 语义：`{ expectedRevision, patch }`。CAS 冲突 → 409
/// `configRevisionMismatch`（调用方重读后由用户决定，不自动覆盖）。
async fn update(
    state: ConnectionsState,
    ctx: RequestContext,
    connection_id: ConnectionId,
    body: Value,
) -> Response {
    let Some(expected) = expected_revision_of(&body) else {
        return invalid_argument("expectedRevision is required (decimal string counter).");
    };
    let patch: ProfilePatch = match body.get("patch").cloned() {
        Some(patch) => match serde_json::from_value(patch) {
            Ok(patch) => patch,
            Err(source) => return invalid_argument(format!("patch: {source}")),
        },
        None => return invalid_argument("patch is required."),
    };
    match state
        .profiles
        .compare_and_set(&ctx, connection_id, ConfigRevision::new(expected), patch)
        .await
    {
        Ok(record) => Json(ProfileView::from_record(&record)).into_response(),
        Err(error) => api_error_response(profile_error(error)),
    }
}
