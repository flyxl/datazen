//! Host-side tests for the AI IPC surface.
//!
//! 2026-10-31 拆分：纯逻辑钉子（响应解析、工具分类、供应商默认值、
//! DTO 反序列化）已随逻辑归位 modules（`ai/application/response_tests.rs`、
//! `ai/application/tools_tests.rs`、`ai/api.rs`、`ai/application/single_turn.rs`）；
//! 这里只留宿主绑定——命令入口行为、流式事件载荷形状、经 `TestAppState`
//! 的配置工作流，以及 MCP 客户端的宿主装配语义。

use super::*;
use crate::commands::workflow_list;
use datazen::ai::*;

#[tokio::test]
async fn execute_mcp_tool_errors_when_server_not_connected() {
    use crate::testing::app_state_test_support::TestAppState;

    let test = TestAppState::new().await;
    let out = execute_mcp_tool(&test.state, "missing", "search", r#"{"q":"x"}"#).await;
    assert!(out.contains("MCP tool error (mcp/missing/search)"));
    assert!(out.contains("not connected"));
}

#[tokio::test]
async fn execute_mcp_tool_rejects_invalid_json() {
    use crate::testing::app_state_test_support::TestAppState;

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
            memory_used: None,
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
async fn local_cli_presets_are_listed_and_validation_detects_the_executable() {
    use crate::testing::app_state_test_support::TestAppState;
    use datazen::ai::AiProviderConfig;
    use datazen::ai::AiProviderType;

    let test = TestAppState::new().await;
    let (state, _keep) = test.into_state();
    let app = CommandApp::new(state);
    let providers = ai_get_providers(app.state()).await.unwrap();
    for (provider_type, display_name) in [
        (AiProviderType::ClaudeCode, "Claude Code"),
        (AiProviderType::Codex, "Codex"),
        (AiProviderType::Cursor, "Cursor"),
        (AiProviderType::OpenCode, "OpenCode"),
        (AiProviderType::Grok, "Grok"),
        (AiProviderType::Qoder, "Qoder"),
        (AiProviderType::CodeBuddy, "CodeBuddy"),
        (AiProviderType::Pi, "Pi"),
    ] {
        let item = providers
            .iter()
            .find(|item| item.provider_type == provider_type)
            .unwrap_or_else(|| panic!("{display_name} missing from provider list"));
        assert_eq!(item.display_name, display_name);
        assert_eq!(item.default_protocol, "local_cli");
        assert!(!item.supports_tools);
    }

    let missing = AiProviderConfig {
        provider_type: AiProviderType::ClaudeCode,
        api_key: None,
        endpoint: None,
        model: String::new(),
        max_tokens: 1,
        extra: serde_json::json!({ "executablePath": "definitely-not-a-real-cli" }),
    };
    let error = ai_validate_config(app.state(), missing).await.unwrap_err();
    assert!(
        error.to_string().contains("not found") || error.to_string().contains("not executable"),
        "{error}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake-claude");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        let config = AiProviderConfig {
            provider_type: AiProviderType::ClaudeCode,
            api_key: None,
            endpoint: None,
            model: String::new(),
            max_tokens: 1,
            extra: serde_json::json!({ "executablePath": path }),
        };
        ai_validate_config(app.state(), config).await.unwrap();
    }
}

#[tokio::test]
async fn ai_providers_config_prompt_workflow() {
    use crate::testing::app_state_test_support::TestAppState;
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

    // B3：workflow_list_impl 转发已回收，测试走真实命令入口。
    let workflows = workflow_list(app.state()).await.unwrap();
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
    use crate::testing::app_state_test_support::TestAppState;
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
    use crate::testing::app_state_test_support::TestAppState;

    let test = TestAppState::new().await;
    assert!(mcp_tool_definitions(&test.state).await.is_empty());
}

#[tokio::test]
async fn mcp_tool_definitions_respects_enabled_for_ai() {
    use crate::mcp::{McpServerConfig, McpToolInfo};
    use crate::testing::app_state_test_support::TestAppState;
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
    use crate::testing::app_state_test_support::TestAppState;
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

// ── desktop AI tool confirmation port ──

#[tokio::test]
async fn desktop_tool_confirmation_accepts_and_rejects_through_the_injected_port() {
    use super::chat::confirm_tool_call_with;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let called = Arc::new(AtomicBool::new(false));
    let called_for_accept = called.clone();

    let accepted = confirm_tool_call_with(
        Some(handle.clone()),
        "query_readonly",
        r#"{"sql":"SELECT 1"}"#,
        Duration::from_secs(1),
        move |_app, title, message| {
            called_for_accept.store(true, Ordering::SeqCst);
            assert!(title.contains("query_readonly"));
            assert!(message.contains("SELECT 1"));
            Box::pin(async { Ok::<bool, crate::commands::error::CommandError>(true) })
        },
    )
    .await;
    assert!(accepted);
    assert!(called.load(Ordering::SeqCst));

    let rejected = confirm_tool_call_with(
        Some(handle),
        "query_readonly",
        "{}",
        Duration::from_secs(1),
        |_app, _title, _message| {
            Box::pin(async { Ok::<bool, crate::commands::error::CommandError>(false) })
        },
    )
    .await;
    assert!(!rejected);
}

#[tokio::test]
async fn desktop_tool_confirmation_timeout_and_missing_handle_are_denied() {
    use super::chat::confirm_tool_call_with;
    use std::time::Duration;

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let timed_out = confirm_tool_call_with(
        Some(handle),
        "query_readonly",
        "{}",
        Duration::from_millis(5),
        |_app, _title, _message| {
            Box::pin(async {
                std::future::pending::<Result<bool, crate::commands::error::CommandError>>().await
            })
        },
    )
    .await;
    assert!(!timed_out);

    let no_handle = confirm_tool_call_with(
        None::<tauri::AppHandle<tauri::test::MockRuntime>>,
        "query_readonly",
        "{}",
        Duration::from_millis(5),
        |_app, _title, _message| {
            Box::pin(async { Ok::<bool, crate::commands::error::CommandError>(true) })
        },
    )
    .await;
    assert!(!no_handle, "headless/test state must fail closed");
}

#[test]
fn tool_confirmation_prompt_redacts_credentials_and_bounds_arguments() {
    use super::chat::tool_confirmation_prompt;

    let (title, message) = tool_confirmation_prompt(
        "query_readonly",
        &format!(
            r#"{{"password":"super-secret","sql":"SELECT 1","blob":"{}"}}"#,
            "x".repeat(2_000)
        ),
    );

    assert!(title.contains("query_readonly"));
    assert!(!message.contains("super-secret"), "{message}");
    assert!(message.len() < 1_200, "dialog text must stay bounded");
}

#[tokio::test]
async fn ai_list_skills_projects_the_enabled_catalog_only() {
    use crate::testing::app_state_test_support::TestAppState;

    let test = TestAppState::new().await;
    let (state, _keep) = test.into_state();
    let app = CommandApp::new(state);

    // Disabled by default: no directory is scanned and no name leaks.
    assert!(ai_list_skills(app.state()).await.unwrap().is_empty());

    let root =
        std::env::temp_dir().join(format!("datazen-skills-command-{}", uuid::Uuid::new_v4()));
    let skill_dir = root.join("review-sql-t17");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: review-sql-t17\ndescription: Review SQL\n---\nBODY_SHOULD_NOT_LEAK\n",
    )
    .unwrap();

    let mut config = datazen::ai::api::get_settings_config(app.state().store.as_ref()).await;
    config.skills.enabled = true;
    config.skills.custom_dir = Some(root.to_string_lossy().into_owned());
    datazen::ai::api::save_settings_config(
        &app.state().ai_registry,
        app.state().store.as_ref(),
        config,
    )
    .await
    .unwrap();

    let listed = ai_list_skills(app.state()).await.unwrap();
    let review = listed
        .iter()
        .find(|skill| skill.name == "review-sql-t17")
        .expect("custom skill is listed");
    assert_eq!(review.description, "Review SQL");
    assert!(
        listed
            .iter()
            .all(|skill| !skill.description.contains("BODY")),
        "listing never carries bodies"
    );

    let mut config = datazen::ai::api::get_settings_config(app.state().store.as_ref()).await;
    config.skills.enabled = false;
    datazen::ai::api::save_settings_config(
        &app.state().ai_registry,
        app.state().store.as_ref(),
        config,
    )
    .await
    .unwrap();
    assert!(ai_list_skills(app.state()).await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn skill_tools_require_agent_mode_and_an_authorized_selection() {
    use datazen::ai::{AiAgentMode, AiSkillsSettings};

    let mut settings = AiSkillsSettings {
        enabled: true,
        custom_dir: None,
        auto_use: false,
        selected: Vec::new(),
    };
    assert!(super::chat::skill_tools_for(&settings, AiAgentMode::Agent, true).is_empty());
    assert!(super::chat::skill_tools_for(&settings, AiAgentMode::Ask, true).is_empty());

    settings.selected = vec!["user:alpha".into()];
    assert_eq!(
        super::chat::skill_tools_for(&settings, AiAgentMode::Agent, true).len(),
        3
    );
    assert!(super::chat::skill_tools_for(&settings, AiAgentMode::Agent, false).is_empty());

    settings.auto_use = true;
    settings.selected.clear();
    assert_eq!(
        super::chat::skill_tools_for(&settings, AiAgentMode::Agent, true).len(),
        3
    );
}
