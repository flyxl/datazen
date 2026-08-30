use super::chat::is_db_tool;
use super::*;
use crate::ai::*;
use crate::commands::workflow_list_impl;
use datazen_ai_api::AiProviderType;

#[test]
fn test_provider_list_item_serialization() {
    let item = ProviderListItem {
        provider_type: AiProviderType::OpenAi,
        display_name: "OpenAI".into(),
        supports_streaming: true,
        supports_tools: true,
        default_endpoint: "https://api.openai.com/v1".into(),
        default_protocol: "open_ai_compatible".into(),
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"providerType\":\"open_ai\""));
    assert!(json.contains("\"displayName\":\"OpenAI\""));
    assert!(json.contains("\"supportsStreaming\":true"));
    assert!(json.contains("\"defaultEndpoint\":"));
    assert!(json.contains("\"defaultProtocol\":"));
}

#[test]
fn test_strip_markdown_fences_plain_json() {
    let input = r#"{"explanation":"test","suggestedSql":null,"changes":[]}"#;
    assert_eq!(strip_markdown_fences(input), input);
}

#[test]
fn test_strip_markdown_fences_with_json_fence() {
    let input = "```json\n{\"explanation\":\"test\"}\n```";
    assert_eq!(strip_markdown_fences(input), "{\"explanation\":\"test\"}");
}

#[test]
fn test_strip_markdown_fences_with_bare_fence() {
    let input = "```\n{\"explanation\":\"test\"}\n```";
    assert_eq!(strip_markdown_fences(input), "{\"explanation\":\"test\"}");
}

#[test]
fn test_strip_markdown_fences_whitespace() {
    let input = "  ```json\n  {\"key\":\"val\"}  \n```  ";
    assert_eq!(strip_markdown_fences(input), "{\"key\":\"val\"}");
}

#[test]
fn test_extract_json_boundary_object_with_trailing() {
    let input = r#"{"key":"val"}The user wants me to..."#;
    assert_eq!(extract_json_boundary(input), Some(r#"{"key":"val"}"#));
}

#[test]
fn test_extract_json_boundary_array_with_trailing() {
    let input = r#"[{"a":1}]Some reasoning text"#;
    assert_eq!(extract_json_boundary(input), Some(r#"[{"a":1}]"#));
}

#[test]
fn test_extract_json_boundary_nested() {
    let input = r#"{"a":{"b":"c"},"d":[1,2]}trailing"#;
    assert_eq!(
        extract_json_boundary(input),
        Some(r#"{"a":{"b":"c"},"d":[1,2]}"#)
    );
}

#[test]
fn test_extract_json_boundary_with_escaped_quotes() {
    let input = r#"{"msg":"say \"hello\""}extra"#;
    assert_eq!(
        extract_json_boundary(input),
        Some(r#"{"msg":"say \"hello\""}"#)
    );
}

#[test]
fn test_extract_json_boundary_no_json() {
    assert_eq!(extract_json_boundary("not json at all"), None);
}

#[test]
fn test_extract_json_boundary_clean() {
    let input = r#"{"key":"val"}"#;
    assert_eq!(extract_json_boundary(input), Some(input));
}

#[test]
fn test_parse_ai_json_with_trailing_reasoning() {
    #[derive(serde::Deserialize)]
    struct Simple {
        key: String,
    }
    let raw = r#"{"key":"val"}The user wants me to analyze..."#;
    let result: Result<Simple, _> = parse_ai_json(raw, None, "test");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().key, "val");
}

#[test]
fn test_connection_diagnosis_deserialization() {
    let json = r#"{
            "diagnosis": "Authentication failed",
            "possibleCauses": ["Wrong password", "User does not exist"],
            "solutions": [
                {"description": "Check password", "command": null},
                {"description": "Create user", "command": "CREATE USER test"}
            ],
            "category": "auth"
    }"#;
    let result: ConnectionDiagnosis = serde_json::from_str(json).unwrap();
    assert_eq!(result.diagnosis, "Authentication failed");
    assert_eq!(result.possible_causes.len(), 2);
    assert_eq!(result.solutions.len(), 2);
    assert_eq!(result.category, "auth");
    assert!(result.solutions[0].command.is_none());
    assert_eq!(
        result.solutions[1].command.as_deref(),
        Some("CREATE USER test")
    );
}

#[test]
fn test_query_analysis_deserialization() {
    let json = r#"{
            "summary": "Mostly read queries",
            "categories": [
                {"name": "SELECT", "count": 10, "examples": ["SELECT * FROM users"]}
            ],
            "insights": ["Heavy read workload"],
            "frequentTables": ["users", "orders"],
            "recommendations": ["Add index on orders.user_id"]
    }"#;
    let result: QueryAnalysis = serde_json::from_str(json).unwrap();
    assert_eq!(result.summary, "Mostly read queries");
    assert_eq!(result.categories.len(), 1);
    assert_eq!(result.categories[0].count, 10);
    assert_eq!(result.frequent_tables, vec!["users", "orders"]);
    assert_eq!(result.recommendations.len(), 1);
}

#[test]
fn test_is_db_tool() {
    assert!(is_db_tool("list_connections"));
    assert!(is_db_tool("list_databases"));
    assert!(is_db_tool("list_tables"));
    assert!(is_db_tool("get_table_schema"));
    assert!(!is_db_tool("ask_questions"));
    assert!(!is_db_tool("unknown_tool"));
    assert!(!is_db_tool(""));
}

#[test]
fn test_classify_tool() {
    assert_eq!(classify_tool("ask_questions"), ToolKind::AskQuestions);
    assert_eq!(
        classify_tool("list_connections"),
        ToolKind::Db("list_connections".into())
    );
    assert_eq!(
        classify_tool("mcp/files/read_file"),
        ToolKind::Mcp {
            server_id: "files".into(),
            tool_name: "read_file".into(),
        }
    );
    assert_eq!(
        classify_tool("mcp/my-server/my_tool_name"),
        ToolKind::Mcp {
            server_id: "my-server".into(),
            tool_name: "my_tool_name".into(),
        }
    );
    assert_eq!(classify_tool("mcp/bad"), ToolKind::Unknown);
    assert_eq!(classify_tool("mcp//tool"), ToolKind::Unknown);
    assert_eq!(classify_tool("unknown_tool"), ToolKind::Unknown);
}

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
fn test_db_tool_definitions_count() {
    let tools = db_tool_definitions();
    assert_eq!(tools.len(), 5);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"list_connections"));
    assert!(names.contains(&"list_databases"));
    assert!(names.contains(&"list_tables"));
    assert!(names.contains(&"get_table_schema"));
}

#[test]
fn test_db_tool_definitions_have_valid_schemas() {
    for tool in db_tool_definitions() {
        assert!(!tool.name.is_empty());
        assert!(!tool.description.is_empty());
        assert!(tool.parameters.is_object());
        let obj = tool.parameters.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    }
}

#[test]
fn test_language_hint_known_locales() {
    assert!(language_hint("zh-CN").contains("Chinese (Simplified)"));
    assert!(language_hint("zh-TW").contains("Chinese (Traditional)"));
    assert!(language_hint("en").contains("English"));
    assert!(language_hint("ja").contains("Japanese"));
    assert!(language_hint("ko").contains("Korean"));
    assert!(language_hint("fr").contains("fr"));
}

#[test]
fn test_inject_language_hint_appends_to_system() {
    let mut messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: "Base prompt".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage {
            role: MessageRole::User,
            content: "Hi".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        },
    ];
    inject_language_hint(&mut messages, "zh-CN");
    assert!(messages[0].content.contains("Base prompt"));
    assert!(messages[0].content.contains("Chinese (Simplified)"));
    assert!(!messages[1].content.contains("Chinese"));
}

#[test]
fn test_provider_defaults_all_types() {
    assert_eq!(
        provider_defaults(AiProviderType::OpenAi),
        ("https://api.openai.com/v1", "open_ai_compatible")
    );
    assert_eq!(
        provider_defaults(AiProviderType::Anthropic),
        ("https://api.anthropic.com", "anthropic_compatible")
    );
    assert_eq!(
        provider_defaults(AiProviderType::DeepSeek),
        ("https://api.deepseek.com", "open_ai_responses")
    );
    assert_eq!(
        provider_defaults(AiProviderType::Custom),
        ("", "open_ai_compatible")
    );
}

#[test]
fn test_truncate_str_multibyte() {
    assert_eq!(truncate_str("héllo", 3), "hé");
    assert_eq!(truncate_str("你好世界", 7), "你好");
}

#[test]
fn test_parse_ai_json_empty_response() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Simple {
        key: String,
    }
    let err = parse_ai_json::<Simple>("   ", None, "test").unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_parse_ai_json_truncated_finish_reason() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Simple {
        items: Vec<String>,
    }
    let raw = r#"{"items": ["unclosed"#;
    let err = parse_ai_json::<Simple>(raw, Some("length"), "test").unwrap_err();
    assert!(err.to_string().contains("truncated"));
}

#[test]
fn test_parse_ai_json_invalid_with_reason() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct NeedsField {
        required_field: String,
    }
    let raw = r#"{"wrong": "field"}"#;
    let err = parse_ai_json::<NeedsField>(raw, Some("stop"), "test").unwrap_err();
    assert!(err.to_string().contains("schema"));
}

#[test]
fn test_language_hint_unknown_locale_uses_code() {
    assert!(language_hint("de").contains("de"));
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
            usage: None,
            tool_calls: None,
            response_id: None,
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
async fn ai_providers_config_prompt_workflow_impl() {
    use crate::testing::app_state::TestAppState;
    use datazen_ai_api::AiProviderConfig;
    use datazen_ai_api::AiProviderType;

    let test = TestAppState::new().await;
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_none());

    let providers = ai_get_providers_impl(&test.state).await.unwrap();
    assert!(!providers.is_empty());

    let prompts = prompt_list_impl(&test.state, None).await.unwrap();
    assert!(!prompts.is_empty());

    let workflows = workflow_list_impl(&test.state).await.unwrap();
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
    test.state.store.save_ai_config(&cfg).await.unwrap();
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_some());
    ai_delete_config_impl(&test.state).await.unwrap();
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_none());
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
    use datazen_ai_api::AiProviderConfig;
    use datazen_ai_api::AiProviderType;

    let test = TestAppState::new().await;
    let cfg = AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: None,
        model: "gpt-4".into(),
        endpoint: None,
        max_tokens: 4096,
        extra: serde_json::json!({}),
    };
    assert!(ai_validate_config_impl(&test.state, cfg).await.is_err());
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
