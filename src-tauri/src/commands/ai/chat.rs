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

// ─── Tool Loop Guard (FR-05) ───

/// Safety guardrails for the streaming tool loop. Prevents runaway loops by
/// enforcing round count, token budget, and duration limits.
pub(crate) struct ToolLoopGuard {
    max_rounds: usize,
    max_total_tokens: usize,
    max_duration: std::time::Duration,
    per_tool_cap: usize,
    #[allow(dead_code)]
    per_round_cap: usize,
    round: usize,
    total_tokens: usize,
    start_time: std::time::Instant,
}

impl ToolLoopGuard {
    /// Default limits per FR-05.
    const DEFAULT_MAX_ROUNDS: usize = 6;
    const DEFAULT_MAX_TOTAL_TOKENS: usize = 24_000;
    const DEFAULT_MAX_DURATION_SECS: u64 = 180;
    const DEFAULT_PER_TOOL_CAP: usize = 2048;
    const DEFAULT_PER_ROUND_CAP: usize = 12_288;

    pub fn new() -> Self {
        Self {
            max_rounds: Self::DEFAULT_MAX_ROUNDS,
            max_total_tokens: Self::DEFAULT_MAX_TOTAL_TOKENS,
            max_duration: std::time::Duration::from_secs(Self::DEFAULT_MAX_DURATION_SECS),
            per_tool_cap: Self::DEFAULT_PER_TOOL_CAP,
            per_round_cap: Self::DEFAULT_PER_ROUND_CAP,
            round: 0,
            total_tokens: 0,
            start_time: std::time::Instant::now(),
        }
    }

    /// Check whether another round is allowed. Returns `Err(message)` if any
    /// limit is exceeded.
    pub fn check_round(&mut self) -> Result<(), String> {
        self.round += 1;
        if self.round > self.max_rounds {
            return Err(format!(
                "Tool loop exceeded max rounds ({})",
                self.max_rounds
            ));
        }
        if self.total_tokens >= self.max_total_tokens {
            return Err(format!(
                "Tool loop exceeded token budget ({}/{})",
                self.total_tokens, self.max_total_tokens
            ));
        }
        if self.start_time.elapsed() >= self.max_duration {
            return Err(format!(
                "Tool loop exceeded time limit ({:.0}s)",
                self.max_duration.as_secs()
            ));
        }
        Ok(())
    }

    /// Accumulate token usage from a completed stream round.
    pub fn accumulate_usage(&mut self, usage: &TokenUsage) {
        self.total_tokens += usage.total_tokens as usize;
    }

    /// Truncate a tool result string to the per-tool byte cap.
    pub fn truncate_tool_result(result: &str, cap: usize) -> String {
        let bytes = result.as_bytes();
        if bytes.len() <= cap {
            result.to_string()
        } else {
            let truncated = String::from_utf8_lossy(&bytes[..cap]).to_string();
            format!(
                "{}\u{2026}(truncated: {} bytes omitted)",
                truncated,
                bytes.len() - cap
            )
        }
    }

    /// Truncate a tool result to the per-tool cap and return it wrapped in a
    /// tool message.
    pub fn truncate_and_wrap(&self, tc: &ToolCall, result: String) -> ChatMessage {
        let truncated = Self::truncate_tool_result(&result, self.per_tool_cap);
        ChatMessage {
            role: MessageRole::Tool,
            content: truncated,
            reasoning: None,
            tool_calls: None,
            tool_call_id: Some(tc.id.clone()),
        }
    }

    /// Check if a total round result exceeds the per-round cap and truncate.
    #[allow(dead_code)]
    pub fn enforce_round_cap(&self, messages: &mut Vec<ChatMessage>) {
        let total: usize = messages.iter().map(|m| m.content.len()).sum();
        if total > self.per_round_cap {
            // Truncate the last message to fit within the cap.
            if let Some(last) = messages.last_mut() {
                let excess = total - self.per_round_cap;
                let bytes = last.content.as_bytes();
                if bytes.len() > excess {
                    last.content = String::from_utf8_lossy(&bytes[..bytes.len() - excess])
                        .to_string()
                        + "\u{2026}(round cap)";
                }
            }
        }
    }
}

impl Default for ToolLoopGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a brief egress summary string describing the connections the AI has
/// tool-based access to. Sent as the first chunk before the AI reply begins
/// (FR-13). Does NOT include full connection details — only counts and types.
async fn build_egress_summary(state: &AppState) -> String {
    let connections = state.store.get_connections().await;
    let connection_count = connections.len();
    if connection_count == 0 {
        return "No database connections configured.".into();
    }
    // Collect distinct database types
    let mut db_types: Vec<String> = Vec::new();
    for c in &connections {
        if !db_types.contains(&c.database_type) {
            db_types.push(c.database_type.clone());
        }
    }
    let types_str = if db_types.is_empty() {
        String::new()
    } else {
        format!(" ({})", db_types.join(", "))
    };
    format!(
        "{connection_count} connection{conn_s}{types_str} accessible via tools.",
        conn_s = if connection_count == 1 { "" } else { "s" },
    )
}

/// Returns `true` if the tool is a read-only DB tool that can run in parallel.
pub(crate) fn is_readonly_db_tool(name: &str) -> bool {
    matches!(
        name,
        "list_connections"
            | "list_databases"
            | "list_tables"
            | "search_tables"
            | "get_table_schema"
    )
}

/// Returns `true` if the MCP tool needs user confirmation before execution.
/// Heuristic: tool name contains write/delete/drop/update/create/insert, or
/// the input schema has `x-write: true`.
#[allow(dead_code)]
pub(crate) fn mcp_needs_confirm(tool_name: &str, input_schema: &serde_json::Value) -> bool {
    // Schema-based check
    if input_schema.get("x-write") == Some(&serde_json::Value::Bool(true)) {
        return true;
    }
    // Name-based heuristic
    let lower = tool_name.to_lowercase();
    ["write", "delete", "drop", "update", "create", "insert"]
        .iter()
        .any(|kw| lower.contains(kw))
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
            crate::services::db_tools::list_tables(
                cm,
                connection_id,
                args["database"].as_str(),
                args["schema"].as_str(),
            )
            .await
        }
        "search_tables" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            let pattern = args["pattern"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            crate::services::db_tools::search_tables(
                cm,
                connection_id,
                args["database"].as_str(),
                args["schema"].as_str(),
                pattern,
                limit,
            )
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
            crate::services::db_tools::get_table_schema(
                cm,
                connection_id,
                &tables,
                args["database"].as_str(),
                args["schema"].as_str(),
            )
            .await
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
    _max_rounds: usize,
    cmd_label: &str,
    gate: &AiSafetyGateConfig,
) -> Result<String, CommandError> {
    let mut guard = ToolLoopGuard::new();

    // ── FR-13: Send egress summary before AI reply begins ──
    {
        let summary = build_egress_summary(state).await;
        on_chunk(
            request_id,
            Ok(StreamChunk {
                content: String::new(),
                reasoning: None,
                done: false,
                cancelled: false,
                usage: None,
                tool_calls: None,
                response_id: None,
                egress_summary: Some(summary),
            }),
        );
    }

    loop {
        // ── Cancel check (Phase C) ──
        if let Some(ref token) = request.cancel_token {
            if token.is_cancelled() {
                tracing::info!(%request_id, "{cmd_label}: cancelled before round {}", guard.round);
                on_chunk(
                    request_id,
                    Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        cancelled: false,
                        usage: None,
                        tool_calls: None,
                        response_id: None,
                        egress_summary: None,
                    }),
                );
                return Ok(request_id.to_string());
            }
        }

        // ── Guard: round / token / time budget ──
        if let Err(msg) = guard.check_round() {
            tracing::warn!(%request_id, "{msg}");
            on_chunk(
                request_id,
                Ok(StreamChunk {
                    content: format!("[Tool loop stopped: {msg}]"),
                    reasoning: None,
                    done: true,
                    cancelled: false,
                    usage: None,
                    tool_calls: None,
                    response_id: None,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }
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
                                        cancelled: false,
                                        usage: None,
                                        tool_calls: None,
                                        response_id: None,
                                        egress_summary: None,
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

        // Accumulate token usage for the guard's budget check.
        if let Some(ref usage) = result.usage {
            guard.accumulate_usage(usage);
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
                        cancelled: false,
                        usage: result.usage,
                        tool_calls: result.tool_calls,
                        response_id: result.response_id,
                        egress_summary: None,
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
                    cancelled: false,
                    usage: result.usage,
                    tool_calls: Some(classified.iter().map(|(tc, _)| tc.clone()).collect()),
                    response_id: result.response_id,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }

        tracing::info!(
            %request_id,
            round = guard.round,
            executable_count,
            has_ask_questions,
            tool_names = ?classified.iter().map(|(t, _)| t.name.as_str()).collect::<Vec<_>>(),
            response_id = ?result.response_id,
            "{cmd_label}: executing tools (round {})", guard.round
        );
        tracing::debug!(
            %request_id,
            round = guard.round,
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
                    cancelled: false,
                    usage: None,
                    tool_calls: Some(mcp_tool_calls),
                    response_id: None,
                    egress_summary: None,
                }),
            );
        }

        for (tc, kind) in &classified {
            match kind {
                ToolKind::AskQuestions => continue,
                ToolKind::Unknown => {
                    // Unknown tool: send error chunk and terminate loop.
                    tracing::warn!(
                        %request_id,
                        tool_name = %tc.name,
                        "unknown tool requested by provider"
                    );
                    let err_msg = format!(
                        "Tool '{}' is not available. The provider requested a tool that is not registered.",
                        tc.name
                    );
                    request.messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: err_msg.clone(),
                        reasoning: None,
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                    });
                    on_chunk(
                        request_id,
                        Ok(StreamChunk {
                            content: err_msg,
                            reasoning: None,
                            done: true,
                            cancelled: false,
                            usage: result.usage,
                            tool_calls: None,
                            response_id: result.response_id,
                            egress_summary: None,
                        }),
                    );
                    return Ok(request_id.to_string());
                }
                _ => {}
            }
        }

        // Separate read-only DB tools for potential future parallel execution.
        // Currently executed sequentially to avoid AppState clone complexity.
        let (parallel_db_tcs, sequential_tcs): (Vec<_>, Vec<_>) = classified
            .iter()
            .filter(|(_, kind)| !matches!(kind, ToolKind::AskQuestions))
            .partition(|(_, kind)| matches!(kind, ToolKind::Db(name) if is_readonly_db_tool(name)));

        // Execute all tools sequentially.
        for (tc, kind) in parallel_db_tcs.iter().chain(sequential_tcs.iter()) {
            let tool_result = match kind {
                ToolKind::Db(_) => execute_db_tool(state, tc).await,
                ToolKind::Mcp {
                    server_id,
                    tool_name,
                } => execute_mcp_tool(state, server_id, tool_name, &tc.arguments).await,
                _ => continue,
            };
            let redacted = redact_for_gate(&tool_result, gate);
            request.messages.push(guard.truncate_and_wrap(tc, redacted));
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
                    cancelled: false,
                    usage: result.usage,
                    tool_calls: Some(ask_tool_calls),
                    response_id: result.response_id,
                    egress_summary: None,
                }),
            );
            return Ok(request_id.to_string());
        }
    }
    // The loop always returns internally (cancel, guard, no-tools, ask_questions,
    // or after tool execution). This point is unreachable.
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
    let cancel_token = state.cancel_registry.register(&request_id).await;
    request.cancel_token = Some(cancel_token.clone());

    let result = run_streaming_tool_loop(
        provider,
        state,
        on_chunk,
        &request_id,
        request,
        10,
        "ai_chat",
        &gate,
    )
    .await;

    // Always unregister — normal completion or cancellation.
    state.cancel_registry.unregister(&request_id).await;
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
