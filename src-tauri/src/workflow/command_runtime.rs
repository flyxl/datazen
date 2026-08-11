//! Runtime bridge between Workflow Command Steps and Driver Commands.
//!
//! The workflow executor uses this module as the only database operation entry
//! point. Driver-specific behavior remains behind `DatabaseDriver::execute_command`.

use crate::commands::AppState;
use crate::workflow::WorkflowCommandStep;
use datazen_driver_api::CommandResult;

pub fn resolve_connection_id<'a>(
    step: &'a WorkflowCommandStep,
    workflow_connection: Option<&'a str>,
) -> Result<&'a str, String> {
    step.effective_connection(workflow_connection)
        .ok_or_else(|| {
            format!(
                "Command step '{}' requires a database connection",
                step.id
            )
        })
}

pub async fn execute_command(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
) -> Result<CommandResult, String> {
    let connection_id = resolve_connection_id(step, workflow_connection)?;
    let (driver, handle) = app_state
        .connection_manager
        .resolve_session(connection_id)
        .await
        .map_err(|e| format!("Failed to connect '{connection_id}': {e}"))?;

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

    let _permissions = definition.permissions;

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
}
