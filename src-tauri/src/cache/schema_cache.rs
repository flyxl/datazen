//! In-memory schema cache keyed by connection and database.

use crate::db::registry::DriverRegistry;
use crate::db::{ConnectionHandle, DatabaseDriver, DriverError, TableSchema};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cached table schema with metadata.
#[derive(Debug)]
pub struct CachedSchema {
    pub schema: TableSchema,
    pub cached_at: Instant,
    pub version: u64,
}

#[derive(Debug, Default)]
pub struct DatabaseCache {
    tables: HashMap<String, CachedSchema>,
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

    pub async fn get_table_schema(
        &self,
        connection_id: &str,
        database: &str,
        table: &str,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
    ) -> Result<TableSchema, DriverError> {
        {
            let caches = self.caches.read().await;
            if let Some(db_caches) = caches.get(connection_id) {
                if let Some(db_cache) = db_caches.get(database) {
                    if let Some(cached) = db_cache.tables.get(table) {
                        if cached.cached_at.elapsed() < self.cache_ttl {
                            tracing::debug!("Schema cache hit: {}.{}", database, table);
                            return Ok(cached.schema.clone());
                        }
                    }
                }
            }
        }

        tracing::debug!("Schema cache miss: {}.{}", database, table);
        let schema = driver.get_table_schema(handle, table).await?;
        self.put_schema(connection_id, database, table, schema.clone())
            .await;
        Ok(schema)
    }

    async fn put_schema(
        &self,
        connection_id: &str,
        database: &str,
        table: &str,
        schema: TableSchema,
    ) {
        let mut caches = self.caches.write().await;

        let db_caches = caches
            .entry(connection_id.to_string())
            .or_insert_with(HashMap::new);

        let db_cache = db_caches
            .entry(database.to_string())
            .or_insert_with(DatabaseCache::default);

        if db_cache.tables.len() >= self.max_tables {
            let oldest = db_cache
                .tables
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone());

            if let Some(key) = oldest {
                db_cache.tables.remove(&key);
            }
        }

        db_cache.tables.insert(
            table.to_string(),
            CachedSchema {
                schema,
                cached_at: Instant::now(),
                version: 0,
            },
        );
    }

    pub async fn invalidate(&self, connection_id: &str, database: &str, table: Option<&str>) {
        let mut caches = self.caches.write().await;

        if let Some(db_caches) = caches.get_mut(connection_id) {
            if let Some(db_cache) = db_caches.get_mut(database) {
                match table {
                    Some(table_name) => {
                        db_cache.tables.remove(table_name);
                    }
                    None => {
                        db_cache.tables.clear();
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
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        tables: &[String],
    ) {
        for table in tables {
            if let Ok(schema) = driver.get_table_schema(handle, table).await {
                self.put_schema(connection_id, database, table, schema).await;
            } else {
                tracing::warn!("Warmup skipped for table {}", table);
            }
        }
    }
}
