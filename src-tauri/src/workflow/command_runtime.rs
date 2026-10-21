//! Runtime bridge between Workflow Command Steps and Driver Commands.

use crate::commands::AppState;
use crate::mcp::permission::McpPermissionMode;
use crate::workflow::error::WorkflowError;
use crate::workflow::WorkflowCommandStep;
use datazen_driver_api::CommandResult;

pub use datazen_feature_core::workflow::command_runtime::{
    inject_inherited_database, resolve_connection_id,
};

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
    super::independent_sql::execute(
        app_state,
        step,
        connection_id,
        permission_mode,
        workflow_deadline,
    )
    .await
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
        let test = crate::testing::app_state::TestAppState::with_resource_provider(
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
        .await;
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
        let test = crate::testing::app_state::TestAppState::with_resource_provider(
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
        .await;
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
