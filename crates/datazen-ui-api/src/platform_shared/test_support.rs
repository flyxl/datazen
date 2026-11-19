//! Shared test doubles for the platform_shared MCP suite (test-only) —
//! the React harness from
//! `packages/frontend/src/platform/shared/__tests__/mcpClient.test.ts`:
//! the response builders (`rawResponse`/`jsonResponse`/`rpcResult`/
//! `rpcError`/`sseResponse`), the `TOOLS` fixture, and `fakeMcpServer` —
//! an [`McpFetch`] double that records every request and answers by
//! JSON-RPC method, with per-method overrides. A shared `Rc<RefCell>`
//! log lets the double be cloned into the manager's `Box<dyn McpFetch>`
//! while the test side keeps a handle for assertions (`vi.fn` +
//! `mock.calls`).

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value, json};

use super::mcp_client::{
    McpFetch, McpHttpResponse, McpRequestInit, WebMcpClientManager, WebMcpServerConfig,
};

/// The React suite's MCP endpoint.
pub const MCP_URL: &str = "https://mcp.example.com/mcp";
/// The React suite's second-server endpoint.
pub const OTHER_URL: &str = "https://other.example.com/mcp";
/// The session id the happy-path initialize answers with.
pub const SESSION: &str = "sess-42";

/// The React suite's `CONFIG` (`{...CONFIG, serverId: …}` derives via
/// [`config_with_id`]).
pub fn example_config() -> WebMcpServerConfig {
    WebMcpServerConfig {
        server_id: "example".to_string(),
        url: MCP_URL.to_string(),
        headers: Some(BTreeMap::from([(
            "Authorization".to_string(),
            "Bearer t".to_string(),
        )])),
    }
}

/// The React suite's second server (`{serverId: 'second', url: OTHER_URL}`).
pub fn second_config() -> WebMcpServerConfig {
    WebMcpServerConfig {
        server_id: "second".to_string(),
        url: OTHER_URL.to_string(),
        headers: None,
    }
}

/// `{...CONFIG, serverId: id}` — the invalid-id cases and the stateless
/// variant.
pub fn config_with_id(server_id: &str) -> WebMcpServerConfig {
    let mut config = example_config();
    config.server_id = server_id.to_string();
    config
}

/// One recorded request — the React suite's `SentRequest`.
#[derive(Clone, Debug)]
pub struct SentRequest {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    /// The parsed JSON-RPC body; `None` when no body was sent (DELETE).
    pub body: Option<Value>,
}

/// React's `Responder` — answers one parsed JSON-RPC request body.
pub type Responder = Box<dyn Fn(&Value) -> McpHttpResponse>;

/// React's `rawResponse`: status + body text + response headers; `ok` is
/// the 2xx range, `statusText` is the suite's `status-{status}`.
pub fn raw_response(
    status: u16,
    text: impl Into<String>,
    headers: Vec<(&str, &str)>,
) -> McpHttpResponse {
    McpHttpResponse {
        ok: (200..300).contains(&status),
        status,
        status_text: format!("status-{status}"),
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        body: text.into(),
    }
}

/// React's `jsonResponse`.
pub fn json_response(status: u16, body: &Value, headers: Vec<(&str, &str)>) -> McpHttpResponse {
    raw_response(status, serde_json::to_string(body).unwrap_or_default(), headers)
}

/// React's `rpcResult`.
pub fn rpc_result(id: &Value, result: Value, headers: Vec<(&str, &str)>) -> McpHttpResponse {
    json_response(200, &json!({ "jsonrpc": "2.0", "id": id, "result": result }), headers)
}

/// React's `rpcError`.
pub fn rpc_error(id: &Value, code: i64, message: &str) -> McpHttpResponse {
    json_response(
        200,
        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        Vec::new(),
    )
}

/// React's `sseResponse`: `data: {message}` blocks, two newlines apart,
/// answered as `text/event-stream`.
pub fn sse_response(messages: &[Value]) -> McpHttpResponse {
    let mut body = String::new();
    for message in messages {
        body.push_str(&format!(
            "data: {}\n\n",
            serde_json::to_string(message).unwrap_or_default()
        ));
    }
    raw_response(200, body, vec![("content-type", "text/event-stream")])
}

/// The React suite's `TOOLS` fixture (echo with a description, ping with
/// an empty schema).
fn tools_fixture() -> Value {
    json!([
        {
            "name": "echo",
            "description": "Echoes input",
            "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } } },
        },
        { "name": "ping", "inputSchema": {} },
    ])
}

/// The fake MCP server — the React suite's `fakeMcpServer` `vi.fn` double.
///
/// Happy path: initialize (session header attached) →
/// notifications/initialized → tools/list (the fixture, no cursor) →
/// tools/call (echoes `arguments.text`) → DELETE (200). Overrides replace
/// any responder by method name. Every request is recorded, in order.
#[derive(Clone)]
pub struct FakeMcpServer {
    responders: Rc<RefCell<BTreeMap<String, Responder>>>,
    sent: Rc<RefCell<Vec<SentRequest>>>,
    /// Set by [`Self::failing`] — every request rejects (a failed fetch).
    fail_with: Option<String>,
}

impl FakeMcpServer {
    /// Happy-path server: initialize → initialized → tools/list
    /// (+ tools/call, DELETE).
    pub fn new() -> Self {
        Self::with_overrides(Vec::new())
    }

    /// Happy path plus per-method overrides (React's `overrides` map).
    pub fn with_overrides(overrides: Vec<(&str, Responder)>) -> Self {
        let mut responders: BTreeMap<String, Responder> = BTreeMap::new();
        responders.insert(
            "initialize".to_string(),
            Box::new(|body| {
                let id = body.get("id").cloned().unwrap_or(json!(0));
                rpc_result(
                    &id,
                    json!({ "protocolVersion": "2025-06-18", "serverInfo": { "name": "Example MCP" } }),
                    vec![("mcp-session-id", SESSION)],
                )
            }),
        );
        responders.insert(
            "notifications/initialized".to_string(),
            Box::new(|_| json_response(202, &Value::Null, Vec::new())),
        );
        responders.insert(
            "tools/list".to_string(),
            Box::new(|body| {
                let id = body.get("id").cloned().unwrap_or(json!(0));
                rpc_result(&id, json!({ "tools": tools_fixture() }), Vec::new())
            }),
        );
        responders.insert(
            "tools/call".to_string(),
            Box::new(|body| {
                let id = body.get("id").cloned().unwrap_or(json!(0));
                let text = body
                    .pointer("/params/arguments/text")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                rpc_result(
                    &id,
                    json!({
                        "content": [ { "type": "text", "text": format!("echo:{text}") } ],
                        "isError": false,
                    }),
                    Vec::new(),
                )
            }),
        );
        for (method, responder) in overrides {
            responders.insert(method.to_string(), responder);
        }
        Self {
            responders: Rc::new(RefCell::new(responders)),
            sent: Rc::new(RefCell::new(Vec::new())),
            fail_with: None,
        }
    }

    /// A transport that rejects every request — a failed fetch
    /// (the suite's only raw-rejection scenario).
    pub fn failing(message: &str) -> Self {
        let mut server = Self::new();
        server.fail_with = Some(message.to_string());
        server
    }

    /// The recorded requests, in order (React's `sentRequests`).
    pub fn sent_requests(&self) -> Vec<SentRequest> {
        self.sent.borrow().clone()
    }
}

impl Default for FakeMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpFetch for FakeMcpServer {
    fn fetch(&self, url: &str, init: &McpRequestInit) -> Result<McpHttpResponse, String> {
        if let Some(failure) = &self.fail_with {
            return Err(failure.clone());
        }
        if init.method == "DELETE" {
            self.sent.borrow_mut().push(SentRequest {
                url: url.to_string(),
                method: init.method.clone(),
                headers: init.headers.clone(),
                body: None,
            });
            return Ok(json_response(200, &Value::Null, Vec::new()));
        }
        let body = init
            .body
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
        let method = body
            .as_ref()
            .and_then(|body| body.get("method"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.sent.borrow_mut().push(SentRequest {
            url: url.to_string(),
            method: init.method.clone(),
            headers: init.headers.clone(),
            body: body.clone(),
        });
        let responders = self.responders.borrow();
        let response = match method.as_deref().and_then(|name| responders.get(name)) {
            Some(respond) => respond(body.as_ref().map_or(&Value::Null, |body| body)),
            None => json_response(
                500,
                &json!({ "error": format!("unexpected method {}", method.as_deref().unwrap_or("")) }),
                Vec::new(),
            ),
        };
        Ok(response)
    }
}

/// The React suite's `connected()` helper: a manager wired to the fake
/// transport with `CONFIG` already connected.
pub fn connected() -> (WebMcpClientManager, FakeMcpServer) {
    connected_with(Vec::new())
}

/// `connected(overrides)`.
pub fn connected_with(overrides: Vec<(&str, Responder)>) -> (WebMcpClientManager, FakeMcpServer) {
    let server = FakeMcpServer::with_overrides(overrides);
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));
    match manager.connect(&example_config()) {
        Ok(_) => (manager, server),
        Err(error) => panic!("expected the happy-path connect, got {}", error.message()),
    }
}
