//! The catalog contract: reading databases, schemas, relations and columns.
//!
//! [`CatalogReader`] is the narrow surface a cache, an object tree or a
//! metadata adapter needs. It takes an explicit target (`database`, optional
//! `schema`) on every relation read — there is no implicit "current database"
//! and no shared relation model forced on drivers that have a different
//! namespace shape.
//!
//! The old [`DatabaseDriver`] stays the entry point during migration:
//! [`LegacyCatalogReader`] borrows it and forwards each read, so a driver
//! override keeps working and there is exactly one implementation of each
//! catalog rule. Consumers move to `&dyn CatalogReader`; the adapter is not a
//! second source of truth.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::traits::DatabaseDriver;
use crate::types::{ColumnSchema, ConnectionHandle, DriverError, TableInfo, TableSchema};

/// Read-only catalog access with explicit relation targets.
///
/// Methods must answer for the arguments they are given, not for whatever the
/// session happens to have selected. A driver with no schema level answers
/// `schema = None`; callers must not invent a default schema.
#[async_trait]
pub trait CatalogReader: Send + Sync {
    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError>;

    /// Complete schema names, including empty schemas, when the driver can
    /// enumerate them. `None` lets catalog adapters retain relation-derived
    /// names.
    async fn get_schemas(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<Vec<String>>, DriverError>;

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError>;

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError>;

    /// Batch-fetch columns for every relation in one target. The default on a
    /// driver is `Unsupported`; callers fall back to per-relation reads.
    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError>;
}

/// Borrowing adapter from an existing [`DatabaseDriver`] to [`CatalogReader`].
///
/// One-way, like [`LegacySqlDialect`](crate::dialect::LegacySqlDialect): every
/// method forwards to the driver method of the same name, so there is nothing
/// to keep in sync beyond the signature list.
#[derive(Debug, Clone, Copy)]
pub struct LegacyCatalogReader<'a, D: DatabaseDriver + ?Sized> {
    driver: &'a D,
}

impl<'a, D: DatabaseDriver + ?Sized> LegacyCatalogReader<'a, D> {
    pub fn new(driver: &'a D) -> Self {
        Self { driver }
    }

    /// The wrapped driver, for callers at the boundary that still need a
    /// driver-only rule.
    pub fn driver(&self) -> &'a D {
        self.driver
    }
}

#[async_trait]
impl<D: DatabaseDriver + ?Sized> CatalogReader for LegacyCatalogReader<'_, D> {
    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.driver.get_databases(handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.driver.get_tables(handle, database, schema).await
    }

    async fn get_schemas(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<Vec<String>>, DriverError> {
        self.driver.get_schemas(handle, database).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        self.driver
            .get_table_schema(handle, table, database, schema)
            .await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.driver
            .get_columns(handle, table, database, schema)
            .await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        self.driver.get_all_columns(handle, database, schema).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_driver::{MockDriver, MockDriverOptions};

    fn handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        }
    }

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn reader_is_object_safe_and_thread_safe() {
        assert_send_sync::<dyn CatalogReader>();
    }

    #[tokio::test]
    async fn adapter_forwards_relation_targets_verbatim() {
        let column = ColumnSchema {
            name: "from_db1".into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        };
        let mut by_database = HashMap::new();
        by_database.insert("db1".to_string(), {
            let mut tables = HashMap::new();
            tables.insert("users".to_string(), vec![column]);
            tables
        });
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns_by_database: by_database,
                ..MockDriverOptions::default()
            },
        );
        let reader = LegacyCatalogReader::new(driver.as_ref());

        let (columns, _) = reader
            .get_columns(&handle(), "users", "db1", None)
            .await
            .unwrap();
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].name, "from_db1");

        // The same relation under another database must not re-use db1's answer.
        let (other, _) = reader
            .get_columns(&handle(), "users", "db2", None)
            .await
            .unwrap();
        assert!(other.is_empty());
    }

    #[tokio::test]
    async fn adapter_forwards_database_list_and_schema_answer() {
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                databases: vec!["app".into()],
                ..MockDriverOptions::default()
            },
        );
        let reader = LegacyCatalogReader::new(driver.as_ref());
        assert_eq!(reader.get_databases(&handle()).await.unwrap(), vec!["app"]);
        // The mock keeps the trait default (`None` = derive from relations);
        // the adapter must pass that answer through rather than invent one.
        assert_eq!(reader.get_schemas(&handle(), "app").await.unwrap(), None);
    }
}
