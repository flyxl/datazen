//! The ported React suite for the web MCP client layer —
//! `packages/frontend/src/platform/shared/__tests__/mcpClient.test.ts`
//! (tasklist T092). One test per React `it()` block, same names in intent
//! and same fixtures (the fake transport lives in
//! [`super::test_support`]); plus the port's own pins: the transport
//! failure path, the closed-client surface, the camelCase wire spellings,
//! and the [`McpJsonRpcError`](super::mcp_client::McpJsonRpcError)
//! message composition.
//!
//! Kept as a sibling of [`super::mcp_client`] (not an inline `mod tests`)
//! because the pair mirrors React's own file pair — `mcpClient.ts` →
//! `mcp_client.rs`, `mcpClient.test.ts` → this file — which also keeps
//! both under the single-file size discipline.

use serde_json::{Value, json};

use super::mcp_client::{
    McpContentBlock, McpFetch, McpJsonRpcError, McpToolCallResult, WebMcpClient,
    WebMcpClientManager, WebMcpConnectedServer, WebMcpServerConfig, WebMcpToolInfo,
    format_call_tool_result, mcp_qualified_name,
};
use super::test_support::{
    FakeMcpServer, MCP_URL, SESSION, Responder, config_with_id, connected, connected_with,
    example_config, second_config,
};
#[test]
fn initialize_handshake_echoes_the_session_header() {
    // React suite 'performs the initialize handshake and echoes the
    // session header back'.
    let (mut manager, server) = connected();
    let reqs = server.sent_requests();
    assert_eq!(reqs.len(), 3);

    let (init, initialized, list) = (&reqs[0], &reqs[1], &reqs[2]);
    // initialize: POST + both Accept channels + the injected custom
    // headers; no session header yet.
    assert_eq!(init.url, MCP_URL);
    assert_eq!(init.method, "POST");
    assert_eq!(
        init.headers.get("Content-Type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        init.headers.get("Accept").map(String::as_str),
        Some("application/json, text/event-stream")
    );
    assert_eq!(
        init.headers.get("Authorization").map(String::as_str),
        Some("Bearer t")
    );
    assert!(!init.headers.contains_key("Mcp-Session-Id"));
    assert_eq!(
        init.body.as_ref(),
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "datazen-web", "version": "0.1.0" },
            },
        }))
    );
    // notifications/initialized carries no id; tools/list is the
    // first id-bearing follow-up.
    assert_eq!(
        initialized.body.as_ref(),
        Some(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
    );
    assert_eq!(
        list.body.as_ref(),
        Some(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }))
    );
    // The Mcp-Session-Id response header is saved and echoed on every
    // later request.
    assert_eq!(
        initialized.headers.get("Mcp-Session-Id").map(String::as_str),
        Some(SESSION)
    );
    assert_eq!(list.headers.get("Mcp-Session-Id").map(String::as_str), Some(SESSION));

    // serverName comes from initialize's serverInfo.name; tools/list
    // is already cached at connect.
    assert_eq!(
        manager.connected_servers(),
        vec![WebMcpConnectedServer {
            server_id: "example".to_string(),
            server_name: "Example MCP".to_string(),
            tool_count: 2,
        }]
    );
    let tools = match manager.tools(None) {
        Ok(tools) => tools,
        Err(error) => panic!("expected the cached tools, got {}", error.message()),
    };
    assert_eq!(tools.len(), 2);
    assert_eq!(
        tools[0],
        WebMcpToolInfo {
            server_id: "example".to_string(),
            server_name: "Example MCP".to_string(),
            tool_name: "echo".to_string(),
            qualified_name: "mcp/example/echo".to_string(),
            description: Some("Echoes input".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } }
            }),
        }
    );
    // Desktop Option::None parity → null on the wire, None here.
    assert_eq!(tools[1].description, None);
}

#[test]
fn a_stateless_server_never_sends_a_session_header_or_delete() {
    // React suite: initialize answered without the session header.
    let initialize: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::rpc_result(
            &id,
            json!({ "serverInfo": { "name": "S" } }),
            Vec::new(),
        )
    });
    let server =
        FakeMcpServer::with_overrides(vec![("initialize", initialize)]);
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));
    let stateless = WebMcpServerConfig {
        server_id: "stateless".to_string(),
        url: MCP_URL.to_string(),
        headers: None,
    };
    match manager.connect(&stateless) {
        Ok(_) => {}
        Err(error) => panic!("expected the stateless connect, got {}", error.message()),
    }

    for req in server.sent_requests().iter().skip(1) {
        if req.method == "POST" {
            assert!(
                !req.headers.contains_key("Mcp-Session-Id"),
                "no session header may be sent without one being issued"
            );
        }
    }
    manager.disconnect("stateless");
    assert!(!server.sent_requests().iter().any(|req| req.method == "DELETE"));
}

#[test]
fn paginates_tools_list_via_next_cursor() {
    // React suite: nextCursor drives a second tools/list with the
    // cursor param.
    let list_calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let counter = list_calls.clone();
    let tools_list: Responder = Box::new(move |body| {
        counter.set(counter.get() + 1);
        let id = body.get("id").cloned().unwrap_or(json!(0));
        if counter.get() == 1 {
            super::test_support::rpc_result(
                &id,
                json!({ "tools": [ { "name": "a", "inputSchema": {} } ], "nextCursor": "page-2" }),
                Vec::new(),
            )
        } else {
            super::test_support::rpc_result(
                &id,
                json!({ "tools": [ { "name": "b", "inputSchema": {} } ] }),
                Vec::new(),
            )
        }
    });
    let (mut manager, server) = connected_with(vec![("tools/list", tools_list)]);

    let tools = match manager.tools(None) {
        Ok(tools) => tools,
        Err(error) => panic!("expected the paginated tools, got {}", error.message()),
    };
    let names: Vec<&str> = tools.iter().map(|tool| tool.tool_name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"]);
    let second_page = &server.sent_requests()[3];
    assert_eq!(
        second_page.body.as_ref(),
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/list",
            "params": { "cursor": "page-2" }
        }))
    );
}

#[test]
fn rejects_invalid_server_ids_before_any_request() {
    let server = FakeMcpServer::new();
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));

    match manager.connect(&config_with_id("bad/id")) {
        Ok(_) => panic!("expected the invalid-id error"),
        Err(error) => assert_eq!(
            error.message(),
            "Invalid MCP server id 'bad/id': must match ^[a-zA-Z0-9_-]+$"
        ),
    }
    match manager.connect(&config_with_id("")) {
        Ok(_) => panic!("expected the empty-id error"),
        Err(error) => assert!(error.message().starts_with("Invalid MCP server id")),
    }
    assert!(server.sent_requests().is_empty());
    assert!(manager.connected_servers().is_empty());
}

#[test]
fn serves_repeat_calls_from_the_cache_without_new_requests() {
    let (mut manager, server) = connected();
    // initialize + notifications/initialized + tools/list.
    assert_eq!(server.sent_requests().len(), 3);
    match manager.tools(None) {
        Ok(_) => {}
        Err(error) => panic!("expected the cached tools, got {}", error.message()),
    }
    match manager.tools(Some("example")) {
        Ok(_) => {}
        Err(error) => panic!("expected the server-filtered tools, got {}", error.message()),
    }
    assert_eq!(server.sent_requests().len(), 3);
}

#[test]
fn flattens_all_servers_and_filters_by_server_id() {
    let server = FakeMcpServer::new();
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));
    for config in [example_config(), second_config()] {
        match manager.connect(&config) {
            Ok(_) => {}
            Err(error) => panic!("expected the connect, got {}", error.message()),
        }
    }

    let all = match manager.tools(None) {
        Ok(tools) => tools,
        Err(error) => panic!("expected the flattened tools, got {}", error.message()),
    };
    assert_eq!(all.len(), 4);
    assert!(
        all.iter()
            .all(|tool| tool.server_id == "example" || tool.server_id == "second")
    );
    let only = match manager.tools(Some("second")) {
        Ok(tools) => tools,
        Err(error) => panic!("expected the second server's tools, got {}", error.message()),
    };
    assert!(only.iter().all(|tool| tool.server_id == "second"));
    let ids: Vec<String> = manager
        .connected_servers()
        .iter()
        .map(|server| server.server_id.clone())
        .collect();
    assert_eq!(ids, ["example", "second"]);
}

#[test]
fn posts_tools_call_with_the_session_header_and_returns_the_flattened_text() {
    let (mut manager, server) = connected();

    let output = match manager.call_tool("example", "echo", &json!({ "text": "hi" })) {
        Ok(text) => text,
        Err(error) => panic!("expected the echoed text, got {}", error.message()),
    };
    assert_eq!(output, "echo:hi");

    let call = &server.sent_requests()[3];
    assert_eq!(
        call.body.as_ref(),
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "echo", "arguments": { "text": "hi" } },
        }))
    );
    assert_eq!(
        call.headers.get("Mcp-Session-Id").map(String::as_str),
        Some(SESSION)
    );
}

#[test]
fn joins_multiple_text_content_blocks_with_newline() {
    let tools_call: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::rpc_result(
            &id,
            json!({ "content": [ { "type": "text", "text": "a" }, { "type": "text", "text": "b" } ] }),
            Vec::new(),
        )
    });
    let (mut manager, _) = connected_with(vec![("tools/call", tools_call)]);

    match manager.call_tool("example", "echo", &json!({})) {
        Ok(text) => assert_eq!(text, "a\nb"),
        Err(error) => panic!("expected the joined text, got {}", error.message()),
    }
}

#[test]
fn an_is_error_result_returns_the_failure_channel_text() {
    let tools_call: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::rpc_result(
            &id,
            json!({ "content": [ { "type": "text", "text": "boom" } ], "isError": true }),
            Vec::new(),
        )
    });
    let (mut manager, _) = connected_with(vec![("tools/call", tools_call)]);

    match manager.call_tool("example", "echo", &json!({})) {
        Ok(text) => panic!("expected the failure channel, got {text}"),
        Err(error) => assert_eq!(error.message(), "MCP tool error (mcp/example/echo): boom"),
    }
}

#[test]
fn a_json_rpc_error_returns_tool_call_failed_with_the_mcp_error_text() {
    // Desktop map_err parity: the JSON-RPC error text is wrapped in
    // "Tool call failed: …".
    let tools_call: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::rpc_error(&id, -32602, "unknown tool")
    });
    let (mut manager, _) = connected_with(vec![("tools/call", tools_call)]);

    match manager.call_tool("example", "echo", &json!({})) {
        Ok(text) => panic!("expected the RPC error, got {text}"),
        Err(error) => assert_eq!(
            error.message(),
            "Tool call failed: MCP error -32602: unknown tool"
        ),
    }
}

#[test]
fn non_text_only_content_resolves_with_the_non_text_summary() {
    let tools_call: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::rpc_result(
            &id,
            json!({ "content": [ { "type": "image", "data": "..." } ] }),
            Vec::new(),
        )
    });
    let (mut manager, _) = connected_with(vec![("tools/call", tools_call)]);

    match manager.call_tool("example", "shot", &json!({})) {
        Ok(text) => assert_eq!(
            text,
            "MCP tool (mcp/example/shot) returned 1 non-text content block(s); \
             no text output available."
        ),
        Err(error) => panic!("expected the summary text, got {}", error.message()),
    }
}

#[test]
fn non_object_arguments_return_the_desktop_error_text() {
    let (mut manager, _) = connected();

    for arguments in [json!("nope"), json!([1, 2]), Value::Null, json!(5)] {
        match manager.call_tool("example", "echo", &arguments) {
            Ok(text) => panic!("expected the argument error, got {text}"),
            Err(error) => assert_eq!(error.message(), "Tool arguments must be a JSON object"),
        }
    }
}

#[test]
fn parses_an_sse_tools_list_reply_skipping_interleaved_notifications() {
    let tools_list: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::sse_response(&[
            // Interleaved, id-less — ignored by the id match.
            json!({ "jsonrpc": "2.0", "method": "notifications/progress" }),
            json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": [ { "name": "sse-tool", "inputSchema": {} } ] } }),
        ])
    });
    let (mut manager, _) = connected_with(vec![("tools/list", tools_list)]);

    let tools = match manager.tools(None) {
        Ok(tools) => tools,
        Err(error) => panic!("expected the SSE tools, got {}", error.message()),
    };
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_name, "sse-tool");
    assert_eq!(tools[0].qualified_name, "mcp/example/sse-tool");
}

#[test]
fn disconnect_sends_delete_with_the_session_header_and_clears_the_registry() {
    let (mut manager, server) = connected();

    manager.disconnect("example");

    let delete = match server
        .sent_requests()
        .iter()
        .find(|req| req.method == "DELETE")
        .cloned()
    {
        Some(delete) => delete,
        None => panic!("expected the DELETE teardown"),
    };
    assert_eq!(delete.url, MCP_URL);
    assert_eq!(
        delete.headers.get("Mcp-Session-Id").map(String::as_str),
        Some(SESSION)
    );
    assert_eq!(
        delete.headers.get("Authorization").map(String::as_str),
        Some("Bearer t")
    );
    assert!(manager.connected_servers().is_empty());

    match manager.call_tool("example", "echo", &json!({})) {
        Ok(_) => panic!("expected the not-connected error"),
        Err(error) => assert_eq!(error.message(), "MCP server 'example' not connected"),
    }
    match manager.tools(Some("example")) {
        Ok(_) => panic!("expected the not-connected error"),
        Err(error) => assert_eq!(error.message(), "MCP server 'example' not connected"),
    }
}

#[test]
fn disconnecting_an_unknown_server_is_a_noop_with_no_requests() {
    let (mut manager, server) = connected();

    manager.disconnect("missing");
    assert_eq!(server.sent_requests().len(), 3);
}

#[test]
fn disconnect_all_clears_every_server() {
    let (mut manager, _) = connected();
    match manager.connect(&second_config()) {
        Ok(_) => {}
        Err(error) => panic!("expected the second connect, got {}", error.message()),
    }

    manager.disconnect_all();
    assert!(manager.connected_servers().is_empty());
    match manager.call_tool("second", "ping", &json!({})) {
        Ok(text) => panic!("expected the not-connected error, got {text}"),
        Err(error) => assert!(error.message().contains("not connected")),
    }
}

#[test]
fn initialize_http_failure_returns_failed_to_initialize_mcp_client() {
    // React suite: a 500 initialize → the wrap prefix with the HTTP text.
    let initialize: Responder = Box::new(|_| {
        super::test_support::json_response(500, &json!({ "message": "nope" }), Vec::new())
    });
    let server = FakeMcpServer::with_overrides(vec![("initialize", initialize)]);
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));

    match manager.connect(&example_config()) {
        Ok(_) => panic!("expected the initialize failure"),
        Err(error) => assert_eq!(
            error.message(),
            "Failed to initialize MCP client: HTTP 500 status-500"
        ),
    }
    assert!(manager.connected_servers().is_empty());
}

#[test]
fn initialize_json_rpc_error_returns_the_prefix_with_the_mcp_error_text() {
    let initialize: Responder = Box::new(|body| {
        let id = body.get("id").cloned().unwrap_or(json!(0));
        super::test_support::rpc_error(&id, -32600, "bad request")
    });
    let server = FakeMcpServer::with_overrides(vec![("initialize", initialize)]);
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));

    match manager.connect(&example_config()) {
        Ok(_) => panic!("expected the initialize failure"),
        Err(error) => assert_eq!(
            error.message(),
            "Failed to initialize MCP client: MCP error -32600: bad request"
        ),
    }
}

#[test]
fn rejected_initialized_notification_returns_failed_to_initialize_mcp_client() {
    let initialized: Responder = Box::new(|_| {
        super::test_support::json_response(500, &Value::Null, Vec::new())
    });
    let server = FakeMcpServer::with_overrides(vec![
        ("notifications/initialized", initialized),
    ]);
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));

    match manager.connect(&example_config()) {
        Ok(_) => panic!("expected the initialized failure"),
        Err(error) => assert_eq!(
            error.message(),
            "Failed to initialize MCP client: HTTP 500 status-500"
        ),
    }
    assert!(manager.connected_servers().is_empty());
}

#[test]
fn tools_list_failure_returns_failed_to_list_tools_and_cleans_up_the_session() {
    // parity: the session is terminated (DELETE + session header) when
    // tools/list fails, exactly like the desktop cancels its service.
    let tools_list: Responder = Box::new(|_| {
        super::test_support::json_response(500, &Value::Null, Vec::new())
    });
    let server = FakeMcpServer::with_overrides(vec![("tools/list", tools_list)]);
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));

    match manager.connect(&example_config()) {
        Ok(_) => panic!("expected the tools/list failure"),
        Err(error) => assert_eq!(
            error.message(),
            "Failed to list tools: HTTP 500 status-500"
        ),
    }
    let delete = match server
        .sent_requests()
        .iter()
        .find(|req| req.method == "DELETE")
        .cloned()
    {
        Some(delete) => delete,
        None => panic!("expected the session teardown"),
    };
    assert_eq!(
        delete.headers.get("Mcp-Session-Id").map(String::as_str),
        Some(SESSION)
    );
    assert!(manager.connected_servers().is_empty());
}

#[test]
fn reconnecting_the_same_id_replaces_the_old_client_and_terminates_its_session() {
    let (mut manager, server) = connected();

    match manager.connect(&example_config()) {
        // same id again — the returned client is the replacement
        Ok(client) => assert_eq!(client.server_name(), "Example MCP"),
        Err(error) => panic!("expected the reconnect, got {}", error.message()),
    }

    let reqs = server.sent_requests();
    assert_eq!(
        reqs.iter().filter(|req| req.method == "DELETE").count(),
        1,
        "the old session is torn down exactly once"
    );
    let initializes = reqs
        .iter()
        .filter(|req| {
            req.body.as_ref().and_then(|body| body.get("method")) == Some(&json!("initialize"))
        })
        .count();
    assert_eq!(initializes, 2);
    assert_eq!(
        manager.connected_servers(),
        vec![WebMcpConnectedServer {
            server_id: "example".to_string(),
            server_name: "Example MCP".to_string(),
            tool_count: 2,
        }]
    );
}

#[test]
fn mcp_qualified_name_matches_the_shared_routing_rule() {
    // The kernel rule (datazen::ai::application::mcp_tools) is
    // `mcp/{serverId}/{toolName}` — mirrored here, pinned by this row.
    assert_eq!(mcp_qualified_name("my-server_1", "search"), "mcp/my-server_1/search");
    assert_eq!(mcp_qualified_name("example", "echo"), "mcp/example/echo");
}

#[test]
fn format_call_tool_result_mirrors_the_channels() {
    // React suite 'mirrors format_call_tool_result channel by channel'.
    let text_only = McpToolCallResult {
        content: Some(vec![McpContentBlock {
            kind: "text".to_string(),
            text: Some("x".to_string()),
        }]),
        is_error: None,
    };
    assert_eq!(format_call_tool_result(&text_only, "mcp/s/t"), "x");

    let empty_error = McpToolCallResult {
        content: Some(Vec::new()),
        is_error: Some(true),
    };
    assert_eq!(
        format_call_tool_result(&empty_error, "mcp/s/t"),
        "MCP tool error (mcp/s/t): "
    );

    let non_text = McpToolCallResult {
        content: Some(vec![
            McpContentBlock { kind: "image".to_string(), text: None },
            McpContentBlock { kind: "resource".to_string(), text: None },
        ]),
        is_error: None,
    };
    assert_eq!(
        format_call_tool_result(&non_text, "q"),
        "MCP tool (q) returned 2 non-text content block(s); no text output available."
    );
}

#[test]
fn fetch_injection_failure_propagates_through_connect() {
    // The seam's Err is the rejected-promise display message; connect
    // wraps it in the initialize prefix.
    let server = FakeMcpServer::failing("network down");
    let mut manager = WebMcpClientManager::new(Box::new(server.clone()));

    match manager.connect(&example_config()) {
        Ok(_) => panic!("expected the transport failure"),
        Err(error) => assert_eq!(
            error.message(),
            "Failed to initialize MCP client: network down"
        ),
    }
}

#[test]
fn a_closed_client_reports_disconnected_without_new_requests() {
    // The direct-client surface (React exports WebMcpClient itself):
    // after close, every operation answers the disconnected error and
    // the DELETE teardown is the last request on the wire. Close is
    // idempotent.
    let server = FakeMcpServer::new();
    let fetch: &dyn McpFetch = &server;
    let mut client = match WebMcpClient::connect(&example_config(), fetch) {
        Ok(client) => client,
        Err(error) => panic!("expected the connect, got {}", error.message()),
    };
    assert_eq!(client.cached_tools().len(), 2);

    client.close(fetch);
    client.close(fetch);
    assert!(client.cached_tools().is_empty());
    let after_close = server.sent_requests().len();

    match client.tools(fetch) {
        Ok(tools) => panic!("expected the disconnected error, got {tools:?}"),
        Err(error) => assert_eq!(error.message(), "MCP server 'example' is disconnected"),
    }
    match client.call_tool(fetch, "echo", &json!({})) {
        Ok(text) => panic!("expected the disconnected error, got {text}"),
        Err(error) => assert_eq!(
            error.message(),
            "Tool call failed: MCP server 'example' is disconnected"
        ),
    }
    assert_eq!(server.sent_requests().len(), after_close);
}

#[test]
fn web_mcp_wire_shapes_pin_the_camel_case_spelling() {
    // The serde faces mirror the React/desktop wire exactly
    // (desktop `test_mcp_tool_info_serialization` parity).
    let info = WebMcpToolInfo {
        server_id: "s1".to_string(),
        server_name: "Server 1".to_string(),
        tool_name: "tool1".to_string(),
        qualified_name: mcp_qualified_name("s1", "tool1"),
        description: None,
        input_schema: json!({
            "type": "object",
            "properties": { "query": { "type": "string" } }
        }),
    };
    let wire = serde_json::to_string(&info).unwrap_or_default();
    assert_eq!(
        wire,
        r#"{"serverId":"s1","serverName":"Server 1","toolName":"tool1","qualifiedName":"mcp/s1/tool1","description":null,"inputSchema":{"properties":{"query":{"type":"string"}},"type":"object"}}"#
    );

    let config = match serde_json::from_str::<WebMcpServerConfig>(
        r#"{"serverId":"example","url":"https://mcp.example.com/mcp","headers":{"Authorization":"Bearer t"}}"#,
    ) {
        Ok(config) => config,
        Err(error) => panic!("expected the config wire, got {error}"),
    };
    assert_eq!(
        serde_json::to_string(&config).unwrap_or_default(),
        r#"{"serverId":"example","url":"https://mcp.example.com/mcp","headers":{"Authorization":"Bearer t"}}"#
    );
    let bare = WebMcpServerConfig {
        server_id: "stateless".to_string(),
        url: MCP_URL.to_string(),
        headers: None,
    };
    assert_eq!(
        serde_json::to_string(&bare).unwrap_or_default(),
        r#"{"serverId":"stateless","url":"https://mcp.example.com/mcp"}"#
    );

    let server_row = WebMcpConnectedServer {
        server_id: "example".to_string(),
        server_name: "Example MCP".to_string(),
        tool_count: 2,
    };
    assert_eq!(
        serde_json::to_string(&server_row).unwrap_or_default(),
        r#"{"serverId":"example","serverName":"Example MCP","toolCount":2}"#
    );
}

#[test]
fn tool_call_result_and_content_block_round_trip_the_wire() {
    let result = McpToolCallResult {
        content: Some(vec![McpContentBlock {
            kind: "text".to_string(),
            text: Some("x".to_string()),
        }]),
        is_error: Some(false),
    };
    assert_eq!(
        serde_json::to_string(&result).unwrap_or_default(),
        r#"{"content":[{"type":"text","text":"x"}],"isError":false}"#
    );
    // Optional fields round-trip as absent keys, not nulls.
    assert_eq!(
        serde_json::to_string(&McpToolCallResult::default()).unwrap_or_default(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&McpContentBlock {
            kind: "image".to_string(),
            text: None,
        })
        .unwrap_or_default(),
        r#"{"type":"image"}"#
    );
    let parsed = match serde_json::from_str::<McpContentBlock>(r#"{"type":"text","text":"y"}"#) {
        Ok(parsed) => parsed,
        Err(error) => panic!("expected the block wire, got {error}"),
    };
    assert_eq!(parsed.kind, "text");
    assert_eq!(parsed.text.as_deref(), Some("y"));
}

#[test]
fn json_rpc_error_message_formats_with_and_without_a_code() {
    // React's McpJsonRpcError constructor: the code prefixes
    // "MCP error …" only when present.
    let with_code = McpJsonRpcError {
        code: Some(serde_json::Number::from(-32602)),
        message: "unknown tool".to_string(),
    };
    assert_eq!(with_code.message(), "MCP error -32602: unknown tool");
    let without_code = McpJsonRpcError {
        code: None,
        message: "plain".to_string(),
    };
    assert_eq!(without_code.message(), "plain");
}
