//! execution 域路由（契约 §6.4a 的 HTTP 薄绑定；W8 刀1 点亮）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `executeInSession` | `POST /api/v1/sessions/{id}/executions` | ✓ | 202 | `ExecuteInSessionRequest & SubmitRequest` → `ExecutionReceipt` |
//! | `executeAtTarget` | `POST /api/v1/executions` | ✓ | 202 | `ExecuteAtTargetRequest & SubmitRequest` → `ExecutionReceipt` |
//! | `getExecution` | `GET /api/v1/executions/{id}` | — | 200 | — → `ExecutionView` |
//! | `cancelExecution` | `POST /api/v1/executions/{id}/cancel` | ✓ | 200 | `SubmitRequest` → `CancelReceipt` |
//!
//! 域裁决（契约 §6.4a 全文逐字对照）：
//!
//! - **202 受理语义（概要 §6.3：异步接受 + 稳定 id）**：`ExecutionReceipt`
//!   = `{executionId, streamId, state}`——「已接受 ≠ SQL 成功」（回执 DTO
//!   原话），执行只登记了记录；终态经 `getExecution` 轮询或事件流读取。
//!   两条提交路由的闸序（profile/驱动/命令策略/SQL 守卫）都在用例层受理
//!   之前——闸后才有 202，**绝不受理跑不起来的执行**。
//! - **可见性三层**：① `executeInSession` 的 `{id}` 先过会话目录闸
//!   （`ensure_visible`，sessions 域同款 404 固定文案防枚举，授权可见性
//!   先于会话存活语义）+ 体 handle 与路径一致性核对；②
//!   `GET /api/v1/executions/{id}` 的两段形状由 L6 登记求值
//!   （`ResourceType::Execution`，`GET → Read`，rbac 基线矩阵裁决；
//!   三段 `/cancel` 子动作与集合级 `POST /api/v1/executions` 不入两层段
//!   表——action 语义在用例层）；③ 用例层 `RuntimeResultSink::view /
//!   handle` 按 `visible_to`（组织 + 主体 + 客户端实例 + 登录会话 + 委托
//!   五元组）过滤，未命中统一 404「execution unavailable」——执行不在
//!   会话目录里，归属事实由 sink 的发布侧登记，**handler 不发明第二条
//!   归属路径**。
//! - **令牌核对**：提交与取消路由的体都带 `SubmitRequest`
//!   （`submissionToken`）；handler 只做在场性与 `idempotencyKey` 绑定
//!   核对（sessions 域 `ensure_submission_token` 同款），签名/过期/
//!   operation 绑定核验在用例层 `validate_token`（签发面 = identity 域
//!   `POST /api/v1/submission-tokens`，同一 runtime 实例）。cancel 的
//!   用例签名（§7.6）不带令牌参数——内部绑定验证先行，令牌在场性仍是
//!   本层的前置闸（防无凭据探测取消）。
//! - **跨进程取消登记（v1 单进程语义登记）**：`cancel_execution` 的
//!   取消登记（`registry.cancel_registered`）是 runtime 进程内语义——
//!   取消标记不跨进程传播。多进程部署形态（任务外置/多副本）的取消
//!   登记随部署切片演进；v1 的 HTTP 面如实表达单进程事实（WR-D6 收口
//!   登记：本刀 = 执行域路由收口点）。
//! - **请求体 = 共享 DTO 原样**：`ExecuteInSessionRequest` /
//!   `ExecuteAtTargetRequest` 的 serde camelCase 形状即 wire 形状
//!   （backend-client `executionClient.ts` 同一类型面），handler 反序列化
//!   后先 `validate()` 再透传用例。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;

use datazen::execution::application::requests::{ExecuteAtTargetRequest, ExecuteInSessionRequest};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::SessionDirectory;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::DbSessionId;

use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};

/// execution 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct ExecutionState {
    /// 执行用例面（租约委托适配器 `LeaseConnectionUseCases`——sessions
    /// 域同一实例，每调用现取；bootstrap 既有纪律）。
    pub sessions: Arc<dyn ConnectionUseCases>,
    /// 会话目录（`executeInSession` 可见性闸的归属事实来源；与用例实现
    /// 持有**同一**实例——sessions 域模块头的接线登记同款纪律）。
    pub directory: Arc<dyn SessionDirectory>,
}

/// execution 域 Router 片段。主线把它 merge 进 `build_router_with_auth`
/// 的认证链（L2–L6 之内）。
pub fn execution_router(state: ExecutionState) -> Router {
    Router::new()
        .route("/api/v1/executions", post(handler_execute_at_target))
        .route("/api/v1/executions/{id}", get(handler_get_execution))
        .route(
            "/api/v1/executions/{id}/cancel",
            post(handler_cancel_execution),
        )
        .route(
            "/api/v1/sessions/{id}/executions",
            post(handler_execute_in_session),
        )
        .with_state(state)
}

// ── 共享判定 ────────────────────────────────────────────────────────────────

/// 会话可见性闸（sessions 域 `ensure_visible` 同款）：越组织/未知 `{id}`
/// → 404 固定文案，先于一切执行语义。命中返回目录条目的 handle（供体
/// 一致性核对）。
async fn ensure_visible(
    state: &ExecutionState,
    organization_id: &str,
    db_session_id: &str,
) -> Result<datazen::execution::SessionHandle, Response> {
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
    if owner.organization_id.as_str() != organization_id {
        return Err(not_visible());
    }
    Ok(owner.to_handle())
}

/// 令牌在场性 + `idempotencyKey` 绑定核对（sessions 域同款：签名与
/// operation 绑定由用例层核验）。
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

/// 越组织/不存在的统一 404（防 ID 枚举；与 L6/sessions 域同一条固定文案）。
fn not_visible() -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

// ── handlers ────────────────────────────────────────────────────────────────

/// `executeInSession`（§6.4a：POST /api/v1/sessions/{id}/executions）→
/// 202 `ExecutionReceipt`。目录可见性闸与 handle 一致性在先（模块头
/// 裁决）；幂等键登记与排队语义（INV-03：同会话普通执行排队不 busy）
/// 在用例层。
async fn handler_execute_in_session(
    State(state): State<ExecutionState>,
    identity: SessionIdentity,
    Path(db_session_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let handle = match ensure_visible(
        &state,
        identity.ctx.organization_id.as_str(),
        &db_session_id,
    )
    .await
    {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    if let Err(response) = ensure_submission_token(&body) {
        return response;
    }
    let request: ExecuteInSessionRequest = match serde_json::from_value(body) {
        Ok(request) => request,
        Err(source) => return invalid_argument(format!("executeInSession: {source}")),
    };
    // 体 handle 与路由 `{id}` 的一致性（§6.3「完全一致」语义——路径是
    // 寻址事实，体是声明；不一致 = 调用方错位，不是会话错误）。
    if request.handle.db_session_id != handle.db_session_id {
        return invalid_argument(
            "request handle does not match the addressed session in the path.",
        );
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

/// `executeAtTarget`（§6.4a：POST /api/v1/executions）→ 202
/// `ExecutionReceipt`。组织级动作（无 id 寻址）：目录闸不适用，闸序
/// （profile → 驱动 → 命令策略 → SQL 守卫）与授权在用例层受理前完成。
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

/// `getExecution`（§6.4a：GET /api/v1/executions/{id}）→ 200
/// `ExecutionView`。可见性由用例层 `visible_to` 五元组过滤兜底（模块头
/// 裁决第 ③ 层）——未知与他人执行统一 404「execution unavailable」，
/// 无存在性预言机；L6 对本形状的基线求值（Execution + Read）在其外。
async fn handler_get_execution(
    State(state): State<ExecutionState>,
    identity: SessionIdentity,
    Path(execution_id): Path<String>,
) -> Response {
    match state
        .sessions
        .get_execution(
            &identity.ctx,
            datazen::shared::id::ExecutionId::new(execution_id),
        )
        .await
    {
        Ok(view) => Json(view).into_response(),
        Err(error) => api_error_response(error),
    }
}

/// `cancelExecution`（§6.4a：POST /api/v1/executions/{id}/cancel）→ 200
/// `CancelReceipt`。§7.6 顺序语义在用例层：先验证内部绑定，再回
/// `disposition: requested`；终态执行幂等回 `AlreadyFinished`。本层前置
/// 闸只有令牌在场性（防无凭据探测取消；绑定核验在用例层 validate_token
/// ——签发面 identity 域同一 runtime）。
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
        .cancel_execution(
            &identity.ctx,
            datazen::shared::id::ExecutionId::new(execution_id),
        )
        .await
    {
        Ok(receipt) => Json(receipt).into_response(),
        Err(error) => api_error_response(error),
    }
}
