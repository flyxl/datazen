#[cfg(test)]
mod host_tests {
    use datazen_driver_api::{ConnectionHandle, Value};
    use datazen_feature_core::services::query_executor::*;
    use std::sync::Arc;
    #[tokio::test]
    async fn get_table_data_uses_schema_cache_and_returns_total() {
        use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
        use datazen_runtime::driver_registry::registry::DriverRegistry;
        use datazen_runtime::schema_cache::SchemaCache;

        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                count_total: 99,
                query_rows: vec![vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("alice".into())),
                ]],
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let cache = Arc::new(SchemaCache::new(registry));
        let executor = QueryExecutor::new(cache);
        let driver = mock.clone() as Arc<dyn datazen_driver_api::DatabaseDriver>;
        let handle = ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        };

        let result = executor
            .get_table_data(
                &driver, &handle, "conn1", "db1", None, "users", 0, 50, None, None, false, None,
            )
            .await
            .unwrap();

        assert_eq!(result.total_rows, Some(99));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(mock.query_calls(), 2, "count + data queries");
    }
}
