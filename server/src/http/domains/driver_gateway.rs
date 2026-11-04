//! driverGateway 过渡域路由（契约 §6.9 的四方法 HTTP 薄绑定）。
//!
//! **过渡域**：snake_case 方法名保留——它们是现存 IPC 命令名，不是内核
//! 名；整个域在内核实现驱动命令面后删除（契约 §6.9 原文）。请求体是
//! **不透明透传**的嵌套驱动命令，不携带 `SubmitRequest`，v1 不引入幂等
//! 令牌——收编进内核域时按各域规则补。
//!
//! ## 4 方法 × 状态表（本刀登记）
//!
//! | 方法 | 路由 | 状态 | 说明 |
//! | --- | --- | --- | --- |
//! | `get_driver_commands` | `GET /api/v1/driver-gateway/commands?driverType=` | ✅ 真实现 | registry 直取驱动 → `discovery_definitions`；v1 空目录下恒 404「Driver not found」（桌面同文案）——registry 点亮后自动变绿 |
//! | `get_connection_commands` | `GET /api/v1/driver-gateway/commands?dbSessionId=` | ✅ 真实现 | 会话目录反查（归属核对 CM-05）→ `ResourceExecutionHost` 闸序（profile → driver）→ 同一份目录合成；v1 空目录下驱动闸 503「Driver unavailable」 |
//! | `execute_driver_command` | `POST /api/v1/driver-gateway/executions` | ✅ 真实现（dbSessionId 路径） | 闸序 + `command_policy` 编排（定义解析/调用校验/输入校验/HighRisk 访问分级/SQL 绑定与守卫）+ `resource_execution::execute` 全量执行；`driverType` 直取（无连接执行）⛔ 501——驱动自建会话的句柄生命周期未装配 |
//! | `execute_driver_command_stream` | `POST /api/v1/driver-gateway/stream-executions` | ⛔ 501（闸后） | 同款闸序跑到驱动闸；驱动可用后的**事件投递桥未装配**（`QueryStreamEvent` → 统一事件端点的桥需要产物化语义裁决——桌面是 Tauri Channel 行推送，HTTP 形态按契约走 events 端点，词汇映射登记随部署构建切片），显式 501 不假装建流 |
//!
//! ## 闸序（与 WR-F7 目标机器同源，同一 `ResourceExecutionHost` 实例）
//!
//! 会话目录（归属核对先于一切）→ profile 存在 → enabled → 驱动可用 →
//! 命令编排 → 执行。每一闸的失败都是**机器真跑出来的应答**，不是登记
//! 文案：v1 空驱动目录下驱动闸（503「Driver unavailable」）先于任何
//! 策略求值；registry 点亮后绿路径自动可达（编排与执行半边已真实现）。
//!
//! ## 档位与守卫（对齐桌面 `run_driver_command` 的编排规则）
//!
//! `permission_mode = None` → `access_level_for_mode(None) = HighRisk`：
//! GUI 的 HTTP 对位就是全量命令面（MCP 面的 SafeWrite 收窄是 MCP 的
//! 装配参数，不适用于本域）。认证链 L2–L6 与 L4 CSRF 由 router merge
//! 循环保证。`validate_invocation` 的 `interactive = false`：声明保存
//! 对话框的命令在 HTTP 形态被拒（与桌面内部复用路径同语义——HTTP 没有
//! 原生对话框可弹）。SQL 守卫（只读连接 / safe_mode / SQL 绑定）在
//! `resource_execution::execute` 内部按连接配置与宿主 safe_mode 求值。
//!
//! ## WorkflowError → HTTP 映射（本域首个映射，登记为裁决）
//!
//! 调用方可修正的（Validation/Target*/Missing*/UnsupportedCommand/
//! CommandNotInWorkflow/UnsupportedDefinitionVersion/UnsupportedPlan/
//! Template/Step/Ai/Other）→ 400；环境不可用（Driver/ConnectionFailed）
//! → 503；超时（GlobalTimeout）→ 504；执行终态异常（SqlUnconfirmed/
//! SqlResultRead/CapabilityUnsupported）→ 500。`recordHistory` 请求字段
//! 透传接受但 v1 无命令历史半边（桌面记 SQL 历史；服务端历史面随命令
//! 历史域切片）——登记缺口，不影响执行语义。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use datazen::execution::application::command_policy::{
    access_level_for_mode, bind_sql_params, discovery_definitions, inject_sql_target_fields,
    resolve_definition, validate_invocation,
};
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::SessionDirectory;
use datazen::settings::ExecutionSettings;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::DbSessionId;
use datazen::workflow::application::resource_execution::{self, ResourceExecutionHost};
use datazen::workflow::domain::command::WorkflowCommandStep;
use datazen::workflow::domain::error::WorkflowError;
use datazen_driver_api::{check_command_access, validate_command_input, DatabaseDriver};

use crate::http::response::ApiErrorResponse;
use crate::http::router::{invalid_argument, SessionIdentity};

/// driverGateway 域的装配状态：驱动注册表、目标执行宿主（与 WR-F7 目标
/// 机器同一实例）、会话目录（sessions 域同一实例）与执行设置。
#[derive(Clone)]
pub struct DriverGatewayState {
    pub registry: Arc<DriverRegistry>,
    pub host: Arc<dyn ResourceExecutionHost>,
    pub directory: Arc<dyn SessionDirectory>,
    pub settings: ExecutionSettings,
}

/// `POST /api/v1/driver-gateway/executions` 的请求体（桌面
/// `ExecuteDriverCommandRequest` 的 wire 形状，serde camelCase 逐字）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteDriverCommandBody {
    #[serde(default)]
    db_session_id: Option<String>,
    #[serde(default)]
    driver_type: Option<String>,
    #[serde(default)]
    command: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    database: Option<String>,
    #[serde(default)]
    schema: Option<String>,
}

/// `POST /api/v1/driver-gateway/stream-executions` 的请求体（桌面
/// `ExecuteDriverCommandStreamRequest` 的 wire 形状）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteDriverCommandStreamBody {
    #[serde(default)]
    db_session_id: Option<String>,
    #[serde(default)]
    command: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    database: Option<String>,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    apply_result_limit: Option<bool>,
    #[serde(default)]
    record_history: Option<bool>,
}

pub fn driver_gateway_router(state: DriverGatewayState) -> Router {
    Router::new()
        .route("/api/v1/driver-gateway/commands", get(handler_get_commands))
        .route(
            "/api/v1/driver-gateway/executions",
            post(handler_execute_command),
        )
        .route(
            "/api/v1/driver-gateway/stream-executions",
            post(handler_execute_stream),
        )
        .with_state(state)
}

// ── 错误映射 ───────────────────────────────────────────────────────────────

/// `WorkflowError` → HTTP（裁决见模块头；每一个状态码都可以指回变体集）。
fn workflow_error_response(error: WorkflowError) -> Response {
    let status = match &error {
        WorkflowError::Driver(_) | WorkflowError::ConnectionFailed { .. } => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        WorkflowError::GlobalTimeout(_) => StatusCode::GATEWAY_TIMEOUT,
        WorkflowError::SqlUnconfirmed { .. }
        | WorkflowError::SqlResultRead { .. }
        | WorkflowError::CapabilityUnsupported { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    let code = match &error {
        WorkflowError::ConnectionFailed { .. } | WorkflowError::Driver(_) => {
            ApiErrorCode::ServiceUnavailable
        }
        _ => ApiErrorCode::InvalidArgument,
    };
    ApiErrorResponse::new(ApiError::new(code, error.to_string()))
        .with_status(status)
        .into_response()
}

/// 501 `capabilityUnsupported`（not-implemented 类；`table_data::fail_closed`
/// 同款形状——路由在账，handler 显式回答不实现，绝不返回看似成功的结果）。
fn fail_closed(reason: String) -> Response {
    let error = ApiError::new(
        ApiErrorCode::CapabilityUnsupported,
        format!("{reason} No statement ran, no plan was built and nothing was written."),
    );
    ApiErrorResponse::new(error)
        .with_status(StatusCode::NOT_IMPLEMENTED)
        .into_response()
}

/// 驱动闸的 503（`Driver("Driver unavailable")` 等环境不可用应答）。
fn driver_gate_error(error: WorkflowError) -> Response {
    workflow_error_response(error)
}

// ── 会话目录闸（归属核对先于一切，CM-05）─────────────────────────────────

/// `dbSessionId` → 连接 id 的反查：未知/越组织统一 404（防枚举口径与
/// sessions 域同一条固定文案）；命中返回 owner 的 connection_id。
async fn resolve_session_connection(
    state: &DriverGatewayState,
    identity: &SessionIdentity,
    db_session_id: &str,
) -> Result<String, Response> {
    let owner = state
        .directory
        .lookup(DbSessionId::new(db_session_id.to_string()))
        .await
        .map_err(|source| {
            ApiErrorResponse::new(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("session directory unavailable: {source}"),
            ))
            .with_status(StatusCode::SERVICE_UNAVAILABLE)
            .into_response()
        })?
        .ok_or_else(not_visible)?;
    if owner.organization_id.as_str() != identity.ctx.organization_id.as_str() {
        return Err(not_visible());
    }
    Ok(owner.connection_id.as_str().to_owned())
}

/// 越组织/不存在的统一 404（防 ID 枚举；与 sessions 域/L6 共享同一固定
/// 文案常量——防枚举口径全仓一条）。
fn not_visible() -> Response {
    ApiErrorResponse::new(ApiError::new(
        ApiErrorCode::NotFound,
        crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE,
    ))
    .with_status(StatusCode::NOT_FOUND)
    .into_response()
}

// ── 命令目录（GET /commands）──────────────────────────────────────────────

/// `get_driver_commands` / `get_connection_commands`（GET，§6.9）：参数
/// 互斥（`driverType` XOR `dbSessionId`，恰一），同一响应形状
/// `Vec<DriverCommandDefinition>`。
async fn handler_get_commands(
    State(state): State<DriverGatewayState>,
    identity: SessionIdentity,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let driver_type = params
        .get("driverType")
        .map(|value| value.trim())
        .filter(|s| !s.is_empty());
    let db_session_id = params
        .get("dbSessionId")
        .map(|value| value.trim())
        .filter(|s| !s.is_empty());
    let definitions = match (driver_type, db_session_id) {
        (Some(driver_type), None) => {
            // driverType 直取：registry 点亮后自动变绿；v1 空目录下这是
            // 「Driver not found」的真源头（registry.get 的真实答案）。
            let Some(driver) = state.registry.get_sql_driver_by_name(driver_type).await else {
                return not_found_driver(driver_type);
            };
            Ok(discovery_definitions(driver.as_ref()))
        }
        (None, Some(db_session_id)) => {
            let connection_id =
                match resolve_session_connection(&state, &identity, db_session_id).await {
                    Ok(connection_id) => connection_id,
                    Err(response) => return response,
                };
            match connection_driver(&state, &connection_id).await {
                Ok(driver) => Ok(discovery_definitions(driver.as_ref())),
                Err(error) => return driver_gate_error(error),
            }
        }
        (Some(_), Some(_)) => Err(invalid_argument(
            "driverType and dbSessionId are mutually exclusive; pass exactly one",
        )),
        (None, None) => Err(invalid_argument(
            "missing query parameter: pass either driverType or dbSessionId",
        )),
    };
    match definitions {
        Ok(definitions) => (StatusCode::OK, Json(definitions)).into_response(),
        Err(response) => response,
    }
}

fn not_found_driver(driver_type: &str) -> Response {
    ApiErrorResponse::new(ApiError::new(
        ApiErrorCode::NotFound,
        format!("Driver not found: {driver_type}"),
    ))
    .with_status(StatusCode::NOT_FOUND)
    .into_response()
}

/// `ResourceExecutionHost` 的前两道闸（profile 存在 → enabled → 驱动可用），
/// 返回驱动实例供命令编排。enabled=false 的回答与目标机器同源
/// （「Connection profile is disabled」）。
async fn connection_driver(
    state: &DriverGatewayState,
    connection_id: &str,
) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
    let (config, _, enabled) = state.host.profile(connection_id).await.map_err(|error| {
        WorkflowError::ConnectionFailed {
            connection_id: connection_id.to_owned(),
            message: error.to_string(),
        }
    })?;
    if !enabled {
        return Err(WorkflowError::Validation(
            "Connection profile is disabled".into(),
        ));
    }
    state.host.driver(&config).await
}

// ── 透传执行（POST /executions）──────────────────────────────────────────

/// `execute_driver_command`（POST，§6.9）：嵌套驱动命令请求 →
/// `CommandResult`。dbSessionId 路径真实现（闸序 + 编排 + 全量执行）；
/// driverType 直取 ⛔ 501（模块头状态表）。
async fn handler_execute_command(
    State(state): State<DriverGatewayState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: ExecuteDriverCommandBody = match serde_json::from_value(body) {
        Ok(body) => body,
        Err(source) => return invalid_argument(format!("request: {source}")),
    };
    let command = body.command.trim().to_owned();
    if command.is_empty() {
        return invalid_argument("missing required input property: command");
    }
    let driver_type = body
        .driver_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let db_session_id = body
        .db_session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (driver_type, db_session_id) {
        (Some(driver_type), None) => {
            let Some(_driver) = state.registry.get_sql_driver_by_name(driver_type).await else {
                return not_found_driver(driver_type);
            };
            fail_closed(
                "direct (connection-less) driver execution is not served by this host yet: \
                 drivers that mint their own session handles have no lifecycle face in the \
                 server form; the session-bound path is the v1 execution face."
                    .to_owned(),
            )
        }
        (None, Some(db_session_id)) => {
            let connection_id =
                match resolve_session_connection(&state, &identity, db_session_id).await {
                    Ok(connection_id) => connection_id,
                    Err(response) => return response,
                };
            execute_session_bound(
                &state,
                connection_id,
                body.command,
                body.input,
                body.database,
                body.schema,
            )
            .await
        }
        (Some(_), Some(_)) => {
            invalid_argument("driverType and dbSessionId are mutually exclusive; pass exactly one")
        }
        (None, None) => invalid_argument(
            "missing required input property: pass either driverType or dbSessionId",
        ),
    }
}

/// 会话绑定执行的完整编排（对齐桌面 `run_driver_command` 的形态中立规则；
/// 闸序 → command_policy 编排 → `resource_execution::execute` 全量执行）。
async fn execute_session_bound(
    state: &DriverGatewayState,
    connection_id: String,
    command: String,
    mut input: Value,
    database: Option<String>,
    schema: Option<String>,
) -> Response {
    let driver = match connection_driver(&state, &connection_id).await {
        Ok(driver) => driver,
        Err(error) => return driver_gate_error(error),
    };
    // 编排规则（形态中立，桌面同一份 command_policy 用例）：
    let envelope_has_target = database.is_some() || schema.is_some();
    let definition = match resolve_definition(driver.as_ref(), &command, envelope_has_target) {
        Ok(definition) => definition,
        Err(reason) => return invalid_argument(reason),
    };
    // HTTP 形态没有原生对话框：声明保存对话框的命令被拒（interactive=false，
    // 与桌面内部复用路径同语义）；permission_mode=None（GUI 全量档位）。
    if let Err(reason) = validate_invocation(&definition, true, None, false) {
        return invalid_argument(reason);
    }
    let is_sql_command = matches!(definition.id.as_str(), "query" | "execute");
    if definition.id == "query" {
        // 结果限额随执行设置（与 MCP query 工具同一 `ExecutionSettings` 面）。
        state.settings.apply_result_limit(&mut input);
    }
    if is_sql_command {
        // F1/F7：信封目标字段注入输入（改写能力驱动自行内联；其余忽略）。
        inject_sql_target_fields(&mut input, database.as_deref(), schema.as_deref());
    }
    if let Err(reason) = validate_command_input(&definition, &input) {
        return invalid_argument(reason);
    }
    if let Err(reason) = check_command_access(&definition, access_level_for_mode(None)) {
        return invalid_argument(reason);
    }
    if is_sql_command {
        if let Err(reason) = bind_sql_params(&mut input) {
            return invalid_argument(reason);
        }
    }
    // SQL 守卫（只读连接 / safe_mode）在 execute 内部按连接配置求值。
    let step = WorkflowCommandStep::new(
        "driver-gateway-exec",
        command,
        Some(connection_id.clone()),
        input,
    );
    match resource_execution::execute(
        state.host.as_ref(),
        &step,
        &connection_id,
        None,
        None,
        false,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => workflow_error_response(error),
    }
}

// ── 流式执行（POST /stream-executions）────────────────────────────────────

/// `execute_driver_command_stream`（POST，§6.9）：同款闸序跑到驱动闸；闸后
/// 的事件投递桥未装配 → 501（模块头状态表；不假装 202 建流）。
async fn handler_execute_stream(
    State(state): State<DriverGatewayState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let body: ExecuteDriverCommandStreamBody = match serde_json::from_value(body) {
        Ok(body) => body,
        Err(source) => return invalid_argument(format!("request: {source}")),
    };
    let command = body.command.trim().to_owned();
    if command.is_empty() {
        return invalid_argument("missing required input property: command");
    }
    let db_session_id = body
        .db_session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(db_session_id) = db_session_id else {
        // 契约请求体没有 driverType 字段（流式请求是会话绑定的）；缺失即
        // 参数错误。
        return invalid_argument(
            "missing required input property: dbSessionId (stream executions are session-bound)",
        );
    };
    let connection_id = match resolve_session_connection(&state, &identity, db_session_id).await {
        Ok(connection_id) => connection_id,
        Err(response) => return response,
    };
    // 闸序：profile → enabled → 驱动。驱动闸错误（v1 空目录 = 503）先于
    // 501 回答——这是机器真跑出来的闸，不是登记文案。
    if let Err(error) = connection_driver(&state, &connection_id).await {
        return driver_gate_error(error);
    }
    // 闸后绿路径：执行半边在，但 `QueryStreamEvent` → 统一事件端点的投递
    // 桥未装配（桌面 Tauri Channel 行推送 → HTTP SSE 的词汇映射需要产物化
    // 语义裁决，登记随部署构建切片）。显式 501，不假装 202 建流。
    fail_closed(
        "streamed driver executions are not served by this host yet: the event delivery \
         bridge from QueryStreamEvent to the unified event endpoint is not assembled \
         (productized-chunk semantics pending deployment slice); the session-bound \
         non-streaming execution path is live."
            .to_owned(),
    )
}
