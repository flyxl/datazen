//! MCP streamable HTTP 传输的 wire 行为（handler 级 + router 片段）。
//!
//! 用桩后端/桩认证器驱动 `handle_post` 核心与 `mcp_router` 片段（不接
//! 主线 router）：会话颁发/校验/终止、initialize、tools/list、tools/call
//! 错误码、202 notification、GET 405。modules 侧真实装配
//! （`runtime.rs`，feature 门控）由主线点亮后复用同一套断言。

use datazen_server::mcp::{contract, http, wire};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use http::{CallIdentity, HttpMcpState, McpBackend, SessionAuthenticator};
use wire::ToolCallError;

/// 桩工具分发：记录调用并按名字返回固定结果/错误。
struct StubBackend;

#[async_trait::async_trait]
impl McpBackend for StubBackend {
    async fn call_tool(
        &self,
        _identity: &CallIdentity,
        name: &str,
        _arguments: Value,
    ) -> Result<String, ToolCallError> {
        match name {
            "query" => Ok(r#"{"columns":["1"],"rows":[[1]]}"#.to_owned()),
            "open_session" => Err(ToolCallError::Plain(
                "SessionLost: MCP client disconnected".to_owned(),
            )),
            other => Err(ToolCallError::internal(format!("stub: {other}"))),
        }
    }
}

/// 桩认证器：带 `Authorization: Bearer ok` 头即视为 L2 登录会话。
struct StubAuth;

impl SessionAuthenticator for StubAuth {
    fn authenticate(&self, headers: &axum::http::HeaderMap) -> Option<CallIdentity> {
        if headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer ok") {
            Some(CallIdentity {
                organization_id: "org-1".to_owned(),
                principal_id: "user-1".to_owned(),
                request_id: http::new_request_id(),
            })
        } else {
            None
        }
    }
}

fn state() -> HttpMcpState {
    HttpMcpState::new(
        std::sync::Arc::new(StubBackend),
        std::sync::Arc::new(StubAuth),
    )
}

fn identity() -> Option<CallIdentity> {
    Some(CallIdentity {
        organization_id: "org-1".to_owned(),
        principal_id: "user-1".to_owned(),
        request_id: "req-1".to_owned(),
    })
}

fn rpc(method: &str, id: Value, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "id": id, "params": params })
}

#[tokio::test]
async fn unauthenticated_request_is_rejected_before_any_mcp_processing() {
    let response =
        http::handle_post(&state(), None, None, rpc("initialize", json!(1), json!({}))).await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.body.unwrap()["error"]["code"], json!(-32600));
}

#[tokio::test]
async fn initialize_issues_mcp_session_header_and_datazen_server_info() {
    let response = http::handle_post(
        &state(),
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    let session = response
        .headers
        .iter()
        .find(|(k, _)| k == wire::MCP_SESSION_ID_HEADER)
        .map(|(_, v)| v.clone())
        .expect("initialize must issue Mcp-Session-Id");
    assert!(!session.is_empty());
    let body = response.body.unwrap();
    assert_eq!(body["result"]["serverInfo"]["name"], json!("datazen"));
    assert_eq!(body["result"]["protocolVersion"], json!("2025-03-26"));
    assert_eq!(body["id"], json!(1));
}

#[tokio::test]
async fn requests_without_session_header_are_rejected() {
    let response = http::handle_post(
        &state(),
        None,
        identity(),
        rpc("tools/list", json!(2), json!({})),
    )
    .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.body.unwrap()["error"]["code"], json!(-32600));
}

#[tokio::test]
async fn tools_list_with_valid_session_returns_all_fifteen_tools() {
    let st = state();
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();

    let response = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc("tools/list", json!(2), json!({})),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    let body = response.body.unwrap();
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 15);
    assert_eq!(tools[0]["name"], json!("close_session"));
    assert!(tools[0]["inputSchema"]["type"] == json!("object"));
}

#[tokio::test]
async fn unknown_session_id_is_not_found() {
    let response = http::handle_post(
        &state(),
        Some("bogus-session"),
        identity(),
        rpc("tools/list", json!(2), json!({})),
    )
    .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tools_call_success_wraps_text_content() {
    let st = state();
    // 成功路径需要真实会话：initialize 颁发的 Mcp-Session-Id 是工具分发的
    // 前置（未知会话 → 404 已由 unknown_session_id_is_not_found 单独钉住）。
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();
    let response = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc(
            "tools/call",
            json!(3),
            json!({"name": "query", "arguments": {"connection_id": "c1", "sql": "SELECT 1"}}),
        ),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    let body = response.body.unwrap();
    assert_eq!(body["result"]["content"][0]["type"], json!("text"));
    assert_eq!(
        body["result"]["content"][0]["text"],
        json!(r#"{"columns":["1"],"rows":[[1]]}"#)
    );
    assert_eq!(body["result"]["isError"], json!(false));
}

#[tokio::test]
async fn tools_call_maps_backend_errors_to_jsonrpc_error_codes() {
    let st = state();
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();

    // SessionLost 家族：裸 invalid_params，不带帮助块（桌面 wire 行为）。
    let lost = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc(
            "tools/call",
            json!(3),
            json!({"name": "open_session", "arguments": {}}),
        ),
    )
    .await;
    let error = lost.body.unwrap()["error"].clone();
    assert_eq!(error["code"], json!(-32602));
    assert_eq!(
        error["message"],
        json!("SessionLost: MCP client disconnected")
    );

    // 成功调用 → text content + isError=false。
    let ok = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc(
            "tools/call",
            json!(4),
            json!({"name": "query", "arguments": {"connection_id": "c1"}}),
        ),
    )
    .await;
    let result = ok.body.unwrap()["result"].clone();
    assert_eq!(result["isError"], json!(false));
    assert_eq!(result["content"][0]["type"], json!("text"));
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("\"rows\""));
}

#[tokio::test]
async fn unknown_tool_gets_invalid_params_with_registered_tool_list() {
    let st = state();
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();
    let response = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc(
            "tools/call",
            json!(5),
            json!({"name": "not_a_real_tool", "arguments": {}}),
        ),
    )
    .await;
    let error = response.body.unwrap()["error"].clone();
    assert_eq!(error["code"], json!(-32602));
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("Unknown or disabled tool"));
    assert!(message.contains("list_connections"));
}

#[tokio::test]
async fn notification_returns_202_without_body() {
    let st = state();
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();
    let response = http::handle_post(
        &st,
        Some(&session),
        identity(),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(response.status, StatusCode::ACCEPTED);
    assert!(response.body.is_none());
}

#[tokio::test]
async fn unknown_method_is_method_not_found() {
    let st = state();
    let response = http::handle_post(
        &st,
        None,
        identity(),
        rpc("sampling/createMessage", json!(9), json!({})),
    )
    .await;
    // initialize-less 请求先被会话守卫拦截；带会话后才是 method not found。
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();
    let response = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc("sampling/createMessage", json!(9), json!({})),
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.body.unwrap()["error"]["code"],
        json!(wire::METHOD_NOT_FOUND)
    );
}

#[tokio::test]
async fn delete_terminates_the_session() {
    let st = state();
    let init = http::handle_post(
        &st,
        None,
        identity(),
        rpc("initialize", json!(1), json!({})),
    )
    .await;
    let session = init.headers[0].1.clone();

    let response = http::handle_delete(&st, Some(&session));
    assert_eq!(response.status, StatusCode::ACCEPTED);

    // 终止后同会话请求 404（MCP streamable HTTP 会话语义）。
    let response = http::handle_post(
        &st,
        Some(&session),
        identity(),
        rpc("tools/list", json!(2), json!({})),
    )
    .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_fragment_serves_the_full_streamable_http_flow() {
    let app = http::mcp_router(state());

    // GET /mcp → 405（server→client SSE 流不启用）。
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/mcp")
                .header("authorization", "Bearer ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    // POST initialize → 200 + Mcp-Session-Id。
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .header("authorization", "Bearer ok")
                .body(Body::from(
                    serde_json::to_vec(&rpc("initialize", json!(1), json!({}))).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let session = response
        .headers()
        .get(wire::MCP_SESSION_ID_HEADER)
        .expect("session header over real HTTP")
        .to_str()
        .unwrap()
        .to_owned();
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["result"]["serverInfo"]["name"], json!("datazen"));

    // tools/list（带会话头）→ 15 工具。
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .header("authorization", "Bearer ok")
                .header(wire::MCP_SESSION_ID_HEADER, session)
                .body(Body::from(
                    serde_json::to_vec(&rpc("tools/list", json!(2), json!({}))).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["result"]["tools"].as_array().unwrap().len(), 15);
}
