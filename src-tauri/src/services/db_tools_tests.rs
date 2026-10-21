#[cfg(test)]
mod tests {
    use crate::testing::{app_state::TestAppState, mock_driver::MockDriverOptions};
    use datazen_driver_api::{TableInfo, TableType};
    use datazen_feature_core::mcp::permission::McpPermissionMode;
    use datazen_feature_core::services::db_tools::*;

    fn options_with_tables(databases: Vec<String>, tables: Vec<TableInfo>) -> MockDriverOptions {
        MockDriverOptions {
            databases,
            tables,
            explain_plan: datazen_driver_api::ExplainResult {
                plan_text: "Seq Scan".into(),
                plan_json: None,
                plan_tree: None,
                total_cost: Some(1.0),
                estimated_rows: Some(100),
            },
            server_version: "PostgreSQL 16".into(),
            count_total: 42,
            query_rows: vec![vec![Some(datazen_driver_api::Value::Integer(1))]],
            ..Default::default()
        }
    }

    async fn test_stack() -> TestAppState {
        TestAppState::with_resource_provider(options_with_tables(
            vec!["app".into(), "test".into()],
            vec![TableInfo {
                name: "users".into(),
                schema: Some("public".into()),
                table_type: TableType::Table,
                row_count: Some(10),
            }],
        ))
        .await
    }

    #[test]
    fn resolve_query_limit_defaults_and_caps() {
        assert_eq!(resolve_query_limit(None), Some(100));
        assert_eq!(resolve_query_limit(Some(500)), Some(500));
        assert_eq!(resolve_query_limit(Some(999_999)), Some(50_000));
    }

    #[tokio::test]
    async fn list_connections_serializes_saved_configs() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let json = list_connections(&test.store).await.unwrap();
        assert!(json.contains("\"id\": \"c1\""));
        assert!(json.contains("Test c1"));
    }

    #[tokio::test]
    async fn database_tool_host_resolves_driver_and_executes_through_runtime() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let driver =
            datazen_feature_core::services::ports::DatabaseToolHost::driver_for_connection(
                &test.state,
                "c1",
            )
            .await
            .unwrap();
        assert_eq!(driver.driver_type(), "postgres");
        let result = datazen_feature_core::services::ports::DatabaseToolHost::execute_target(
            &test.state,
            "c1",
            "list_databases",
            serde_json::json!({}),
        )
        .await
        .unwrap();
        assert!(result.data.to_string().contains("app"));
    }

    #[tokio::test]
    async fn list_databases_returns_json() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let json = list_databases(&test.state, "c1").await.unwrap();
        assert!(json.contains("app"));
        assert!(json.contains("test"));
    }

    #[tokio::test]
    async fn list_tables_returns_json() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let json = list_tables(&test.state, "c1", Some("app"), None)
            .await
            .unwrap();
        assert!(json.contains("users"));
    }

    #[tokio::test]
    async fn get_table_schema_returns_pretty_json() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let json = get_table_schema(&test.state, "c1", &["users".into()], Some("app"), None)
            .await
            .unwrap();
        assert!(json.contains("users"));
        assert!(json.contains("id"));
    }

    #[tokio::test]
    async fn query_executes_without_permission_gate() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let json = query(&test.state, "c1", "SELECT * FROM users", Some(10), None)
            .await
            .unwrap();
        assert!(json.contains("1"));
        assert!(test.mock.query_calls() >= 1);
    }

    #[tokio::test]
    async fn query_blocks_sql_in_read_only_permission_mode() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let err = query(
            &test.state,
            "c1",
            "SELECT 1",
            None,
            Some(McpPermissionMode::ReadOnly),
        )
        .await
        .unwrap_err();
        assert!(err.contains("read-only"));
    }

    #[tokio::test]
    async fn explain_query_returns_plan_json() {
        let test = test_stack().await;
        test.save_connection("c1").await;
        let json = explain_query(&test.state, "c1", "SELECT 1").await.unwrap();
        assert!(json.contains("Seq Scan"));
    }

    #[tokio::test]
    async fn search_tables_filters_by_pattern() {
        let test = TestAppState::with_resource_provider(options_with_tables(
            vec!["app".into()],
            vec![
                TableInfo {
                    name: "users".into(),
                    schema: Some("public".into()),
                    table_type: TableType::Table,
                    row_count: Some(10),
                },
                TableInfo {
                    name: "user_roles".into(),
                    schema: Some("public".into()),
                    table_type: TableType::Table,
                    row_count: Some(5),
                },
                TableInfo {
                    name: "orders".into(),
                    schema: Some("public".into()),
                    table_type: TableType::Table,
                    row_count: Some(100),
                },
                TableInfo {
                    name: "products".into(),
                    schema: Some("public".into()),
                    table_type: TableType::Table,
                    row_count: Some(50),
                },
            ],
        ))
        .await;
        test.save_connection("c1").await;

        let json = search_tables(&test.state, "c1", Some("app"), None, "user", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(2));
        assert_eq!(parsed["totalTables"].as_u64(), Some(4));
        assert_eq!(parsed["pattern"].as_str(), Some("user"));
        let matched = parsed["matched"].as_array().unwrap();
        assert_eq!(matched.len(), 2);

        let json_limited = search_tables(&test.state, "c1", Some("app"), None, "user", 1)
            .await
            .unwrap();
        let parsed_limited: serde_json::Value = serde_json::from_str(&json_limited).unwrap();
        assert_eq!(parsed_limited["matched"].as_array().unwrap().len(), 1);
        assert_eq!(parsed_limited["totalMatches"].as_u64(), Some(2));
    }

    #[tokio::test]
    async fn search_tables_case_insensitive() {
        let test = TestAppState::with_resource_provider(options_with_tables(
            vec!["app".into()],
            vec![
                TableInfo {
                    name: "UserAccounts".into(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                },
                TableInfo {
                    name: "order_items".into(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                },
            ],
        ))
        .await;
        test.save_connection("c1").await;

        let json = search_tables(&test.state, "c1", Some("app"), None, "USER", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(1));
        assert_eq!(parsed["matched"][0]["name"].as_str(), Some("UserAccounts"));
    }

    #[tokio::test]
    async fn search_tables_empty_pattern_returns_all() {
        let test = TestAppState::with_resource_provider(options_with_tables(
            vec!["app".into()],
            vec![
                TableInfo {
                    name: "a".into(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                },
                TableInfo {
                    name: "b".into(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                },
            ],
        ))
        .await;
        test.save_connection("c1").await;

        let json = search_tables(&test.state, "c1", Some("app"), None, "", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(2));
        assert_eq!(parsed["matched"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn search_tables_no_match() {
        let test = test_stack().await;
        test.save_connection("c1").await;

        let json = search_tables(&test.state, "c1", Some("app"), None, "nonexistent_xyz", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(0));
        assert!(parsed["matched"].as_array().unwrap().is_empty());
        assert_eq!(parsed["totalTables"].as_u64(), Some(1));
    }
}
