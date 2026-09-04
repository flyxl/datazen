//! Runtime bridge between Workflow Command Steps and Driver Commands.

use crate::commands::AppState;
use crate::mcp::permission::McpPermissionMode;
use crate::workflow::WorkflowCommandStep;
use datazen_driver_api::{check_command_access, validate_command_input, CommandResult};

pub fn resolve_connection_id<'a>(
    step: &'a WorkflowCommandStep,
    workflow_connection: Option<&'a str>,
) -> Result<&'a str, String> {
    step.effective_connection(workflow_connection)
        .ok_or_else(|| format!("Command step '{}' requires a database connection", step.id))
}

pub async fn execute_command(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
) -> Result<CommandResult, String> {
    execute_command_with_mode(app_state, step, workflow_connection, None).await
}

pub async fn execute_command_with_mode(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
    permission_mode: Option<McpPermissionMode>,
) -> Result<CommandResult, String> {
    let connection_id = resolve_connection_id(step, workflow_connection)?;
    let (_runtime_id, driver, handle) = app_state
        .connection_manager
        .resolve_session_for_connection(connection_id)
        .await
        .map_err(|e| format!("Failed to connect '{connection_id}': {}", crate::log_redact::redact_secrets_for_log(&e.to_string())))?;

    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == step.command)
        .ok_or_else(|| {
            format!(
                "Unsupported driver command '{}' for connection '{}'",
                step.command, connection_id
            )
        })?;
    if !definition.metadata.workflow {
        return Err(format!(
            "Command '{}' is not available in workflows",
            step.command
        ));
    }
    validate_command_input(&definition, &step.input)?;
    check_command_access(
        &definition,
        crate::commands::access_level_for_mode(permission_mode),
    )?;

    if matches!(definition.id.as_str(), "query" | "execute") {
        if let Some(sql) = step.input.get("sql").and_then(|v| v.as_str()) {
            let read_only = app_state
                .connection_manager
                .get_session_config(&handle.id)
                .await
                .map(|c| c.read_only)
                .unwrap_or(false);
            let safe_mode = app_state.store.get_settings().await.safe_mode;
            crate::sql_guard::check_sql(sql, read_only, safe_mode)?;
        }
        if let Some(mode) = permission_mode {
            if let Some(sql) = step.input.get("sql").and_then(|v| v.as_str()) {
                crate::mcp::permission::check_sql_allowed(sql, mode)?;
            }
        }
    }

    // Legacy SQL workflows can select a database per step. The generic Command
    // API deliberately does not know about SQL session state, so this adapter
    // applies the optional database field before dispatching the command.
    if let Some(database) = step.input.get("database").and_then(|v| v.as_str()) {
        if !database.is_empty() {
            driver
                .use_database(&handle, database)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    driver
        .execute_command(&handle, &step.command, step.input.clone())
        .await
        .map_err(|e| e.to_string())
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
        assert!(error.contains("query"));
        assert!(error.contains("requires a database connection"));
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
        assert!(error.contains("not allowed"));
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
        assert!(error.contains("WHERE"));
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
        assert!(error.contains("read-only"));
    }
}
