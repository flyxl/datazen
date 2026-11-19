//! Web MCP client layer — port of React
//! `packages/frontend/src/platform/shared/mcpClient.ts` (tasklist T092).
//!
//! React's own file header states the boundary; ported in intent:
//! this is the browser counterpart of the desktop `McpClientManager`
//! (`crates/datazen-desktop/src/mcp/client.rs`) — browsers reach external
//! MCP servers directly over streamable HTTP (POST JSON-RPC; the
//! initialize response's `Mcp-Session-Id` header is saved and echoed on
//! every later request; DELETE terminates the session) instead of stdio
//! subprocesses. The handshake is isomorphic: initialize →
//! notifications/initialized → tools/list cached.
//!
//! ## What deliberately stays on the desktop
//!
//! Only the transport and process model differ. The server identity
//! validation and stdio spawn allowlist (`validate_mcp_spawn_command`,
//! env/token checks) stay desktop-side: [`WebMcpServerConfig`] has no
//! command/args/env/token fields by construction — only
//! `serverId`/`url`/`headers?`. The desktop's 30s connect timeout is
//! likewise an assembly concern (React delegates it to `AbortSignal` on
//! the real fetch); the port has no timeout by construction.
//!
//! ## The consumer port (not wired here)
//!
//! Both forms share the kernel consumer port `McpToolBroker`
//! (`crates/datazen/src/ai/application/ports.rs` — `tools_for_ai` /
//! `call_tool`, pre-formatted text on success). The desktop half is the
//! host `McpClientManager`; the web half's broker adapter is the web
//! assembly layer's job (React's header says the same), so this module
//! only provides the client layer and wires no assembly — the kernel
//! crate is a `cfg(not(wasm32))`-only dependency of this crate in any
//! case.
//!
//! ## Async → sync mapping
//!
//! Every `await` in mcpClient.ts becomes `Result` propagation in a
//! synchronous function: the port's methods return the resolved value or
//! the error the React promise chain would deliver, and the async bridge
//! (the web host assembly that owns the real fetch) re-adds the
//! event-loop hop when it wires the adapter. `res.text()` becomes the
//! [`McpHttpResponse::body`] field — the adapter materializes the body
//! text before handing the response over.
//!
//! ## The injected transport
//!
//! All fetch goes through [`McpFetch`] — the constructor-injected seam
//! React calls `McpFetch` (web assembly passes `globalThis.fetch`;
//! tests pass the in-memory double). The seam is passed **per call**
//! rather than stored in the client: Rust cannot store the borrow of the
//! one transport the manager owns, so the manager re-attaches its single
//! seam on every call (same single-transport observable behavior; the
//! per-call-argument precedent is ai_sessions' `ActiveConnections`).
//!
//! ## Types added beyond the React export list
//!
//! - [`McpError`] — React throws; the port discriminates the exception
//!   shapes that survive to callers (see the enum docs for which variant
//!   escapes where). [`Self::message`] is the exact `Error.message`
//!   React's callers read.
//! - [`McpRequestInit`] — the structural subset of `RequestInit` the
//!   client builds (`method`/`headers`/`body`).
//! - [`WebMcpConnectedServer`] — React's anonymous inline
//!   `{serverId, serverName, toolCount}` return type, named for Rust.
//!
//! ## Wire leniency (documented deviations)
//!
//! - A malformed tool entry in `tools/list` (missing/non-string `name`)
//!   fails the request with an `invalid tool entry` message instead of
//!   registering a tool with a garbage name — desktop parity (rmcp's
//!   `Tool` deserialization is equally strict); React's `tool.name`
//!   would silently read `undefined`.
//! - A non-string `description` or non-object `inputSchema` folds exactly
//!   as React does (`null` / `{}`); a non-string `type`/`text` on a
//!   content block counts as a non-text block, again as React does.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Accept header naming both streamable-HTTP reply channels.
const ACCEPT_STREAMABLE_HTTP: &str = "application/json, text/event-stream";
/// Protocol version the web client advertises at initialize.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
/// The session header the server answers with and the client echoes back.
const SESSION_HEADER: &str = "Mcp-Session-Id";
/// `clientInfo.name` the web client identifies itself with.
const WEB_CLIENT_NAME: &str = "datazen-web";
/// `clientInfo.version` the web client identifies itself with.
const WEB_CLIENT_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// Contract types
// ---------------------------------------------------------------------------

/// One external MCP server's HTTP endpoint — no stdio/token fields by
/// construction (React `WebMcpServerConfig`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebMcpServerConfig {
    /// Registry key + AI routing segment (`mcp/{serverId}/{toolName}`).
    pub server_id: String,
    /// Streamable-HTTP endpoint of the MCP server.
    pub url: String,
    /// Extra request headers (auth etc.), sent on every request to this
    /// server.
    ///
    /// `Record<string, string>` → `BTreeMap` (header sets are name-keyed
    /// lookups; iteration order is not observable through HTTP or this
    /// client's seams — the same note the ai_types mirrors carry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
}

/// One tool surfaced by a connected server — mirror of the desktop
/// `McpToolInfo` (serde camelCase), same field names as React's
/// `WebMcpToolInfo`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebMcpToolInfo {
    pub server_id: String,
    pub server_name: String,
    pub tool_name: String,
    /// Routing name the model sees: `mcp/{serverId}/{toolName}` — same
    /// rule as [`mcp_qualified_name`].
    pub qualified_name: String,
    /// `string | null` — required-but-nullable: `None` serializes as
    /// `null` and the key stays present.
    pub description: Option<String>,
    /// `Record<string, unknown>` stays opaque.
    pub input_schema: Value,
}

/// One MCP tool result content block — only `type`/`text` are consumed
/// (the structural subset the formatter reads).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McpContentBlock {
    /// React field name `type`, kept on the wire spelling.
    #[serde(rename = "type")]
    pub kind: String,
    /// `text?: string` — absent or non-string counts as a non-text block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// The MCP `tools/call` result shape the formatter consumes.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<McpContentBlock>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Structural subset of `RequestInit` the client builds — only
/// `method`/`headers`/`body` are ever set (React's `init` argument).
#[derive(Clone, Debug, PartialEq)]
pub struct McpRequestInit {
    /// `"POST"` or `"DELETE"` — the only methods the client issues.
    pub method: String,
    /// Request headers; merged per request (base + config + session),
    /// later values overriding earlier ones as JS object spread does.
    pub headers: BTreeMap<String, String>,
    /// `JSON.stringify(body)`; `None` is a body-less request (DELETE).
    pub body: Option<String>,
}

/// Structural subset of `Response` the client relies on (React
/// `McpHttpResponse`) — keeps tests dependency-free.
///
/// React's `text(): Promise<string>` becomes the [`Self::body`] field: the
/// port is synchronous, so the adapter resolves the body text before
/// handing the response over (the materialization `await res.text()`
/// performs). `headers.get(name)` becomes [`Self::header`] with fetch's
/// case-insensitive lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct McpHttpResponse {
    /// 2xx only — fetch's `Response.ok`.
    pub ok: bool,
    pub status: u16,
    pub status_text: String,
    /// Response headers as raw pairs; lookup is case-insensitive.
    pub headers: Vec<(String, String)>,
    /// The body text `res.text()` would resolve with.
    pub body: String,
}

impl McpHttpResponse {
    /// Case-insensitive header lookup (`headers.get(name)`), first match.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// The injected transport — React's `McpFetch` function type
/// `(url, init) => Promise<McpHttpResponse>`.
///
/// The web assembly passes a wrapper over `globalThis.fetch`; tests pass
/// the in-memory double. `Err` is the *display message* of a rejected
/// promise: React reads it via `e instanceof Error ? e.message :
/// String(e)` at its catch sites, so the adapter performs that extraction
/// at this boundary (the T040 `errorMessage` trade — the port composes
/// the exact user-visible strings React builds).
pub trait McpFetch {
    /// One HTTP exchange: `(url, init) => Promise<McpHttpResponse>` with
    /// the rejected side folded to `Err(display message)`.
    fn fetch(&self, url: &str, init: &McpRequestInit) -> Result<McpHttpResponse, String>;
}

/// JSON-RPC error answered by the server (protocol level) — React's
/// `McpJsonRpcError` class.
///
/// The class's constructor composes the message its callers read, so the
/// composition lives on [`Self::message`]; React code never surfaces the
/// raw server message on its own.
#[derive(Clone, Debug, PartialEq)]
pub struct McpJsonRpcError {
    /// `number | undefined` — the JSON-RPC error code, when the server
    /// sent one (a `null` code folds to absent, as JS `null` does).
    pub code: Option<serde_json::Number>,
    /// The raw server message.
    pub message: String,
}

impl McpJsonRpcError {
    /// React: `code === undefined ? message : \`MCP error ${code}:
    /// ${message}\``.
    pub fn message(&self) -> String {
        match &self.code {
            Some(code) => format!("MCP error {code}: {}", self.message),
            None => self.message.clone(),
        }
    }
}

impl fmt::Display for McpJsonRpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

/// Everything the client surfaces as an error — the Rust shape of the
/// exceptions mcpClient.ts throws, discriminated where React's class
/// distinctions survive to callers:
///
/// - [`McpError::Rpc`] is React's `McpJsonRpcError`. It escapes *raw*
///   only from the cold-cache `tools()` path — every other site wraps it
///   into a plain `Error` message (`"Tool call failed: …"`,
///   `"Failed to initialize MCP client: …"`, `"Failed to list tools: …"`).
/// - [`McpError::Http`] / [`McpError::NoReply`] are the plain `Error`s
///   `readJsonRpcResponse` throws.
/// - [`McpError::Transport`] is a rejected injected fetch.
/// - [`McpError::Message`] is every text React builds verbatim — the
///   invalid-id error, non-object arguments, the disconnected client,
///   the `isError` failure channel, and the wrapped prefixes above.
///
/// [`Self::message`] is the exact `Error.message` React's callers read.
#[derive(Clone, Debug, PartialEq)]
pub enum McpError {
    /// A JSON-RPC error object in a server reply.
    Rpc(McpJsonRpcError),
    /// `HTTP {status} {statusText}` — an HTTP-level failure.
    Http {
        status: u16,
        status_text: String,
    },
    /// `no JSON-RPC reply for '{method}'`.
    NoReply {
        method: String,
    },
    /// The injected transport rejected (network-level failure).
    Transport(String),
    /// A plain `Error` with its message built verbatim.
    Message(String),
}

impl McpError {
    /// The display message React's caller reads off the thrown `Error`.
    pub fn message(&self) -> String {
        match self {
            McpError::Rpc(error) => error.message(),
            McpError::Http { status, status_text } => {
                // React: `HTTP ${status} ${statusText}`.trim() — the trim
                // drops the trailing space an empty statusText leaves.
                format!("HTTP {status} {status_text}").trim().to_string()
            }
            McpError::NoReply { method } => format!("no JSON-RPC reply for '{method}'"),
            McpError::Transport(message) | McpError::Message(message) => message.clone(),
        }
    }
}

impl fmt::Display for McpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for McpError {}

/// One connected server row the manager reports — React's anonymous
/// inline `{serverId, serverName, toolCount}` (desktop
/// `connected_servers` parity, typed and serde'd camelCase for the IPC
/// convention).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebMcpConnectedServer {
    pub server_id: String,
    pub server_name: String,
    pub tool_count: usize,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Routing name for AI — same rule as the desktop's shared-layer helper.
///
/// The rule is owned by the kernel shared layer
/// (`datazen::ai::application::mcp_tools::qualified_tool_name`,
/// `crates/datazen/src/ai/application/mcp_tools.rs`:
/// `format!("mcp/{server_id}/{tool_name}")`); the desktop client carries
/// an alias for it, and React's web twin documents the same-rule
/// contract. The kernel crate is a `cfg(not(wasm32))`-only dependency of
/// this crate while the wasm host compiles this module, so — the T080 /
/// T029 mirror trade — the rule is mirrored here and pinned by test
/// rather than re-exported.
pub fn mcp_qualified_name(server_id: &str, tool_name: &str) -> String {
    format!("mcp/{server_id}/{tool_name}")
}

/// Web twin of the desktop `format_call_tool_result`: text blocks
/// flatten and join with `\n`; `isError` renders the failure channel
/// `MCP tool error ({qualified}): {output}`; text-absent answers the
/// non-text summary. Throwing vs returning is the caller's choice.
pub fn format_call_tool_result(result: &McpToolCallResult, qualified_name: &str) -> String {
    let mut text_parts: Vec<&str> = Vec::new();
    let mut non_text_count = 0usize;
    for block in result.content.as_deref().unwrap_or(&[]) {
        // React: `block?.type === 'text' && typeof block.text === 'string'`.
        if block.kind == "text" {
            match block.text.as_deref() {
                Some(text) => text_parts.push(text),
                None => non_text_count += 1,
            }
        } else {
            non_text_count += 1;
        }
    }
    let output = text_parts.join("\n");
    if result.is_error == Some(true) {
        return format!("MCP tool error ({qualified_name}): {output}");
    }
    if output.is_empty() && non_text_count > 0 {
        return format!(
            "MCP tool ({qualified_name}) returned {non_text_count} non-text content \
             block(s); no text output available."
        );
    }
    output
}

/// `^[a-zA-Z0-9_-]+$`, non-empty — the id pattern both forms share.
/// Hand-rolled (`is_ascii_alphanumeric`) so the module needs no regex
/// dependency on the wasm path.
fn is_valid_server_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// React's module-private `validateServerId`: the exact error text is
/// part of the parity contract (the React suite pins it).
fn validate_server_id(server_id: &str) -> Result<(), McpError> {
    if is_valid_server_id(server_id) {
        Ok(())
    } else {
        Err(McpError::Message(format!(
            "Invalid MCP server id '{server_id}': must match ^[a-zA-Z0-9_-]+$"
        )))
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC wire plumbing (private)
// ---------------------------------------------------------------------------

/// One `tools/list` entry as it arrives on the wire.
///
/// Only `name` is strict (a malformed entry fails the whole list — desktop
/// parity, see module docs); `description`/`inputSchema` keep React's
/// lenient folds at the mapping site.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireTool {
    name: String,
    description: Option<Value>,
    input_schema: Option<Value>,
}

/// JS truthiness for the JSON values the client branches on
/// (`if (message?.error)`): `null`/`false`/`0`/`""` are falsy.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// `JSON.stringify` parity for Values — the compact form; total for a
/// `serde_json::Value` (no non-string keys can exist), so the fallback is
/// unreachable in practice.
fn stringify(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// Extract JSON-RPC messages from an SSE body (`data:` lines), skipping
/// non-JSON payloads.
fn parse_sse_data(body: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    for line in body.split('\n') {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("data:") else {
            continue;
        };
        let payload = rest.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        // Not a JSON-RPC message — ignore and keep scanning.
        if let Ok(message) = serde_json::from_str::<Value>(payload) {
            messages.push(message);
        }
    }
    messages
}

/// `m.id === id` — strict JS equality. The client only ever sends integer
/// ids, so numeric ids compare by value (1 and 1.0 are the same JS
/// number); a string or null id never matches a number.
fn reply_id_matches(candidate: &Value, id: u64) -> bool {
    match candidate.get("id").and_then(Value::as_f64) {
        Some(candidate_id) => candidate_id == id as f64,
        None => false,
    }
}

/// Build [`McpJsonRpcError`] from the error object the way the React
/// constructor receives it: `err.code` (number or undefined) and
/// `typeof err.message === 'string' ? err.message : JSON.stringify(err)`.
fn rpc_error_from(error: &Value) -> McpJsonRpcError {
    McpJsonRpcError {
        code: error.get("code").and_then(Value::as_number).cloned(),
        message: match error.get("message") {
            Some(Value::String(text)) => text.clone(),
            _ => stringify(error),
        },
    }
}

/// Read one JSON-RPC reply out of an HTTP response (React
/// `readJsonRpcResponse`).
///
/// SSE bodies are scanned for the message whose `id` matches; plain
/// bodies are parsed whole. A JSON-RPC error object wins over an HTTP
/// failure (checked first, as React checks it); then an HTTP-level
/// failure wins over any non-RPC body (a plain JSON error page must not
/// be mistaken for a JSON-RPC result); then a missing message is the
/// no-reply error.
fn read_json_rpc_response(
    response: &McpHttpResponse,
    method: &str,
    id: u64,
) -> Result<Value, McpError> {
    let content_type = response.header("content-type").unwrap_or("");
    let message = if content_type.contains("text/event-stream") {
        parse_sse_data(&response.body)
            .into_iter()
            .find(|candidate| reply_id_matches(candidate, id))
    } else {
        serde_json::from_str::<Value>(&response.body).ok()
    };
    if let Some(error) = message.as_ref().and_then(|m| m.get("error")) {
        if is_truthy(error) {
            return Err(McpError::Rpc(rpc_error_from(error)));
        }
    }
    if !response.ok {
        return Err(McpError::Http {
            status: response.status,
            status_text: response.status_text.clone(),
        });
    }
    match message {
        Some(message) => Ok(message),
        None => Err(McpError::NoReply {
            method: method.to_string(),
        }),
    }
}

/// The content block subset the formatter consumes, extracted leniently
/// from the wire exactly as React's pass-through does (non-string `type`
/// or `text` reads as absent).
fn mcp_content_block_from(value: &Value) -> McpContentBlock {
    McpContentBlock {
        kind: value
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default(),
        text: value
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// The initialize handshake's wrap prefix (React's single `catch` around
/// both initialize awaits produces exactly this text).
fn initialize_failed(error: McpError) -> McpError {
    McpError::Message(format!(
        "Failed to initialize MCP client: {}",
        error.message()
    ))
}

// ---------------------------------------------------------------------------
// WebMcpClient
// ---------------------------------------------------------------------------

/// One external MCP server's HTTP connection. Built through
/// [`WebMcpClient::connect`] — the constructor is private so a client
/// cannot exist without its initialize handshake.
///
/// The seam (React's constructor-stored `fetchFn`) is passed per call
/// instead of stored: see the module docs — the manager re-attaches its
/// one seam on every call.
pub struct WebMcpClient {
    config: WebMcpServerConfig,
    /// At most one entry: `Mcp-Session-Id` once the server issues it.
    session_headers: BTreeMap<String, String>,
    tools_cache: Option<Vec<WebMcpToolInfo>>,
    closed: bool,
    /// React's `nextRequestId` — 1-based, incremented per request.
    next_request_id: u64,
    server_name: String,
}

impl WebMcpClient {
    /// Registry key of the server this client talks to.
    pub fn server_id(&self) -> &str {
        &self.config.server_id
    }

    /// From the initialize response's `serverInfo.name`; falls back to
    /// `serverId` (React's getter — web config has no name source).
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Cache-only view (populated at connect) — used for counts, never
    /// fetches. React's getter returns a fresh array (`toolsCache ?? []`);
    /// the port returns a clone for the same value semantics.
    pub fn cached_tools(&self) -> Vec<WebMcpToolInfo> {
        self.tools_cache.clone().unwrap_or_default()
    }

    /// Desktop connect parity: validate id → initialize handshake →
    /// notifications/initialized → tools/list cache. Any handshake
    /// failure leaves nothing registered and (best-effort) terminates the
    /// session.
    pub fn connect(config: &WebMcpServerConfig, fetch: &dyn McpFetch) -> Result<Self, McpError> {
        validate_server_id(&config.server_id)?;
        let mut client = Self {
            config: config.clone(),
            session_headers: BTreeMap::new(),
            tools_cache: None,
            closed: false,
            next_request_id: 1,
            server_name: config.server_id.clone(),
        };

        let init_id = client.next_request_id;
        client.next_request_id += 1;
        let init_body = json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": WEB_CLIENT_NAME, "version": WEB_CLIENT_VERSION },
            },
        });
        let response = client.post(fetch, &init_body).map_err(initialize_failed)?;
        let init_message =
            read_json_rpc_response(&response, "initialize", init_id).map_err(initialize_failed)?;

        // `Mcp-Session-Id` answered → saved and echoed on every later
        // request (an empty value stays unsaved, as JS falsiness does).
        if let Some(session_id) = response.header(SESSION_HEADER) {
            if !session_id.is_empty() {
                client
                    .session_headers
                    .insert(SESSION_HEADER.to_string(), session_id.to_string());
            }
        }
        if let Some(name) = init_message
            .get("result")
            .and_then(|result| result.get("serverInfo"))
            .and_then(|server_info| server_info.get("name"))
            .and_then(Value::as_str)
        {
            if !name.is_empty() {
                client.server_name = name.to_string();
            }
        }

        if let Err(error) = client.notify(fetch, "notifications/initialized") {
            client.close(fetch);
            return Err(initialize_failed(error));
        }
        if let Err(error) = client.fetch_tools(fetch) {
            // parity: desktop cancels the session when tools/list fails
            client.close(fetch);
            return Err(McpError::Message(format!(
                "Failed to list tools: {}",
                error.message()
            )));
        }
        Ok(client)
    }

    /// Cached tool list, or a fresh `tools/list` when the cache is cold.
    pub fn tools(&mut self, fetch: &dyn McpFetch) -> Result<Vec<WebMcpToolInfo>, McpError> {
        if let Some(cached) = &self.tools_cache {
            return Ok(cached.clone());
        }
        self.fetch_tools(fetch)
    }

    /// Desktop `call_tool` parity: non-object arguments are the same
    /// error text; a JSON-RPC error maps to `"Tool call failed: …"`; an
    /// `isError` result returns the `format_call_tool_result`
    /// failure-channel text; success returns the flattened result text
    /// (the `McpToolBroker.call_tool` contract: pre-formatted text).
    pub fn call_tool(
        &mut self,
        fetch: &dyn McpFetch,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<String, McpError> {
        if arguments.as_object().is_none() {
            return Err(McpError::Message(
                "Tool arguments must be a JSON object".to_string(),
            ));
        }
        let qualified = mcp_qualified_name(&self.config.server_id, tool_name);
        let result = self
            .request(
                fetch,
                "tools/call",
                json!({ "name": tool_name, "arguments": arguments }),
            )
            .map_err(|error| {
                McpError::Message(format!("Tool call failed: {}", error.message()))
            })?;
        // React builds `{content: Array.isArray(result['content']) ? … :
        // [], isError: result['isError'] === true}` — both keys present.
        let content = result
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| blocks.iter().map(mcp_content_block_from).collect::<Vec<_>>())
            .unwrap_or_default();
        let is_error = matches!(result.get("isError"), Some(Value::Bool(true)));
        let call_result = McpToolCallResult {
            content: Some(content),
            is_error: Some(is_error),
        };
        let text = format_call_tool_result(&call_result, &qualified);
        if is_error {
            return Err(McpError::Message(text));
        }
        Ok(text)
    }

    /// Terminate the HTTP session with DELETE (session header attached)
    /// and drop the cache. Best-effort — transport failures are
    /// swallowed, mirroring the desktop's `let _ = entry.service
    /// .cancel()`. Idempotent.
    pub fn close(&mut self, fetch: &dyn McpFetch) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.tools_cache = None;
        let session_id = self.session_headers.get(SESSION_HEADER).cloned();
        self.session_headers.clear();
        // A stateless server (no session header) has nothing to terminate.
        let Some(session_id) = session_id else {
            return;
        };
        let mut headers = self.config.headers.clone().unwrap_or_default();
        headers.insert(SESSION_HEADER.to_string(), session_id);
        // Best-effort teardown — never surface.
        let _ = fetch.fetch(
            &self.config.url,
            &McpRequestInit {
                method: "DELETE".to_string(),
                headers,
                body: None,
            },
        );
    }

    /// One POST with the merged headers (base + config + session).
    fn post(&self, fetch: &dyn McpFetch, body: &Value) -> Result<McpHttpResponse, McpError> {
        if self.closed {
            return Err(McpError::Message(format!(
                "MCP server '{}' is disconnected",
                self.config.server_id
            )));
        }
        let mut headers = BTreeMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), ACCEPT_STREAMABLE_HTTP.to_string());
        if let Some(extra) = &self.config.headers {
            for (name, value) in extra {
                headers.insert(name.clone(), value.clone());
            }
        }
        for (name, value) in &self.session_headers {
            headers.insert(name.clone(), value.clone());
        }
        let init = McpRequestInit {
            method: "POST".to_string(),
            headers,
            body: Some(stringify(body)),
        };
        fetch.fetch(&self.config.url, &init).map_err(McpError::Transport)
    }

    /// One JSON-RPC request/response round trip — returns
    /// `message.result ?? {}` (null or absent folds to an empty object,
    /// the same nullish coalescing React applies).
    fn request(
        &mut self,
        fetch: &dyn McpFetch,
        method: &str,
        params: Value,
    ) -> Result<Value, McpError> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        let response = self.post(
            fetch,
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )?;
        let message = read_json_rpc_response(&response, method, id)?;
        Ok(match message.get("result") {
            Some(result) if !result.is_null() => result.clone(),
            _ => Value::Object(Map::new()),
        })
    }

    /// One notification POST — only the HTTP status is checked (the body
    /// carries no reply by definition).
    fn notify(&self, fetch: &dyn McpFetch, method: &str) -> Result<(), McpError> {
        let response = self.post(fetch, &json!({ "jsonrpc": "2.0", "method": method }))?;
        if !response.ok {
            return Err(McpError::Http {
                status: response.status,
                status_text: response.status_text.clone(),
            });
        }
        Ok(())
    }

    /// `tools/list` with `nextCursor` pagination, mapped and cached.
    fn fetch_tools(&mut self, fetch: &dyn McpFetch) -> Result<Vec<WebMcpToolInfo>, McpError> {
        let mut collected: Vec<WireTool> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let result = self.request(fetch, "tools/list", params)?;
            if let Some(tools) = result.get("tools").and_then(Value::as_array) {
                for entry in tools {
                    let wire: WireTool = serde_json::from_value(entry.clone())
                        .map_err(|error| McpError::Message(format!("invalid tool entry: {error}")))?;
                    collected.push(wire);
                }
            }
            // React: `typeof next === 'string' && next !== ''` — else stop.
            cursor = match result.get("nextCursor").and_then(Value::as_str) {
                Some(next) if !next.is_empty() => Some(next.to_string()),
                _ => None,
            };
            if cursor.is_none() {
                break;
            }
        }
        let mapped: Vec<WebMcpToolInfo> = collected
            .into_iter()
            .map(|tool| WebMcpToolInfo {
                server_id: self.config.server_id.clone(),
                server_name: self.server_name.clone(),
                qualified_name: mcp_qualified_name(&self.config.server_id, &tool.name),
                // React: `typeof description === 'string' ? … : null`.
                description: tool
                    .description
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                // React: truthy and `typeof === 'object'` — arrays pass
                // both, so they are kept verbatim; everything else folds
                // to `{}`.
                input_schema: match tool.input_schema {
                    Some(schema) if schema.as_object().is_some() || schema.as_array().is_some() => {
                        schema
                    }
                    _ => Value::Object(Map::new()),
                },
                tool_name: tool.name,
            })
            .collect();
        self.tools_cache = Some(mapped.clone());
        Ok(mapped)
    }
}

// ---------------------------------------------------------------------------
// WebMcpClientManager
// ---------------------------------------------------------------------------

/// Multi-server registry — web twin of the desktop `McpClientManager`.
///
/// Reconnects replace the previous entry (closing its session first),
/// unknown ids on `disconnect` are a noop, and `tools`/`call_tool` carry
/// the same "not connected" error text as the desktop.
///
/// React's `Promise<void>` disconnects never reject — the port's
/// disconnects return `()` (close is best-effort by construction).
pub struct WebMcpClientManager {
    /// React's constructor-stored `fetchFn` — the one transport every
    /// client call re-attaches (see [`WebMcpClient`] docs).
    fetch: Box<dyn McpFetch>,
    /// Insertion-ordered registry — JS `Map` order is observable through
    /// `connected_servers` and the flattened `tools()`, so a `Vec` mirrors
    /// it (linear scan at UI server counts).
    clients: Vec<(String, WebMcpClient)>,
}

impl WebMcpClientManager {
    /// React's `new WebMcpClientManager(fetchFn)`.
    pub fn new(fetch: Box<dyn McpFetch>) -> Self {
        Self {
            fetch,
            clients: Vec::new(),
        }
    }

    /// Register one external MCP server (handshake included). Replaces an
    /// existing same-id entry — the old session is terminated *after*
    /// the new handshake succeeds, so a failed reconnect leaves the old
    /// entry intact (React's order).
    pub fn connect(&mut self, config: &WebMcpServerConfig) -> Result<&mut WebMcpClient, McpError> {
        let client = WebMcpClient::connect(config, &*self.fetch)?;
        let index = match self
            .clients
            .iter()
            .position(|(id, _)| id == &config.server_id)
        {
            Some(index) => {
                // React: close the old entry before replacing it.
                let fetch: &dyn McpFetch = &*self.fetch;
                self.clients[index].1.close(fetch);
                self.clients[index] = (config.server_id.clone(), client);
                index
            }
            None => {
                self.clients.push((config.server_id.clone(), client));
                self.clients.len() - 1
            }
        };
        Ok(&mut self.clients[index].1)
    }

    /// Desktop parity: unknown server is a noop; otherwise DELETE the
    /// session and clear the registry entry.
    pub fn disconnect(&mut self, server_id: &str) {
        if let Some(index) = self.clients.iter().position(|(id, _)| id == server_id) {
            // React deletes the entry, then closes it.
            let (_, mut client) = self.clients.remove(index);
            client.close(&*self.fetch);
        }
    }

    /// Close every session and clear the registry (React's
    /// `Promise.all` — sequential here, order-preserving).
    pub fn disconnect_all(&mut self) {
        for (_, mut client) in self.clients.drain(..) {
            client.close(&*self.fetch);
        }
    }

    /// All servers' tools flattened (`all_tools` parity), or one server's
    /// list. `None` is React's no-argument call.
    pub fn tools(&mut self, server_id: Option<&str>) -> Result<Vec<WebMcpToolInfo>, McpError> {
        match server_id {
            None => {
                let mut all = Vec::new();
                for (_, client) in &mut self.clients {
                    all.extend(client.tools(&*self.fetch)?);
                }
                Ok(all)
            }
            Some(server_id) => {
                let index = self.client_index(server_id)?;
                self.clients[index].1.tools(&*self.fetch)
            }
        }
    }

    /// Pre-formatted result text — the shape `McpToolBroker.call_tool`
    /// returns.
    pub fn call_tool(
        &mut self,
        server_id: &str,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<String, McpError> {
        let index = self.client_index(server_id)?;
        self.clients[index].1.call_tool(&*self.fetch, tool_name, arguments)
    }

    /// Desktop `connected_servers` parity: (id, name, tool count), typed.
    pub fn connected_servers(&self) -> Vec<WebMcpConnectedServer> {
        self.clients
            .iter()
            .map(|(_, client)| WebMcpConnectedServer {
                server_id: client.server_id().to_string(),
                server_name: client.server_name().to_string(),
                tool_count: client.cached_tools().len(),
            })
            .collect()
    }

    /// React's `requireClient` — the registry lookup with the desktop's
    /// "not connected" error text. Index-based so the caller can borrow
    /// the client and the transport simultaneously.
    fn client_index(&self, server_id: &str) -> Result<usize, McpError> {
        match self.clients.iter().position(|(id, _)| id == server_id) {
            Some(index) => Ok(index),
            None => Err(McpError::Message(format!(
                "MCP server '{server_id}' not connected"
            ))),
        }
    }
}
