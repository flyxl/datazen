//! Host-side tests for the AI IPC surface.
//!
//! 2026-10-31 拆分：纯逻辑钉子（响应解析、工具分类、供应商默认值、
//! DTO 反序列化）已随逻辑归位 modules（`ai/application/response_tests.rs`、
//! `ai/application/tools_tests.rs`、`ai/api.rs`、`ai/application/single_turn.rs`）；
//! 这里只留宿主绑定——命令入口行为、流式事件载荷形状、经 `TestAppState`
//! 的配置工作流，以及 MCP 客户端的宿主装配语义。

use super::*;
use crate::commands::workflow_list_impl;
use datazen::ai::*;

#[tokio::test]
async fn execute_mcp_tool_errors_when_server_not_connected() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let out = execute_mcp_tool(&test.state, "missing", "search", r#"{"q":"x"}"#).await;
    assert!(out.contains("MCP tool error (mcp/missing/search)"));
    assert!(out.contains("not connected"));
}

#[tokio::test]
async fn execute_mcp_tool_rejects_invalid_json() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let out = execute_mcp_tool(&test.state, "srv", "tool", "not-json").await;
    assert!(out.contains("MCP tool error (mcp/srv/tool)"));
    assert!(out.contains("invalid JSON arguments"));
}

#[test]
fn test_emit_stream_chunk_builds_payload_via_callback() {
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Captured {
        events: Vec<(String, serde_json::Value)>,
    }

    let captured = Arc::new(Mutex::new(Captured::default()));
    let cap = captured.clone();
    let cb: StreamCallback = Arc::new(move |request_id, result| match result {
        Ok(chunk) => {
            let mut payload = serde_json::json!({
                "requestId": request_id,
                "content": chunk.content,
                "done": chunk.done,
            });
            if let Some(reasoning) = &chunk.reasoning {
                payload["reasoning"] = serde_json::Value::String(reasoning.clone());
            }
            cap.lock()
                .unwrap()
                .events
                .push(("ai:stream-chunk".into(), payload));
        }
        Err(e) => {
            cap.lock().unwrap().events.push((
                "ai:stream-error".into(),
                serde_json::json!({ "requestId": request_id, "error": e.to_string() }),
            ));
        }
    });

    cb(
        "req-1",
        Ok(StreamChunk {
            content: "hello".into(),
            reasoning: Some("think".into()),
            done: false,
            cancelled: false,
            usage: None,
            tool_calls: None,
            response_id: None,
            egress_summary: None,
        }),
    );
    cb("req-2", Err(AiError::RequestFailed("bad".into())));

    let events = captured.lock().unwrap();
    assert_eq!(events.events.len(), 2);
    assert_eq!(events.events[0].0, "ai:stream-chunk");
    assert_eq!(events.events[0].1["content"], "hello");
    assert_eq!(events.events[0].1["reasoning"], "think");
    assert_eq!(events.events[1].0, "ai:stream-error");
}

#[tokio::test]
async fn ai_providers_config_prompt_workflow() {
    use crate::testing::app_state::TestAppState;
    use datazen::ai::AiProviderConfig;
    use datazen::ai::AiProviderType;

    let test = TestAppState::new().await;
    let (state, _keep) = test.into_state();
    let app = CommandApp::new(state);
    assert!(ai_get_config(app.state()).await.unwrap().is_none());

    let providers = ai_get_providers(app.state()).await.unwrap();
    assert!(!providers.is_empty());

    let prompts = prompt_list(app.state(), None).await.unwrap();
    assert!(!prompts.is_empty());

    let workflows = workflow_list_impl(&app.state()).await.unwrap();
    assert!(
        workflows.iter().any(|w| w.id.starts_with("builtin-")),
        "empty workflow dir should be seeded with builtin workflows, got: {workflows:?}"
    );

    let cfg = AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: Some("test-key".into()),
        model: "gpt-4".into(),
        endpoint: None,
        max_tokens: 4096,
        extra: serde_json::json!({}),
    };
    app.state().store.save_ai_config(&cfg).await.unwrap();
    assert!(ai_get_config(app.state()).await.unwrap().is_some());
    ai_delete_config(app.handle(), app.state()).await.unwrap();
    assert!(ai_get_config(app.state()).await.unwrap().is_none());
}

#[tokio::test]
async fn ai_fetch_remote_models_rejects_unknown_protocol() {
    let err = ai_fetch_remote_models(
        "unknown_proto".into(),
        "https://example.com".into(),
        "key".into(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Unknown protocol"));
}

#[tokio::test]
async fn ai_validate_config_rejects_missing_api_key() {
    use crate::testing::app_state::TestAppState;
    use datazen::ai::AiProviderConfig;
    use datazen::ai::AiProviderType;

    let test = TestAppState::new().await;
    let (state, _keep) = test.into_state();
    let app = CommandApp::new(state);
    let cfg = AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: None,
        model: "gpt-4".into(),
        endpoint: None,
        max_tokens: 4096,
        extra: serde_json::json!({}),
    };
    assert!(ai_validate_config(app.state(), cfg).await.is_err());
}

#[tokio::test]
async fn mcp_tool_definitions_empty_when_no_connections() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    assert!(mcp_tool_definitions(&test.state).await.is_empty());
}

#[tokio::test]
async fn mcp_tool_definitions_respects_enabled_for_ai() {
    use crate::mcp::{McpServerConfig, McpToolInfo};
    use crate::testing::app_state::TestAppState;
    use rmcp::model::{CallToolResult, ContentBlock, Tool};
    use std::collections::HashMap;
    use std::sync::Arc;

    let test = TestAppState::new().await;
    let schema = serde_json::json!({"type": "object"});
    let tool = Tool::new("ping", "Ping", schema.as_object().unwrap().clone());
    test.state
        .mcp_client_manager
        .register_test_server(
            "hidden_srv",
            "Hidden",
            vec![tool],
            Arc::new(|_, _| Ok(CallToolResult::success(vec![ContentBlock::text("pong")]))),
        )
        .await;

    let mut settings = test.state.store.get_settings().await;
    settings.mcp_client_servers = vec![McpServerConfig {
        id: "hidden_srv".into(),
        name: "Hidden".into(),
        transport: "stdio".into(),
        command: None,
        args: vec![],
        env: HashMap::new(),
        enabled: true,
        enabled_for_ai: false,
    }];
    test.state.store.save_settings(settings).await.unwrap();

    assert!(mcp_tool_definitions(&test.state).await.is_empty());

    let tools = test.state.mcp_client_manager.all_tools().await;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_name, "ping");
    let _info: McpToolInfo = tools[0].clone();
}

#[tokio::test]
async fn mcp_connect_rejects_invalid_server_id() {
    use crate::mcp::McpServerConfig;
    use crate::testing::app_state::TestAppState;
    use std::collections::HashMap;

    let test = TestAppState::new().await;
    let config = McpServerConfig {
        id: "bad id".into(),
        name: "Bad".into(),
        transport: "stdio".into(),
        command: Some("/bin/echo".into()),
        args: vec![],
        env: HashMap::new(),
        enabled: true,
        enabled_for_ai: true,
    };
    let err = test
        .state
        .mcp_client_manager
        .connect(&config)
        .await
        .unwrap_err();
    assert!(err.contains("Invalid MCP server id"));
}
