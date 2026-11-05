//! driverGateway 过渡域路由（契约 §6.9 的四方法 HTTP 薄绑定）。
//!
//! **过渡域**：snake_case 方法名保留——它们是现存 IPC 命令名，不是内核
//! 名；整个域在内核实现驱动命令面后删除（契约 §6.9 原文）。请求体是
//! **不透明透传**的嵌套驱动命令，不携带 `SubmitRequest`，v1 不引入幂等
//! 令牌——收编进内核域时按各域规则补。
//!
//! ## 4 方法 × 状态表（本刀登记；W8-b 三件套全绿）
//!
//! | 方法 | 路由 | 状态 | 说明 |
//! | --- | --- | --- | --- |
//! | `get_driver_commands` | `GET /api/v1/driver-gateway/commands?driverType=` | ✅ 真实现 | registry 直取驱动 → `discovery_definitions`；v1 空目录下恒 404「Driver not found」（桌面同文案）——registry 点亮后自动变绿 |
//! | `get_connection_commands` | `GET /api/v1/driver-gateway/commands?dbSessionId=` | ✅ 真实现 | 会话目录反查（归属核对 CM-05）→ `ResourceExecutionHost` 闸序（profile → driver）→ 同一份目录合成；v1 空目录下驱动闸 503「Driver unavailable」 |
//! | `execute_driver_command` | `POST /api/v1/driver-gateway/executions` | ✅ 真实现 | 会话绑定路径：闸序 + `command_policy` 全闸 + `resource_execution::execute` 全量执行。**driverType 直取（W8-b ②）**：registry 直取驱动 → ephemeral 会话句柄（CSPRNG + 归属盖章 CM-05 + TTL 30 分钟，`execution::ephemeral`）→ 同一份 `command_policy` 编排（`bound=false`，SQL 守卫与桌面 unbound 同口径跳过）→ `driver.execute_command` 直执行 |
//! | `execute_driver_command_stream` | `POST /api/v1/driver-gateway/stream-executions` | ✅ 真实现（W8-b ①） | 同款闸序跑到驱动闸 → 登记 SSE 订阅（`EventStreamRegistry`，归属盖章）→ **202 + `{streamId}`** → tokio spawn 执行（与非流路径同一编排）→ 事件经 `EventSink` 按 SSE 帧语义投递；客户端 `GET /api/v1/events/{streamId}` 消费（WR-D5 统一事件端点）。终态必达：完成帧 `done` / 错误帧 `executionChanged`（state=failed），不留悬挂流 |
//!
//! ## 流投递桥的词汇映射表（W8-b ①登记）
//!
//! SSE 帧 `event:` 行的 payload kind 词汇（`sse.rs::frame_of` 是唯一落地
//! 点，此处登记全貌）：
//!
//! | 事件事实 | SSE `event:` | 载荷形状 |
//! | --- | --- | --- |
//! | 执行开始 | `executionStarted` | 桌面 `QueryStreamEvent::ExecutionStarted`（`type` tag，`executionId`） |
//! | 语句开始（列到达） | `statementStart` | `StatementStart { index, sql, columns }` |
//! | 查询结果行块 | `rows` | `Rows { index, rows }`（500 行/帧，对齐 `QUERY_STREAM_BATCH_SIZE`） |
//! | 语句终态 | `statementEnd` | `StatementEnd { index, rowsAffected, executionTimeMs, truncated }` |
//! | 执行完成（终态） | `done` | `Done { totalTimeMs }` |
//! | 执行失败/超时（终态） | `executionChanged` | 统一事件词汇既有形态：`ExecutionChanged { execution: ExecutionView }`，`state=failed` + `errorCode`（`Timeout`/`SqlError`/`ProtocolError`） |
//!
//! 前五种是**桌面词汇原样**（`datazen_driver_api::QueryStreamEvent` 的
//! `type` tag，wire 逐字不变，经 `ConnectionEvent::QueryStream` 进统一事件
//! 信封）；错误终态用统一事件词汇的既有 `executionChanged`——HTTP 202 形态
//! 下错误无法像桌面那样随命令返回值回传，必须以既有执行状态词汇作终态帧，
//! 不发明新事件词。
//!
//! ## 流投递桥的最小组合裁决（W8-b ①登记）
//!
//! 共享层没有「产出 `QueryStreamEvent` 流」的服务端用例：桌面流式走驱动级
//! `query_stream_with_execution_at` + Tauri Channel 行推送，依赖活会话句柄
//! （runtime epoch 绑定），driverGateway 的会话目录只有归属事实没有句柄。
//! 因此本桥按登记的裁决取**最小组合**：闸后用 `resource_execution::execute`
//! （与非流路径同一全量执行、同一 SQL 守卫）拿最终 `CommandResult`，把查询
//! 形状（`{"results":[…],"totalTimeMs":…}`）以桌面词汇帧化投递。执行本体在
//! tokio spawn 里跑；`resource_execution::execute` 自带步骤超时与取消编排，
//! 终态（完成/错误/超时）三种落点都必发终态帧。流式端点只受理 SQL 命令
//! （`query` / `execute`——行块词汇只对它们成立，桌面流式面同款「仅流式
//! 查询」收窄），其余命令 400 指回非流端点。
//!
//! ## 命令历史（W8-b ③登记）
//!
//! 执行终态落 `audit_events` v6（append-only，`shared::audit::AuditSink`
//! 端口，transfer 域同款落库半边）。语义：
//!
//! * 会话绑定路径的 wire **没有** `recordHistory` 开关（桌面
//!   `ExecuteDriverCommandRequest` 同形状，勘察裁决：不补字段）——执行终态
//!   即落账；流式路径按请求体 `recordHistory` 开关（缺省 `true`，桌面
//!   `unwrap_or(true)` 同缺省）。
//! * 动作词：`driver_command_{command}`（如 `driver_command_query`）——
//!   payload 是 §2.4 白名单投影（四键），命令名只能住在 `action` 开放词表。
//! * 流式请求体的 `applyResultLimit`（桌面 `unwrap_or(true)` 同缺省）控制
//!   结果限额是否注入 `input`（只对 `query` 生效，桌面 streaming 同款门）；
//!   非流请求体没有该字段（桌面同形状），限额恒注入。
//! * 成功 → `AuditOutcome::Succeeded`；执行失败 → `Failed` + 错误码
//!   （桌面 `record_sql_command_outcome` 的成败两录同语义；流式路径 202 已
//!   返回，审计是失败事实的唯一服务端账面）。闸序/编排拒绝（400/403/404/503
//!   前闸）不落账——那是控制面拒绝，不是命令历史。
//! * 落账失败**不阻断**执行应答：best-effort + `tracing::warn!`
//!   （transfer `SubmissionService::record` 同款失败语义）。
//! * 流式路径的 payload 带白名单键 `streamRef`（streamId + 终态帧
//!   sequence），审计行与 SSE 流可互相对齐。
//!
//! ## 闸序（与 WR-F7 目标机器同源，同一 `ResourceExecutionHost` 实例）
//!
//! 会话目录（归属核对先于一切）→ profile 存在 → enabled → 驱动可用 →
//! 命令编排 → 执行。每一闸的失败都是**机器真跑出来的应答**，不是登记
//! 文案：v1 空驱动目录下驱动闸（503「Driver unavailable」）先于任何
//! 策略求值。SSE 订阅登记在**驱动闸之后**（闸不过不建流；表满 429）。
//!
//! ## 档位与守卫（对齐桌面 `run_driver_command` 的编排规则）
//!
//! `permission_mode = None` → `access_level_for_mode(None) = HighRisk`：
//! GUI 的 HTTP 对位就是全量命令面（MCP 面的 SafeWrite 收窄是 MCP 的
//! 装配参数，不适用于本域）。认证链 L2–L6 与 L4 CSRF 由 router merge
//! 循环保证。`validate_invocation` 的 `interactive = false`：声明保存
//! 对话框的命令在 HTTP 形态被拒（与桌面内部复用路径同语义——HTTP 没有
//! 原生对话框可弹）。SQL 守卫：会话绑定路径在 `resource_execution::execute`
//! 内部按连接配置与宿主 safe_mode 求值；driverType 直取没有连接配置
//! （read_only 事实来源不存在），与桌面 unbound 路径同口径不跑守卫。
//!
//! ## WorkflowError → HTTP 映射（本域首个映射，登记为裁决）
//!
//! 调用方可修正的（Validation/Target*/Missing*/UnsupportedCommand/
//! CommandNotInWorkflow/UnsupportedDefinitionVersion/UnsupportedPlan/
//! Template/Step/Ai/Other）→ 400；环境不可用（Driver/ConnectionFailed）
//! → 503；超时（GlobalTimeout）→ 504；执行终态异常（SqlUnconfirmed/
//! SqlResultRead/CapabilityUnsupported）→ 500。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use datazen::execution::application::command_policy::{
    access_level_for_mode, bind_sql_params, discovery_definitions, inject_sql_target_fields,
    resolve_definition, validate_invocation,
};
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::{
    EffectOutcome, EventEnvelope, EventSequence, ExecutionErrorCode, ExecutionId, ExecutionState,
    ExecutionView, ResultCompleteness, RuntimeEpoch, SessionDirectory, StreamId,
};
use datazen::settings::ExecutionSettings;
use datazen::shared::audit::{
    AuditEvent, AuditOutcome, AuditPayload, AuditPrincipalKind, AuditSink, AuditStreamRef,
};
use datazen::shared::clock::Clock;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{ConnectionId, DbSessionId};
use datazen::workflow::application::resource_execution::{self, ResourceExecutionHost};
use datazen::workflow::domain::command::WorkflowCommandStep;
use datazen::workflow::domain::error::WorkflowError;
use datazen_driver_api::{
    check_command_access, validate_command_input, ColumnInfo, CommandResult, ConnectionHandle,
    DatabaseDriver, DriverCommandDefinition, QueryStreamEvent, QUERY_STREAM_BATCH_SIZE,
};

use crate::execution::ephemeral::EphemeralSessions;
use crate::http::response::ApiErrorResponse;
use crate::http::router::{invalid_argument, SessionIdentity};
use crate::http::sse::{SseState, SERVER_RUNTIME_EPOCH};

/// driverGateway 域的装配状态：驱动注册表、目标执行宿主（与 WR-F7 目标
/// 机器同一实例）、会话目录（sessions 域同一实例）、执行设置、统一事件
/// 面（W8-b ①流投递桥的登记/发布，与 `GET /api/v1/events/{streamId}`
/// 同一 registry + sink）、审计端口（W8-b ③命令历史）与 ephemeral
/// 会话注册表（W8-b ②driverType 直取）。
#[derive(Clone)]
pub struct DriverGatewayState {
    pub registry: Arc<DriverRegistry>,
    pub host: Arc<dyn ResourceExecutionHost>,
    pub directory: Arc<dyn SessionDirectory>,
    pub settings: ExecutionSettings,
    /// W8-b ①：统一事件面。`None` = 部署未配置事件流（publicOrigin 缺席
    /// ——`GET /api/v1/events/{streamId}` 都不挂载），流式端点显式 503，
    /// 不假装建流。
    pub sse: Option<SseState>,
    /// W8-b ③：命令历史落账（`audit_events` v6 的 append-only 端口）。
    pub audit: Arc<dyn AuditSink>,
    /// 审计时间戳来源（§2.3：无隐式单例）。
    pub clock: Arc<dyn Clock>,
    /// W8-b ②：driverType 直取的 ephemeral 会话注册表。
    pub ephemeral: Arc<EphemeralSessions>,
    /// unix 秒时钟（ephemeral TTL 用；与 sse 的注册表时钟同形，可注入）。
    pub now_unix: Arc<dyn Fn() -> u64 + Send + Sync>,
}

/// `POST /api/v1/driver-gateway/executions` 的请求体（桌面
/// `ExecuteDriverCommandRequest` 的 wire 形状，serde camelCase 逐字）。
///
/// 勘察裁决（W8-b ③）：桌面该形状**没有** `recordHistory` 字段（只有流式
/// 请求体有）——服务端不补字段，会话绑定路径的命令历史语义见模块头。
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

// ── 命令编排（两条路径共用的形态中立规则）─────────────────────────────────

/// 编排头部（桌面 `run_driver_command` 的形态中立规则，`command_policy`
/// 同一份用例；会话绑定与 driverType 直取共用）。失败一律
/// `WorkflowError::Validation`——wire 400 `invalidArgument`，与旧
/// `invalid_argument` 应答同形同文。
#[allow(clippy::too_many_arguments)]
async fn orchestrate_command(
    state: &DriverGatewayState,
    driver: &dyn DatabaseDriver,
    bound: bool,
    apply_result_limit: bool,
    command: &str,
    input: &mut Value,
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<DriverCommandDefinition, WorkflowError> {
    let envelope_has_target = database.is_some() || schema.is_some();
    let definition =
        resolve_definition(driver, command, envelope_has_target).map_err(WorkflowError::Validation)?;
    // HTTP 形态没有原生对话框：声明保存对话框的命令被拒（interactive=false，
    // 与桌面内部复用路径同语义）；permission_mode=None（GUI 全量档位）。
    // `bound` 按路径传：会话绑定 true / driverType 直取 false（桌面同参）。
    validate_invocation(&definition, bound, None, false).map_err(WorkflowError::Validation)?;
    let is_sql_command = matches!(definition.id.as_str(), "query" | "execute");
    if definition.id == "query" && apply_result_limit {
        // 结果限额随执行设置（与 MCP query 工具同一 `ExecutionSettings` 面）；
        // 流式路径按请求体开关（桌面 streaming 同款门），其余路径恒开。
        state.settings.apply_result_limit(input);
    }
    if is_sql_command {
        // F1/F7：信封目标字段注入输入（改写能力驱动自行内联；其余忽略）。
        inject_sql_target_fields(input, database, schema);
    }
    validate_command_input(&definition, input).map_err(WorkflowError::Validation)?;
    check_command_access(&definition, access_level_for_mode(None))
        .map_err(WorkflowError::Validation)?;
    if is_sql_command {
        bind_sql_params(input).map_err(WorkflowError::Validation)?;
    }
    Ok(definition)
}

// ── 透传执行（POST /executions）──────────────────────────────────────────

/// `execute_driver_command`（POST，§6.9）：嵌套驱动命令请求 →
/// `CommandResult`。会话绑定与 driverType 直取两条路径都真实现
/// （模块头状态表）。
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
            let Some(driver) = state.registry.get_sql_driver_by_name(driver_type).await else {
                return not_found_driver(driver_type);
            };
            // W8-b ②：ephemeral 会话句柄（CSPRNG + 归属盖章 + TTL）。铸造
            // 失败按环境不可用回答（表满 429 / 其余 503）。
            let handle = match mint_ephemeral_handle(&state, &identity) {
                Ok(handle) => handle,
                Err(response) => return response,
            };
            let outcome = run_driver_direct(
                &state,
                driver.as_ref(),
                &identity.ctx,
                handle,
                &command,
                body.input,
                body.database.as_deref(),
                body.schema.as_deref(),
            )
            .await;
            finish_with_history(
                &state,
                &identity.ctx,
                None,
                &command,
                outcome,
                None,
            )
        }
        (None, Some(db_session_id)) => {
            let connection_id =
                match resolve_session_connection(&state, &identity, db_session_id).await {
                    Ok(connection_id) => connection_id,
                    Err(response) => return response,
                };
            let outcome = run_session_bound(
                &state,
                &connection_id,
                command.clone(),
                body.input,
                body.database,
                body.schema,
                true,
            )
            .await;
            finish_with_history(
                &state,
                &identity.ctx,
                Some(&connection_id),
                &command,
                outcome,
                None,
            )
        }
        (Some(_), Some(_)) => {
            invalid_argument("driverType and dbSessionId are mutually exclusive; pass exactly one")
        }
        (None, None) => invalid_argument(
            "missing required input property: pass either driverType or dbSessionId",
        ),
    }
}

/// 铸造绑定请求主体的 ephemeral 句柄（W8-b ②）。表满 429（与 SSE 订阅
/// 表满同款形状 + Retry-After）；注册表不可用 503。
fn mint_ephemeral_handle(
    state: &DriverGatewayState,
    identity: &SessionIdentity,
) -> Result<ConnectionHandle, Response> {
    let now_unix = (state.now_unix)();
    match state.ephemeral.mint(
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
        now_unix,
    ) {
        Ok(handle) => Ok(handle),
        Err(PortError::QuotaExceeded(_)) => {
            let mut response = ApiErrorResponse::new(ApiError::new(
                ApiErrorCode::RateLimited,
                "Too many pending ephemeral driver sessions.",
            ))
            .with_status(StatusCode::TOO_MANY_REQUESTS)
            .into_response();
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("5"),
            );
            Err(response)
        }
        Err(source) => Err(ApiErrorResponse::new(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            format!("ephemeral session registry unavailable: {source}"),
        ))
        .with_status(StatusCode::SERVICE_UNAVAILABLE)
        .into_response()),
    }
}

/// driverType 直取的执行半边（W8-b ②）：同一份 command_policy 编排
/// （`bound=false`）→ `driver.execute_command` 直执行。直取面没有连接
/// 配置，SQL 守卫的 read_only 事实来源不存在——与桌面 unbound 路径同
/// 口径不跑守卫（模块头「档位与守卫」）。
#[allow(clippy::too_many_arguments)]
async fn run_driver_direct(
    state: &DriverGatewayState,
    driver: &dyn DatabaseDriver,
    ctx: &RequestContext,
    handle: ConnectionHandle,
    command: &str,
    mut input: Value,
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<CommandResult, WorkflowError> {
    orchestrate_command(state, driver, false, true, command, &mut input, database, schema).await?;
    // 句柄消费（一次有效 + 归属核对）：执行编排用掉它的唯一一跳。同一请求
    // 内的冒领不可能成立（句柄不离开 handler），这一跳是资源生命周期的
    // 正常终结；核对失败按环境不可用回答（fail-closed）。
    let now_unix = (state.now_unix)();
    if state
        .ephemeral
        .take(&handle, ctx.organization_id.as_str(), ctx.principal_id.as_str(), now_unix)
        .is_none()
    {
        return Err(WorkflowError::Driver(
            "ephemeral session handle is unknown or expired".into(),
        ));
    }
    driver
        .execute_command(&handle, command, input)
        .await
        .map_err(|error| WorkflowError::Driver(error.to_string()))
}

/// 会话绑定执行的完整编排（对齐桌面 `run_driver_command` 的形态中立规则；
/// 闸序 → command_policy 全闸 → `resource_execution::execute` 全量执行）。
/// 非流路径与流投递桥（W8-b ①）共用同一份。
async fn run_session_bound(
    state: &DriverGatewayState,
    connection_id: &str,
    command: String,
    mut input: Value,
    database: Option<String>,
    schema: Option<String>,
    apply_result_limit: bool,
) -> Result<CommandResult, WorkflowError> {
    let driver = connection_driver(state, connection_id).await?;
    orchestrate_command(
        state,
        driver.as_ref(),
        true,
        apply_result_limit,
        &command,
        &mut input,
        database.as_deref(),
        schema.as_deref(),
    )
    .await?;
    // SQL 守卫（只读连接 / safe_mode）在 execute 内部按连接配置求值。
    let step = WorkflowCommandStep::new(
        "driver-gateway-exec",
        command,
        Some(connection_id.to_owned()),
        input,
    );
    resource_execution::execute(
        state.host.as_ref(),
        &step,
        connection_id,
        None,
        None,
        false,
    )
    .await
}

/// 执行终态 → HTTP 应答 + 命令历史落账（W8-b ③；best-effort，落账失败
/// 不阻断应答——模块头「命令历史」）。`stream_ref` 仅流式路径携带。
fn finish_with_history(
    state: &DriverGatewayState,
    ctx: &RequestContext,
    connection_id: Option<&str>,
    command: &str,
    outcome: Result<CommandResult, WorkflowError>,
    stream_ref: Option<AuditStreamRef>,
) -> Response {
    match outcome {
        Ok(result) => {
            record_command_history(
                state,
                ctx,
                connection_id,
                command,
                AuditOutcome::Succeeded,
                None,
                stream_ref,
            );
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(error) => {
            record_command_history(
                state,
                ctx,
                connection_id,
                command,
                AuditOutcome::Failed,
                Some(error_code_of(&error)),
                stream_ref,
            );
            workflow_error_response(error)
        }
    }
}

/// WorkflowError → 审计错误码字面量（开放词表；取映射裁决的状态语义）。
fn error_code_of(error: &WorkflowError) -> String {
    match error {
        WorkflowError::GlobalTimeout(_) => "GlobalTimeout".into(),
        WorkflowError::SqlUnconfirmed { code, .. }
        | WorkflowError::SqlResultRead { code, .. }
        | WorkflowError::SqlExecution { code, .. } => code.clone(),
        WorkflowError::ConnectionFailed { .. } | WorkflowError::Driver(_) => {
            "DriverUnavailable".into()
        }
        _ => "Validation".into(),
    }
}

/// 命令历史落账（W8-b ③；语义全貌见模块头「命令历史」）。
fn record_command_history(
    state: &DriverGatewayState,
    ctx: &RequestContext,
    connection_id: Option<&str>,
    command: &str,
    outcome: AuditOutcome,
    error_code: Option<String>,
    stream_ref: Option<AuditStreamRef>,
) {
    let event = AuditEvent {
        principal_id: Some(ctx.principal_id.clone()),
        client_instance_id: Some(ctx.client_instance_id.clone()),
        delegation_id: ctx.delegation_id.clone(),
        connection_id: connection_id.map(ConnectionId::new),
        error_code,
        request_id: Some(ctx.request_id.clone()),
        payload: AuditPayload {
            stream_ref,
            ..AuditPayload::empty()
        },
        // 动作词开放词表：命令名住在 action（payload 白名单不带自由键）。
        ..AuditEvent::new(
            ctx.organization_id.clone(),
            AuditPrincipalKind::User,
            format!("driver_command_{command}"),
            outcome,
            state.clock.now(),
        )
    };
    if let Err(source) = state.audit.record(event) {
        tracing::warn!(
            error = %source,
            command = %command,
            "driver command history append failed"
        );
    }
}

// ── 流式执行（POST /stream-executions，W8-b ①投递桥）─────────────────────

/// `execute_driver_command_stream`（POST，§6.9）：同款闸序跑到驱动闸；
/// 闸后登记订阅 → **202 + `{streamId}`** → tokio spawn 执行 → 事件帧经
/// `EventSink` 投递（模块头词汇映射表 + 最小组合裁决）。
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
    // 行块词汇只对 SQL 命令成立（桌面流式面同款收窄；模块头裁决）。
    if !matches!(command.as_str(), "query" | "execute") {
        return invalid_argument(
            "stream executions frame SQL results (command 'query' or 'execute'); \
             use the non-streaming executions endpoint for other commands",
        );
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
    // 订阅登记——这是机器真跑出来的闸，不过闸不建流。
    if let Err(error) = connection_driver(&state, &connection_id).await {
        return driver_gate_error(error);
    }
    // 闸后绿路径：登记订阅（归属盖章）→ 拿 streamId → spawn 执行 → 202。
    let Some(sse) = state.sse.clone() else {
        return ApiErrorResponse::new(ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            "event streaming is not configured on this host (publicOrigin unset)",
        ))
        .with_status(StatusCode::SERVICE_UNAVAILABLE)
        .into_response();
    };
    let now_unix = (sse.now_unix)();
    let stream_id = match sse
        .registry
        .create(identity.ctx.organization_id.as_str(), identity.ctx.principal_id.as_str(), now_unix)
    {
        Ok(stream_id) => stream_id,
        Err(PortError::QuotaExceeded(_)) => {
            let mut response = ApiErrorResponse::new(ApiError::new(
                ApiErrorCode::RateLimited,
                "Too many pending event stream subscriptions.",
            ))
            .with_status(StatusCode::TOO_MANY_REQUESTS)
            .into_response();
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("5"),
            );
            return response;
        }
        Err(source) => {
            return ApiErrorResponse::new(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                format!("event subscription registry unavailable: {source}"),
            ))
            .with_status(StatusCode::SERVICE_UNAVAILABLE)
            .into_response();
        }
    };
    let task = StreamExecutionTask {
        sse,
        state,
        ctx: identity.ctx.clone(),
        connection_id,
        command,
        input: body.input,
        database: body.database,
        schema: body.schema,
        stream_id: stream_id.clone(),
        apply_result_limit: body.apply_result_limit.unwrap_or(true),
        record_history: body.record_history.unwrap_or(true),
    };
    tokio::spawn(task.run());
    // 与 SSE 订阅应答同一形状（`{streamId}`）；执行已启动 → 202。
    (StatusCode::ACCEPTED, Json(json!({ "streamId": stream_id.as_str() }))).into_response()
}

/// 流投递桥的后台执行任务（模块头「最小组合裁决」）。
struct StreamExecutionTask {
    /// 统一事件面（registry + sink；与挂载的 GET 事件端点同一实例）。
    sse: SseState,
    state: DriverGatewayState,
    ctx: RequestContext,
    connection_id: String,
    command: String,
    input: Value,
    database: Option<String>,
    schema: Option<String>,
    stream_id: StreamId,
    /// 请求体 `applyResultLimit`（缺省 true，桌面同缺省）：`query` 的结果
    /// 限额注入开关（模块头「命令历史」同一段登记）。
    apply_result_limit: bool,
    record_history: bool,
}

impl StreamExecutionTask {
    async fn run(mut self) {
        // 执行身份：流式面的 opaque id（桌面 `QueryExecutionId::new(uuid)`
        // 同形——一次流一个新身份；错误终态帧与审计都携带它）。
        let execution_id = uuid::Uuid::new_v4().to_string();
        self.publish_query_frame(QueryStreamEvent::ExecutionStarted {
            execution_id: execution_id.clone(),
        })
        .await;
        let outcome = run_session_bound(
            &self.state,
            &self.connection_id,
            self.command.clone(),
            self.input.clone(),
            self.database.clone(),
            self.schema.clone(),
            self.apply_result_limit,
        )
        .await;
        match outcome {
            Ok(result) => {
                let last_sequence = self.publish_result_frames(&result).await;
                if self.record_history {
                    record_command_history(
                        &self.state,
                        &self.ctx,
                        Some(&self.connection_id),
                        &self.command,
                        AuditOutcome::Succeeded,
                        None,
                        Some(AuditStreamRef {
                            stream_id: self.stream_id.as_str().to_owned(),
                            sequence: last_sequence,
                        }),
                    );
                }
            }
            Err(error) => {
                let last_sequence = self.publish_execution_failed(&execution_id, &error).await;
                if self.record_history {
                    record_command_history(
                        &self.state,
                        &self.ctx,
                        Some(&self.connection_id),
                        &self.command,
                        AuditOutcome::Failed,
                        Some(error_code_of(&error)),
                        Some(AuditStreamRef {
                            stream_id: self.stream_id.as_str().to_owned(),
                            sequence: last_sequence,
                        }),
                    );
                }
            }
        }
    }

    /// 发布一帧桌面词汇查询事件。发布失败只 `tracing::warn!`（端口无
    /// 错误通道的消费面——订阅迭代器以缺口/None 终结暴露投递问题），
    /// 执行继续、终态帧照发，不留悬挂流。
    async fn publish_query_frame(&self, event: QueryStreamEvent) -> i64 {
        let envelope = EventEnvelope::new(
            self.stream_id.clone(),
            EventSequence::new(0), // sink 分配流内 sequence（占位一律覆盖）
            RuntimeEpoch::new(SERVER_RUNTIME_EPOCH),
            datazen::execution::ConnectionEvent::QueryStream(event),
        );
        match self.sse.sink.publish(&self.ctx, envelope).await {
            Ok(sequence) => sequence.get() as i64,
            Err(source) => {
                tracing::warn!(
                    error = %source,
                    stream_id = %self.stream_id.as_str(),
                    "stream frame publish failed"
                );
                0
            }
        }
    }

    /// `CommandResult` → 桌面词汇帧序列（模块头映射表）：查询形状逐语句
    /// statementStart → rows（500 行/帧）→ statementEnd；收尾 done 终态帧。
    /// 返回值是**终态帧**（done）的 sequence——审计 `streamRef` 的落点；
    /// 中间帧的 sequence 不进账。
    async fn publish_result_frames(&self, result: &CommandResult) -> i64 {
        let data = &result.data;
        let statements = data
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (index, statement) in statements.iter().enumerate() {
            let columns = statement
                .get("columns")
                .cloned()
                .and_then(|value| serde_json::from_value::<Vec<ColumnInfo>>(value).ok())
                .unwrap_or_default();
            self.publish_query_frame(QueryStreamEvent::StatementStart {
                index,
                sql: statement
                    .get("sql")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                columns,
            })
            .await;
            let rows: Vec<Vec<Option<datazen_driver_api::Value>>> = statement
                .get("rows")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or_default();
            for chunk in rows.chunks(QUERY_STREAM_BATCH_SIZE) {
                self.publish_query_frame(QueryStreamEvent::Rows {
                    index,
                    rows: chunk.to_vec(),
                })
                .await;
            }
            self.publish_query_frame(QueryStreamEvent::StatementEnd {
                index,
                rows_affected: statement.get("rowsAffected").and_then(Value::as_u64),
                execution_time_ms: statement
                    .get("executionTimeMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                truncated: statement
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
            .await;
        }
        self.publish_query_frame(QueryStreamEvent::Done {
            total_time_ms: data
                .get("totalTimeMs")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
        .await
    }

    /// 错误/超时终态帧：统一事件词汇的既有 `executionChanged`
    /// （state=failed + errorCode，模块头映射表）。
    async fn publish_execution_failed(&self, execution_id: &str, error: &WorkflowError) -> i64 {
        let execution = ExecutionView {
            execution_id: ExecutionId::new(execution_id.to_owned()),
            state: ExecutionState::Failed,
            effect_outcome: EffectOutcome::NotStarted,
            provenance: None,
            artifact_ids: Vec::new(),
            result_completeness: ResultCompleteness::Pending,
            truncation_reason: None,
            error_code: Some(execution_error_code(error)),
            runtime_binding: None,
        };
        let envelope = EventEnvelope::new(
            self.stream_id.clone(),
            EventSequence::new(0),
            RuntimeEpoch::new(SERVER_RUNTIME_EPOCH),
            datazen::execution::ConnectionEvent::ExecutionChanged { execution },
        );
        match self.sse.sink.publish(&self.ctx, envelope).await {
            Ok(sequence) => sequence.get() as i64,
            Err(source) => {
                tracing::warn!(
                    error = %source,
                    stream_id = %self.stream_id.as_str(),
                    "stream terminal frame publish failed"
                );
                0
            }
        }
    }
}

/// WorkflowError → `ExecutionErrorCode`（既有词汇；映射裁决见模块头）。
fn execution_error_code(error: &WorkflowError) -> ExecutionErrorCode {
    match error {
        WorkflowError::GlobalTimeout(_) => ExecutionErrorCode::Timeout,
        WorkflowError::SqlExecution { .. } | WorkflowError::SqlUnconfirmed { .. } => {
            ExecutionErrorCode::SqlError
        }
        WorkflowError::SqlResultRead { .. } => ExecutionErrorCode::ProtocolError,
        _ => ExecutionErrorCode::HostRejected,
    }
}
