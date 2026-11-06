//! MCP call scope and approval classification.
//!
//! The connection allowlist is the outer gate. Database and namespace lists
//! narrow it further; empty lists mean "no additional scope" so existing
//! installs keep working. A call is rejected before dispatch when the host can
//! see a target outside the configured scope, or when a restricted scope would
//! make the target unknowable (for example a query with no database argument).

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::execution::domain::permission::{self, McpPermissionMode};
use crate::execution::domain::sql_guard::sql_main_keyword;
use crate::mcp::domain::allowlist;

/// The complete pre-dispatch policy for one MCP tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolPolicy {
    pub permission_mode: McpPermissionMode,
    pub disabled_tools: HashSet<String>,
    pub allowed_connection_ids: Vec<String>,
    pub allowed_databases: Vec<String>,
    pub allowed_namespaces: Vec<String>,
}

impl Default for McpToolPolicy {
    fn default() -> Self {
        Self {
            permission_mode: McpPermissionMode::default(),
            disabled_tools: HashSet::new(),
            allowed_connection_ids: Vec::new(),
            allowed_databases: Vec::new(),
            allowed_namespaces: Vec::new(),
        }
    }
}

/// What the dispatcher knows about an allowed call after policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCallPlan {
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub namespace: Option<String>,
    pub sql: Option<String>,
    pub requires_approval: bool,
}

impl McpToolPolicy {
    /// Check tool permission, connection/database/namespace scope, and return
    /// the exact execution target. No tool body runs before this returns.
    pub fn evaluate(
        &self,
        tool_name: &str,
        arguments: Option<&Map<String, Value>>,
    ) -> Result<McpCallPlan, String> {
        permission::check_tool_call(
            tool_name,
            self.permission_mode,
            &self.disabled_tools,
            arguments,
        )?;

        let connection_id = string_arg(arguments, "connection_id");
        if let Some(connection_id) = connection_id.as_deref() {
            allowlist::ensure_connection_allowed(connection_id, &self.allowed_connection_ids)?;
        } else if requires_connection_arg(tool_name)
            || (!self.allowed_connection_ids.is_empty() && tool_name == "run_workflow")
        {
            return Err(format!(
                "Tool '{tool_name}' requires a connection_id inside the MCP connection scope"
            ));
        }

        let database = string_arg(arguments, "database");
        let namespace =
            string_arg(arguments, "schema").or_else(|| string_arg(arguments, "namespace"));
        let sql = string_arg(arguments, "sql").or_else(|| {
            arguments
                .and_then(|args| args.get("input"))
                .and_then(Value::as_object)
                .and_then(|input| input.get("sql"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });

        self.ensure_scope(
            "database",
            database.as_deref(),
            tool_name,
            &self.allowed_databases,
        )?;
        self.ensure_scope(
            "namespace",
            namespace.as_deref(),
            tool_name,
            &self.allowed_namespaces,
        )?;

        // Tools that can enumerate resources need a verifiable target when a
        // database scope is active. Returning every database would leak names
        // outside the scope, so fail closed before the reader runs.
        if !self.allowed_databases.is_empty() {
            if tool_name == "list_databases" {
                return Err(
                    "MCP database scope is restricted; list_databases is disabled because it would enumerate databases outside the scope"
                        .to_string(),
                );
            }
            if database.is_none() && targets_database(tool_name) && tool_name != "use_session" {
                return Err(format!(
                    "MCP database scope is restricted; tool '{tool_name}' must specify a database inside the scope"
                ));
            }
        }
        if !self.allowed_namespaces.is_empty()
            && namespace.is_none()
            && targets_namespace(tool_name)
            && tool_name != "use_session"
        {
            return Err(format!(
                "MCP namespace scope is restricted; tool '{tool_name}' must specify a schema inside the scope"
            ));
        }

        let command = string_arg(arguments, "command");
        let requires_approval = requires_approval(tool_name, sql.as_deref(), command.as_deref());
        Ok(McpCallPlan {
            connection_id,
            database,
            namespace,
            sql,
            requires_approval,
        })
    }

    /// Scope check for schema resources, which have a database but no tool
    /// arguments and no namespace. Kept separate from `evaluate` so disabling
    /// the `get_schema` tool does not silently disable the resource endpoint.
    pub fn ensure_schema_resource_scope(
        &self,
        database: &str,
        namespace: Option<&str>,
    ) -> Result<(), String> {
        if !self.allowed_databases.is_empty()
            && !self
                .allowed_databases
                .iter()
                .any(|entry| entry.trim() == "*" || entry.trim() == database)
        {
            return Err(format!(
                "database '{database}' is outside the MCP database scope for schema resources"
            ));
        }
        if !self.allowed_namespaces.is_empty() {
            let namespace = namespace.ok_or_else(|| {
                "MCP namespace scope is restricted; the actual schema for this resource is unknown"
                    .to_string()
            })?;
            if !self
                .allowed_namespaces
                .iter()
                .any(|entry| entry.trim() == "*" || entry.trim() == namespace)
            {
                return Err(format!(
                    "namespace '{namespace}' is outside the MCP namespace scope for schema resources"
                ));
            }
        }
        Ok(())
    }

    fn ensure_scope(
        &self,
        label: &str,
        value: Option<&str>,
        tool_name: &str,
        allowed: &[String],
    ) -> Result<(), String> {
        if allowed.is_empty() {
            return Ok(());
        }
        let Some(value) = value else {
            return Ok(());
        };
        if allowed
            .iter()
            .any(|entry| entry.trim() == "*" || entry.trim() == value)
        {
            return Ok(());
        }
        Err(format!(
            "{label} '{value}' is outside the MCP {label} scope for tool '{tool_name}'"
        ))
    }
}

fn requires_connection_arg(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "list_connections" | "list_workflows" | "run_workflow" | "use_session" | "close_session"
    )
}

fn targets_database(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "list_tables"
            | "search_tables"
            | "get_schema"
            | "describe_table"
            | "query"
            | "explain_query"
            | "open_session"
            | "use_session"
            | "run_workflow"
    )
}

fn targets_namespace(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "list_tables" | "search_tables" | "get_schema" | "describe_table" | "open_session"
    )
}

fn string_arg(arguments: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    arguments
        .and_then(|args| args.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Non-read-only or high-risk calls pause for an Allow once/Deny decision.
///
/// The SQL classifier is the same lexical guard used by the permission gate;
/// `EXPLAIN ANALYZE <write>` is treated as a write because some drivers execute
/// the analyzed statement.
pub fn requires_approval(tool_name: &str, sql: Option<&str>, command: Option<&str>) -> bool {
    match tool_name {
        "run_workflow" => true,
        "query" => sql.is_some_and(|sql| !is_read_only_sql(sql)),
        "use_session" => command.is_some_and(|command| !is_read_only_command(command, sql)),
        _ => false,
    }
}

pub fn is_read_only_sql(sql: &str) -> bool {
    let Some(keyword) = sql_main_keyword(sql) else {
        return false;
    };
    if matches!(
        keyword.as_str(),
        "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "VALUES" | "TABLE"
    ) {
        // EXPLAIN ANALYZE can execute its child statement. Treat it as a write
        // when a write verb survives after the leading keyword.
        if keyword == "EXPLAIN" {
            let upper = sql.to_ascii_uppercase();
            return ![
                "INSERT", "UPDATE", "DELETE", "MERGE", "REPLACE", "CREATE", "DROP",
            ]
            .iter()
            .any(|verb| upper.contains(verb));
        }
        return true;
    }
    false
}

pub fn is_read_only_command(command: &str, sql: Option<&str>) -> bool {
    match command {
        "query" | "execute" | "execute_query" => sql.is_some_and(is_read_only_sql),
        "list_databases" | "list_tables" | "get_table_schema" | "get_schema" | "describe_table"
        | "explain" | "explain_query" | "list_schemas" | "list_columns" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(mode: McpPermissionMode) -> McpToolPolicy {
        McpToolPolicy {
            permission_mode: mode,
            disabled_tools: HashSet::new(),
            allowed_connection_ids: vec!["c1".into()],
            allowed_databases: Vec::new(),
            allowed_namespaces: Vec::new(),
        }
    }

    fn args(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn connection_allowlist_is_still_the_outer_gate() {
        let error = policy(McpPermissionMode::SafeWrite)
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "other",
                    "sql": "SELECT 1"
                }))),
            )
            .unwrap_err();
        assert!(error.contains("allowlist"));
    }

    #[test]
    fn database_scope_rejects_outside_target() {
        let mut policy = policy(McpPermissionMode::SafeWrite);
        policy.allowed_databases = vec!["analytics".into()];
        let error = policy
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "database": "billing",
                    "sql": "SELECT 1"
                }))),
            )
            .unwrap_err();
        assert!(error.contains("database scope"));
        assert!(policy
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "database": "analytics",
                    "sql": "SELECT 1"
                })))
            )
            .is_ok());
    }

    #[test]
    fn namespace_scope_rejects_outside_target() {
        let mut policy = policy(McpPermissionMode::SafeWrite);
        policy.allowed_namespaces = vec!["public".into()];
        assert!(policy
            .evaluate(
                "list_tables",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "database": "analytics",
                    "schema": "secret"
                })))
            )
            .unwrap_err()
            .contains("namespace scope"));
    }

    #[test]
    fn missing_database_is_rejected_when_scope_is_active() {
        let mut policy = policy(McpPermissionMode::SafeWrite);
        policy.allowed_databases = vec!["analytics".into()];
        assert!(policy
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "SELECT 1"
                })))
            )
            .unwrap_err()
            .contains("must specify a database"));
    }

    #[test]
    fn read_only_select_needs_no_approval_but_writes_do() {
        assert!(!requires_approval("query", Some("SELECT 1"), None));
        assert!(requires_approval("query", Some("UPDATE t SET x=1"), None));
        assert!(requires_approval("run_workflow", None, None));
        assert!(!requires_approval(
            "use_session",
            Some("SELECT 1"),
            Some("query")
        ));
        assert!(requires_approval(
            "use_session",
            Some("DELETE FROM t"),
            Some("query")
        ));
    }

    #[test]
    fn empty_connection_allowlist_denies_every_connection() {
        let mut policy = policy(McpPermissionMode::SafeWrite);
        policy.allowed_connection_ids.clear();
        assert!(policy
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "SELECT 1"
                })))
            )
            .unwrap_err()
            .contains("deny-all"));
    }

    #[test]
    fn permission_matrix_read_only_write_and_dangerous() {
        let read_only = policy(McpPermissionMode::ReadOnly);
        // Read-only mode does not expose `query` at all; it is the existing
        // permission gate, not the SQL classifier, that rejects it.
        assert!(read_only
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "SELECT 1"
                })))
            )
            .is_err());
        assert!(read_only
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "UPDATE t SET x = 1"
                })))
            )
            .is_err());

        let safe_write = policy(McpPermissionMode::SafeWrite);
        assert!(safe_write
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "SELECT 1"
                })))
            )
            .is_ok());
        assert!(safe_write
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "DELETE FROM t WHERE id = 1"
                })))
            )
            .is_ok());
        assert!(safe_write
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "DROP TABLE t"
                })))
            )
            .is_err());

        let high_risk = policy(McpPermissionMode::HighRiskWrite);
        assert!(high_risk
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "sql": "DROP TABLE t"
                })))
            )
            .is_ok());
    }

    #[test]
    fn disabled_tool_is_rejected_before_scope() {
        let mut policy = policy(McpPermissionMode::SafeWrite);
        policy.disabled_tools.insert("query".into());
        assert!(policy
            .evaluate(
                "query",
                Some(&args(serde_json::json!({
                    "connection_id": "other",
                    "sql": "SELECT 1"
                })))
            )
            .unwrap_err()
            .contains("disabled"));
    }

    #[test]
    fn stateful_session_tools_do_not_require_a_connection_argument() {
        let policy = policy(McpPermissionMode::SafeWrite);
        assert!(policy
            .evaluate(
                "use_session",
                Some(&args(serde_json::json!({
                    "session_id": "s1",
                    "command": "list_tables"
                })))
            )
            .is_ok());
        assert!(policy
            .evaluate(
                "close_session",
                Some(&args(serde_json::json!({"session_id": "s1"})))
            )
            .is_ok());
    }

    #[test]
    fn namespace_scope_without_schema_fails_closed() {
        let mut policy = policy(McpPermissionMode::SafeWrite);
        policy.allowed_namespaces = vec!["public".into()];
        assert!(policy
            .evaluate(
                "list_tables",
                Some(&args(serde_json::json!({
                    "connection_id": "c1",
                    "database": "analytics"
                })))
            )
            .unwrap_err()
            .contains("must specify a schema"));
    }

    #[test]
    fn explain_analyze_write_is_not_read_only() {
        assert!(!is_read_only_sql("EXPLAIN ANALYZE DELETE FROM t"));
        assert!(is_read_only_sql("EXPLAIN SELECT 1"));
    }
}
