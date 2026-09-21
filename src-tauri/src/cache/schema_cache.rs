//! In-memory schema cache keyed by connection and database.
//!
//! Two cache tiers:
//!   1. **Columns cache** – lightweight (ColumnSchema + PK names).
//!      Populated by `driver.get_columns()` which skips indexes/FK queries.
//!   2. **Full schema cache** – complete TableSchema including indexes & FK.
//!      Populated on demand by `driver.get_table_schema()`.
//!
//! When the full schema is cached, the columns tier is also satisfied from it.

use crate::db::registry::DriverRegistry;
use crate::db::{ColumnSchema, ConnectionHandle, DatabaseDriver, DriverError, TableSchema};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cached table schema with metadata.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CachedSchema {
    pub schema: TableSchema,
    pub cached_at: Instant,
    pub version: u64,
}

/// Lightweight cached columns (no indexes / foreign keys).
#[derive(Debug, Clone)]
pub struct CachedColumns {
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub table_name: String,
    pub cached_at: Instant,
}

#[derive(Debug, Default)]
pub struct DatabaseCache {
    tables: HashMap<String, CachedSchema>,
    columns: HashMap<String, CachedColumns>,
    #[allow(dead_code)]
    pub db_version: u64,
}

/// Multi-level schema cache.
pub struct SchemaCache {
    caches: Arc<RwLock<HashMap<String, HashMap<String, DatabaseCache>>>>,
    cache_ttl: Duration,
    max_tables: usize,
    #[allow(dead_code)]
    registry: Arc<DriverRegistry>,
}

impl SchemaCache {
    pub fn new(registry: Arc<DriverRegistry>) -> Self {
        Self {
            caches: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(300),
            max_tables: 1000,
            registry,
        }
    }

    /// Cache bucket for one `(database, schema)` scope.
    ///
    /// The schema is part of the key, not decoration: `public.users` and
    /// `other.users` are different tables, and caching them under one key was
    /// how a foreign-schema read could serve another schema's columns.
    fn scope_key(database: &str, schema: Option<&str>) -> String {
        match schema.map(str::trim).filter(|s| !s.is_empty()) {
            Some(schema) => format!("{database}\u{1}{schema}"),
            None => database.to_string(),
        }
    }

    fn get_db_cache_mut<'a>(
        caches: &'a mut HashMap<String, HashMap<String, DatabaseCache>>,
        connection_id: &str,
        scope: &str,
    ) -> &'a mut DatabaseCache {
        caches
            .entry(connection_id.to_string())
            .or_default()
            .entry(scope.to_string())
            .or_default()
    }

    /// Fast path: returns columns + PK info only.
    /// Checks full-schema cache first, then columns-only cache,
    /// finally calls `driver.get_columns()` on cache miss.
    pub async fn get_columns(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
    ) -> Result<CachedColumns, DriverError> {
        let scope = Self::scope_key(database, schema);
        {
            let caches = self.caches.read().await;
            if let Some(db_caches) = caches.get(connection_id) {
                if let Some(db_cache) = db_caches.get(&scope) {
                    // An entry with no columns is never a usable answer: it means
                    // the read resolved against the wrong catalog (or the table is
                    // gone). Treat it as a miss so a single bad read cannot blank
                    // the structure view / ER diagram / data grid for the whole TTL.
                    if let Some(cached) = db_cache.tables.get(table) {
                        if cached.cached_at.elapsed() < self.cache_ttl
                            && !cached.schema.columns.is_empty()
                        {
                            return Ok(CachedColumns {
                                columns: cached.schema.columns.clone(),
                                primary_keys: cached.schema.primary_keys.clone(),
                                table_name: cached.schema.table_name.clone(),
                                cached_at: cached.cached_at,
                            });
                        }
                    }
                    if let Some(cached) = db_cache.columns.get(table) {
                        if cached.cached_at.elapsed() < self.cache_ttl && !cached.columns.is_empty()
                        {
                            return Ok(cached.clone());
                        }
                    }
                }
            }
        }

        tracing::debug!("Columns cache miss: {}.{}", scope, table);
        let (columns, primary_keys) = driver.get_columns(handle, table, database, schema).await?;
        let entry = CachedColumns {
            columns,
            primary_keys,
            table_name: table.to_string(),
            cached_at: Instant::now(),
        };

        {
            let mut caches = self.caches.write().await;
            let db_cache = Self::get_db_cache_mut(&mut caches, connection_id, &scope);
            Self::evict_if_needed(&mut db_cache.columns, self.max_tables);
            db_cache.columns.insert(table.to_string(), entry.clone());
        }

        Ok(entry)
    }

    /// Returns a cached full schema when fresh; otherwise `None`.
    pub async fn try_get_cached_schema(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<TableSchema> {
        let scope = Self::scope_key(database, schema);
        let caches = self.caches.read().await;
        let db_caches = caches.get(connection_id)?;
        let db_cache = db_caches.get(&scope)?;
        let cached = db_cache.tables.get(table)?;
        // See `get_columns`: a column-less entry is not a usable answer.
        if cached.cached_at.elapsed() < self.cache_ttl && !cached.schema.columns.is_empty() {
            tracing::debug!("Schema cache hit: {}.{}", scope, table);
            Some(cached.schema.clone())
        } else {
            None
        }
    }

    /// Store a full schema in both the schema and columns cache tiers.
    ///
    /// Column-less schemas are deliberately **not** cached: they are the
    /// signature of a schema read that resolved against the wrong catalog, and
    /// caching one used to blank every later read of that table for the whole
    /// TTL. Re-fetching a genuine zero-column table is the accepted cost.
    pub async fn store_table_schema(
        &self,
        connection_id: &str,
        database: &str,
        schema_name: Option<&str>,
        table: &str,
        schema: TableSchema,
    ) {
        if schema.columns.is_empty() {
            tracing::warn!(
                database = %database,
                schema = ?schema_name,
                table = %table,
                "schema_cache: refusing to cache a schema with no columns"
            );
            return;
        }

        let scope = Self::scope_key(database, schema_name);
        let mut caches = self.caches.write().await;
        let db_cache = Self::get_db_cache_mut(&mut caches, connection_id, &scope);

        Self::evict_if_needed(&mut db_cache.tables, self.max_tables);
        db_cache.tables.insert(
            table.to_string(),
            CachedSchema {
                schema: schema.clone(),
                cached_at: Instant::now(),
                version: 0,
            },
        );

        db_cache.columns.insert(
            table.to_string(),
            CachedColumns {
                columns: schema.columns.clone(),
                primary_keys: schema.primary_keys.clone(),
                table_name: schema.table_name.clone(),
                cached_at: Instant::now(),
            },
        );
    }

    /// Full schema (columns + indexes + foreign keys).
    pub async fn get_table_schema(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
    ) -> Result<TableSchema, DriverError> {
        if let Some(cached) = self
            .try_get_cached_schema(connection_id, database, schema, table)
            .await
        {
            return Ok(cached);
        }

        tracing::debug!("Schema cache miss: {}", Self::scope_key(database, schema));
        let table_schema = driver
            .get_table_schema(handle, table, database, schema)
            .await?;
        self.store_table_schema(connection_id, database, schema, table, table_schema.clone())
            .await;
        Ok(table_schema)
    }

    fn evict_if_needed<V: std::fmt::Debug>(map: &mut HashMap<String, V>, max: usize) {
        if map.len() >= max {
            if let Some(key) = map.keys().next().map(|k| k.clone()) {
                map.remove(&key);
            }
        }
    }

    pub async fn invalidate(&self, connection_id: &str, database: &str, table: Option<&str>) {
        let mut caches = self.caches.write().await;

        // No schema argument: invalidate every schema scope of this database.
        if let Some(db_caches) = caches.get_mut(connection_id) {
            let prefix = format!("{database}\u{1}");
            let scopes: Vec<String> = db_caches
                .keys()
                .filter(|k| k.as_str() == database || k.starts_with(&prefix))
                .cloned()
                .collect();
            for scope in scopes {
                if let Some(db_cache) = db_caches.get_mut(&scope) {
                    match table {
                        Some(table_name) => {
                            db_cache.tables.remove(table_name);
                            db_cache.columns.remove(table_name);
                        }
                        None => {
                            db_cache.tables.clear();
                            db_cache.columns.clear();
                        }
                    }
                }
            }
        }
    }

    pub async fn clear_connection(&self, connection_id: &str) {
        let mut caches = self.caches.write().await;
        caches.remove(connection_id);
    }

    #[allow(dead_code)]
    pub async fn warmup(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<&str>,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        tables: &[String],
    ) {
        let scope = Self::scope_key(database, schema);
        for table in tables {
            if let Ok(schema) = driver
                .get_table_schema(handle, table, database, schema)
                .await
            {
                if schema.columns.is_empty() {
                    tracing::warn!("Warmup skipped for column-less table {}", table);
                    continue;
                }
                let mut caches = self.caches.write().await;
                let db_cache = Self::get_db_cache_mut(&mut caches, connection_id, &scope);
                db_cache.tables.insert(
                    table.to_string(),
                    CachedSchema {
                        schema,
                        cached_at: Instant::now(),
                        version: 0,
                    },
                );
            } else {
                tracing::warn!("Warmup skipped for table {}", table);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

    fn test_handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        }
    }

    async fn cache_with_mock() -> (SchemaCache, Arc<MockDriver>, ConnectionHandle) {
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new("postgres", MockDriverOptions::default());
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let cache = SchemaCache::new(registry);
        (cache, mock, test_handle())
    }

    #[tokio::test]
    async fn get_columns_fetches_and_caches_on_miss() {
        let (cache, mock, handle) = cache_with_mock().await;
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;

        let first = cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(first.table_name, "users");
        assert_eq!(mock.get_columns_calls(), 1);

        cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(mock.get_columns_calls(), 1, "second call should hit cache");
    }

    #[tokio::test]
    async fn get_table_schema_populates_both_cache_tiers() {
        let (cache, mock, handle) = cache_with_mock().await;
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;

        let schema = cache
            .get_table_schema("conn1", "db1", None, "orders", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(schema.table_name, "orders");
        assert_eq!(mock.get_schema_calls(), 1);

        mock.reset_columns_calls();
        let cols = cache
            .get_columns("conn1", "db1", None, "orders", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(cols.columns.len(), schema.columns.len());
        assert_eq!(
            mock.get_columns_calls(),
            0,
            "columns served from full schema cache"
        );
    }

    #[tokio::test]
    async fn invalidate_table_forces_refetch() {
        let (cache, mock, handle) = cache_with_mock().await;
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;

        cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        cache.invalidate("conn1", "db1", Some("users")).await;

        cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(mock.get_columns_calls(), 2);
    }

    #[tokio::test]
    async fn invalidate_all_tables_clears_database_cache() {
        let (cache, mock, handle) = cache_with_mock().await;
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;

        cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        cache
            .get_columns("conn1", "db1", None, "orders", &driver, &handle)
            .await
            .unwrap();
        cache.invalidate("conn1", "db1", None).await;

        cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        assert!(mock.get_columns_calls() >= 3);
    }

    #[tokio::test]
    async fn clear_connection_removes_all_databases() {
        let (cache, mock, handle) = cache_with_mock().await;
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;

        cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        cache.clear_connection("conn1").await;

        cache
            .get_columns("conn1", "db2", None, "users", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(mock.get_columns_calls(), 2);
    }

    fn sample_columns(table: &str) -> CachedColumns {
        CachedColumns {
            columns: vec![],
            primary_keys: vec![],
            table_name: table.to_string(),
            cached_at: Instant::now(),
        }
    }

    #[tokio::test]
    async fn invalidate_removes_one_table() {
        let cache = SchemaCache::new(Arc::new(DriverRegistry::new()));
        {
            let mut caches = cache.caches.write().await;
            let db = SchemaCache::get_db_cache_mut(&mut caches, "conn-1", "db-a");
            db.columns.insert("t1".into(), sample_columns("t1"));
            db.columns.insert("t2".into(), sample_columns("t2"));
        }

        cache.invalidate("conn-1", "db-a", Some("t1")).await;

        let caches = cache.caches.read().await;
        let db = caches.get("conn-1").unwrap().get("db-a").unwrap();
        assert!(!db.columns.contains_key("t1"));
        assert!(db.columns.contains_key("t2"));
    }

    #[tokio::test]
    async fn invalidate_clears_whole_database_when_table_omitted() {
        let cache = SchemaCache::new(Arc::new(DriverRegistry::new()));
        {
            let mut caches = cache.caches.write().await;
            let db = SchemaCache::get_db_cache_mut(&mut caches, "conn-1", "db-a");
            db.columns.insert("t1".into(), sample_columns("t1"));
        }

        cache.invalidate("conn-1", "db-a", None).await;

        let caches = cache.caches.read().await;
        let db = caches.get("conn-1").unwrap().get("db-a").unwrap();
        assert!(db.columns.is_empty());
    }

    fn empty_schema(table: &str) -> TableSchema {
        TableSchema {
            table_name: table.to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[tokio::test]
    async fn column_less_schema_is_never_cached() {
        let (cache, _mock, _handle) = cache_with_mock().await;

        cache
            .store_table_schema("conn1", "db1", None, "ghost", empty_schema("ghost"))
            .await;

        assert!(
            cache
                .try_get_cached_schema("conn1", "db1", None, "ghost")
                .await
                .is_none(),
            "a column-less schema must not be served from the full-schema tier"
        );

        let caches = cache.caches.read().await;
        let stored = caches
            .get("conn1")
            .and_then(|databases| databases.get("db1"))
            .is_some_and(|db| db.tables.contains_key("ghost") || db.columns.contains_key("ghost"));
        assert!(
            !stored,
            "a column-less schema must not enter either cache tier"
        );
    }

    /// A poisoned entry (written before the guard existed, or by another
    /// writer) must not be served: it is treated as a miss so the next read
    /// goes back to the driver.
    #[tokio::test]
    async fn column_less_cache_entries_are_treated_as_miss() {
        let (cache, mock, handle) = cache_with_mock().await;
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;

        {
            let mut caches = cache.caches.write().await;
            let db = SchemaCache::get_db_cache_mut(&mut caches, "conn1", "db1");
            db.columns.insert("users".into(), sample_columns("users"));
            db.tables.insert(
                "orders".into(),
                CachedSchema {
                    schema: empty_schema("orders"),
                    cached_at: Instant::now(),
                    version: 0,
                },
            );
        }

        assert!(
            cache
                .try_get_cached_schema("conn1", "db1", None, "orders")
                .await
                .is_none(),
            "column-less full-schema entry must not be served"
        );

        let cols = cache
            .get_columns("conn1", "db1", None, "users", &driver, &handle)
            .await
            .unwrap();
        assert_eq!(
            mock.get_columns_calls(),
            1,
            "column-less columns-tier entry must be re-fetched from the driver"
        );
        assert!(!cols.columns.is_empty());
    }
}
