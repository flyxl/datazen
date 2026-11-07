//! MCP streamable HTTP 传输绑定（axum，server web 形态）。
//!
//! wire 语义（MCP 规范 2025-03-26 streamable HTTP，单 JSON 响应模式）：
//! * `POST /mcp`：body 为单条 JSON-RPC 消息。`initialize` 颁发
//!   `Mcp-Session-Id` 响应头；其后的请求必须携带该会话头，未知会话 404。
//!   notification（无 id）返回 202 Accepted 空 body。
//! * `GET /mcp`：server→client SSE 流在本形态不启用（单 JSON 模式），
//!   返回 405。
//! * `DELETE /mcp`：客户端终止会话，返回 202 并销毁服务端会话状态——
//!   与桌面形态传输断开（`release_sessions`）对齐的会话回收点。
//!
//! 服务端身份 = L2 认证链的登录会话：[`SessionAuthenticator`] 端口由主线
//! 接到 `http::middleware::auth` 的 `AuthenticatedSession`（cookie 会话行），
//! 组织隔离沿 ctx 下沉到每个工具调用（[`CallIdentity`] → modules
//! RequestContext，见 `runtime.rs`）。MCP 层自带会话头只标识 MCP 客户端，
//! **不是**身份（请求体中的 org/principal 永不生效，INV-S1 同款纪律）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

use super::wire;
use crate::mcp::contract;
use rand::RngCore;

/// 一次工具调用携带的调用者身份（来自 L2 登录会话，组织隔离的源头）。
#[derive(Debug, Clone)]
pub struct CallIdentity {
    pub organization_id: String,
    pub principal_id: String,
    /// 服务端生成的 request id（uuid v4；ctx 溯源用，不信任客户端）。
    pub request_id: String,
}

/// L2 登录会话认证端口：主线把 `middleware::auth` 的
/// `AuthenticatedSession` 适配到本端口（handler 不直接依赖中间件类型，
/// 保持接线归主线）。
pub trait SessionAuthenticator: Send + Sync {
    /// 从请求头解析登录会话；未认证返回 None（→ 401）。
    fn authenticate(&self, headers: &HeaderMap) -> Option<CallIdentity>;
}

/// 工具分发端口：modules application 面的 server 侧实现挂在这里
/// （`runtime::ModulesBackend`，feature 门控）。
#[async_trait::async_trait]
pub trait McpBackend: Send + Sync {
    async fn call_tool(
        &self,
        identity: &CallIdentity,
        name: &str,
        arguments: Value,
    ) -> Result<String, wire::ToolCallError>;
}

/// 一个已建立的 MCP HTTP 会话（仅传输层状态；无状态载荷）。
#[derive(Debug, Clone)]
pub struct McpHttpSession {
    pub session_id: String,
}

/// 传输层共享状态。`sessions` 用互斥表承载会话（单进程 v1，与
/// `middleware::auth` 的单进程会话语义一致）。
#[derive(Clone)]
pub struct HttpMcpState {
    pub backend: Arc<dyn McpBackend>,
    pub auth: Arc<dyn SessionAuthenticator>,
    /// `Arc` 包裹使 `HttpMcpState: Clone`（AuthDeps 装配需要）；互斥语义
    /// 不变——克隆共享同一张会话表（单进程 v1）。
    sessions: Arc<Mutex<HashMap<String, McpHttpSession>>>,
}

impl HttpMcpState {
    pub fn new(backend: Arc<dyn McpBackend>, auth: Arc<dyn SessionAuthenticator>) -> Self {
        Self {
            backend,
            auth,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn create_session(&self) -> McpHttpSession {
        let session = McpHttpSession {
            session_id: new_mcp_session_id(),
        };
        self.sessions
            .lock()
            .expect("mcp http session table poisoned")
            .insert(session.session_id.clone(), session.clone());
        session
    }

    fn has_session(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .expect("mcp http session table poisoned")
            .contains_key(session_id)
    }

    fn terminate_session(&self, session_id: &str) {
        self.sessions
            .lock()
            .expect("mcp http session table poisoned")
            .remove(session_id);
    }
}

/// 后端生成的 MCP 会话 id（§4.1：不由客户端提供）。16 字节 CSPRNG hex。
fn new_mcp_session_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 本 server 实例的后端生成 clientInstanceId（§4.1；uuid v4）。
pub fn new_client_instance_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 服务端生成的 request id（ctx 溯源；不信任客户端输入）。
pub fn new_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// handler 层的传输响应：纯数据形状，wire 测试无需 body 收集即可断言；
/// axum 包装（[`McpHttpResponse::into_response`]）只做转换。
pub struct McpHttpResponse {
    pub status: StatusCode,
    /// 附加响应头（如 `Mcp-Session-Id`）；键保留 wire 大小写，转换时经
    /// `HeaderName::from_bytes` 规范化。
    pub headers: Vec<(String, String)>,
    /// JSON 响应体；notification 的 202 为 None。
    pub body: Option<Value>,
}

impl McpHttpResponse {
    fn json(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Some(body),
        }
    }

    fn accepted() -> Self {
        Self {
            status: StatusCode::ACCEPTED,
            headers: Vec::new(),
            body: None,
        }
    }

    fn with_session_header(mut self, session_id: &str) -> Self {
        self.headers.push((
            wire::MCP_SESSION_ID_HEADER.to_owned(),
            session_id.to_owned(),
        ));
        self
    }
}

impl IntoResponse for McpHttpResponse {
    fn into_response(self) -> Response {
        let mut response = match self.body {
            Some(body) => (self.status, Json(body)).into_response(),
            None => self.status.into_response(),
        };
        for (name, value) in self.headers {
            let name =
                HeaderName::from_bytes(name.as_bytes()).expect("mcp response header name is valid");
            let value = value.parse().expect("mcp response header value is valid");
            response.headers_mut().insert(name, value);
        }
        response
    }
}

/// POST /mcp 的核心处理（handler 级函数，不接 router）。
///
/// `session_header`：请求携带的 `Mcp-Session-Id`（可空）。`identity`：
/// L2 登录会话解析产物（None → 401）。JSON-RPC id 原样回显（notification
/// 无 id → 202）。
pub async fn handle_post(
    state: &HttpMcpState,
    session_header: Option<&str>,
    identity: Option<CallIdentity>,
    message: Value,
) -> McpHttpResponse {
    let Some(identity) = identity else {
        return McpHttpResponse::json(
            StatusCode::UNAUTHORIZED,
            json_rpc_error(
                Value::Null,
                wire::INVALID_REQUEST,
                "authentication required",
            ),
        );
    };

    let Some(request) = message.as_object() else {
        return McpHttpResponse::json(
            StatusCode::BAD_REQUEST,
            json_rpc_error(
                Value::Null,
                wire::PARSE_ERROR,
                "request body must be a JSON object",
            ),
        );
    };
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let is_notification = request.get("id").is_none();

    // initialize 是唯一无需既有会话的请求；其余请求必须携带有效会话头。
    if method != "initialize" {
        let Some(session_id) = session_header else {
            return McpHttpResponse::json(
                StatusCode::BAD_REQUEST,
                json_rpc_error(id, wire::INVALID_REQUEST, "missing Mcp-Session-Id header"),
            );
        };
        if !state.has_session(session_id) {
            return McpHttpResponse::json(
                StatusCode::NOT_FOUND,
                json_rpc_error(
                    id,
                    wire::INVALID_REQUEST,
                    "unknown or terminated MCP session",
                ),
            );
        }
    }

    match method {
        "initialize" => {
            let session = state.create_session();
            McpHttpResponse::json(
                StatusCode::OK,
                json!({ "jsonrpc": "2.0", "id": id, "result": wire::initialize_result() }),
            )
            .with_session_header(&session.session_id)
        }
        // 客户端确认 initialize 的 notification：无响应体（202）。
        "notifications/initialized" if is_notification => McpHttpResponse::accepted(),
        "ping" => McpHttpResponse::json(
            StatusCode::OK,
            json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
        ),
        "tools/list" => McpHttpResponse::json(
            StatusCode::OK,
            json!({ "jsonrpc": "2.0", "id": id, "result": wire::list_tools_result() }),
        ),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            if contract::lookup(name).is_none() {
                return McpHttpResponse::json(
                    StatusCode::OK,
                    json!({ "jsonrpc": "2.0", "id": id, "error": wire::unknown_tool_error(name) }),
                );
            }
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match state.backend.call_tool(&identity, name, arguments).await {
                Ok(text) => McpHttpResponse::json(
                    StatusCode::OK,
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": text }],
                            "isError": false
                        }
                    }),
                ),
                Err(error) => McpHttpResponse::json(
                    StatusCode::OK,
                    json!({ "jsonrpc": "2.0", "id": id, "error": wire::tool_call_error(error) }),
                ),
            }
        }
        "resources/list" => McpHttpResponse::json(
            StatusCode::OK,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "resources": [
                    { "uri": "datazen://connections", "name": "Connections" },
                    { "uri": "datazen://query-history", "name": "Query History" },
                    { "uri": "datazen://workflows", "name": "Workflows" }
                ], "nextCursor": null }
            }),
        ),
        "resources/templates/list" => McpHttpResponse::json(
            StatusCode::OK,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "resourceTemplates": [
                    { "uriTemplate": "datazen://schema/{connectionId}/{database}", "name": "Database Schema" }
                ], "nextCursor": null }
            }),
        ),
        "" | "notifications/initialized" => McpHttpResponse::json(
            StatusCode::BAD_REQUEST,
            json_rpc_error(id, wire::INVALID_REQUEST, "missing method"),
        ),
        other if other.starts_with("notifications/") => McpHttpResponse::accepted(),
        _ => McpHttpResponse::json(
            StatusCode::OK,
            json_rpc_error(
                id,
                wire::METHOD_NOT_FOUND,
                &format!("method not found: {method}"),
            ),
        ),
    }
}

/// DELETE /mcp：客户端终止会话（回收传输层状态；modules 侧映射的清理走
/// `runtime` 的 release 路径，接线见报告）。
pub fn handle_delete(state: &HttpMcpState, session_header: Option<&str>) -> McpHttpResponse {
    match session_header {
        Some(session_id) if state.has_session(session_id) => {
            state.terminate_session(session_id);
            McpHttpResponse::accepted()
        }
        Some(_) => McpHttpResponse {
            status: StatusCode::NOT_FOUND,
            headers: Vec::new(),
            body: None,
        },
        None => McpHttpResponse::json(
            StatusCode::BAD_REQUEST,
            wire::wire_error(wire::INVALID_REQUEST, "missing Mcp-Session-Id header"),
        ),
    }
}

fn json_rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": wire::wire_error(code, message) })
}

/// MCP 传输的 Router 片段：`POST/GET/DELETE /mcp`。主线把它 merge 进
/// `http::router`（建议挂 `/mcp`，认证链之内）。
pub fn mcp_router(state: HttpMcpState) -> Router {
    Router::new()
        .route("/mcp", post(post_mcp).get(get_mcp).delete(delete_mcp))
        .with_state(Arc::new(state))
}

async fn post_mcp(
    State(state): State<Arc<HttpMcpState>>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let session_header = headers
        .get(wire::MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());
    let identity = state.auth.authenticate(&headers);
    handle_post(&state, session_header, identity, message)
        .await
        .into_response()
}

async fn get_mcp() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        "SSE server-to-client stream is not enabled in the server form (single JSON responses)",
    )
        .into_response()
}

async fn delete_mcp(State(state): State<Arc<HttpMcpState>>, headers: HeaderMap) -> Response {
    let session_header = headers
        .get(wire::MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());
    handle_delete(&state, session_header).into_response()
}
