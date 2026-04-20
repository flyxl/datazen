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

    fn get_db_cache_mut<'a>(
        caches: &'a mut HashMap<String, HashMap<String, DatabaseCache>>,
        connection_id: &str,
        database: &str,
    ) -> &'a mut DatabaseCache {
        caches
            .entry(connection_id.to_string())
            .or_default()
            .entry(database.to_string())
            .or_default()
    }

    /// Fast path: returns columns + PK info only.
    /// Checks full-schema cache first, then columns-only cache,
    /// finally calls `driver.get_columns()` on cache miss.
    pub async fn get_columns(
        &self,
        connection_id: &str,
        database: &str,
        table: &str,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
    ) -> Result<CachedColumns, DriverError> {
        {
            let caches = self.caches.read().await;
            if let Some(db_caches) = caches.get(connection_id) {
                if let Some(db_cache) = db_caches.get(database) {
                    if let Some(cached) = db_cache.tables.get(table) {
                        if cached.cached_at.elapsed() < self.cache_ttl {
                            return Ok(CachedColumns {
                                columns: cached.schema.columns.clone(),
                                primary_keys: cached.schema.primary_keys.clone(),
                                table_name: cached.schema.table_name.clone(),
                                cached_at: cached.cached_at,
                            });
                        }
                    }
                    if let Some(cached) = db_cache.columns.get(table) {
                        if cached.cached_at.elapsed() < self.cache_ttl {
                            return Ok(cached.clone());
                        }
                    }
                }
            }
        }

        tracing::debug!("Columns cache miss: {}.{}", database, table);
        let (columns, primary_keys) = driver.get_columns(handle, table).await?;
        let entry = CachedColumns {
            columns,
            primary_keys,
            table_name: table.to_string(),
            cached_at: Instant::now(),
        };

        {
            let mut caches = self.caches.write().await;
            let db_cache = Self::get_db_cache_mut(&mut caches, connection_id, database);
            Self::evict_if_needed(&mut db_cache.columns, self.max_tables);
            db_cache.columns.insert(table.to_string(), entry.clone());
        }

        Ok(entry)
    }

    /// Full schema (columns + indexes + foreign keys).
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

        {
            let mut caches = self.caches.write().await;
            let db_cache = Self::get_db_cache_mut(&mut caches, connection_id, database);

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

        Ok(schema)
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

        if let Some(db_caches) = caches.get_mut(connection_id) {
            if let Some(db_cache) = db_caches.get_mut(database) {
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
                let mut caches = self.caches.write().await;
                let db_cache = Self::get_db_cache_mut(&mut caches, connection_id, database);
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
