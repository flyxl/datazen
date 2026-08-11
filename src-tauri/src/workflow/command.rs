//! Canonical workflow data-operation command model.
//!
//! This module intentionally stays independent from the workflow executor. It
//! provides the normalized representation used when migrating legacy `query`
//! steps to the generic Driver Command model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCommandStep {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub on_error: Option<super::workflows::ErrorHandlingConfig>,
}

impl WorkflowCommandStep {
    pub fn new(
        id: impl Into<String>,
        command: impl Into<String>,
        connection: Option<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            connection,
            input,
            timeout_secs: None,
            on_error: None,
        }
    }

    /// Normalize the existing workflow `query` representation without changing
    /// the on-disk YAML. This is the compatibility boundary between the old
    /// configuration format and the generic command runtime.
    ///
    /// Legacy `database` selection is preserved as a command input so the
    /// migration does not silently lose the existing per-step database
    /// semantics. The workflow executor can apply it before dispatching the
    /// command through the Driver API.
    pub fn from_legacy_query(
        id: impl Into<String>,
        sql: impl Into<String>,
        connection: Option<String>,
        database: Option<String>,
        timeout_secs: Option<u64>,
        on_error: Option<super::workflows::ErrorHandlingConfig>,
    ) -> Self {
        let mut input = serde_json::json!({ "sql": sql.into() });
        if let Some(database) = database {
            input["database"] = serde_json::Value::String(database);
        }

        Self {
            id: id.into(),
            command: "query".into(),
            connection,
            input,
            timeout_secs,
            on_error,
        }
    }

    /// Resolve the connection used by this step.
    pub fn effective_connection<'a>(
        &'a self,
        workflow_connection: Option<&'a str>,
    ) -> Option<&'a str> {
        self.connection.as_deref().or(workflow_connection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_query_normalizes_to_query_command() {
        let step = WorkflowCommandStep::from_legacy_query(
            "users",
            "SELECT id FROM users",
            Some("mysql-prod".into()),
            None,
            Some(10),
            None,
        );

        assert_eq!(step.id, "users");
        assert_eq!(step.command, "query");
        assert_eq!(step.connection.as_deref(), Some("mysql-prod"));
        assert_eq!(step.input["sql"], "SELECT id FROM users");
        assert_eq!(step.input.get("database"), None);
        assert_eq!(step.timeout_secs, Some(10));
    }

    #[test]
    fn legacy_query_preserves_database_selection() {
        let step = WorkflowCommandStep::from_legacy_query(
            "users",
            "SELECT id FROM users",
            Some("mysql-prod".into()),
            Some("reporting".into()),
            None,
            None,
        );

        assert_eq!(step.command, "query");
        assert_eq!(step.input["database"], "reporting");
        assert_eq!(step.input["sql"], "SELECT id FROM users");
    }

    #[test]
    fn explicit_connection_overrides_workflow_connection() {
        let step = WorkflowCommandStep::new(
            "aggregate",
            "aggregate",
            Some("mongo-prod".into()),
            serde_json::json!({"collection": "orders"}),
        );

        assert_eq!(
            step.effective_connection(Some("mysql-prod")),
            Some("mongo-prod")
        );
    }

    #[test]
    fn absent_connection_inherits_workflow_connection() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1"}),
        );

        assert_eq!(
            step.effective_connection(Some("mysql-prod")),
            Some("mysql-prod")
        );
    }

    #[test]
    fn no_connection_remains_unresolved() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1"}),
        );

        assert_eq!(step.effective_connection(None), None);
    }

    #[test]
    fn command_step_serializes_with_camel_case_fields() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            Some("mysql-prod".into()),
            serde_json::json!({"sql": "SELECT 1"}),
        );

        let value = serde_json::to_value(step).unwrap();
        assert_eq!(value["connection"], "mysql-prod");
        assert_eq!(value["command"], "query");
        assert_eq!(value["input"]["sql"], "SELECT 1");
        assert!(value.get("connection_id").is_none());
    }
}
