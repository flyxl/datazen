//! AI chat IPC and tool-loop execution.

use super::util::{
    build_connections_context, inject_language_hint, resolve_ai, resolve_safety_gate,
    window_stream_callback, StreamCallback,
};
use crate::ai::budget;
use crate::ai::safety::redact_for_gate;
use crate::ai::*;
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen_driver_api::PromptScenario;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{State, WebviewWindow};

// ─── Database Tool Definitions & Execution ───

pub(crate) use datazen_feature_core::ai::tools::db_tool_definitions;

/// Convert connected MCP client tools into AI `ToolDefinition`s (Phase 3 wires into chat).
pub(crate) async fn mcp_tool_definitions(state: &AppState) -> Vec<ToolDefinition> {
    let settings = state.store.get_settings().await;
    state
        .mcp_client_manager
        .all_tools()
        .await
        .into_iter()
        .filter(|tool| {
            settings
                .mcp_client_servers
                .iter()
                .find(|c| c.id == tool.server_id)
                .map(|c| c.enabled_for_ai)
                .unwrap_or(true)
        })
        .map(|tool| ToolDefinition {
            name: tool.qualified_name,
            description: tool.description.unwrap_or_else(|| tool.tool_name.clone()),
            parameters: tool.input_schema,
        })
        .collect()
}

pub(crate) use datazen_feature_core::ai::tools::{
    classify_tool, is_db_tool, is_readonly_db_tool, mcp_needs_confirm, ToolKind, ToolLoopGuard,
};

pub(crate) async fn execute_mcp_tool(
    state: &AppState,
    server_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    let qualified = crate::mcp::client::mcp_qualified_name(server_id, tool_name);
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => {
            return format!("MCP tool error ({qualified}): invalid JSON arguments: {e}");
        }
    };

    match state
        .mcp_client_manager
        .call_tool(server_id, tool_name, args)
        .await
    {
        Ok(result) => crate::mcp::format_call_tool_result(&result, &qualified),
        Err(msg) => format!("MCP tool error ({qualified}): {msg}"),
    }
}

pub(crate) async fn execute_db_tool(state: &AppState, tool_call: &ToolCall) -> String {
    datazen_feature_core::ai::tools::execute_db_tool(state, state.store.as_ref(), tool_call).await
}

#[async_trait::async_trait]
impl datazen_feature_core::ai::tool_loop::AiToolLoopHost for AppState {
    async fn egress_summary(&self) -> String {
        datazen_feature_core::ai::tools::build_egress_summary(self.store.as_ref()).await
    }
    async fn execute_db_tool(&self, call: &ToolCall) -> String {
        execute_db_tool(self, call).await
    }
    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String {
        execute_mcp_tool(self, server, tool, arguments).await
    }
}
pub(crate) async fn run_streaming_tool_loop(
    provider: Arc<dyn AiProvider>,
    state: &AppState,
    on_chunk: StreamCallback,
    request_id: &str,
    request: CompletionRequest,
    max_rounds: usize,
    cmd_label: &str,
    gate: &AiSafetyGateConfig,
) -> Result<String, CommandError> {
    run_streaming_tool_loop_with_claim(
        provider,
        state,
        on_chunk,
        request_id,
        request,
        max_rounds,
        cmd_label,
        gate,
        &datazen_feature_core::ai::ownership::AiTargetClaim::Independent,
    )
    .await
}

pub(crate) async fn run_streaming_tool_loop_with_claim(
    provider: Arc<dyn AiProvider>,
    state: &AppState,
    on_chunk: StreamCallback,
    request_id: &str,
    request: CompletionRequest,
    max_rounds: usize,
    cmd_label: &str,
    gate: &AiSafetyGateConfig,
    claim: &datazen_feature_core::ai::ownership::AiTargetClaim,
) -> Result<String, CommandError> {
    // The claim decides which resource each tool may touch; independent and
    // editor-granted calls never share an execution path.
    let host = super::AiRequestHost {
        state,
        claim: claim.clone(),
    };
    datazen_feature_core::ai::tool_loop::run_streaming_tool_loop(
        provider, &host, on_chunk, request_id, request, max_rounds, cmd_label, gate,
    )
    .await
    .map_err(Into::into)
}

// ─── AI Chat ───

pub(crate) async fn ai_chat_impl(
    state: &AppState,
    on_chunk: StreamCallback,
    db_session_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
    scenario: Option<String>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
) -> Result<String, CommandError> {
    ai_chat_impl_with_claim(
        state,
        on_chunk,
        None,
        db_session_id,
        database,
        messages,
        request_id,
        include_schema,
        scenario,
        context_files,
        context_tables,
    )
    .await
}

pub(crate) async fn ai_chat_impl_with_claim(
    state: &AppState,
    on_chunk: StreamCallback,
    target_claim: Option<datazen_feature_core::ai::ownership::AiTargetClaim>,
    db_session_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
    scenario: Option<String>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
) -> Result<String, CommandError> {
    let is_workflow = scenario.as_deref() == Some("workflow_generate");
    let prompt_scenario = if is_workflow {
        PromptScenario::WorkflowGenerate
    } else {
        PromptScenario::Chat
    };

    tracing::info!(
        %request_id,
        db_session_id = ?db_session_id,
        database = ?database,
        messages_count = messages.len(),
        %include_schema,
        scenario = ?scenario,
        context_tables_count = context_tables.as_ref().map(|t| t.len()).unwrap_or(0),
        last_user_msg_len = messages.last().map(|m| m.content.len()).unwrap_or(0),
        "ai_chat: start"
    );
    let (provider, ai_config) = resolve_ai(&state).await?;

    let app_settings = state.store.get_settings().await;
    let gate = resolve_safety_gate(&state).await;
    let lang = app_settings.language;
    let mut full_messages: Vec<ChatMessage> = Vec::new();
    let mut attach_db_tools = true;

    if include_schema {
        if let Some(ref conn_id) = db_session_id {
            let db = database.as_deref().unwrap_or("");
            if let Ok((driver, _handle)) = state.connection_manager.get_session(conn_id).await {
                let pinned = context_tables.clone().unwrap_or_default();
                let supports_tools = provider.supports_tools();
                let pipeline = SchemaContextPipeline::new(state.schema_context_builder.clone());
                match pipeline
                    .resolve(
                        conn_id,
                        db,
                        &pinned,
                        supports_tools,
                        budget::PINNED_DDL,
                        budget::FALLBACK_DDL,
                    )
                    .await
                {
                    Ok(seed) => {
                        attach_db_tools = seed.attach_db_tools;
                        let db_type = seed.database_type.clone();
                        let suffix = compose_schema_system_suffix(&seed);

                        let mut vars = HashMap::new();
                        vars.insert("db_type", db_type.as_str());
                        vars.insert("schema", "");

                        let connections_ctx = if is_workflow {
                            build_connections_context(&state, &lang).await
                        } else {
                            String::new()
                        };
                        vars.insert("connections", connections_ctx.as_str());

                        let base_tpl = state
                            .prompt_resolver
                            .resolve(prompt_scenario, Some(driver.as_ref()), &lang)
                            .await;
                        let base = crate::ai::prompt_resolver::render_template(&base_tpl, &vars);

                        let desc = if is_workflow {
                            base
                        } else if lang.starts_with("zh") {
                            format!("{base}\n\n用户已连接到 {db_type} 数据库。")
                        } else {
                            format!("{base}\n\nThe user is connected to a {db_type} database.")
                        };

                        full_messages.push(ChatMessage {
                            role: MessageRole::System,
                            content: format!("{desc}\n\n{suffix}"),
                            reasoning: None,
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            %request_id,
                            db_session_id = %conn_id,
                            database = %db,
                            error_len = e.to_string().len(),
                            "ai_chat: schema context pipeline resolve failed; disabling DB tools"
                        );
                        attach_db_tools = false;
                    }
                }
            }
        }
    }

    if full_messages.is_empty() {
        if is_workflow {
            let connections_ctx = build_connections_context(&state, &lang).await;
            let mut vars = HashMap::new();
            vars.insert("connections", connections_ctx.as_str());
            vars.insert("schema", "");
            vars.insert("db_type", "");
            let tpl = state
                .prompt_resolver
                .resolve(prompt_scenario, None, &lang)
                .await;
            let prompt = crate::ai::prompt_resolver::render_template(&tpl, &vars);
            full_messages.push(ChatMessage {
                role: MessageRole::System,
                content: prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            });
        } else {
            let chat_prompt = state
                .prompt_resolver
                .resolve(prompt_scenario, None, &lang)
                .await;
            full_messages.push(ChatMessage {
                role: MessageRole::System,
                content: chat_prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    full_messages.extend(messages.into_iter().map(|mut message| {
        message.content = redact_for_gate(&message.content, &gate);
        message
    }));

    // Inject context files into last user message
    if let Some(ref ctx_files) = context_files {
        if !ctx_files.is_empty() {
            let ctx_dir = crate::commands::context::resolve_context_dir_from_state(&state).await?;
            let entries = crate::commands::context::read_context_paths(&ctx_dir, ctx_files).await?;
            if !entries.is_empty() {
                if let Some(last_user) = full_messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.role == MessageRole::User)
                {
                    let sanitized_entries: Vec<(String, String)> = entries
                        .into_iter()
                        .map(|(path, content)| (path, redact_for_gate(&content, &gate)))
                        .collect();
                    let context_block =
                        crate::commands::context::format_context_block(&sanitized_entries);
                    last_user.content = format!("{context_block}\n\n{}", last_user.content);
                }
            }
        }
    }

    let ask_questions_tool = ToolDefinition {
        name: "ask_questions".into(),
        description: "Ask the user structured questions to gather information. Use when you need the user to choose between options or provide specific input.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Unique question identifier" },
                            "prompt": { "type": "string", "description": "The question text" },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": { "type": "string" },
                                        "label": { "type": "string" }
                                    },
                                    "required": ["id", "label"]
                                },
                                "description": "Predefined options. Can be empty for free-text input."
                            },
                            "allowMultiple": { "type": "boolean", "description": "Allow selecting multiple options", "default": false }
                        },
                        "required": ["id", "prompt"]
                    }
                }
            },
            "required": ["questions"]
        }),
    };

    let mut all_tools = vec![ask_questions_tool];
    if attach_db_tools {
        all_tools.extend(db_tool_definitions());
    }
    if provider.supports_tools() {
        all_tools.extend(mcp_tool_definitions(state).await);
    }

    let mut request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: full_messages,
        temperature: Some(0.7),
        stop: None,
        tools: Some(all_tools),
        previous_response_id: None,
        cancel_token: None,
    };
    inject_language_hint(&mut request.messages, &lang);

    // Register cancel token so `ai_cancel` IPC can interrupt this stream.
    let cancel_owner = super::ai_owner_fingerprint(state);
    let cancel_token = state
        .cancel_registry
        .register(&request_id, &cancel_owner)
        .await;
    request.cancel_token = Some(cancel_token.clone());

    let claim =
        target_claim.unwrap_or(datazen_feature_core::ai::ownership::AiTargetClaim::Independent);
    let result = run_streaming_tool_loop_with_claim(
        provider,
        state,
        on_chunk,
        &request_id,
        request,
        10,
        "ai_chat",
        &gate,
        &claim,
    )
    .await;

    // Always unregister — normal completion or cancellation.
    state
        .cancel_registry
        .unregister(&request_id, &cancel_owner)
        .await;
    result
}

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    window: WebviewWindow,
    db_session_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
    scenario: Option<String>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
    target_claim: Option<datazen_feature_core::ai::ownership::AiTargetClaim>,
) -> Result<String, CommandError> {
    ai_chat_impl_with_claim(
        &state,
        window_stream_callback(&window),
        target_claim,
        db_session_id,
        database,
        messages,
        request_id,
        include_schema,
        scenario,
        context_files,
        context_tables,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ai_api::TokenUsage;

    // ── ToolLoopGuard unit tests [tester] ──

    #[test]
    fn test_tester_guard_new_defaults() {
        let guard = ToolLoopGuard::new();
        assert_eq!(guard.round, 0);
        assert_eq!(guard.total_tokens, 0);
        assert_eq!(guard.max_rounds, 6);
        assert_eq!(guard.max_total_tokens, 24_000);
        assert_eq!(guard.max_duration.as_secs(), 180);
        assert_eq!(guard.per_tool_cap, 2048);
        assert_eq!(guard.per_round_cap, 12_288);
    }

    #[test]
    fn test_tester_guard_check_round_succeeds_within_limits() {
        let mut guard = ToolLoopGuard::new();
        // First 6 rounds should succeed (max_rounds = 6)
        for i in 1..=6 {
            assert!(guard.check_round().is_ok(), "round {i} should succeed");
        }
        assert_eq!(guard.round, 6);
    }

    #[test]
    fn test_tester_guard_check_round_fails_on_max_rounds() {
        let mut guard = ToolLoopGuard::new();
        for _ in 0..6 {
            guard.check_round().unwrap();
        }
        // 7th round should fail
        let err = guard.check_round().unwrap_err();
        assert!(
            err.contains("max rounds"),
            "error should mention max rounds: {err}"
        );
    }

    #[test]
    fn test_tester_guard_check_round_fails_on_token_budget() {
        let mut guard = ToolLoopGuard::new();
        guard.total_tokens = 24_000; // at budget
        let err = guard.check_round().unwrap_err();
        assert!(
            err.contains("token budget"),
            "error should mention token budget: {err}"
        );
    }

    #[test]
    fn test_tester_guard_check_round_fails_on_time_limit() {
        let mut guard = ToolLoopGuard::new();
        guard.max_duration = std::time::Duration::from_millis(1); // 1ms
                                                                  // Wait a bit to exceed the limit
        std::thread::sleep(std::time::Duration::from_millis(5));
        let err = guard.check_round().unwrap_err();
        assert!(
            err.contains("time limit"),
            "error should mention time limit: {err}"
        );
    }

    #[test]
    fn test_tester_guard_accumulate_usage() {
        let mut guard = ToolLoopGuard::new();
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 200,
            total_tokens: 300,
        };
        guard.accumulate_usage(&usage);
        assert_eq!(guard.total_tokens, 300);

        guard.accumulate_usage(&usage);
        assert_eq!(guard.total_tokens, 600);
    }

    #[test]
    fn test_tester_guard_truncate_under_cap() {
        let result = ToolLoopGuard::truncate_tool_result("hello", 100);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_tester_guard_truncate_over_cap() {
        let long = "a".repeat(3000);
        let result = ToolLoopGuard::truncate_tool_result(&long, 100);
        assert!(result.len() < 3000);
        assert!(result.contains("truncated"));
        assert!(result.contains("bytes omitted"));
    }

    #[test]
    fn test_tester_guard_truncate_exact_cap() {
        let exact = "a".repeat(100);
        let result = ToolLoopGuard::truncate_tool_result(&exact, 100);
        assert_eq!(result, exact);
    }

    #[test]
    fn test_tester_guard_truncate_and_wrap() {
        let guard = ToolLoopGuard::new();
        let tc = ToolCall {
            id: "call_1".into(),
            name: "test".into(),
            arguments: "{}".into(),
        };
        let msg = guard.truncate_and_wrap(&tc, "result content".into());
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(msg.content, "result content");
    }

    #[test]
    fn test_tester_guard_truncate_and_wrap_truncates_large() {
        let guard = ToolLoopGuard::new();
        let tc = ToolCall {
            id: "call_2".into(),
            name: "test".into(),
            arguments: "{}".into(),
        };
        let big = "x".repeat(4096);
        let msg = guard.truncate_and_wrap(&tc, big);
        assert!(msg.content.len() < 4096);
        assert!(msg.content.contains("truncated"));
    }

    // ── classify_tool tests [tester] ──

    #[test]
    fn test_tester_classify_tool_ask_questions() {
        assert!(matches!(
            classify_tool("ask_questions"),
            ToolKind::AskQuestions
        ));
    }

    #[test]
    fn test_tester_classify_tool_db() {
        assert!(matches!(classify_tool("list_connections"), ToolKind::Db(_)));
        assert!(matches!(classify_tool("list_databases"), ToolKind::Db(_)));
        assert!(matches!(classify_tool("list_tables"), ToolKind::Db(_)));
        assert!(matches!(classify_tool("search_tables"), ToolKind::Db(_)));
        assert!(matches!(classify_tool("get_table_schema"), ToolKind::Db(_)));
    }

    #[test]
    fn test_tester_classify_tool_mcp() {
        let kind = classify_tool("mcp/server1/get_data");
        match kind {
            ToolKind::Mcp {
                server_id,
                tool_name,
            } => {
                assert_eq!(server_id, "server1");
                assert_eq!(tool_name, "get_data");
            }
            _ => panic!("expected Mcp kind"),
        }
    }

    #[test]
    fn test_tester_classify_tool_unknown() {
        assert!(matches!(
            classify_tool("nonexistent_tool"),
            ToolKind::Unknown
        ));
        assert!(matches!(classify_tool("mcp/"), ToolKind::Unknown));
        assert!(matches!(classify_tool("mcp/server/"), ToolKind::Unknown));
    }

    // ── is_readonly_db_tool tests [tester] ──

    #[test]
    fn test_tester_is_readonly_db_tool() {
        assert!(is_readonly_db_tool("list_connections"));
        assert!(is_readonly_db_tool("list_databases"));
        assert!(is_readonly_db_tool("list_tables"));
        assert!(is_readonly_db_tool("search_tables"));
        assert!(is_readonly_db_tool("get_table_schema"));
        assert!(!is_readonly_db_tool("ask_questions"));
        assert!(!is_readonly_db_tool("unknown"));
    }

    // ── mcp_needs_confirm tests [tester] ──

    #[test]
    fn test_tester_mcp_needs_confirm_schema_based() {
        let schema = serde_json::json!({"x-write": true});
        assert!(mcp_needs_confirm("any_tool", &schema));
    }

    #[test]
    fn test_tester_mcp_needs_confirm_name_based() {
        let schema = serde_json::json!({});
        assert!(mcp_needs_confirm("write_file", &schema));
        assert!(mcp_needs_confirm("delete_record", &schema));
        assert!(mcp_needs_confirm("drop_table", &schema));
        assert!(mcp_needs_confirm("update_user", &schema));
        assert!(mcp_needs_confirm("create_index", &schema));
        assert!(mcp_needs_confirm("insert_data", &schema));
    }

    #[test]
    fn test_tester_mcp_needs_confirm_readonly() {
        let schema = serde_json::json!({});
        assert!(!mcp_needs_confirm("get_data", &schema));
        assert!(!mcp_needs_confirm("list_files", &schema));
    }
}
