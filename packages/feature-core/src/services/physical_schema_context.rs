//! The adapter that presents a physical metadata source through the logical port.
//!
//! The metadata reads that need a live connection handle used to be default
//! methods of the metadata port. That put two wrong things on one trait: the
//! port every consumer depended on carried a method that always failed on the
//! Runtime path, and the cache that memoizes the reads was a parameter the
//! caller had to supply — a parameter the Runtime path ignored and the
//! physical path used.
//!
//! Both live here now:
//!
//! * the *physical* contract is [`PhysicalSchemaContextSource`] — the source
//!   that owns a handle, and
//! * the *logical* contract is [`LogicalSchemaContextSource`] — the surface
//!   every metadata consumer depends on.
//!
//! The adapter joins them: it owns the [`SchemaCache`] (the cache belongs to
//! the implementation that owns the session) and answers the logical surface
//! from the physical one.

use crate::services::ports::{LogicalSchemaContextSource, PhysicalSchemaContextSource};
use datazen_driver_api::{DatabaseDriver, TableInfo, TableSchema};
use datazen_kernel::schema_cache::SchemaCache;
use std::sync::Arc;

/// Presents a physical metadata source through the logical port.
///
/// The `ConnectionHandle`-based reads are the ones a physical source exists
/// for, and they are the ones that populate the cache. The cache is held here,
/// next to the source whose sessions it memoizes — not passed in by a caller
/// that may not even have one.
pub struct PhysicalSchemaContextAdapter {
    physical: Arc<dyn PhysicalSchemaContextSource>,
    cache: Arc<SchemaCache>,
}

impl PhysicalSchemaContextAdapter {
    pub fn new(physical: Arc<dyn PhysicalSchemaContextSource>, cache: Arc<SchemaCache>) -> Self {
        Self { physical, cache }
    }
}

#[async_trait::async_trait]
impl LogicalSchemaContextSource for PhysicalSchemaContextAdapter {
    async fn driver_for_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn DatabaseDriver>, String> {
        Ok(self.physical.get_session(session_id).await?.0)
    }

    async fn session_schema(&self, session_id: &str) -> Option<String> {
        self.physical.configured_schema(session_id).await
    }

    async fn tables(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String> {
        let (driver, handle) = self.physical.get_session(session_id).await?;
        driver
            .get_tables(&handle, database, schema)
            .await
            .map_err(|error| error.to_string())
    }

    async fn table_schema(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<TableSchema, String> {
        let (driver, handle) = self.physical.get_session(session_id).await?;
        self.cache
            .get_table_schema(
                session_id,
                database,
                schema,
                table,
                &datazen_driver_api::LegacyCatalogReader::new(driver.as_ref()),
                &handle,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::{
        mock_driver::{MockDriver, MockDriverOptions},
        ConnectionHandle, DatabaseDriver, TableInfo, TableType,
    };
    use datazen_kernel::driver_registry::DriverRegistry;

    /// A physical source that owns a handle and a driver.
    struct PhysicalSource {
        driver: Arc<dyn DatabaseDriver>,
        handle: ConnectionHandle,
        schema: Option<String>,
    }

    #[async_trait::async_trait]
    impl PhysicalSchemaContextSource for PhysicalSource {
        async fn get_session(
            &self,
            _session_id: &str,
        ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
            Ok((self.driver.clone(), self.handle.clone()))
        }

        async fn configured_schema(&self, _session_id: &str) -> Option<String> {
            self.schema.clone()
        }
    }

    fn test_handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        }
    }

    fn adapter(
        driver: Arc<MockDriver>,
        schema: Option<String>,
    ) -> (PhysicalSchemaContextAdapter, Arc<SchemaCache>) {
        let registry = Arc::new(DriverRegistry::new());
        let cache = Arc::new(SchemaCache::new(registry));
        let physical = Arc::new(PhysicalSource {
            driver: driver.clone(),
            handle: test_handle(),
            schema,
        });
        (
            PhysicalSchemaContextAdapter::new(physical, cache.clone()),
            cache,
        )
    }

    /// A mock driver that answers with one table and one schema.
    fn mock_with_tables() -> Arc<MockDriver> {
        MockDriver::new(
            "postgres",
            MockDriverOptions {
                tables: vec![TableInfo {
                    name: "orders".into(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                }],
                ..MockDriverOptions::default()
            },
        )
    }

    #[tokio::test]
    async fn driver_for_session_answers_with_the_physical_driver() {
        let mock = mock_with_tables();
        let (adapter, _cache) = adapter(mock.clone(), None);

        let driver = adapter.driver_for_session("session-1").await.unwrap();
        assert_eq!(driver.driver_type(), mock.driver_type());
    }

    #[tokio::test]
    async fn session_schema_answers_with_the_physical_source() {
        let mock = mock_with_tables();
        let (adapter, _cache) = adapter(mock.clone(), Some("sales".into()));

        assert_eq!(
            adapter.session_schema("session-1").await,
            Some("sales".into())
        );
    }

    #[tokio::test]
    async fn tables_reads_through_the_physical_handle() {
        let mock = mock_with_tables();
        let (adapter, _cache) = adapter(mock.clone(), None);

        let tables = adapter.tables("session-1", "db1", None).await.unwrap();
        let names: Vec<&str> = tables.iter().map(|table| table.name.as_str()).collect();
        assert_eq!(names, vec!["orders"]);
    }

    #[tokio::test]
    async fn table_schema_populates_the_cache_the_adapter_owns() {
        let mock = mock_with_tables();
        let (adapter, _cache) = adapter(mock.clone(), None);

        let schema = adapter
            .table_schema("session-1", "db1", None, "orders")
            .await
            .unwrap();
        assert_eq!(schema.table_name, "orders");
        assert_eq!(mock.get_schema_calls(), 1);

        // The second read is served from the cache the adapter owns.
        let again = adapter
            .table_schema("session-1", "db1", None, "orders")
            .await
            .unwrap();
        assert_eq!(again.table_name, "orders");
        assert_eq!(
            mock.get_schema_calls(),
            1,
            "second read should hit the cache"
        );
    }
}
