//! Reusable AI tool definitions, dispatch and tool-loop limits.

use super::ports::DatabaseToolHost;
use crate::ai::domain::types::{ChatMessage, MessageRole, TokenUsage, ToolCall, ToolDefinition};
use crate::connection::domain::{ConnectionRepository, ConnectionScope};
pub fn db_tool_definitions() -> Vec<ToolDefinition> {
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

pub fn is_db_tool(name: &str) -> bool {
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
pub enum ToolKind {
    AskQuestions,
    Db(String),
    Mcp {
        server_id: String,
        tool_name: String,
    },
    Unknown,
}

pub fn classify_tool(name: &str) -> ToolKind {
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
pub struct ToolLoopGuard {
    pub max_rounds: usize,
    pub max_total_tokens: usize,
    pub max_duration: std::time::Duration,
    pub per_tool_cap: usize,
    #[allow(dead_code)]
    pub per_round_cap: usize,
    pub round: usize,
    pub total_tokens: usize,
    pub start_time: std::time::Instant,
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
pub async fn build_egress_summary(
    catalog: &dyn ConnectionRepository,
    scope: &ConnectionScope,
) -> String {
    let connections = catalog.list(scope).await.unwrap_or_default();
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
pub fn is_readonly_db_tool(name: &str) -> bool {
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
pub fn mcp_needs_confirm(tool_name: &str, input_schema: &serde_json::Value) -> bool {
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

pub async fn execute_db_tool(
    host: &dyn DatabaseToolHost,
    catalog: &dyn ConnectionRepository,
    scope: &ConnectionScope,
    tool_call: &ToolCall,
) -> String {
    let args: serde_json::Value = serde_json::from_str(&tool_call.arguments).unwrap_or_default();
    let args_str = args.to_string();
    tracing::info!(tool = %tool_call.name, args_len = args_str.len(), "execute_db_tool");

    let cm = host;
    let result = match tool_call.name.as_str() {
        "list_connections" => super::db_tools::list_connections(catalog, scope).await,
        "list_databases" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            super::db_tools::list_databases(cm, connection_id).await
        }
        "list_tables" => {
            let connection_id = args["connection_id"].as_str().unwrap_or("");
            super::db_tools::list_tables(
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
            super::db_tools::search_tables(
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
            super::db_tools::get_table_schema(
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
