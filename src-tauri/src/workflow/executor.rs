pub use datazen_feature_core::workflow::executor::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::{
        MergeSource, StepStatus, TransformColumn, WorkflowDefinition, WorkflowStep,
    };

    #[tokio::test]
    async fn workflow_default_connection_is_inherited_by_legacy_query() {
        let test = crate::testing::app_state::TestAppState::with_resource_provider(
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
        .await;
        test.save_connection("wf-default").await;
        let workflow = WorkflowDefinition {
            database: None,
            id: "inherit".into(),
            name: "Inherit".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: Some("wf-default".into()),
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.steps[0].step_type, "query");
        assert_eq!(result.steps[0].status, StepStatus::Success);
    }

    #[tokio::test]
    async fn step_connection_override_executes_command() {
        let test = crate::testing::app_state::TestAppState::with_resource_provider(
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
        .await;
        test.save_connection("wf-default").await;
        test.save_connection("wf-override").await;
        let workflow = WorkflowDefinition {
            database: None,
            id: "override".into(),
            name: "Override".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: Some("wf-default".into()),
            steps: vec![WorkflowStep::Command {
                id: "c1".into(),
                command: "query".into(),
                connection: Some("wf-override".into()),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.steps[0].step_type, "query");
    }

    #[tokio::test]
    async fn merge_and_transform_steps_execute_without_db() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let workflow = WorkflowDefinition {
            database: None,
            id: "data-step".into(),
            name: "DataStep".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: None,
            steps: vec![
                WorkflowStep::Transform {
                    id: "t1".into(),
                    from: "[{\"a\":\"x\",\"amount\":10},{\"a\":\"y\",\"amount\":30}]".into(),
                    add_columns: vec![TransformColumn {
                        name: "taxed".into(),
                        expr: "amount * 1.1".into(),
                    }],
                    filter: Some("amount > 5".into()),
                    sort_by: Some("-amount".into()),
                    offset: None,
                    limit: None,
                    timeout_secs: None,
                    on_error: None,
                },
                WorkflowStep::Merge {
                    id: "m1".into(),
                    sources: vec![
                        MergeSource {
                            source: "steps.t1.rows".into(),
                            columns: serde_json::Map::new(),
                            add: serde_json::json!({"src":"CALC"})
                                .as_object()
                                .cloned()
                                .unwrap(),
                        },
                        MergeSource {
                            source: "[{\"a\":\"z\",\"amount\":1}]".into(),
                            columns: serde_json::Map::new(),
                            add: serde_json::json!({"src":"LIT"})
                                .as_object()
                                .cloned()
                                .unwrap(),
                        },
                    ],
                    columns: None,
                    timeout_secs: None,
                    on_error: None,
                },
            ],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        // transform t1: rows y(30) then x(10), each with taxed column.
        assert_eq!(result.steps[0].step_type, "transform");
        let t1 = result.steps[0].result.clone().unwrap();
        assert_eq!(t1["rows"][0]["amount"], 30);
        // merge m1: concat t1 (2 rows) + literal (1 row) = 3 rows, each with src.
        assert_eq!(result.steps[1].step_type, "merge");
        let m1 = result.steps[1].result.clone().unwrap();
        assert_eq!(m1["rows_count"], 3);
        assert_eq!(m1["rows"][2]["src"], "LIT");
    }
}

#[cfg(test)]
mod database_required_tests {
    use super::database_is_required;

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
}
