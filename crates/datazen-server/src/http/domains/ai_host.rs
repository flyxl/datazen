//! ai-host 域路由（WR-F1#3 的 HTTP 半边；TS 半边 =
//! `packages/web/src/platform/web/httpRoutes.ts` 的 `aiHost.*` 两行 +
//! `ai/aiPorts.ts` 的请求形状，体 = payload 原样，逐字冻结）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `aiHost.getSessionConfig` | `POST /api/v1/ai-host/session-config` | — | 200 | payload 原样 → `ExecutionSettings` JSON |
//! | `aiHost.executeTarget` | `POST /api/v1/ai-host/execute-target` | — | 200 | `{id, command, input, claim}` → `CommandResult`（W8 刀1 点亮） |
//!
//! 域裁决：
//!
//! - **getSessionConfig = 设置域的同一切片**：会话配置就是
//!   `datazen::settings::ExecutionSettings`（serde camelCase 已有，直接
//!   `Json(settings)`）。请求 payload（`{sessionId}` 等）原样接受、不解读
//!   ——v1 的会话配置是服务端事实，不由请求塑形；体缺失/非 JSON 也按
//!   `None` 接受（payload 原样透传的宽容读面）。缺行按节补缺省
//!   （fresh install 语义，`storage::settings` 的装配口径）。
//! - **executeTarget 已点亮（W8 刀1，替换原 501 登记）**：体四字段
//!   （`id` 连接 id / `command` 逻辑命令名 / `input` 命令参数 /
//!   `claim` 归因声明）解析后走与桌面 `AiRequestHost::execute_target`
//!   **同一条共享函数** `workflow::resource_execution::execute`（步骤名
//!   `ai-tool`，与桌面同值；一次性资源、独立固定租约、步骤超时 30s）。
//!   裁决分臂：
//!   - **`claim: independent`**：真执行。每调用包
//!     [`crate::execution::resource_host::PrincipalScopedHost`]（P3 同款
//!     装饰器）——执行面（token/execute/poll）携带请求真主体，闸序读
//!     半边的合成主体残留照 `resource_host.rs` 第 2 条登记。闸序失
//!     （profile/驱动/命令/SQL 守卫）在受理前如实映射：
//!     `Driver`/`ConnectionFailed` → 503，其余 → 400（driver_gateway
//!     既有裁决）；成功 → 200 `CommandResult`（`{data}` camelCase）。
//!   - **`claim: editorGrant`** → 501 `capabilityUnsupported` 精确文案：
//!     编辑器授权登记处（`AiGrantRegistry` 的签发/续期/过期机器）是桌面
//!     形态部件，server 无编辑器会话可借——登记不立语义，绝不返回看似
//!     成功。
//!   - claim 缺失/未知 kind → 400 `invalidArgument`。
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
use datazen::workflow::application::resource_execution::ResourceExecutionHost;
use datazen::workflow::domain::command::WorkflowCommandStep;
use datazen::workflow::domain::error::WorkflowError;

use crate::execution::resource_host::PrincipalScopedHost;
use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::response::ApiErrorResponse;
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
    /// 目标执行宿主（WR-F7 闸序端口；与 driverGateway/tableData/MCP
    /// 同一实例——bootstrap 既有纪律）。
    pub host: Arc<dyn ResourceExecutionHost>,
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
/// `{id, command, input, claim}`，轨 4-e 冻结契约的 payload 原样形状）→
/// 200 `CommandResult`（W8 刀1 点亮；分臂裁决见模块头）。
///
/// 归因声明（`AiTargetClaim`，`datazen::ai::application::ownership` 的
/// serde tag 形状）决定资源归属语义：`independent` 真执行（共享函数与
/// 桌面同源）；`editorGrant` 501 fail-closed（server 无编辑器授权登记
/// 处——登记不立语义，绝不返回看似成功）。
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
    // 四字段解析（轨 4-e 冻结契约：id/command/input/claim；字段面与
    // `aiPorts.ts::AiHostExecuteTargetRequest` 逐字一致）。
    let Some(id) = body.get("id").and_then(Value::as_str) else {
        return invalid_argument("executeTarget: id is required.");
    };
    let Some(command) = body.get("command").and_then(Value::as_str) else {
        return invalid_argument("executeTarget: command is required.");
    };
    let Some(input) = body.get("input").cloned() else {
        return invalid_argument("executeTarget: input is required.");
    };
    let Some(claim) = body.get("claim") else {
        return invalid_argument("executeTarget: claim is required.");
    };
    match claim.get("kind").and_then(Value::as_str) {
        Some("independent") => {}
        Some("editorGrant") => {
            // 501（not-implemented 类）：`CapabilityUnsupported` 默认 405，
            // 501 类由调用点 `with_status` 显式覆盖（http::response 状态码
            // 表的同款先例）。文案显式声明「没有开始任何执行」——调用方
            // 不能把本响应解读成受理。
            let error = ApiError::new(
                ApiErrorCode::CapabilityUnsupported,
                "aiHost.executeTarget does not serve editorGrant claims on this host: \
                 the editor grant registry is a desktop-form component and there is no \
                 editor session to borrow. No execution was started.",
            );
            return ApiErrorResponse::new(error)
                .with_status(StatusCode::NOT_IMPLEMENTED)
                .into_response();
        }
        _ => {
            return invalid_argument(
                "executeTarget: claim.kind must be \"independent\" or \"editorGrant\".",
            );
        }
    }
    // 与桌面 `AiRequestHost::execute_target` 的 Independent 分臂同一条共享
    // 函数：步骤名 `ai-tool`、一次性资源、无工作流截止、不要求工作流命令。
    // 每调用包 PrincipalScopedHost——执行面真主体（P3 同款装饰器）。
    let step = WorkflowCommandStep::new("ai-tool", command, Some(id.to_owned()), input);
    let host = PrincipalScopedHost {
        inner: state.host.clone(),
        context: identity.ctx.clone(),
    };
    match datazen::workflow::application::resource_execution::execute(
        &host, &step, id, None, None, false,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => workflow_gate_error(error),
    }
}

/// `WorkflowError` → HTTP（driver_gateway 既有裁决的 wire 半边）：
/// `Driver` / `ConnectionFailed` → 503 `serviceUnavailable`（目标不可
/// 服务，不是调用方的输入错误）；其余 → 400 `invalidArgument`（闸序/
/// 校验拒绝在受理前如实表达）。
fn workflow_gate_error(error: WorkflowError) -> Response {
    let code = match &error {
        WorkflowError::Driver(_) | WorkflowError::ConnectionFailed { .. } => {
            ApiErrorCode::ServiceUnavailable
        }
        _ => ApiErrorCode::InvalidArgument,
    };
    api_error_response(ApiError::new(code, error.to_string()))
}
