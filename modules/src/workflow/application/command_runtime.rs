//! Shared Command execution rules for Workflow steps.
//!
//! The rules that decide *which* connection a command step runs against and
//! *what* its input must look like live here, so every host agrees on them.
//! The host supplies only the execution binding ([`ResourceExecutionHost`]):
//! profile lookup, driver lookup, safe mode and target preparation.

use super::resource_execution::{self, ResourceExecutionHost};
use crate::mcp::domain::permission::McpPermissionMode;
use crate::workflow::domain::command::WorkflowCommandStep;
use crate::workflow::domain::error::WorkflowError;
use datazen_driver_api::CommandResult;

/// Decide whether a SQL step must name a database before it can run.
///
/// True only when all three hold: the step (and workflow) resolved nothing,
/// the connection pins no default of its own, and the driver can address more
/// than one database. A single-database driver has nothing to disambiguate, so
/// omitting the value stays legal for it.
pub fn database_is_required(
    resolved_database: Option<&str>,
    connection_database: Option<&str>,
    driver_has_multi_database: bool,
) -> bool {
    let named = |v: Option<&str>| v.map(str::trim).filter(|s| !s.is_empty()).is_some();
    !named(resolved_database) && !named(connection_database) && driver_has_multi_database
}

pub fn resolve_connection_id<'a>(
    step: &'a WorkflowCommandStep,
    workflow_connection: Option<&'a str>,
) -> Result<&'a str, WorkflowError> {
    step.effective_connection(workflow_connection)
        .ok_or_else(|| WorkflowError::MissingConnection {
            step_id: step.id.clone(),
        })
}

/// Let a command step inherit the workflow-level default database.
///
/// A `Command` step has no `database` field of its own (`WorkflowStep::Command`
/// in `model.rs`); its execution target travels inside `input.database`, which
/// `command_runtime` later reads. So the workflow default has to be injected
/// here to give command steps the same inheritance a query step already gets
/// from `step_database.or(inherited_database)` in `executor.rs`.
///
/// A non-empty explicit `input.database` wins, including malformed values that
/// must be rejected by execution validation rather than silently replaced.
pub fn inject_inherited_database(
    mut input: serde_json::Value,
    workflow_database: &str,
) -> serde_json::Value {
    if !input.is_object() {
        return input;
    }
    let should_inherit = match input.get("database") {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(database)) => database.trim().is_empty(),
        // Keep malformed, non-string values intact so the normal input
        // validator can reject them instead of quietly choosing another DB.
        Some(_) => false,
    };
    if !should_inherit {
        return input;
    }
    // `inject_sql_target_fields` already filters blank/empty values, so a
    // whitespace-only workflow default is a no-op here.
    if let Some(database) = Some(workflow_database)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        input["database"] = serde_json::Value::String(database.into());
    }
    input
}

/// Validate explicit namespace input before resolving or opening a resource.
///
/// Null/absent fields inherit the workflow policy; malformed explicit fields
/// must not silently execute against the connection's configured namespace.
pub fn validate_explicit_namespace_input(step: &WorkflowCommandStep) -> Result<(), WorkflowError> {
    for field in ["database", "schema"] {
        if let Some(value) = step.input.get(field) {
            if !value.is_null() && !value.as_str().is_some_and(|name| !name.trim().is_empty()) {
                return Err(WorkflowError::Validation(format!(
                    "explicit {field} must be a non-empty string"
                )));
            }
        }
    }
    Ok(())
}

/// Run one command step end to end.
///
/// The chain is deliberately host-agnostic: validate the explicit namespace,
/// resolve the effective connection, then dispatch the command as a *workflow*
/// command (commands that opt out of `metadata.workflow` are rejected). The
/// host binds the execution primitives through [`ResourceExecutionHost`].
pub async fn execute_command(
    state: &dyn ResourceExecutionHost,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
    permission_mode: Option<McpPermissionMode>,
    deadline: Option<tokio::time::Instant>,
) -> Result<CommandResult, WorkflowError> {
    validate_explicit_namespace_input(step)?;
    let connection_id = resolve_connection_id(step, workflow_connection)?;
    resource_execution::execute(state, step, connection_id, permission_mode, deadline, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_step_connection_wins() {
        let step = WorkflowCommandStep::new(
            "aggregate",
            "aggregate",
            Some("mongo-prod".into()),
            serde_json::json!({}),
        );
        assert_eq!(
            resolve_connection_id(&step, Some("mysql-prod")).unwrap(),
            "mongo-prod"
        );
    }

    #[test]
    fn workflow_connection_is_used_when_step_has_none() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1"}),
        );
        assert_eq!(
            resolve_connection_id(&step, Some("mysql-prod")).unwrap(),
            "mysql-prod"
        );
    }

    #[test]
    fn missing_connection_is_a_clear_workflow_error() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1"}),
        );
        let error = resolve_connection_id(&step, None).unwrap_err();
        assert!(matches!(error, WorkflowError::MissingConnection { .. }));
        assert!(error.to_string().contains("query"));
    }

    #[test]
    fn a_blank_explicit_namespace_is_rejected() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1", "database": "   "}),
        );
        let error = validate_explicit_namespace_input(&step).unwrap_err();
        assert!(error.to_string().contains("explicit database"));
    }

    #[test]
    fn a_non_string_explicit_namespace_is_rejected() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1", "schema": 7}),
        );
        let error = validate_explicit_namespace_input(&step).unwrap_err();
        assert!(error.to_string().contains("explicit schema"));
    }

    #[test]
    fn an_absent_or_null_namespace_is_accepted() {
        for input in [
            serde_json::json!({"sql": "SELECT 1"}),
            serde_json::json!({"sql": "SELECT 1", "database": null}),
            serde_json::json!({"sql": "SELECT 1", "schema": null}),
        ] {
            let step = WorkflowCommandStep::new("query", "query", None, input);
            assert!(validate_explicit_namespace_input(&step).is_ok());
        }
    }

    #[test]
    fn an_explicit_namespace_is_accepted() {
        let step = WorkflowCommandStep::new(
            "query",
            "query",
            None,
            serde_json::json!({"sql": "SELECT 1", "database": "reporting"}),
        );
        assert!(validate_explicit_namespace_input(&step).is_ok());
    }
}

#[cfg(test)]
mod inherit_tests {
    use super::inject_inherited_database;
    use serde_json::json;

    #[test]
    fn injects_workflow_database_when_step_omits_it() {
        let out = inject_inherited_database(json!({ "limit": 10 }), "datazen_demo");
        assert_eq!(out["database"], json!("datazen_demo"));
        assert_eq!(out["limit"], json!(10));
    }

    #[test]
    fn explicit_step_database_wins() {
        let out = inject_inherited_database(json!({ "database": "other_db" }), "datazen_demo");
        assert_eq!(out["database"], json!("other_db"));
    }

    #[test]
    fn blank_step_database_is_treated_as_unset() {
        let out = inject_inherited_database(json!({ "database": "   " }), "datazen_demo");
        assert_eq!(out["database"], json!("datazen_demo"));
    }

    #[test]
    fn blank_workflow_database_is_a_noop() {
        let out = inject_inherited_database(json!({ "limit": 1 }), "   ");
        assert!(out.get("database").is_none());
    }

    #[test]
    fn workflow_database_is_trimmed() {
        let out = inject_inherited_database(json!({}), "  datazen_demo  ");
        assert_eq!(out["database"], json!("datazen_demo"));
    }

    #[test]
    fn non_object_input_is_left_alone() {
        // `inject_sql_target_fields` bails on non-objects; make sure we do not
        // panic or corrupt the payload.
        let out = inject_inherited_database(json!("just a string"), "datazen_demo");
        assert_eq!(out, json!("just a string"));
    }
}
