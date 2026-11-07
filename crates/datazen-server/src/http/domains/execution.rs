//! execution 域路由（契约 §6.10；WR-F 执行域路由收口刀）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `executeInSession` | `POST /api/v1/sessions/{id}/executions` | ✓ | 202 | `ExecuteInSessionRequest & SubmitRequest` → `ExecutionReceipt` |
//! | `executeAtTarget` | `POST /api/v1/executions` | ✓ | 202 | `ExecuteAtTargetRequest & SubmitRequest` → `ExecutionReceipt` |
//! | `getExecution` | `GET /api/v1/executions/{id}` | — | 200 | — → `ExecutionView` |
//! | `cancelExecution` | `POST /api/v1/executions/{id}/cancel` | ✓ | 200 | `SubmitRequest` → `CancelReceipt` |
//!
//! 域裁决：
//!
//! - **异步接受语义（契约 §3 规则 3，team-server §9.3）**：两条 execute 路由
//!   回 **202** + 稳定 `executionId`（`ExecutionReceipt`）；已接受 ≠ SQL
//!   成功——终态经 `getExecution` / 事件流取。查询与取消回 200。
//! - **会话内执行的闸序（setSessionContext 同款）**：令牌在场 → 可见性
//!   （[`sessions::ensure_visible`]，越组织/未知会话 404 合并）→ 体解析 →
//!   `handle.dbSessionId` 与路由 `{id}` 一致性 → DTO 校验 → 用例。体
//!   handle 原样传递、**不**以目录条目改写——epoch 核对归用例层
//!   （`validate_token` 的 handle 绑定 + 注册记录完全一致语义）。
//! - **目标执行/执行读取/取消的组织闸**：`executeAtTarget` 的
//!   `target.connectionId` 归属与 `getExecution` / `cancelExecution` 的
//!   可见性由用例层承担（编排层 `authorize` + sink `visible_to` 的
//!   NotFound 合并口径，INV-01）——HTTP 层不做第二套组织判定。
//! - **取消体只留幂等凭据**（`submissionTokenBody` 半边）：
//!   `executionId` 进路径；`disposition: unsupported` 是正常取值
//!   （`CancelReceipt`，不以异常表达）。
//! - **幂等令牌签发的会话绑定操作**：`issueSubmissionToken` 对
//!   `openSession` / `executeInSession` / `executeAtTarget` /
//!   `setSessionContext` 需要 `SessionHandle`（router.rs 的登记随本刀
//!   放开）；令牌与请求的 operation/handle 绑定核验在用例层
//!   `validate_token` 闭环。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;

use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::requests::{ExecuteAtTargetRequest, ExecuteInSessionRequest};
use datazen::execution::SessionDirectory;
use datazen::shared::id::ExecutionId;

use crate::http::domains::sessions::{
    ensure_handle_matches_path, ensure_submission_token, ensure_visible,
};
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};

/// execution 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct ExecutionState {
    /// 连接用例面（sessions 域**同一** `ConnectionUseCases` 实例——租约
    /// 委托适配器；可见性闸查的目录也必须是同一实例）。
    pub sessions: Arc<dyn ConnectionUseCases>,
    /// 会话目录（会话内执行的可见性判定；bootstrap 与 `SessionsState`
    /// 递同一实例）。
    pub directory: Arc<dyn SessionDirectory>,
}

/// execution 域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内）。
pub fn execution_router(state: ExecutionState) -> Router {
    Router::new()
        .route(
            "/api/v1/sessions/{id}/executions",
            post(handler_execute_in_session),
        )
        .route("/api/v1/executions", post(handler_execute_at_target))
        .route("/api/v1/executions/{id}", get(handler_get_execution))
        .route(
            "/api/v1/executions/{id}/cancel",
            post(handler_cancel_execution),
        )
        .with_state(state)
}

/// `executeInSession`（§6.10：POST /api/v1/sessions/{id}/executions）→
/// 202 `ExecutionReceipt`。闸序见模块头（setSessionContext 同款）。
async fn handler_execute_in_session(
    State(state): State<ExecutionState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    if let Err(response) = ensure_visible(&state.directory, &identity.ctx, &db_session_id).await {
        return response;
    }
    let request: ExecuteInSessionRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("executeInSession: {source}")),
    };
    if let Err(response) = ensure_handle_matches_path(&request.handle, &db_session_id) {
        return response;
    }
    if let Err(error) = request.validate() {
        return api_error_response(error);
    }
    match state
        .sessions
        .execute_in_session(&identity.ctx, request)
        .await
    {
        Ok(receipt) => (StatusCode::ACCEPTED, Json(receipt)).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `executeAtTarget`（§6.10：POST /api/v1/executions）→ 202
/// `ExecutionReceipt`。无会话一次性执行；`expectedConfigRevision` 的 CAS
/// 语义由用例层判（配置已变 → `configRevisionMismatch`）。
async fn handler_execute_at_target(
    State(state): State<ExecutionState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let request: ExecuteAtTargetRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("executeAtTarget: {source}")),
    };
    if let Err(error) = request.validate() {
        return api_error_response(error);
    }
    match state
        .sessions
        .execute_at_target(&identity.ctx, request)
        .await
    {
        Ok(receipt) => (StatusCode::ACCEPTED, Json(receipt)).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `getExecution`（§6.10：GET /api/v1/executions/{id}）→ 200
/// `ExecutionView`。越组织/未知执行 → 用例层 NotFound（防枚举合并口径）。
async fn handler_get_execution(
    State(state): State<ExecutionState>,
    identity: SessionIdentity,
    Path(execution_id): Path<String>,
) -> Response {
    match state
        .sessions
        .get_execution(&identity.ctx, ExecutionId::new(execution_id))
        .await
    {
        Ok(view) => Json(view).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `cancelExecution`（§6.10：POST /api/v1/executions/{id}/cancel）→ 200
/// `CancelReceipt`。体只留幂等凭据（`submissionToken`）；取消登记语义
/// （先验证内部绑定，再回 `disposition: requested`）归用例层。
async fn handler_cancel_execution(
    State(state): State<ExecutionState>,
    identity: SessionIdentity,
    Path(execution_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    match state
        .sessions
        .cancel_execution(&identity.ctx, ExecutionId::new(execution_id))
        .await
    {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => api_error_response(error),
    }
}
