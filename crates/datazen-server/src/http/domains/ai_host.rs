//! ai-host 域路由（WR-F1#3 的 HTTP 半边；TS 半边 =
//! `packages/web/src/platform/web/httpRoutes.ts` 的 `aiHost.*` 两行 +
//! `ai/aiPorts.ts` 的请求形状，体 = payload 原样，逐字冻结）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `aiHost.getSessionConfig` | `POST /api/v1/ai-host/session-config` | — | 200 | payload 原样 → `ExecutionSettings` JSON |
//! | `aiHost.executeTarget` | `POST /api/v1/ai-host/execute-target` | — | 200 | `{id, command, input, claim?}` → `CommandResult`（`WorkflowError` → driver_gateway 同表错误映射） |
//!
//! 域裁决：
//!
//! - **getSessionConfig = 设置域的同一切片**：会话配置就是
//!   `datazen::settings::ExecutionSettings`（serde camelCase 已有，直接
//!   `Json(settings)`）。请求 payload（`{sessionId}` 等）原样接受、不解读
//!   ——v1 的会话配置是服务端事实，不由请求塑形；体缺失/非 JSON 也按
//!   `None` 接受（payload 原样透传的宽容读面）。缺行按节补缺省
//!   （fresh install 语义，`storage::settings` 的装配口径）。
//! - **executeTarget = 目标命令执行（执行域路由收口刀点亮）**：体
//!   `{id, command, input, claim?}`——`id` 是连接 id，`command` 是驱动命令
//!   目录名，`input` 是命令输入（claim 原样接受不解读：授予态是客户端
//!   事实，桌面 `execute_target` 同款不消费它）。执行经共享
//!   `workflow::resource_execution::execute`（宿主 = bootstrap 装配的
//!   `ServerResourceExecutionHost`，与 MCP 目标机器 / driverGateway 同一
//!   实例）；目标闸序（驱动可用性、safe_mode、目标要求）全部复用该层。
//!   成功 200 → `CommandResult`；`WorkflowError` →
//!   driver_gateway 同一张错误映射表（503 连接/驱动不可用等）。同步
//!   应答：本端点不签发稳定执行 id、不入异步执行记录（与桌面 IPC 同一
//!   形态——异步语义属 execution 域的 `executeAtTarget`，见契约
//!   §6.10）。payload 在缺 `id`/`command` 时 400 明拒，绝不假装成功。
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
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::ServerSettingsStore;

/// ai-host 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct AiHostState {
    /// 部署组织的设置存储（`ServerSettingsStore::scoped` 的预装配实例；
    /// 调用方负责先 `migrate`）。
    pub settings: Arc<ServerSettingsStore>,
    /// 本 state 绑定的组织（INV-01：与会话组织不一致的请求一律 404）。
    pub organization_id: String,
    /// 目标执行宿主（执行域路由收口刀新增）：bootstrap 装配的
    /// `ServerResourceExecutionHost`，与 MCP 目标机器 / driverGateway 同一
    /// 实例（共享驱动注册表 / 设置存储 / 租约源）。
    pub host: Arc<crate::execution::resource_host::ServerResourceExecutionHost>,
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
/// `{id, command, input, claim?}`）→ 200 `CommandResult`。
///
/// 目标命令执行（执行域路由收口刀点亮，桌面 `execute_target` 同一执行
/// 路径）：`WorkflowCommandStep`（kind="tool"，connection=id）经共享
/// `workflow::resource_execution::execute` 跑在 bootstrap 装配的
/// `ServerResourceExecutionHost` 上。`claim` 原样接受不解读（授予态是
/// 客户端事实，桌面同款）；缺 `id` / `command`（或类型不符）→ 400，
/// 绝不假装成功；`WorkflowError` → driver_gateway 同一张错误映射表。
async fn handler_execute_target(
    State(state): State<AiHostState>,
    identity: SessionIdentity,
    body: Option<Json<Value>>,
) -> Response {
    if let Err(response) = ensure_bound_organization(&state, &identity) {
        return response;
    }
    let Some(Json(body)) = body else {
        return invalid_argument("aiHost.executeTarget requires a JSON body.");
    };
    let Some(connection_id) = body.get("id").and_then(Value::as_str) else {
        return invalid_argument("id (connection id) is required.");
    };
    let Some(command) = body.get("command").and_then(Value::as_str) else {
        return invalid_argument("command (driver command name) is required.");
    };
    let input = body.get("input").cloned().unwrap_or(Value::Null);
    // claim 原样接受不解读（见模块头）；体其余字段不阻断（payload 原样
    // 透传的宽容读面）。
    let step = datazen::workflow::domain::command::WorkflowCommandStep::new(
        "tool",
        command,
        Some(connection_id.to_owned()),
        input,
    );
    match datazen::workflow::resource_execution::execute(
        state.host.as_ref(),
        &step,
        connection_id,
        None,
        None,
        false,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => crate::http::domains::driver_gateway::workflow_error_response(error),
    }
}
