//! v1 路由表（team-server §2.3 的 M/N 步）。
//!
//! D1 骨架只挂机器面（`/healthz` / `/readyz`）；`/api/v1` 的域路由按
//! WR-C2 冻结的契约表（`docs/architecture/platform/http-api-contract.md`）
//! 随 WR-D2（仓储 + 逐域 handler）、WR-D4（执行接线）、WR-D5（SSE）点亮。
//! 在此之前，一切 `/api/` 路径统一回答 501 `capabilityUnsupported`
//! （契约 §7：`not-implemented` 类 → 501），非 `/api/` 的未知路径回答
//! 404 `notFound`。**没有 catch-all 之外的任何域端点**——契约的「多余
//! 端点为零」验收在 D1 就成立。
//!
//! 两个装配器：
//!
//! - [`build_router`]：D1 骨架（L0/L1），行为被 `tests/skeleton.rs` 钉住，
//!   **不得改动**——501/404/413/requestId 的断言一字不能破。
//! - [`build_router_with_auth`]：WR-D3 认证链（L2→L3→L4→L6）挂载在
//!   L1b 之内；§3.3 早期拒绝表的探针矩阵在 `tests/auth_chain.rs`。

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine as _;
use serde_json::{json, Value};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;

use datazen::execution::application::submission::{SubmissionPresentation, SubmissionTokenIssuer};
use datazen::execution::application::{IdempotentOperation, SubmissionToken};
use datazen::execution::domain::artifact_repository::ArtifactSpec;
use datazen::execution::SessionHandle;
use datazen::jobs::domain::job::{JobDefinition, JobState};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{
    ClientInstanceId, Counter, ExecutionId, IdempotencyKey, JobId, OrganizationId, PrincipalId,
    RequestId, Timestamp,
};

use crate::http::middleware;
use crate::http::response::ApiErrorResponse;

/// 请求体字节上限：team-server §9.1 首版权威值（8 MiB，L1 物理闸 → 413）。
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// L1a 的 from_fn 形状适配：上限取本模块常量，逻辑在 `middleware::body_limit`。
async fn content_length_gate(req: Request, next: Next) -> Response {
    middleware::body_limit::reject_oversized_content_length(req, next, MAX_BODY_BYTES).await
}

/// 组装完整 Router（机器面 + `/api/v1` 骨架 + 中间件链）。
///
/// 幂等：不读环境、不建连接，可以反复调用（集成测试就依赖这一点）。
pub fn build_router() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .fallback(api_or_not_found)
        // 装配顺序（外 → 内）：requestId → L1a Content-Length 闸 →
        // catch-panic → L1b 流式体闸 → handler。`Router::layer` 后加的在外层；
        // L0 必须最外，保证 413/500 也带 requestId。
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(CatchPanicLayer::new())
        .layer(axum::middleware::from_fn(content_length_gate))
        .layer(axum::middleware::from_fn(middleware::request_id::assign))
}

/// 认证链的装配依赖（composition root 或测试构造）。
pub struct AuthDeps {
    /// 管理库（登录会话行与 `csrf_rejected` 审计的落点；调用方负责先
    /// `migrate`——v5 起有 `login_sessions`，v6 起有 `audit_events`）。
    pub store: std::sync::Arc<crate::storage::AdminStore>,
    /// 授权引擎（§6；`server_secret` 在其构造时注入）。
    pub policy: std::sync::Arc<crate::policy::PolicyService>,
    /// `server_secret`（§5.1 CSRF HMAC 密钥；config 注入，不落库不进日志）。
    pub server_secret: Vec<u8>,
    /// 空闲寿命（秒；§4.3 默认 8 小时，来自部署配置）。
    pub idle_ttl_seconds: u64,
    /// L5 速率限制（§9.1；None = 不挂——骨架腿与最小部署形态）。
    pub rate_limit: Option<crate::http::middleware::ratelimit::RateLimitState>,
    /// SSE 事件流（WR-D5；None = 不挂 `/api/v1/events/*`）。
    pub sse: Option<crate::http::sse::SseState>,
    /// OIDC 登录面（§4；None = 不挂 `/api/v1/auth/*`——这些路径落到
    /// fallback 501，部署未启用 OIDC 的显式形态）。
    pub oidc: Option<crate::auth::oidc::OidcState>,
    /// WR-D6/D7 域路由（jobs / submission-tokens / artifacts；None = 不挂
    /// ——这些路径落到 fallback 501，最小部署形态显式）。
    pub domains: Option<DomainState>,
    /// WR-F2：MCP streamable HTTP 传输态（None = 不挂 `/mcp`）。
    pub mcp: Option<crate::mcp::http::HttpMcpState>,
    /// WR-C2 §6 读侧域路由片段（settings/identity/connections/sessions；
    /// composition root 预装配，None = 不挂——最小部署形态显式）。
    pub domain_routers: Option<Vec<Router>>,
}

/// WR-D6/D7 域路由的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct DomainState {
    /// `issueSubmissionToken` 的签发面（服务端密钥持有者，§6.3 88 行）。
    pub issuer: std::sync::Arc<dyn SubmissionTokenIssuer>,
    /// 幂等提交（含 `PreSubmitGate` 闸门，WR-D6 QuotaGate 适配点）。
    pub submission: std::sync::Arc<crate::transfer::SubmissionService>,
    /// 受控上传（begin/append/finalize 语义，wire envelope 见 handler 文档）。
    pub uploads: std::sync::Arc<crate::transfer::UploadService>,
    /// 受控读取（metadata/chunk/range 三取法，对齐前端 `readArtifact`）。
    pub downloads: std::sync::Arc<crate::transfer::DownloadService>,
    /// 任务中心查询面（list/detail/取消登记）。
    pub tasks: std::sync::Arc<crate::operations::TaskCenter<crate::storage::ServerJobs>>,
    /// 执行用例面（execution 域路由刀新增）：`issueSubmissionToken` 的会话
    /// 绑定操作（openSession/executeInSession/executeAtTarget/
    /// setSessionContext/cancelExecution）经它签发——令牌注册进运行时令牌
    /// 表，与 execute/cancel 路径的 `validate_token` 同一实例闭环。
    pub sessions:
        std::sync::Arc<dyn datazen::execution::application::use_cases::ConnectionUseCases>,
}

/// 组装带认证链的 Router：`build_router` 的全部面 + L2→L3→L4→(L5)→L6，
/// 以及本刀点亮的域路由（OIDC 登录面 / SSE 事件流——构造参数缺省即不挂）。
///
/// 层序（§3.1，请求穿过顺序）：L0 requestId → L1a Content-Length →
/// catch-panic → L1b 流式体闸 → **L2 认证 → L3 会话参数 → L4 CSRF →
/// L5 速率 → L6 授权** → handler/fallback。`Router::layer` 后加的在外层，
/// 所以代码里 L6 先写、L0 最后写；L5 在 L4 与 L6 之间（可选挂载）。
///
/// 失败策略 fail-closed：空/坏 `server_secret` 拒绝装配（`CsrfSecret` 的
/// 构造裁决）。幂等性同 [`build_router`]：不读环境，可反复调用。
pub fn build_router_with_auth(deps: AuthDeps) -> Result<Router, crate::error::ServerError> {
    use axum::routing::post;

    let csrf_secret = crate::csrf::CsrfSecret::new(deps.server_secret)
        .map_err(|message| crate::error::ServerError::ConfigInvalid(format!("csrf: {message}")))?;
    let state = middleware::AuthChainState {
        sessions: crate::storage::LoginSessions::new(deps.store.clone()),
        policy: deps.policy,
        csrf: std::sync::Arc::new(csrf_secret),
        audit: std::sync::Arc::new(crate::storage::ServerAudit::new(deps.store.clone())),
        idle_ttl_seconds: deps.idle_ttl_seconds,
    };

    // 域路由先挂（layer 只作用于先加的路由）；各自带独立状态。
    let mut router = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .fallback(api_or_not_found);
    if let Some(oidc) = deps.oidc.clone() {
        // §3.3：login/callback 是 L2 豁免路径（auth_exempt）；未认证的
        // 到达层语义由 L5 的 IP 桶承接。
        router = router
            .route(
                "/api/v1/auth/login",
                get(crate::auth::oidc::login).with_state(oidc.clone()),
            )
            .route(
                "/api/v1/auth/callback",
                get(crate::auth::oidc::callback).with_state(oidc),
            );
    }
    if let Some(sse) = deps.sse.clone() {
        // WR-D5：订阅建立（POST，L4 CSRF 适用）+ 事件流（GET，§8.1 的
        // Origin 证明在 handler 内）。
        router = router
            .route(
                "/api/v1/events/subscriptions",
                post(crate::http::sse::create_subscription).with_state(sse.clone()),
            )
            .route(
                "/api/v1/events/{streamId}",
                get(crate::http::sse::events).with_state(sse),
            );
    }
    if let Some(domains) = deps.domains.clone() {
        // WR-D6/D7：jobs 域（start/list/detail/cancel）+ 提交令牌签发面 +
        // artifacts 受控上传/读取（契约 §6.3 总账 + §6.5 在账端点）。
        router = router
            .route(
                "/api/v1/jobs",
                post(handler_jobs_submit)
                    .get(handler_jobs_list)
                    .with_state(domains.clone()),
            )
            .route(
                "/api/v1/jobs/{id}",
                get(handler_jobs_get).with_state(domains.clone()),
            )
            .route(
                "/api/v1/jobs/{id}/cancel",
                post(handler_jobs_cancel).with_state(domains.clone()),
            )
            .route(
                "/api/v1/submission-tokens",
                post(handler_issue_token).with_state(domains.clone()),
            )
            .route(
                "/api/v1/artifacts/uploads",
                post(handler_artifact_upload).with_state(domains.clone()),
            )
            .route(
                "/api/v1/artifacts/{id}",
                get(handler_artifact_read).with_state(domains),
            );
    }
    if let Some(mcp_state) = deps.mcp.clone() {
        // WR-F2：MCP streamable HTTP（`/mcp`，认证链内——传输层会话由
        // 登录会话背书，工具分发走 modules application 面）。
        router = router.merge(crate::mcp::http::mcp_router(mcp_state));
    }
    if let Some(extra) = deps.domain_routers.clone() {
        // WR-C2 §6 读侧域路由（settings/identity/connections/sessions——
        // 各域 Router 片段带独立 state，见 http/domains/ 逐域文件）。
        for child in extra {
            router = router.merge(child);
        }
    }

    // 内 → 外：L6 → (L5) → L4 → L3 → L2 → L1b → catch-panic → L1a → L0。
    let mut router = router.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        middleware::authorize::guard,
    ));
    if let Some(rate_limit) = deps.rate_limit {
        router = router.layer(axum::middleware::from_fn_with_state(
            rate_limit,
            middleware::ratelimit::guard,
        ));
    }
    let router = router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::csrf::guard,
        ))
        .layer(axum::middleware::from_fn(
            middleware::session::session_params,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state,
            middleware::auth::authenticate,
        ))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(CatchPanicLayer::new())
        .layer(axum::middleware::from_fn(content_length_gate))
        .layer(axum::middleware::from_fn(middleware::request_id::assign));
    Ok(router)
}

/// 存活探针：进程在服务即 200（team-server §2.4 / §5）。
async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// 就绪探针：composition root 走完即 200。WR-D2 引入管理库后，这里改为
/// 探测管理库可达（§2.4 的 drain 语义随 D6 补齐）。
async fn readyz() -> impl IntoResponse {
    (StatusCode::OK, "ready")
}

/// 一切未挂路由路径的统一出口。
///
/// `/api/` 前缀 → 501 `capabilityUnsupported`（本 build 尚未点亮任何域
/// 路由；点亮后本函数对已挂路由不再可达，只剩真正未实现的新路径）；
/// 其余 → 404 `notFound`。两条都走 [`ApiErrorResponse`]，wire 形状与
/// 契约一致。
async fn api_or_not_found(uri: Uri) -> Response {
    // `/api/` 前缀 → 501：本 build 尚未点亮任何域路由（not-implemented 类，
    // 契约 §7 的映射裁决）；点亮后本函数对已挂路由不再可达，只剩真正未
    // 实现的新路径。其余 → 404（status_for 的默认推导，防 ID 枚举口径）。
    // 两条都走 [`ApiErrorResponse`]，wire 形状与契约一致。
    if uri.path().starts_with("/api/") {
        let error = ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            "This build serves no /api/v1 domain routes yet; they arrive with WR-D2 and later slices.",
        );
        return ApiErrorResponse::new(error)
            .with_status(StatusCode::NOT_IMPLEMENTED)
            .into_response();
    }
    let error = ApiError::new(ApiErrorCode::NotFound, "No such route.");
    ApiErrorResponse::new(error).into_response()
}

// ── WR-D6/D7 域 handler ─────────────────────────────────────────────────────
//
// 身份来源只有 L2 认证链（INV-S1：extensions 的 `AuthenticatedSession`，
// 请求体里的身份字段永不生效）。wire 形状：契约 §6.3（jobs / submission-
// tokens）与 §6.5（uploads 在账端点，envelope 在此定形）。

/// 从 L2 认证链产物组装 `RequestContext` 的提取器——域 handler 的统一
/// 入口（对齐 sse.rs 的模式）。
pub(crate) struct SessionIdentity {
    pub ctx: RequestContext,
}

impl<S: Send + Sync> FromRequestParts<S> for SessionIdentity {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let request_id = parts.extensions.get::<RequestId>().cloned();
        let Some(session) = parts
            .extensions
            .get::<crate::http::middleware::auth::AuthenticatedSession>()
        else {
            return Err(unauthorized());
        };
        let ctx = RequestContext::new(
            OrganizationId::new(session.organization_id.clone()),
            PrincipalId::new(session.principal_id.clone()),
            None,
            ClientInstanceId::new("http"),
            request_id.unwrap_or_else(|| RequestId::new("req-http")),
            None,
        );
        Ok(Self { ctx })
    }
}

pub(crate) fn unauthorized() -> Response {
    let error = ApiError::new(ApiErrorCode::Unauthenticated, "Authentication required.");
    ApiErrorResponse::new(error)
        .with_status(StatusCode::UNAUTHORIZED)
        .into_response()
}

pub(crate) fn invalid_argument(message: impl Into<String>) -> Response {
    let error = ApiError::new(ApiErrorCode::InvalidArgument, message.into());
    ApiErrorResponse::new(error)
        .with_status(StatusCode::BAD_REQUEST)
        .into_response()
}

pub(crate) fn api_error_response(error: ApiError) -> Response {
    ApiErrorResponse::new(error).into_response()
}

/// `issueSubmissionToken`（§6.3：POST /api/v1/submission-tokens）。
///
/// 请求 `{ operation, handle?, connectionId?, owner? }`。签发面**按操作分派**
/// （execution 域路由刀的裁决，对齐各消费方的核验器）：
///
/// - `startJob` / `createProfile`：HMAC 签发器（`state.issuer`）——消费方
///   `SubmissionService` 用同一签发器的 `verify` 核验（无状态 HMAC）。
/// - `openSession` / `executeInSession` / `executeAtTarget` /
///   `setSessionContext` / `cancelExecution`：**执行用例面**
///   （`state.sessions.issue_submission_token`）——令牌注册进运行时令牌表
///   并被 execute/cancel 路径的 `validate_token` 按表核验；句柄要求、组织
///   归属、epoch 绑定全部由用例层闭环（本层不复制一套判定）。
///
/// 201 → `SubmissionToken`（`idempotencyKey` 是不透明串，客户端原样回传）。
/// 本刀顺带修复：此前会话绑定操作在 HTTP 面被硬拒（400）而
/// `openSession`/`cancelExecution` 走 HMAC 签发器时因绑 epoch 而必然失败
/// ——web 形态的开会话/取消执行在此前主上是端到端不通的（详见 PR 说明）。
async fn handler_issue_token(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let Some(operation) = body.get("operation").and_then(Value::as_str) else {
        return invalid_argument("operation is required.");
    };
    let operation = match operation {
        "startJob" => IdempotentOperation::StartJob,
        "createProfile" => IdempotentOperation::CreateProfile,
        "openSession" => IdempotentOperation::OpenSession,
        "executeInSession" => IdempotentOperation::ExecuteInSession,
        "executeAtTarget" => IdempotentOperation::ExecuteAtTarget,
        "setSessionContext" => IdempotentOperation::SetSessionContext,
        "cancelExecution" => IdempotentOperation::CancelExecution,
        other => return invalid_argument(format!("unknown operation '{other}'.")),
    };
    // 会话绑定操作：体里的 `handle`（camelCase `SessionHandle`）原样透传给
    // 用例面（在场性/归属/epoch 由 `RuntimeConnectionUseCases::issue` 闭环
    // ——`openSession` 无会话可绑，体 handle 为空也放行，运行时按 epoch 0 签；
    // `executeAtTarget` 是无会话一次性执行，同样不带句柄——W8 刀1对账
    // 修正：此前的在场性闸把 executeAtTarget 也算进必须带句柄的集合，而
    // 用例层的 execute 核验（`validate_token` 的 handle=None）不接受绑了
    // 句柄的令牌，端到端必然 permissionDenied）。
    let handle = if operation.binds_runtime_epoch() {
        match body.get("handle") {
            Some(Value::Null) | None => None,
            Some(value) => match serde_json::from_value::<SessionHandle>(value.clone()) {
                Ok(handle) => Some(handle),
                Err(source) => return invalid_argument(format!("handle: {source}")),
            },
        }
    } else {
        None
    };
    // 句柄在场性的 fail-fast 半边（与用例层 `issue` 的要求集合同一集合：
    // executeInSession / setSessionContext / cancelExecution；openSession 与
    // executeAtTarget 无会话可绑，用例层按 epoch 0 签发）。
    if matches!(
        operation,
        IdempotentOperation::ExecuteInSession
            | IdempotentOperation::SetSessionContext
            | IdempotentOperation::CancelExecution
    ) && handle.is_none()
    {
        return invalid_argument(
            "session-bound operations require the runtime session handle they bind to.",
        );
    }
    let issued = if matches!(
        operation,
        IdempotentOperation::StartJob | IdempotentOperation::CreateProfile
    ) {
        state
            .issuer
            .issue(&identity.ctx, operation, None)
            .await
            .map_err(|source| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    format!("token issue failed: {source}"),
                )
            })
    } else {
        state
            .sessions
            .issue_submission_token(&identity.ctx, operation, handle.as_ref())
            .await
    };
    match issued {
        Ok(token) => (
            StatusCode::CREATED,
            Json(json!({
                "idempotencyKey": token.idempotency_key.as_str(),
                "expiresAt": token.expires_at.as_str(),
            })),
        )
            .into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `startJob`（§6.3：POST /api/v1/jobs，携带 `issueSubmissionToken` 签发的
/// 幂等令牌）。请求 envelope：`{ submissionToken, definition }`——
/// `submissionToken` 接受字符串（`payload.tag` 原文）或
/// `{ idempotencyKey, expiresAt }`；`definition` 是 `JobDefinition` 的
/// camelCase JSON（受理时仓储按 ctx 组织重新盖章，定义里的 organizationId
/// 不作数）。成功 201 → `JobRecord`。
async fn handler_jobs_submit(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    const DISTANT_FUTURE: &str = "9999-12-31T23:59:59Z";
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
        _ => return invalid_argument("submissionToken is required."),
    };
    let definition = match serde_json::from_value::<JobDefinition>(
        body.get("definition").cloned().unwrap_or(Value::Null),
    ) {
        Ok(definition) => definition,
        Err(source) => return invalid_argument(format!("definition: {source}")),
    };
    // claimed 身份取自认证链事实（签名载荷与认证链不一致 → 服务判 Unauthenticated）。
    let presentation = SubmissionPresentation {
        token: SubmissionToken {
            idempotency_key: IdempotencyKey::new(idempotency_key),
            expires_at: Timestamp::new(expires_at),
        },
        claimed_organization_id: identity.ctx.organization_id.clone(),
        claimed_principal_id: identity.ctx.principal_id.clone(),
        claimed_client_instance_id: identity.ctx.client_instance_id.clone(),
        claimed_delegation_id: identity.ctx.delegation_id.clone(),
    };
    match state
        .submission
        .submit_job(&identity.ctx, &presentation, definition)
        .await
    {
        Ok(record) => (StatusCode::CREATED, Json(record)).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `listJobs`（§6.3：GET /api/v1/jobs；过滤包平铺为简单标量查询参数）。
/// `states` 逗号分隔（queued/running/succeeded/failed/cancelled）、
/// `after` 游标（十进制）、`limit` 页大小。owner 过滤随执行域路由点亮。
async fn handler_jobs_list(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut states = Vec::new();
    if let Some(raw) = params.get("states") {
        for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let job_state = match part {
                "queued" => JobState::Queued,
                "running" => JobState::Running,
                "succeeded" => JobState::Succeeded,
                "failed" => JobState::Failed,
                "cancelled" => JobState::Cancelled,
                other => return invalid_argument(format!("unknown job state '{other}'.")),
            };
            states.push(job_state);
        }
    }
    let after = match params.get("after") {
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(value) => Some(Counter::new(value)),
            Err(_) => return invalid_argument("after must be a decimal counter string."),
        },
        None => None,
    };
    let limit = match params.get("limit") {
        Some(raw) => match raw.trim().parse::<u32>() {
            Ok(value) => Some(value),
            Err(_) => return invalid_argument("limit must be a u32."),
        },
        None => None,
    };
    match state
        .tasks
        .list_jobs(
            &identity.ctx,
            crate::operations::TaskListQuery {
                states,
                owner: None,
                after,
                limit,
            },
        )
        .await
    {
        Ok(page) => Json(page).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `getJob`（§6.3：GET /api/v1/jobs/{id}）。跨组织按「不存在」答 404。
async fn handler_jobs_get(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
) -> Response {
    match state
        .tasks
        .get_job(&identity.ctx, &JobId::new(job_id))
        .await
    {
        Ok(record) => Json(record).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `cancelJob`（§6.3：POST /api/v1/jobs/{id}/cancel）。登记是幂等的；
/// 已终态任务如实回答（不伪造 202）。
async fn handler_jobs_cancel(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Path(job_id): Path<String>,
) -> Response {
    match state
        .tasks
        .request_cancel(&identity.ctx, &JobId::new(job_id))
        .await
    {
        Ok(outcome) => {
            let label = match &outcome {
                crate::operations::CancelOutcome::Registered => "registered",
                crate::operations::CancelOutcome::AlreadyRequested => "alreadyRequested",
                crate::operations::CancelOutcome::AlreadyTerminal { .. } => "alreadyTerminal",
            };
            (StatusCode::ACCEPTED, Json(json!({ "outcome": label }))).into_response()
        }
        Err(error) => api_error_response(error),
    }
}

/// 在账端点 `POST /api/v1/artifacts/uploads`（§6.5）的 v1 envelope：
/// `{ executionId, targetFingerprint, ttlSeconds?, payloadBase64,
/// expectedSha256? }` ——单请求完成 begin+append+finalize（分块流式写
/// 入器协议在 web 导出流立项时与 `artifactClient` 两头合拢；L1 字节闸
/// 是单请求形态的物理上界）。成功 201 → `{ artifactId }`。
async fn handler_artifact_upload(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let Some(execution_id) = body.get("executionId").and_then(Value::as_str) else {
        return invalid_argument("executionId is required.");
    };
    let Some(target_fingerprint) = body.get("targetFingerprint").and_then(Value::as_str) else {
        return invalid_argument("targetFingerprint is required.");
    };
    let ttl_seconds = body
        .get("ttlSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(3_600);
    let Some(payload_b64) = body.get("payloadBase64").and_then(Value::as_str) else {
        return invalid_argument("payloadBase64 is required.");
    };
    let expected_sha256 = body
        .get("expectedSha256")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let payload = match base64::engine::general_purpose::STANDARD.decode(payload_b64) {
        Ok(bytes) => bytes,
        Err(_) => return invalid_argument("payloadBase64 is not valid base64."),
    };
    let mut spec = ArtifactSpec::new(
        identity.ctx.organization_id.clone(),
        identity.ctx.principal_id.clone(),
        ExecutionId::new(execution_id),
        target_fingerprint.to_string(),
    );
    spec.ttl_seconds = ttl_seconds;
    let mut session = match state.uploads.begin_upload(&identity.ctx, spec).await {
        Ok(session) => session,
        Err(error) => return api_error_response(error),
    };
    if let Err(error) = state
        .uploads
        .append_upload_chunk(&identity.ctx, &mut session, payload)
        .await
    {
        return api_error_response(error);
    }
    match state
        .uploads
        .finalize_upload(&identity.ctx, session, expected_sha256.as_deref())
        .await
    {
        Ok(artifact_id) => (
            StatusCode::CREATED,
            Json(json!({ "artifactId": artifact_id.as_str() })),
        )
            .into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `readArtifact`（§6.3：GET /api/v1/artifacts/{id}）。三取法互斥：
/// `?metadata=true` / `?chunkIndex=N` / `?offset=&limit=`（对齐前端
/// `httpRoutes.ts` 的 `readArtifact` 与 `x-dz-*` 四头）。
async fn handler_artifact_read(
    State(state): State<DomainState>,
    identity: SessionIdentity,
    Path(artifact_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let parse_u64 = |key: &str| params.get(key).and_then(|v| v.trim().parse::<u64>().ok());
    let metadata = params
        .get("metadata")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let mode = match crate::transfer::ArtifactReadMode::from_query(
        metadata,
        parse_u64("chunkIndex"),
        parse_u64("offset"),
        parse_u64("limit"),
    ) {
        Ok(mode) => mode,
        Err(error) => return api_error_response(error),
    };
    match state
        .downloads
        .read(&identity.ctx, &artifact_id, mode)
        .await
    {
        Ok(reply) => reply.into_axum_response(),
        Err(error) => api_error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_limit_is_the_documented_first_version_value() {
        assert_eq!(MAX_BODY_BYTES, 8 * 1024 * 1024);
    }
}
