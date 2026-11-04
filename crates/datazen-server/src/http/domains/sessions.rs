//! sessions 域路由（契约 §6.4 的 HTTP 薄绑定）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `openSession` | `POST /api/v1/sessions` | ✓ | 201 | `OpenSessionRequest & SubmitRequest` → `OpenSessionReceipt` |
//! | `getSession` | `GET /api/v1/sessions/{id}` | — | 200 | — → `SessionView` |
//! | `setSessionContext` | `PUT /api/v1/sessions/{id}/context` | ✓ | 200 | `SetSessionContextRequest & SubmitRequest` → `ContextChangeReceipt` |
//! | `attachSession` | `POST /api/v1/sessions/{id}/attach` | — | 200 | `AttachmentRequest` → `SessionView` |
//! | `detachSession` | `POST /api/v1/sessions/{id}/detach` | — | 200 | `AttachmentRequest` → `SessionView` |
//! | `closeSession` | `POST /api/v1/sessions/{id}/close` | — | 200 | `CloseSessionRequest` → `CloseReceipt`（幂等关闭） |
//!
//! 域裁决（契约 §6.4 全文逐字对照）：
//!
//! - **`{id}` = `SessionHandle.dbSessionId`**（运行时句柄的稳定半边，
//!   前端 `httpRoutes.ts` 的 `pathSessionId` 裁决）。体里带完整 handle 的
//!   路由（context/attach/detach/close）先核对 `handle.dbSessionId` 与
//!   路径一致再进用例；`runtimeEpoch` 的核对留在用例层（§6.3「完全一致」
//!   语义——旧句柄 `SessionLost`，handler 不代位判定）。
//! - **`getSession` 的 epoch 补全**：路由只有稳定半边，`runtimeEpoch`
//!   以服务端会话目录的当前条目为准——`{id}` 寻址回答的是该 db 会话的
//!   **当前视图**。
//! - **授权可见性先于会话存活错误**：目录条目自带组织归属
//!   （`SessionOwner.organization_id`），越组织/未知 `{id}` 在触碰任何
//!   会话存活语义**之前**回答 404 固定文案（§7.1 的 `NotVisible` 映射，
//!   与 L6 的 `NOT_VISIBLE_MESSAGE` 同一条——不是 403，也不是
//!   `sessionNotFound`/`sessionLost` 的 409，防 ID 枚举）。
//! - **幂等令牌（openSession / setSessionContext 令牌列 ✓）**：handler
//!   只做在场性与 `idempotencyKey` 绑定核对；签名/过期/operation 绑定
//!   由用例层的令牌核验承担（`RuntimeConnectionUseCases::validate_token`
//!   ——签发面在 `router.rs` 的 `handler_issue_token`，主线把会话绑定
//!   分支改放行后，签发与消费走**同一**用例实例）。
//! - **close 幂等**（§7.5）：重复 close 查墓碑，同键回答原 `CloseReceipt`
//!   ——handler 不做去重，原样透传用例语义。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::Value;

use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::requests::{
    AttachmentRequest, CloseSessionRequest, OpenSessionRequest, SetSessionContextRequest,
};
use datazen::execution::{SessionDirectory, SessionHandle};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::DbSessionId;

use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};

/// sessions 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct SessionsState {
    /// 会话用例面（modules `ConnectionUseCases`——桌面 session 域命令调的
    /// 同一用例；server 侧实现 = 执行域装配的 `RuntimeConnectionUseCases`
    /// 或等价物，WR-D4 接线）。
    pub sessions: Arc<dyn ConnectionUseCases>,
    /// 会话目录（可见性判定的归属事实来源；条目带组织归属——主线装配时
    /// 必须与用例实现持有**同一**目录实例，见模块报告的接线登记）。
    pub directory: Arc<dyn SessionDirectory>,
}

/// sessions 域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内）。
pub fn sessions_router(state: SessionsState) -> Router {
    Router::new()
        .route("/api/v1/sessions", post(handler_open_session))
        .route("/api/v1/sessions/{id}", get(handler_get_session))
        .route(
            "/api/v1/sessions/{id}/context",
            put(handler_set_session_context),
        )
        .route("/api/v1/sessions/{id}/attach", post(handler_attach_session))
        .route("/api/v1/sessions/{id}/detach", post(handler_detach_session))
        .route("/api/v1/sessions/{id}/close", post(handler_close_session))
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 授权可见性闸（模块头裁决）：越组织/未知 `{id}` → 404 固定文案，
/// 先于一切会话存活语义。命中时返回目录句柄（`getSession` 用它补全
/// `runtimeEpoch` 半边）。
/// close 专用闸（闭容忍）：幂等 close 的墓碑回放（§7.5）要求已关闭会话
/// 的归属事实仍可查——`lookup_owner` 不筛 routable；防枚举的 404 合并口径
/// 与 [`ensure_visible`] 相同（CM-05）。命中返回目录 owner（epoch 供句柄
/// 核对），供墓碑路径继续走用例。
async fn ensure_owner(
    state: &SessionsState,
    ctx: &RequestContext,
    db_session_id: &str,
) -> Result<SessionHandle, Response> {
    let owner = match state
        .directory
        .lookup_owner(DbSessionId::new(db_session_id.to_string()))
        .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return Err(not_visible()),
        Err(source) => {
            return Err(api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            )));
        }
    };
    if owner.organization_id != ctx.organization_id {
        return Err(not_visible());
    }
    Ok(SessionHandle {
        db_session_id: DbSessionId::new(db_session_id.to_string()),
        runtime_epoch: owner.runtime_epoch.clone(),
    })
}

async fn ensure_visible(
    state: &SessionsState,
    ctx: &RequestContext,
    db_session_id: &str,
) -> Result<SessionHandle, Response> {
    let owner = match state
        .directory
        .lookup(DbSessionId::new(db_session_id.to_string()))
        .await
    {
        Ok(Some(owner)) => owner,
        Ok(None) => return Err(not_visible()),
        Err(source) => {
            return Err(api_error_response(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            )));
        }
    };
    if owner.organization_id != ctx.organization_id {
        return Err(not_visible());
    }
    Ok(owner.to_handle())
}

/// 越组织/不存在的统一 404（防 ID 枚举；与 L6 同一条固定文案）。
fn not_visible() -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

/// 体 handle 与路由 `{id}` 的一致性（`{id}` 是 dbSessionId 的取值）。
fn ensure_handle_matches_path(handle: &SessionHandle, db_session_id: &str) -> Result<(), Response> {
    if handle.db_session_id.as_str() != db_session_id {
        return Err(invalid_argument(
            "handle.dbSessionId does not match the addressed session.",
        ));
    }
    Ok(())
}

/// 令牌在场性 + `idempotencyKey` 绑定核对（令牌列 ✓ 的两条路由；签名与
/// operation 绑定由用例层核验，见模块头）。
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

// ── handlers ────────────────────────────────────────────────────────────────

/// `openSession`（§6.4：POST /api/v1/sessions）→ 201 `OpenSessionReceipt`。
/// §7.1：先写注册表与幂等记录，再返回回执——顺序语义在用例层。
async fn handler_open_session(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let request: OpenSessionRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("openSession: {source}")),
    };
    if let Err(error) = request.validate() {
        return api_error_response(error);
    }
    match state.sessions.open_session(&identity.ctx, request).await {
        Ok(receipt) => (StatusCode::CREATED, Json(receipt)).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `getSession`（§6.4：GET /api/v1/sessions/{id}）→ 200 `SessionView`。
/// 可见性闸在先（模块头裁决）；epoch 以目录当前条目补全。
async fn handler_get_session(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
) -> Response {
    let handle = match ensure_visible(&state, &identity.ctx, &db_session_id).await {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    match state.sessions.get_session(&identity.ctx, handle).await {
        Ok(view) => Json(view).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `setSessionContext`（§6.4：PUT /api/v1/sessions/{id}/context）→ 200
/// `ContextChangeReceipt`。§7.4 两阶段候选 → 服务端提交；`expectedContextRevision`
/// 冲突由用例层判 `contextConflict`。
async fn handler_set_session_context(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    if let Err(response) = ensure_visible(&state, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: SetSessionContextRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("setSessionContext: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return api_error_response(error);
    }
    match state
        .sessions
        .set_session_context(&identity.ctx, request)
        .await
    {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `attachSession`（§6.4：POST /api/v1/sessions/{id}/attach）→ 200
/// `SessionView`。`attachmentToken` 不匹配 ⇒ `contextConflict`（用例层）。
async fn handler_attach_session(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    attach_detach(state, identity, db_session_id, body, true).await
}

/// `detachSession`（§6.4：POST /api/v1/sessions/{id}/detach）→ 200
/// `SessionView`。分离只解除这个客户端的视图，**不**关闭会话（§7.5）。
async fn handler_detach_session(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    attach_detach(state, identity, db_session_id, body, false).await
}

/// attach/detach 共享路径（同请求形状 `AttachmentRequest`，wire 语义只差
/// 用例方法）。
async fn attach_detach(
    state: SessionsState,
    identity: SessionIdentity,
    db_session_id: String,
    body: Value,
    attach: bool,
) -> Response {
    if let Err(response) = ensure_visible(&state, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: AttachmentRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("attachment: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return api_error_response(error);
    }
    let outcome = if attach {
        state.sessions.attach_session(&identity.ctx, request).await
    } else {
        state.sessions.detach_session(&identity.ctx, request).await
    };
    match outcome {
        Ok(view) => Json(view).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `closeSession`（§6.4：POST /api/v1/sessions/{id}/close）→ 200
/// `CloseReceipt`。幂等关闭：重复 close 回答原墓碑回执（用例层语义，
/// handler 原样透传）。
async fn handler_close_session(
    State(state): State<SessionsState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_owner(&state, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: CloseSessionRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("closeSession: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return api_error_response(error);
    }
    match state.sessions.close_session(&identity.ctx, request).await {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => api_error_response(error),
    }
}
