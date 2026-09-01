//! AI chat IPC and tool-loop execution.

use super::util::{
    build_connections_context, inject_language_hint, resolve_ai, window_stream_callback,
    StreamCallback,
};
use crate::ai::budget;
use crate::ai::*;
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen_driver_api::PromptScenario;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{State, WebviewWindow};
use tokio::sync::mpsc;

// ─── Database Tool Definitions & Execution ───

pub(crate) fn db_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_connections".into(),
            description: "List all configured database connections with their IDs, names, database types, and hosts. Call this first to discover available data sources.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "list_databases".into(),
            description: "List all databases on a connected database server.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" }
                },
                "required": ["connection_id"]
            }),
        },
        ToolDefinition {
            name: "list_tables".into(),
            description: "List all tables in a database with their types and row counts.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" },
                    "database": { "type": "string", "description": "Database name (optional for some database types)" }
                },
                "required": ["connection_id"]
            }),
        },
        ToolDefinition {
            name: "search_tables".into(),
            description: "Search for tables by name pattern (case-insensitive substring match). Use this instead of list_tables when the database has many tables and you need to find specific ones.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" },
                    "database": { "type": "string", "description": "Database name" },
                    "pattern": { "type": "string", "description": "Search keyword to match against table names" },
                    "limit": { "type": "integer", "description": "Max results to return (default 20)", "default": 20 }
                },
                "required": ["connection_id", "pattern"]
            }),
        },
        ToolDefinition {
            name: "get_table_schema".into(),
            description: "Get detailed schema of one or more tables, including column names, data types, primary keys, foreign keys, and indexes. Supports batch queries for multiple tables.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "The connection ID from list_connections" },
                    "tables": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "One or more table names to get schema for"
                    }
                },
                "required": ["connection_id", "tables"]
            }),
        },
    ]
}

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

pub(crate) fn is_db_tool(name: &str) -> bool {
    matches!(
        name,
        "list_connections"
            | "list_databases"
            | "list_tables"
            | "search_tables"
            | "get_table_schema"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolKind {
    AskQuestions,
    Db(String),
    Mcp {
        server_id: String,
        tool_name: String,
    },
    Unknown,
}

pub(crate) fn classify_tool(name: &str) -> ToolKind {
    if name == "ask_questions" {
        return ToolKind::AskQuestions;
    }
    if is_db_tool(name) {
        return ToolKind::Db(name.to_string());
    }
    if let Some(rest) = name.strip_prefix("mcp/") {
        if let Some((server_id, tool_name)) = rest.split_once('/') {
            if !server_id.is_empty() && !tool_name.is_empty() {
                return ToolKind::Mcp {
                    server_id: server_id.to_string(),
                    tool_name: tool_name.to_string(),
                };
            }
        }
    }
    ToolKind::Unknown
}

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
    let args: serde_json::Value = serde_json::from_str(&tool_call.arguments).unwrap_or_default();
    let args_str = args.to_string();
    tracing::info!(tool = %tool_call.name, args_len = args_str.len(), "execute_db_tool");

    let cm = &state.connection_manager;
    let result = match tool_call.name.as_str() {
        "list_connections" => crate::services::db_tools::list_connections(&state.store).await,
        "list_databases" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            crate::services::db_tools::list_databases(cm, connection_id).await
        }
        "list_tables" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let database = args["database"].as_str().unwrap_or("");
            crate::services::db_tools::list_tables(cm, connection_id, database).await
        }
        "search_tables" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let database = args["database"].as_str().unwrap_or("");
            let pattern = args["pattern"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            crate::services::db_tools::search_tables(cm, connection_id, database, pattern, limit)
                .await
        }
        "get_table_schema" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let tables: Vec<String> = args["tables"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            crate::services::db_tools::get_table_schema(cm, connection_id, &tables).await
        }
        other => Err(format!("Unknown tool: {other}")),
    };
    result.unwrap_or_else(|e| e)
}

struct StreamRoundResult {
    content: String,
    reasoning: String,
    tool_calls: Option<Vec<ToolCall>>,
    usage: Option<TokenUsage>,
    had_error: bool,
    response_id: Option<String>,
}

pub(crate) async fn run_streaming_tool_loop(
    provider: Arc<dyn AiProvider>,
    state: &AppState,
    on_chunk: StreamCallback,
    request_id: &str,
    mut request: CompletionRequest,
    max_rounds: usize,
    cmd_label: &str,
) -> Result<String, CommandError> {
    for round in 0..max_rounds {
        let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);

        let (result_tx, result_rx) = tokio::sync::oneshot::channel::<StreamRoundResult>();
        let on_chunk_c = on_chunk.clone();
        let rid_c = request_id.to_string();

        tokio::spawn(async move {
            let mut full_content = String::new();
            let mut full_reasoning = String::new();
            let mut final_tool_calls: Option<Vec<ToolCall>> = None;
            let mut final_usage = None;
            let mut final_response_id = None;
            let mut had_error = false;

            while let Some(chunk_result) = rx.recv().await {
                match chunk_result {
                    Ok(chunk) => {
                        if chunk.done {
                            let content = chunk.content;
                            let reasoning = chunk.reasoning;
                            full_content.push_str(&content);
                            if let Some(r) = &reasoning {
                                full_reasoning.push_str(r);
                            }
                            final_tool_calls = chunk.tool_calls;
                            final_usage = chunk.usage;
                            final_response_id = chunk.response_id;
                            if !content.is_empty() || reasoning.is_some() {
                                on_chunk_c(
                                    &rid_c,
                                    Ok(StreamChunk {
                                        content,
                                        reasoning,
                                        done: false,
                                        usage: None,
                                        tool_calls: None,
                                        response_id: None,
                                    }),
                                );
                            }
                        } else {
                            full_content.push_str(&chunk.content);
                            if let Some(r) = &chunk.reasoning {
                                full_reasoning.push_str(r);
                            }
                            on_chunk_c(&rid_c, Ok(chunk));
                        }
                    }
                    Err(e) => {
                        on_chunk_c(&rid_c, Err(e));
                        had_error = true;
                        break;
                    }
                }
            }

            let _ = result_tx.send(StreamRoundResult {
                content: full_content,
                reasoning: full_reasoning,
                tool_calls: final_tool_calls,
                usage: final_usage,
                had_error,
                response_id: final_response_id,
            });
        });

        provider
            .stream_complete(&request, tx)
            .await
            .cmd_err(cmd_label)?;

        let result = result_rx
            .await
            .map_err(|_| CommandError::Internal("Stream result channel closed".into()))?;

        if result.had_error {
            return Ok(request_id.to_string());
        }

        let all_tcs = match result.tool_calls {
            Some(tcs) if !tcs.is_empty() => tcs,
            _ => {
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        usage: result.usage,
                        tool_calls: result.tool_calls,
                        response_id: result.response_id,
                    }),
                );
                return Ok(request_id.to_string());
            }
        };

        let classified: Vec<(ToolCall, ToolKind)> = all_tcs
            .into_iter()
            .map(|tc| {
                let kind = classify_tool(&tc.name);
                (tc, kind)
            })
            .collect();

        let executable_count = classified
            .iter()
            .filter(|(_, kind)| matches!(kind, ToolKind::Db(_) | ToolKind::Mcp { .. }))
            .count();

        let has_ask_questions = classified
            .iter()
            .any(|(_, kind)| matches!(kind, ToolKind::AskQuestions));
        let all_ask_questions = classified
            .iter()
            .all(|(_, kind)| matches!(kind, ToolKind::AskQuestions));

        if executable_count == 0 && all_ask_questions {
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    usage: result.usage,
                    tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
                    response_id: result.response_id,
                }),
            );
            return Ok(request_id.to_string());
        }

        tracing::info!(
            %request_id,
            round,
            executable_count,
            has_ask_questions,
            tool_names = ?classified.iter().map(|(t, _)| t.name.as_str()).collect::<Vec<_>>(),
            response_id = ?result.response_id,
            "{cmd_label}: executing tools (round {round})"
        );
        tracing::debug!(
            %request_id,
            round,
            tool_count = classified.len(),
            "{cmd_label}: tools selected"
        );

        request.messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: result.content,
            reasoning: if result.reasoning.is_empty() {
                None
            } else {
                Some(result.reasoning)
            },
            tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
            tool_call_id: None,
        });

        let mcp_tool_calls: Vec<ToolCall> = classified
            .iter()
            .filter(|(_, kind)| matches!(kind, ToolKind::Mcp { .. }))
            .map(|(tc, _)| tc.clone())
            .collect();
        if !mcp_tool_calls.is_empty() {
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: false,
                    usage: None,
                    tool_calls: Some(mcp_tool_calls),
                    response_id: None,
                }),
            );
        }

        for (tc, kind) in &classified {
            let tool_result = match kind {
                ToolKind::AskQuestions => continue,
                ToolKind::Db(_) => execute_db_tool(state, tc).await,
                ToolKind::Mcp {
                    server_id,
                    tool_name,
                } => execute_mcp_tool(state, server_id, tool_name, &tc.arguments).await,
                ToolKind::Unknown => format!("Unknown tool: {}", tc.name),
            };
            request.messages.push(ChatMessage {
                role: MessageRole::Tool,
                content: tool_result,
                reasoning: None,
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });
        }

        if has_ask_questions {
            let ask_tool_calls: Vec<ToolCall> = classified
                .iter()
                .filter(|(_, kind)| matches!(kind, ToolKind::AskQuestions))
                .map(|(tc, _)| tc.clone())
                .collect();
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: String::new(),
                    reasoning: None,
                    done: true,
                    usage: result.usage,
                    tool_calls: Some(ask_tool_calls),
                    response_id: result.response_id,
                }),
            );
            return Ok(request_id.to_string());
        }
    }

    tracing::warn!(%request_id, "{cmd_label}: reached max tool rounds");
    on_chunk(
        request_id,
        Ok(StreamChunk {
            content: String::new(),
            reasoning: None,
            done: true,
            usage: None,
            tool_calls: None,
            response_id: None,
        }),
    );
    Ok(request_id.to_string())
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

    let lang = state.store.get_settings().await.language;
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

    full_messages.extend(messages);

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
                    let context_block = crate::commands::context::format_context_block(&entries);
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
    };
    inject_language_hint(&mut request.messages, &lang);

    run_streaming_tool_loop(
        provider,
        state,
        on_chunk,
        &request_id,
        request,
        10,
        "ai_chat",
    )
    .await
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
) -> Result<String, CommandError> {
    ai_chat_impl(
        &state,
        window_stream_callback(&window),
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
