#[cfg(test)]
use crate::services::ConnectionManager;
pub use datazen_feature_core::services::db_tools::*;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::registry::DriverRegistry;
    use crate::db::{ConnectionConfig, SslMode, TableInfo, TableType};
    use crate::mcp::permission::McpPermissionMode;
    use crate::store::Store;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

    async fn test_stack() -> (
        crate::testing::FileKeyringGuard,
        Arc<Store>,
        Arc<ConnectionManager>,
        Arc<MockDriver>,
    ) {
        let keyring = crate::testing::FileKeyringGuard::set();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                databases: vec!["app".into(), "test".into()],
                tables: vec![TableInfo {
                    name: "users".into(),
                    schema: Some("public".into()),
                    table_type: TableType::Table,
                    row_count: Some(10),
                }],
                explain_plan: crate::db::ExplainResult {
                    plan_text: "Seq Scan".into(),
                    plan_json: None,
                    plan_tree: None,
                    total_cost: Some(1.0),
                    estimated_rows: Some(100),
                },
                server_version: "PostgreSQL 16".into(),
                count_total: 42,
                query_rows: vec![vec![Some(crate::db::Value::Integer(1))]],
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
        (keyring, store, mgr, mock)
    }

    fn sample_config(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: "Test DB".into(),
            database_type: "postgres".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("app".into()),
            schema: None,
            username: Some("user".into()),
            password: None,
            ssl_mode: SslMode::Prefer,
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[test]
    fn resolve_query_limit_defaults_and_caps() {
        assert_eq!(resolve_query_limit(None), Some(100));
        assert_eq!(resolve_query_limit(Some(500)), Some(500));
        assert_eq!(resolve_query_limit(Some(999_999)), Some(50_000));
    }

    #[tokio::test]
    async fn list_connections_serializes_saved_configs() {
        let (_keyring, store, _, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let json = list_connections(&store).await.unwrap();
        assert!(json.contains("\"id\": \"c1\""));
        assert!(json.contains("Test DB"));
    }

    #[tokio::test]
    async fn resolve_connection_connects_when_not_in_session() {
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let (_, driver, handle) = mgr.resolve_session_for_connection("c1").await.unwrap();
        assert_eq!(driver.driver_type(), "postgres");
        assert!(handle.id.starts_with("mock-c1"));
    }

    #[tokio::test]
    async fn list_databases_returns_json() {
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let json = list_databases(&mgr, "c1").await.unwrap();
        assert!(json.contains("app"));
        assert!(json.contains("test"));
    }

    #[tokio::test]
    async fn list_tables_returns_json() {
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let json = list_tables(&mgr, "c1", Some("app"), None).await.unwrap();
        assert!(json.contains("users"));
    }

    #[tokio::test]
    async fn get_table_schema_returns_pretty_json() {
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let json = get_table_schema(&mgr, "c1", &["users".into()], Some("app"), None)
            .await
            .unwrap();
        assert!(json.contains("users"));
        assert!(json.contains("id"));
    }

    #[tokio::test]
    async fn query_executes_without_permission_gate() {
        let (_keyring, store, mgr, mock) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let json = query(&mgr, "c1", "SELECT * FROM users", Some(10), None)
            .await
            .unwrap();
        assert!(json.contains("1"));
        assert!(mock.query_calls() >= 1);
    }

    #[tokio::test]
    async fn query_blocks_sql_in_read_only_permission_mode() {
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let err = query(
            &mgr,
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
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();
        let json = explain_query(&mgr, "c1", "SELECT 1").await.unwrap();
        assert!(json.contains("Seq Scan"));
    }

    #[tokio::test]
    async fn search_tables_filters_by_pattern() {
        let keyring = crate::testing::FileKeyringGuard::set();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                databases: vec!["app".into()],
                tables: vec![
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
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
        store.save_connection(sample_config("c1")).await.unwrap();

        let json = search_tables(&mgr, "c1", Some("app"), None, "user", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(2));
        assert_eq!(parsed["totalTables"].as_u64(), Some(4));
        assert_eq!(parsed["pattern"].as_str(), Some("user"));
        let matched = parsed["matched"].as_array().unwrap();
        assert_eq!(matched.len(), 2);

        let json_limited = search_tables(&mgr, "c1", Some("app"), None, "user", 1)
            .await
            .unwrap();
        let parsed_limited: serde_json::Value = serde_json::from_str(&json_limited).unwrap();
        assert_eq!(parsed_limited["matched"].as_array().unwrap().len(), 1);
        assert_eq!(parsed_limited["totalMatches"].as_u64(), Some(2));

        let _ = keyring;
    }

    #[tokio::test]
    async fn search_tables_case_insensitive() {
        let keyring = crate::testing::FileKeyringGuard::set();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                databases: vec!["app".into()],
                tables: vec![
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
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
        store.save_connection(sample_config("c1")).await.unwrap();

        let json = search_tables(&mgr, "c1", Some("app"), None, "USER", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(1));
        assert_eq!(parsed["matched"][0]["name"].as_str(), Some("UserAccounts"));

        let _ = keyring;
    }

    #[tokio::test]
    async fn search_tables_empty_pattern_returns_all() {
        let keyring = crate::testing::FileKeyringGuard::set();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                databases: vec!["app".into()],
                tables: vec![
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
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
        store.save_connection(sample_config("c1")).await.unwrap();

        let json = search_tables(&mgr, "c1", Some("app"), None, "", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(2));
        assert_eq!(parsed["matched"].as_array().unwrap().len(), 2);

        let _ = keyring;
    }

    #[tokio::test]
    async fn search_tables_no_match() {
        let (_keyring, store, mgr, _) = test_stack().await;
        store.save_connection(sample_config("c1")).await.unwrap();

        let json = search_tables(&mgr, "c1", Some("app"), None, "nonexistent_xyz", 20)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(0));
        assert!(parsed["matched"].as_array().unwrap().is_empty());
        assert_eq!(parsed["totalTables"].as_u64(), Some(1));
    }
}
