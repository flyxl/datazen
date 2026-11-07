//! Tests for the shared executor's pure decision rules.
//!
//! These rules live in `application/command_runtime.rs` (D6 item 3 切片 b
//! 起从 executor 随「共享命令规则」归位)；the cases are kept in a sibling
//! file so the engine stays readable (the same split `ai/safety.rs` uses).

use std::sync::Mutex;

use crate::sql_template::SqlTemplateDialect;
use crate::workflow::application::command_runtime::database_is_required;
use crate::workflow::application::host::WorkflowHost;
use crate::workflow::domain::command::WorkflowCommandStep;
use crate::workflow::domain::model::{
    WorkflowDefinition, WorkflowStep, WorkflowVariable, WorkflowVisibility,
};
use crate::workflow::engine::executor::WorkflowExecutor;
use crate::workflow::WorkflowError;
use datazen_driver_api::CommandResult;

#[test]
fn requires_a_database_for_an_unpinned_multi_db_connection() {
    // The reported failure: a PostgreSQL connection with no database, and a
    // step that names none. PG would fall back to its hardcoded "postgres".
    assert!(database_is_required(None, None, true));
}

#[test]
fn a_step_database_satisfies_the_rule() {
    assert!(!database_is_required(Some("datazen_demo"), None, true));
}

#[test]
fn a_connection_default_satisfies_the_rule() {
    assert!(!database_is_required(None, Some("datazen_demo"), true));
}

#[test]
fn a_single_database_driver_never_requires_one() {
    // Keeps existing workflows that legitimately omit the value working.
    assert!(!database_is_required(None, None, false));
}

#[test]
fn blank_values_count_as_unset() {
    assert!(database_is_required(Some("   "), Some("\t"), true));
    assert!(!database_is_required(Some(" real_db "), None, true));
    assert!(!database_is_required(None, Some(" real_db "), true));
}

struct CapturingHost {
    commands: Mutex<Vec<WorkflowCommandStep>>,
    prompts: Mutex<Vec<String>>,
    dialect: SqlTemplateDialect,
}

impl Default for CapturingHost {
    fn default() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
            prompts: Mutex::new(Vec::new()),
            dialect: SqlTemplateDialect::Standard,
        }
    }
}

#[async_trait::async_trait]
impl WorkflowHost for CapturingHost {
    async fn connection_name(&self, id: &str) -> Result<String, WorkflowError> {
        Ok(id.into())
    }

    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        _workflow_connection: Option<&str>,
        _permission_mode: Option<crate::execution::domain::permission::McpPermissionMode>,
        _deadline: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError> {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(step.clone());
        }
        Ok(CommandResult::new(serde_json::json!({
            "results": [{
                "sql": "",
                "columns": [{"name": "name", "dataType": "text", "nullable": false}],
                "rows": [["O'Brien"]],
                "rowsAffected": null,
                "executionTimeMs": 0,
                "truncated": false
            }],
            "totalTimeMs": 0
        })))
    }

    async fn complete_ai(&self, prompt: String) -> Result<String, WorkflowError> {
        if let Ok(mut prompts) = self.prompts.lock() {
            prompts.push(prompt);
        }
        Ok("unused".into())
    }

    async fn sql_dialect(&self, _connection_id: &str) -> SqlTemplateDialect {
        self.dialect
    }
}

fn workflow_with(steps: Vec<WorkflowStep>, variables: Vec<WorkflowVariable>) -> WorkflowDefinition {
    WorkflowDefinition {
        id: "wf-sql".into(),
        name: "wf-sql".into(),
        description: String::new(),
        version: None,
        author: None,
        variables,
        connection: Some("conn".into()),
        database: None,
        steps,
        blocks: Vec::new(),
        output: None,
        timeout_secs: None,
        error_handling: None,
        schedule: None,
        visibility: WorkflowVisibility::User,
    }
}

fn query(id: &str, sql: &str) -> WorkflowStep {
    WorkflowStep::Query {
        id: id.into(),
        sql: sql.into(),
        connection: None,
        database: None,
        timeout_secs: None,
        on_error: None,
    }
}

fn variable(name: &str) -> WorkflowVariable {
    WorkflowVariable {
        name: name.into(),
        var_type: "string".into(),
        description: String::new(),
        required: Some(false),
        default: None,
    }
}

fn captured_sql(host: &CapturingHost) -> String {
    host.commands
        .lock()
        .unwrap()
        .first()
        .and_then(|step| step.input.get("sql"))
        .and_then(|sql| sql.as_str())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn sql_template_escapes_strings_and_keeps_numbers_typed() {
    let host = CapturingHost::default();
    let workflow = workflow_with(
        vec![query(
            "q1",
            "SELECT * FROM users WHERE name = '{{name}}' LIMIT {{limit}}",
        )],
        vec![variable("name"), variable("limit")],
    );
    let result = WorkflowExecutor::execute(
        &workflow,
        &host,
        None,
        &serde_json::json!({"name": "O'Brien", "limit": 5}),
    )
    .await
    .unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        captured_sql(&host),
        "SELECT * FROM users WHERE name = 'O''Brien' LIMIT 5"
    );
}

#[tokio::test]
async fn sql_template_rejects_injection_outside_a_string_before_dispatch() {
    let host = CapturingHost::default();
    let workflow = workflow_with(
        vec![query("q1", "SELECT * FROM users WHERE name = {{name}}")],
        vec![variable("name")],
    );
    let result = WorkflowExecutor::execute(
        &workflow,
        &host,
        None,
        &serde_json::json!({"name": "x' OR 1=1 --"}),
    )
    .await
    .unwrap();

    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("single-quoted SQL literal"),
        "unexpected error: {error:?}"
    );
    assert!(
        host.commands.lock().unwrap().is_empty(),
        "SQL was dispatched"
    );
}

#[tokio::test]
async fn sql_template_renders_steps_and_loop_values_with_their_types() {
    let host = CapturingHost::default();
    let workflow = workflow_with(
        vec![
            query("q1", "SELECT 'O''Brien' AS name"),
            query("q2", "SELECT '{{steps.q1.rows[0].name}}' AS echoed"),
            WorkflowStep::ForEach {
                id: "loop".into(),
                items: "{{items}}".into(),
                as_var: "item".into(),
                steps: vec![query("inner", "SELECT '{{item.name}}', {{item.id}}")],
                max_iterations: None,
            },
        ],
        vec![variable("items")],
    );
    let result = WorkflowExecutor::execute(
        &workflow,
        &host,
        None,
        &serde_json::json!({"items": [{"id": 3, "name": "O'Brien"}]}),
    )
    .await
    .unwrap();

    assert!(result.success, "{:?}", result.error);
    let commands = host.commands.lock().unwrap();
    let sqls: Vec<&str> = commands
        .iter()
        .filter_map(|command| command.input.get("sql").and_then(|sql| sql.as_str()))
        .collect();
    assert!(
        sqls.iter().any(|sql| sql.contains("'O''Brien'")),
        "{sqls:?}"
    );
    assert!(
        sqls.iter().any(|sql| sql == &"SELECT 'O''Brien', 3"),
        "{sqls:?}"
    );
}

#[tokio::test]
async fn sql_template_uses_the_target_connection_backslash_dialect() {
    let host = CapturingHost {
        commands: Mutex::new(Vec::new()),
        prompts: Mutex::new(Vec::new()),
        dialect: SqlTemplateDialect::BackslashEscapes,
    };
    let workflow = workflow_with(
        vec![query("q1", "SELECT '{{name}}' AS value")],
        vec![variable("name")],
    );
    let result =
        WorkflowExecutor::execute(&workflow, &host, None, &serde_json::json!({"name": "a\\b"}))
            .await
            .unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(captured_sql(&host), "SELECT 'a\\\\b' AS value");
}

#[tokio::test]
async fn non_sql_fields_keep_the_legacy_raw_substitution() {
    let host = CapturingHost::default();
    let workflow = workflow_with(
        vec![WorkflowStep::Ai {
            id: "ask".into(),
            prompt: "name={{name}}".into(),
            timeout_secs: None,
            on_error: None,
        }],
        vec![variable("name")],
    );
    let result = WorkflowExecutor::execute(
        &workflow,
        &host,
        None,
        &serde_json::json!({"name": "O'Brien"}),
    )
    .await
    .unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        host.prompts.lock().unwrap().as_slice(),
        &["name=O'Brien".to_string()]
    );
}
