//! Runtime bridge between Workflow Command Steps and Driver Commands.

use crate::commands::AppState;
use crate::mcp::permission::McpPermissionMode;
use crate::workflow::error::WorkflowError;
use crate::workflow::WorkflowCommandStep;
use datazen_driver_api::{check_command_access, validate_command_input, CommandResult};

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
    crate::commands::driver_command::inject_sql_target_fields(
        &mut input,
        Some(workflow_database),
        None,
    );
    input
}

pub async fn execute_command(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
) -> Result<CommandResult, WorkflowError> {
    execute_command_with_mode(app_state, step, workflow_connection, None).await
}

pub async fn execute_command_with_mode(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
    permission_mode: Option<McpPermissionMode>,
) -> Result<CommandResult, WorkflowError> {
    execute_command_with_deadline(app_state, step, workflow_connection, permission_mode, None).await
}

pub(super) async fn execute_command_with_deadline(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
    permission_mode: Option<McpPermissionMode>,
    workflow_deadline: Option<tokio::time::Instant>,
) -> Result<CommandResult, WorkflowError> {
    // Validate explicit namespace input before resolving or opening a resource.
    // Null/absent fields inherit the workflow policy; malformed explicit fields
    // must not silently execute against the connection's configured namespace.
    for field in ["database", "schema"] {
        if let Some(value) = step.input.get(field) {
            if !value.is_null() && !value.as_str().is_some_and(|name| !name.trim().is_empty()) {
                return Err(WorkflowError::Validation(format!(
                    "explicit {field} must be a non-empty string"
                )));
            }
        }
    }
    let connection_id = resolve_connection_id(step, workflow_connection)?;
    if matches!(step.command.as_str(), "query" | "execute") {
        return super::independent_sql::execute(
            app_state,
            step,
            connection_id,
            permission_mode,
            workflow_deadline,
        )
        .await;
    }
    let (_runtime_id, driver, handle) = app_state
        .connection_manager
        .resolve_session_for_connection(connection_id)
        .await
        .map_err(|e| WorkflowError::ConnectionFailed {
            connection_id: connection_id.to_string(),
            message: crate::log_redact::redact_secrets_for_log(&e.to_string()),
        })?;

    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == step.command)
        .ok_or_else(|| WorkflowError::UnsupportedCommand {
            command: step.command.clone(),
            connection_id: connection_id.to_string(),
        })?;
    if !definition.metadata.workflow {
        return Err(WorkflowError::CommandNotInWorkflow {
            command: step.command.clone(),
        });
    }
    validate_command_input(&definition, &step.input).map_err(|e| WorkflowError::Validation(e))?;
    check_command_access(
        &definition,
        crate::commands::access_level_for_mode(permission_mode),
    )
    .map_err(|e| WorkflowError::Validation(e))?;

    // Non-SQL driver commands retain their explicit namespace in the input.
    let mut input = step.input.clone();
    let database = input
        .get("database")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let schema = input
        .get("schema")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    crate::commands::driver_command::inject_sql_target_fields(
        &mut input,
        database.as_deref(),
        schema.as_deref(),
    );

    driver
        .execute_command(&handle, &step.command, input)
        .await
        .map_err(|e| WorkflowError::Driver(e.to_string()))
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

    #[tokio::test]
    async fn inherited_connection_executes_query_command() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("wf-inherit").await;
        let step = WorkflowCommandStep::new(
            "q1",
            "query",
            None,
            serde_json::json!({ "sql": "SELECT 1" }),
        );
        let result = execute_command(&test.state, &step, Some("wf-inherit"))
            .await
            .unwrap();
        assert!(result.data.is_object());
    }

    #[tokio::test]
    async fn legacy_query_normalization_executes_through_command_runtime() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("wf-legacy").await;
        let step = WorkflowCommandStep::from_legacy_query(
            "users",
            "SELECT id FROM users",
            Some("wf-legacy".into()),
            None,
            None,
            None,
        );
        let result = execute_command(&test.state, &step, None).await.unwrap();
        assert!(result.data.is_object());
    }

    #[tokio::test]
    async fn read_only_mode_rejects_execute_command() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("wf-ro").await;
        let step = WorkflowCommandStep::new(
            "e1",
            "execute",
            None,
            serde_json::json!({ "sql": "DELETE FROM t" }),
        );
        let error = execute_command_with_mode(
            &test.state,
            &step,
            Some("wf-ro"),
            Some(crate::mcp::permission::McpPermissionMode::ReadOnly),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn safe_mode_blocks_workflow_update_without_where() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("wf-safe").await;
        let step = WorkflowCommandStep::new(
            "u1",
            "query",
            None,
            serde_json::json!({ "sql": "UPDATE t SET x = 1" }),
        );
        let error = execute_command(&test.state, &step, Some("wf-safe"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("WHERE"));
    }

    #[tokio::test]
    async fn connection_read_only_blocks_workflow_writes() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let mut config = crate::testing::app_state::sample_postgres_config("wf-conn-ro");
        config.read_only = true;
        test.store.save_connection(config).await.unwrap();
        let step = WorkflowCommandStep::new(
            "u1",
            "query",
            None,
            serde_json::json!({ "sql": "DELETE FROM t WHERE id = 1" }),
        );
        let error = execute_command(&test.state, &step, Some("wf-conn-ro"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("read-only"));
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
