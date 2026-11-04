//! ai-host 域路由（WR-F1#3 的 HTTP 半边；TS 半边 =
//! `packages/web/src/platform/web/httpRoutes.ts` 的 `aiHost.*` 两行 +
//! `ai/aiPorts.ts` 的请求形状，体 = payload 原样，逐字冻结）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `aiHost.getSessionConfig` | `POST /api/v1/ai-host/session-config` | — | 200 | payload 原样 → `ExecutionSettings` JSON |
//! | `aiHost.executeTarget` | `POST /api/v1/ai-host/execute-target` | — | **501** | payload 原样（claim 进体）→ `capabilityUnsupported` |
//!
//! 域裁决：
//!
//! - **getSessionConfig = 设置域的同一切片**：会话配置就是
//!   `datazen::settings::ExecutionSettings`（serde camelCase 已有，直接
//!   `Json(settings)`）。请求 payload（`{sessionId}` 等）原样接受、不解读
//!   ——v1 的会话配置是服务端事实，不由请求塑形；体缺失/非 JSON 也按
//!   `None` 接受（payload 原样透传的宽容读面）。缺行按节补缺省
//!   （fresh install 语义，`storage::settings` 的装配口径）。
//! - **executeTarget fail-closed（v1 登记）**：目标执行机器（规范化目标
//!   元数据）随无状态 MCP 目标命令切片——本刀没有执行面，回答 501
//!   `capabilityUnsupported` + 精确文案，**绝不返回看似成功**（也不发稳定
//!   执行 id；异步接受语义 202 + 稳定 id 属该切片的契约 §3 规则 3）。
//!   payload 在 fail-closed 之前不解读、不落任何执行面。
//! - **组织边界（INV-01）**：[`AiHostState`] 由 composition root 绑定部署
//!   组织（`config.organization_id`，预装配 scoped store——settings.rs 的
//!   「每组织一个实例」口径）；会话组织与绑定组织不一致时，该 store 不服务
//!   此会话——按 CM-05 不可区分口径回 404 固定文案（不暴露
//!   「组织存在但未装配」）。
//! - **错误 wire**：设置读取故障 → 503 `serviceUnavailable`（settings 域的
//!   存储故障同款映射）。

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::Value;

use datazen::settings::SettingsRepository;
use datazen::shared::error::{ApiError, ApiErrorCode};

use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::response::ApiErrorResponse;
use crate::http::router::{api_error_response, SessionIdentity};
use crate::storage::ServerSettingsStore;

/// ai-host 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct AiHostState {
    /// 部署组织的设置存储（`ServerSettingsStore::scoped` 的预装配实例；
    /// 调用方负责先 `migrate`）。
    pub settings: Arc<ServerSettingsStore>,
    /// 本 state 绑定的组织（INV-01：与会话组织不一致的请求一律 404）。
    pub organization_id: String,
}

/// ai-host 域 Router 片段：`POST /api/v1/ai-host/{session-config,execute-target}`。
/// 主线把它 merge 进 `build_router_with_auth` 的认证链（L2–L6 之内）。
pub fn ai_host_router(state: AiHostState) -> Router {
    Router::new()
        .route(
            "/api/v1/ai-host/session-config",
            post(handler_get_session_config),
        )
        .route(
            "/api/v1/ai-host/execute-target",
            post(handler_execute_target),
        )
        .with_state(state)
}

/// 预装配 state 的组织边界核对（见模块头 INV-01 裁决）。
fn ensure_bound_organization(
    state: &AiHostState,
    identity: &SessionIdentity,
) -> Result<(), Response> {
    if identity.ctx.organization_id.as_str() == state.organization_id {
        return Ok(());
    }
    Err(api_error_response(ApiError::new(
        ApiErrorCode::NotFound,
        NOT_VISIBLE_MESSAGE,
    )))
}

fn service_unavailable(message: impl Into<String>) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        message.into(),
    ))
}

/// `aiHost.getSessionConfig`（POST /api/v1/ai-host/session-config，体 =
/// payload 原样）→ 200 `ExecutionSettings` JSON。
///
/// 「会话配置」v1 就是执行设置切片（安全门与设置域同一底层事实，TS 半边
/// `executionSettings()` 也读同一切片）：直接序列化存储侧的
/// `SharedSettings.execution`。
async fn handler_get_session_config(
    State(state): State<AiHostState>,
    identity: SessionIdentity,
    body: Option<Json<Value>>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    // payload 原样接受、不解读（v1 会话配置不由请求塑形；见模块头）。
    let _ = body;
    match state.settings.read_settings().await {
        Ok(shared) => (StatusCode::OK, Json(shared.execution)).into_response(),
        Err(source) => service_unavailable(format!("settings read failed: {source}")),
    }
}

/// `aiHost.executeTarget`（POST /api/v1/ai-host/execute-target，体 =
/// payload 原样，`AiTargetClaim` 进体）→ **501 `capabilityUnsupported`**。
///
/// fail-closed 登记（见模块头）：目标执行机器随无状态 MCP 目标命令切片。
/// 文案显式声明「没有开始任何执行、没有签发任何执行 id」——调用方不能把
/// 本响应解读成受理。payload 只被原样接受，不解读、不落执行面。
async fn handler_execute_target(
    State(state): State<AiHostState>,
    identity: SessionIdentity,
    body: Option<Json<Value>>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let _ = body;
    let error = ApiError::new(
        ApiErrorCode::CapabilityUnsupported,
        "aiHost.executeTarget is not served by this host yet: the target execution machine \
         (normalized target metadata) arrives with the stateless MCP target-command slice. \
         No execution was started and no stable execution id was issued.",
    );
    // 501（not-implemented 类）：`CapabilityUnsupported` 默认 405，501 类由
    // 调用点 `with_status` 显式覆盖（http::response 状态码表的同款先例——
    // 骨架 fallback 对未点亮 /api/ 路径的回答）。
    ApiErrorResponse::new(error)
        .with_status(StatusCode::NOT_IMPLEMENTED)
        .into_response()
}
