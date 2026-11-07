//! Reusable AI tool definitions, dispatch and tool-loop limits.

use super::ports::DatabaseToolHost;
use super::tool_loop::AiToolContext;
use crate::ai::domain::types::{
    AiSafetyGateConfig, AiToolPermissionPolicy, ChatMessage, MessageRole, TokenUsage, ToolCall,
    ToolDefinition,
};
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

/// Chat/Agent DB tools: metadata discovery, plus read-only query and sampling
/// only when the session is bound to a concrete connection.
///
/// Kept separate from [`db_tool_definitions`] because SQL generation must stay
/// on metadata tools only.
pub fn agent_db_tool_definitions(allow_query: bool) -> Vec<ToolDefinition> {
    let mut tools = db_tool_definitions();
    if !allow_query {
        return tools;
    }
    tools.push(ToolDefinition {
        name: "query_readonly".into(),
        description: "Run a read-only SQL query against the current chat connection. Write statements are rejected. Prefer adding a LIMIT; the tool caps results at 1000 rows.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "connection_id": { "type": "string", "description": "The current session connection ID" },
                "sql": { "type": "string", "description": "A read-only SQL statement (SELECT/EXPLAIN/SHOW)" },
                "limit": { "type": "integer", "description": "Max rows to return (default 100, max 1000)", "default": 100, "minimum": 1, "maximum": 1000 }
            },
            "required": ["connection_id", "sql"]
        }),
    });
    tools.push(ToolDefinition {
        name: "propose_write".into(),
        description: "Create a reviewable write proposal for the current chat connection. This NEVER executes SQL. The user must approve the exact SQL in the UI before a separate one-time apply can run. Use for INSERT/UPDATE/DELETE/DDL requested by the user; give the complete exact SQL.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "connection_id": { "type": "string", "description": "The current session connection ID" },
                "database": { "type": "string", "description": "Target database (defaults to the connection database)" },
                "sql": { "type": "string", "description": "The complete exact SQL to propose; it will not run until the user allows it" },
                "previous_sql": { "type": "string", "description": "Optional current editor SQL, used only to show a review diff" }
            },
            "required": ["connection_id", "sql"]
        }),
    });
    tools.push(ToolDefinition {
        name: "sample_rows".into(),
        description: "Read a small sample of rows from one table on the current chat connection. Use this to inspect real values without writing SQL.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "connection_id": { "type": "string", "description": "The current session connection ID" },
                "database": { "type": "string", "description": "Database name (optional for some database types)" },
                "schema": { "type": "string", "description": "Schema name (optional)" },
                "table": { "type": "string", "description": "Table name to sample" },
                "limit": { "type": "integer", "description": "Max rows to return (defaults to the safety gate sample limit, max 100)", "minimum": 1, "maximum": 100 }
            },
            "required": ["connection_id", "table"]
        }),
    });
    tools
}

pub fn is_db_tool(name: &str) -> bool {
    matches!(
        name,
        "list_connections"
            | "list_databases"
            | "list_tables"
            | "search_tables"
            | "get_table_schema"
            | "query_readonly"
            | "propose_write"
            | "sample_rows"
    )
}

/// Apply the DB tool permission policy at the provider tool-face boundary.
///
/// - `Disabled`: no DB tools at all (chat still keeps `ask_questions`).
/// - `ReadOnly`: only tools that cannot write. Today every DB tool is
///   read-only, but the filter is deliberate so a future write tool cannot
///   leak into this policy.
/// - `RequireConfirm` / `Unrestricted`: expose the full mode-appropriate set;
///   confirmation is enforced by the tool loop.
pub fn filter_db_tool_definitions(
    tools: Vec<ToolDefinition>,
    policy: AiToolPermissionPolicy,
) -> Vec<ToolDefinition> {
    match policy {
        AiToolPermissionPolicy::Disabled => Vec::new(),
        AiToolPermissionPolicy::ReadOnly => tools
            .into_iter()
            .filter(|tool| is_readonly_db_tool(&tool.name))
            .collect(),
        AiToolPermissionPolicy::RequireConfirm | AiToolPermissionPolicy::Unrestricted => tools,
    }
}

/// Whether a DB tool must pass through the host confirmation port before it
/// executes.
///
/// `require_sql_confirm` is intentionally scoped to tools that run model-
/// supplied SQL (`query_readonly` today; future SQL execution tools join this
/// match). `sample_rows` builds a bounded host-side sample query and is not a
/// model SQL execution tool, so it follows only the DB policy.
pub fn db_tool_requires_confirmation(gate: &AiSafetyGateConfig, name: &str) -> bool {
    gate.db_tool_policy == AiToolPermissionPolicy::RequireConfirm
        || (gate.require_sql_confirm && name == "query_readonly")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolKind {
    AskQuestions,
    Db(String),
    Skill(String),
    Mcp {
        server_id: String,
        tool_name: String,
    },
    Plugin {
        plugin_id: String,
        tool_name: String,
    },
    Workflow {
        workflow_id: String,
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
    if crate::ai::skills::tools::is_skill_tool(name) {
        return ToolKind::Skill(name.to_string());
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
    match super::tool_bridge::parse_bridge_tool(name) {
        Some(super::tool_bridge::BridgeToolKind::Plugin {
            plugin_id,
            tool_name,
        }) => ToolKind::Plugin {
            plugin_id,
            tool_name,
        },
        Some(super::tool_bridge::BridgeToolKind::Workflow { workflow_id }) => {
            ToolKind::Workflow { workflow_id }
        }
        None => ToolKind::Unknown,
    }
}

// ─── Tool Loop Guard (FR-05) ───

/// Limits for one streaming tool loop. `Default` keeps the pre-T05 values;
/// Agent chat swaps in [`ToolLoopLimits::agent`] or values from the safety gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolLoopLimits {
    pub max_rounds: usize,
    pub max_total_tokens: usize,
    pub max_duration: std::time::Duration,
    pub per_tool_cap: usize,
    pub per_round_cap: usize,
}

impl ToolLoopLimits {
    pub const DEFAULT_MAX_ROUNDS: usize = 6;
    pub const DEFAULT_MAX_TOTAL_TOKENS: usize = 24_000;
    pub const DEFAULT_MAX_DURATION_SECS: u64 = 180;
    pub const DEFAULT_PER_TOOL_CAP: usize = 2048;
    pub const DEFAULT_PER_ROUND_CAP: usize = 12_288;

    /// Agent chat defaults: longer loop, larger token budget and tool results.
    pub fn agent() -> Self {
        Self {
            max_rounds: 30,
            max_total_tokens: 48_000,
            max_duration: std::time::Duration::from_secs(300),
            per_tool_cap: 8_192,
            per_round_cap: Self::DEFAULT_PER_ROUND_CAP,
        }
    }

    /// Read the configurable values from a safety gate. Zero/invalid values are
    /// clamped to one so a malformed profile cannot disable the guard entirely.
    pub fn from_gate(gate: &crate::ai::domain::types::AiSafetyGateConfig) -> Self {
        Self {
            max_rounds: gate.max_tool_rounds.max(1),
            max_total_tokens: gate.max_tool_tokens.max(1),
            max_duration: std::time::Duration::from_secs(gate.max_tool_duration_secs.max(1)),
            per_tool_cap: gate.max_tool_result_bytes.max(1),
            per_round_cap: Self::DEFAULT_PER_ROUND_CAP,
        }
    }
}

impl Default for ToolLoopLimits {
    fn default() -> Self {
        Self {
            max_rounds: Self::DEFAULT_MAX_ROUNDS,
            max_total_tokens: Self::DEFAULT_MAX_TOTAL_TOKENS,
            max_duration: std::time::Duration::from_secs(Self::DEFAULT_MAX_DURATION_SECS),
            per_tool_cap: Self::DEFAULT_PER_TOOL_CAP,
            per_round_cap: Self::DEFAULT_PER_ROUND_CAP,
        }
    }
}

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
    pub fn new() -> Self {
        Self::with_limits(ToolLoopLimits::default())
    }

    pub fn with_limits(limits: ToolLoopLimits) -> Self {
        Self {
            max_rounds: limits.max_rounds,
            max_total_tokens: limits.max_total_tokens,
            max_duration: limits.max_duration,
            per_tool_cap: limits.per_tool_cap,
            per_round_cap: limits.per_round_cap,
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
            | "query_readonly"
            | "sample_rows"
    )
}

/// Returns `true` if the MCP tool needs user confirmation before execution.
/// Heuristic: tool name contains write/delete/drop/update/create/insert, or
/// the input schema has `x-write: true`.
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

fn allowed_connection_id<'a>(ctx: &AiToolContext, requested: &'a str) -> Result<&'a str, String> {
    let Some(allowed) = ctx.allowed_connection_id.as_deref() else {
        return Err(
            "no active connection bound; read-only query/sample tools require a connection-bound session"
                .into(),
        );
    };
    let requested = requested.trim();
    if requested.is_empty() {
        return Err("connection_id is required".into());
    }
    if requested != allowed {
        return Err(format!(
            "connection_id '{requested}' is not allowed; the current session is bound to '{allowed}'"
        ));
    }
    Ok(requested)
}

fn clamp_tool_limit(raw: Option<u64>, default: u32, max: u32) -> u32 {
    raw.and_then(|value| u32::try_from(value).ok())
        .unwrap_or(default)
        .clamp(1, max)
}

pub async fn execute_db_tool(
    host: &dyn DatabaseToolHost,
    ctx: &AiToolContext,
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
        "query_readonly" => {
            match allowed_connection_id(ctx, args["connection_id"].as_str().unwrap_or("")) {
                Ok(connection_id) => {
                    let sql = args["sql"].as_str().unwrap_or("").trim();
                    if sql.is_empty() {
                        Err("sql is required".into())
                    } else {
                        let limit = clamp_tool_limit(args["limit"].as_u64(), 100, 1000);
                        super::db_tools::query_readonly(cm, connection_id, sql, Some(limit)).await
                    }
                }
                Err(error) => Err(error),
            }
        }
        "propose_write" => {
            match allowed_connection_id(ctx, args["connection_id"].as_str().unwrap_or("")) {
                Ok(connection_id) => {
                    let sql = args["sql"].as_str().unwrap_or("");
                    if sql.trim().is_empty() {
                        Err("sql is required".into())
                    } else {
                        super::db_tools::propose_write(
                            cm,
                            connection_id,
                            args["database"].as_str(),
                            sql,
                            args["previous_sql"].as_str(),
                        )
                        .await
                    }
                }
                Err(error) => Err(error),
            }
        }
        "sample_rows" => {
            match allowed_connection_id(ctx, args["connection_id"].as_str().unwrap_or("")) {
                Ok(connection_id) => {
                    let table = args["table"].as_str().unwrap_or("").trim();
                    if table.is_empty() {
                        Err("table is required".into())
                    } else {
                        let default_limit = ctx.max_sample_rows.max(1);
                        let limit = clamp_tool_limit(args["limit"].as_u64(), default_limit, 100);
                        super::db_tools::sample_rows(
                            cm,
                            connection_id,
                            args["database"].as_str(),
                            args["schema"].as_str(),
                            table,
                            limit,
                        )
                        .await
                    }
                }
                Err(error) => Err(error),
            }
        }
        other => Err(format!("Unknown tool: {other}")),
    };
    result.unwrap_or_else(|e| e)
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
