//! Human-readable MCP tool help appended to tool call errors.

use rmcp::ErrorData as McpError;
use schemars::schema_for;
use serde_json::Value;

use super::server::{
    DescribeTableInput, ExplainQueryInput, GetSchemaInput, ListDatabasesInput, ListTablesInput,
    QueryInput, RunWorkflowInput, SearchTablesInput,
};

/// Static help metadata for a registered MCP tool.
#[derive(Debug, Clone, Copy)]
pub struct ToolHelp {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
    pub example: &'static str,
}

const TOOL_HELPS: &[ToolHelp] = &[
    ToolHelp {
        name: "list_connections",
        description: "List all configured database connections.",
        parameters: "(no parameters)",
        example: r#"{"name":"list_connections","arguments":{}}"#,
    },
    ToolHelp {
        name: "list_databases",
        description: "List all databases on a connected server.",
        parameters: "connection_id (string, required) — persistent connection id from list_connections",
        example: r#"{"name":"list_databases","arguments":{"connection_id":"<uuid>"}}"#,
    },
    ToolHelp {
        name: "list_tables",
        description: "List tables in a database with types and row counts.",
        parameters:
            "connection_id (string, required); database (string, optional) — defaults to connection default",
        example: r#"{"name":"list_tables","arguments":{"connection_id":"<uuid>","database":"app"}}"#,
    },
    ToolHelp {
        name: "search_tables",
        description: "Search tables by name pattern (case-insensitive substring).",
        parameters:
            "connection_id (string, required); pattern (string, required); database (string, optional); limit (number, optional, default 20)",
        example: r#"{"name":"search_tables","arguments":{"connection_id":"<uuid>","pattern":"user","limit":20}}"#,
    },
    ToolHelp {
        name: "query",
        description: "Execute SQL and return JSON rows.",
        parameters:
            "connection_id (string, required); sql (string, required); limit (number, optional, default 100, max 50000)",
        example: r#"{"name":"query","arguments":{"connection_id":"<uuid>","sql":"SELECT 1","limit":100}}"#,
    },
    ToolHelp {
        name: "get_schema",
        description: "Get table schema: columns, PKs, FKs, indexes.",
        parameters: "connection_id (string, required); table (string, required)",
        example: r#"{"name":"get_schema","arguments":{"connection_id":"<uuid>","table":"users"}}"#,
    },
    ToolHelp {
        name: "explain_query",
        description: "Return EXPLAIN plan for a SQL query.",
        parameters: "connection_id (string, required); sql (string, required)",
        example: r#"{"name":"explain_query","arguments":{"connection_id":"<uuid>","sql":"SELECT * FROM users"}}"#,
    },
    ToolHelp {
        name: "describe_table",
        description: "Human-readable table description.",
        parameters: "connection_id (string, required); table (string, required)",
        example: r#"{"name":"describe_table","arguments":{"connection_id":"<uuid>","table":"users"}}"#,
    },
    ToolHelp {
        name: "list_workflows",
        description: "List available user-defined workflows.",
        parameters: "(no parameters)",
        example: r#"{"name":"list_workflows","arguments":{}}"#,
    },
    ToolHelp {
        name: "run_workflow",
        description: "Execute a workflow by id (see list_workflows).",
        parameters:
            "workflow_id (string, required); variables (object, optional); connection_id (string, optional)",
        example: r#"{"name":"run_workflow","arguments":{"workflow_id":"my-flow","variables":{}}}"#,
    },
];

/// Lookup static help for a tool name.
pub fn lookup(tool_name: &str) -> Option<&'static ToolHelp> {
    TOOL_HELPS.iter().find(|h| h.name == tool_name)
}

/// JSON Schema snippet for tool input (when available).
pub fn input_schema_json(tool_name: &str) -> Option<String> {
    let schema = match tool_name {
        "list_databases" => schema_for!(ListDatabasesInput),
        "list_tables" => schema_for!(ListTablesInput),
        "search_tables" => schema_for!(SearchTablesInput),
        "query" => schema_for!(QueryInput),
        "get_schema" => schema_for!(GetSchemaInput),
        "explain_query" => schema_for!(ExplainQueryInput),
        "describe_table" => schema_for!(DescribeTableInput),
        "run_workflow" => schema_for!(RunWorkflowInput),
        "list_connections" | "list_workflows" => return None,
        _ => return None,
    };
    serde_json::to_string_pretty(&schema).ok()
}

/// Format a `--help`-style block for a tool.
pub fn format_help_block(tool_name: &str) -> String {
    let mut out = String::new();
    if let Some(help) = lookup(tool_name) {
        out.push_str(&format!("Tool: {}\n", help.name));
        out.push_str(&format!("Description: {}\n", help.description));
        out.push_str(&format!("Parameters: {}\n", help.parameters));
        if let Some(schema) = input_schema_json(tool_name) {
            out.push_str("\nJSON Schema:\n");
            out.push_str(&schema);
            out.push('\n');
        }
        out.push_str("\nExample:\n");
        out.push_str(help.example);
        out.push('\n');
    } else {
        out.push_str(&format!("Tool: {tool_name}\n"));
        out.push_str("(no help available — check list_tools for registered names)\n");
    }
    out
}

/// Build an invalid-params MCP error with help text.
pub fn tool_error(tool_name: &str, reason: &str) -> McpError {
    let message = format!("{reason}\n\n{}", format_help_block(tool_name));
    McpError::invalid_params(message, Some(Value::String(format_help_block(tool_name))))
}

/// Build help for an unknown or disabled tool name.
pub fn unknown_tool_error(tool_name: &str) -> McpError {
    let mut block = format!("Unknown or disabled tool '{tool_name}'.\n\nRegistered tools:\n");
    for help in TOOL_HELPS {
        block.push_str(&format!("  - {} — {}\n", help.name, help.description));
    }
    block.push_str("\nExample (list_connections):\n");
    block.push_str(lookup("list_connections").unwrap().example);
    block.push('\n');

    let reason = format!("Unknown or disabled tool '{tool_name}'");
    let message = format!("{reason}\n\n{block}");
    McpError::invalid_params(message, Some(Value::String(block)))
}

/// Append help to an existing MCP error when it relates to a tool call.
pub fn enrich_tool_error(tool_name: &str, err: McpError) -> McpError {
    let help = format_help_block(tool_name);
    let message = format!("{}\n\n{}", err.message, help);
    McpError::invalid_params(message, Some(Value::String(help)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_query_help() {
        let help = lookup("query").unwrap();
        assert!(help.description.contains("SQL"));
        assert!(help.example.contains("connection_id"));
    }

    #[test]
    fn format_help_block_contains_sections() {
        let block = format_help_block("query");
        assert!(block.contains("Tool: query"));
        assert!(block.contains("Description:"));
        assert!(block.contains("Parameters:"));
        assert!(block.contains("Example:"));
        assert!(block.contains("JSON Schema:"));
    }

    #[test]
    fn tool_error_includes_reason_and_help() {
        let err = tool_error("query", "missing connection_id");
        assert!(err.message.contains("missing connection_id"));
        assert!(err.message.contains("Tool: query"));
        assert!(err.message.contains("Example:"));
    }

    #[test]
    fn unknown_tool_error_lists_registered_tools() {
        let err = unknown_tool_error("not_a_real_tool");
        assert!(err.message.contains("Unknown or disabled tool"));
        assert!(err.message.contains("list_connections"));
        assert!(err.message.contains("Example"));
    }

    #[test]
    fn enrich_tool_error_appends_help() {
        let base = McpError::invalid_params("bad args", None);
        let enriched = enrich_tool_error("list_tables", base);
        assert!(enriched.message.contains("bad args"));
        assert!(enriched.message.contains("Tool: list_tables"));
    }
}
