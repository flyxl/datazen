# Schema 缓存与查询优化

> [返回架构总览](../README.md)

### 1.1 设计目标

**核心原则：SQL 执行路径最短化**

```
传统方式（每次查询都获取元数据）:
┌─────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ 执行SQL  │ -> │ 查询表结构 │ -> │ 获取列信息 │ -> │ 返回结果  │
└─────────┘    └──────────┘    └──────────┘    └──────────┘
                    多次查询系统表，开销大

优化方式（Schema 缓存）:
┌─────────┐    ┌──────────────────┐    ┌──────────┐
│ 执行SQL  │ -> │ 从缓存读取列信息  │ -> │ 返回结果  │
└─────────┘    └──────────────────┘    └──────────┘
                    缓存命中，零开销
```

### 1.2 Schema 缓存架构

```rust
// src-tauri/src/cache/schema_cache.rs

use crate::db::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Instant, Duration};

/// Schema 缓存项
#[derive(Debug, Clone)]
pub struct CachedSchema {
    /// 表结构
    pub schema: TableSchema,
    /// 缓存时间
    pub cached_at: Instant,
    /// 版本号（用于检测表结构变更）
    pub version: u64,
}

/// 数据库级别的缓存
#[derive(Debug, Default)]
pub struct DatabaseCache {
    /// 表结构缓存: table_name -> CachedSchema
    tables: HashMap<String, CachedSchema>,
    /// 数据库版本（PostgreSQL 的 xmin 等）
    db_version: u64,
}

/// Schema 缓存管理器
pub struct SchemaCache {
    /// 多级缓存: connection_id -> database -> DatabaseCache
    caches: Arc<RwLock<HashMap<String, HashMap<String, DatabaseCache>>>>,
    /// 缓存过期时间
    cache_ttl: Duration,
    /// 最大缓存表数量
    max_tables: usize,
    /// 驱动注册表（用于刷新缓存）
    registry: Arc<DriverRegistry>,
}

impl SchemaCache {
    pub fn new(registry: Arc<DriverRegistry>) -> Self {
        Self {
            caches: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(300), // 5分钟
            max_tables: 1000,
            registry,
        }
    }
    
    /// 获取表结构（优先从缓存读取）
    pub async fn get_table_schema(
        &self,
        connection_id: &str,
        database: &str,
        table: &str,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
    ) -> Result<TableSchema, DriverError> {
        // 1. 尝试从缓存读取
        {
            let caches = self.caches.read().await;
            if let Some(db_caches) = caches.get(connection_id) {
                if let Some(db_cache) = db_caches.get(database) {
                    if let Some(cached) = db_cache.tables.get(table) {
                        // 检查是否过期
                        if cached.cached_at.elapsed() < self.cache_ttl {
                            tracing::debug!("Schema cache hit: {}.{}", database, table);
                            return Ok(cached.schema.clone());
                        }
                    }
                }
            }
        }
        
        // 2. 缓存未命中，从数据库获取
        tracing::debug!("Schema cache miss: {}.{}", database, table);
        let schema = driver.get_table_schema(handle, table).await?;
        
        // 3. 更新缓存
        self.put_schema(connection_id, database, table, schema.clone()).await;
        
        Ok(schema)
    }
    
    /// 存入缓存
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
        
        // LRU 淘汰策略
        if db_cache.tables.len() >= self.max_tables {
            // 移除最旧的条目
            let oldest = db_cache.tables
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone());
            
            if let Some(key) = oldest {
                db_cache.tables.remove(&key);
            }
        }
        
        db_cache.tables.insert(table.to_string(), CachedSchema {
            schema,
            cached_at: Instant::now(),
            version: 0,
        });
    }
    
    /// 使缓存失效（表结构变更后调用）
    pub async fn invalidate(
        &self,
        connection_id: &str,
        database: &str,
        table: Option<&str>,
    ) {
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
    
    /// 清除连接的所有缓存（断开连接时调用）
    pub async fn clear_connection(&self, connection_id: &str) {
        let mut caches = self.caches.write().await;
        caches.remove(connection_id);
    }
    
    /// 预热缓存（连接建立后预加载常用表）
    pub async fn warmup(
        &self,
        connection_id: &str,
        database: &str,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        tables: &[String],
    ) {
        for table in tables {
            match driver.get_table_schema(handle, table).await {
                Ok(schema) => {
                    self.put_schema(connection_id, database, table, schema).await;
                }
                Err(e) => {
                    tracing::warn!("Failed to warmup schema for {}: {}", table, e);
                }
            }
        }
    }
}

/// 智能缓存预加载策略
pub struct CacheWarmupStrategy {
    /// 最近访问的表（用于决定预加载哪些表）
    recent_tables: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl CacheWarmupStrategy {
    pub fn new() -> Self {
        Self {
            recent_tables: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 记录表访问
    pub async fn record_access(&self, connection_id: &str, table: &str) {
        let mut recent = self.recent_tables.write().await;
        let tables = recent.entry(connection_id.to_string()).or_default();
        
        // 移到最前面（如果已存在）
        tables.retain(|t| t != table);
        tables.insert(0, table.to_string());
        
        // 保留最近 20 个
        tables.truncate(20);
    }
    
    /// 获取预加载表列表
    pub async fn get_warmup_tables(&self, connection_id: &str) -> Vec<String> {
        let recent = self.recent_tables.read().await;
        recent.get(connection_id).cloned().unwrap_or_default()
    }
}
```

### 1.3 优化的查询执行流程

```rust
// src-tauri/src/services/query_executor.rs

use crate::db::*;
use crate::cache::*;
use std::sync::Arc;

/// 查询执行器 - 带缓存的优化版本
pub struct QueryExecutor {
    schema_cache: Arc<SchemaCache>,
}

impl QueryExecutor {
    /// 执行查询（优化版本）
    /// 
    /// 优化点：
    /// 1. 不再每次查询都获取列信息
    /// 2. 结果集列信息直接从驱动返回的元数据获取
    /// 3. 仅在首次访问表时获取完整 schema
    pub async fn execute_query(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        sql: &str,
        connection_id: &str,
        database: &str,
    ) -> Result<QueryResult, DriverError> {
        let start = std::time::Instant::now();
        
        // 直接执行 SQL，不查询表结构
        // 列信息从结果集元数据获取，无需额外查询
        let result = driver.query(handle, sql).await?;
        
        tracing::debug!(
            "Query executed in {}ms, {} rows returned",
            result.execution_time_ms,
            result.rows.len()
        );
        
        Ok(result)
    }
    
    /// 获取表数据（带缓存）
    pub async fn get_table_data(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        connection_id: &str,
        database: &str,
        table: &str,
        page: u32,
        page_size: u32,
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
    ) -> Result<TableDataResult, DriverError> {
        // 1. 从缓存获取表结构（用于构建查询）
        let schema = self.schema_cache
            .get_table_schema(connection_id, database, table, driver, handle)
            .await?;
        
        // 2. 构建优化的查询 SQL
        let sql = self.build_select_sql(&schema, page, page_size, filters, order_by);
        
        // 3. 执行查询
        let result = driver.query(handle, &sql).await?;
        
        Ok(TableDataResult {
            columns: schema.columns,
            rows: result.rows,
            total_rows: None, // 需要额外查询
            page,
            page_size,
        })
    }
    
    /// 构建分页查询 SQL
    fn build_select_sql(
        &self,
        schema: &TableSchema,
        page: u32,
        page_size: u32,
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
    ) -> String {
        let mut sql = String::new();
        
        // SELECT 字段列表（直接使用缓存的列名）
        sql.push_str("SELECT ");
        sql.push_str(&schema.columns.iter()
            .map(|c| format!("\"{}\"", c.name))
            .collect::<Vec<_>>()
            .join(", "));
        
        // FROM 子句
        sql.push_str(&format!(" FROM \"{}\"", schema.table_name));
        
        // WHERE 子句
        if let Some(conditions) = filters {
            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.iter()
                    .map(|c| self.format_condition(c))
                    .collect::<Vec<_>>()
                    .join(" AND "));
            }
        }
        
        // ORDER BY 子句
        if let Some(order) = order_by {
            sql.push_str(&format!(
                " ORDER BY \"{}\" {}",
                order.column,
                if order.descending { "DESC" } else { "ASC" }
            ));
        }
        
        // 分页
        let offset = page * page_size;
        sql.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));
        
        sql
    }
    
    fn format_condition(&self, condition: &FilterCondition) -> String {
        match condition.operator {
            FilterOperator::Eq => format!("\"{}\" = {}", condition.column, self.format_value(&condition.value)),
            FilterOperator::Ne => format!("\"{}\" != {}", condition.column, self.format_value(&condition.value)),
            FilterOperator::Gt => format!("\"{}\" > {}", condition.column, self.format_value(&condition.value)),
            FilterOperator::Lt => format!("\"{}\" < {}", condition.column, self.format_value(&condition.value)),
            FilterOperator::Like => format!("\"{}\" LIKE {}", condition.column, self.format_value(&condition.value)),
            FilterOperator::IsNull => format!("\"{}\" IS NULL", condition.column),
            FilterOperator::IsNotNull => format!("\"{}\" IS NOT NULL", condition.column),
            // ... 其他操作符
            _ => String::new(),
        }
    }
    
    fn format_value(&self, value: &Value) -> String {
        match value {
            Value::Null => "NULL".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => format!("'{}'", s.replace("'", "''")), // SQL 转义
            _ => "NULL".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    pub value: Value,
}

#[derive(Debug)]
pub enum FilterOperator {
    Eq,      // =
    Ne,      // !=
    Gt,      // >
    Lt,      // <
    Gte,     // >=
    Lte,     // <=
    Like,    // LIKE
    In,      // IN
    IsNull,  // IS NULL
    IsNotNull, // IS NOT NULL
}

#[derive(Debug)]
pub struct OrderBy {
    pub column: String,
    pub descending: bool,
}

#[derive(Debug)]
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>,
    pub page: u32,
    pub page_size: u32,
}
```

### 1.4 驱动层优化

```rust
// src-tauri/src/db/postgres_optimized.rs

use super::*;
use sqlx::postgres::PgRow;

impl PostgresDriver {
    /// 优化的查询执行 - 直接从结果集获取列信息
    /// 
    /// 不再执行额外的系统表查询
    pub async fn query_optimized(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        let start = std::time::Instant::now();
        
        // 执行查询
        let result = sqlx::query(sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let execution_time_ms = start.elapsed().as_millis() as u64;
        
        if result.is_empty() {
            // 空结果集：使用 EXPLAIN 获取列信息（仅对 SELECT）
            if sql.trim().to_uppercase().starts_with("SELECT") {
                return Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                    rows_affected: None,
                    execution_time_ms,
                });
            }
            
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms,
            });
        }
        
        // 从第一行直接获取列信息（零额外查询）
        let first_row = &result[0];
        let columns: Vec<ColumnInfo> = first_row
            .columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                data_type: self.map_pg_type(col.type_info()),
                nullable: true,
            })
            .collect();
        
        // 转换行数据
        let rows: Vec<Vec<Option<Value>>> = result
            .iter()
            .map(|row| Self::row_to_values_fast(row, &columns))
            .collect();
        
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms,
        })
    }
    
    /// 快速行值转换（避免重复类型检查）
    fn row_to_values_fast(row: &PgRow, columns: &[ColumnInfo]) -> Vec<Option<Value>> {
        columns
            .iter()
            .enumerate()
            .map(|(i, _)| {
                // 使用更快的类型推断
                row.try_get_raw(i)
                    .ok()
                    .and_then(|raw| {
                        // 根据 PostgreSQL 的 OID 直接判断类型
                        let type_oid = raw.type_info().oid().unwrap_or(0);
                        match type_oid {
                            // int2, int4, int8
                            21 | 23 | 20 => row.try_get::<Option<i64>, _>(i).ok()?.map(Value::Integer),
                            // float4, float8
                            700 | 701 => row.try_get::<Option<f64>, _>(i).ok()?.map(Value::Float),
                            // bool
                            16 => row.try_get::<Option<bool>, _>(i).ok()?.map(Value::Bool),
                            // bytea
                            17 => row.try_get::<Option<Vec<u8>>, _>(i).ok()?.map(Value::Bytes),
                            // json, jsonb
                            114 | 3802 => {
                                row.try_get::<Option<serde_json::Value>, _>(i)
                                    .ok()?
                                    .map(Value::Json)
                            }
                            // timestamp, timestamptz
                            1114 | 1184 => {
                                row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i)
                                    .ok()?
                                    .map(|dt| Value::Timestamp(dt.to_rfc3339()))
                            }
                            // 默认作为字符串
                            _ => row.try_get::<Option<String>, _>(i).ok()?.map(Value::String),
                        }
                    })
            })
            .collect()
    }
    
    /// PostgreSQL 类型 OID 到友好名称的映射
    fn map_pg_type(&self, type_info: &sqlx::postgres::PgTypeInfo) -> String {
        match type_info.oid().unwrap_or(0) {
            16 => "boolean".to_string(),
            17 => "bytea".to_string(),
            20 => "bigint".to_string(),
            21 => "smallint".to_string(),
            23 => "integer".to_string(),
            25 => "text".to_string(),
            114 => "json".to_string(),
            700 => "real".to_string(),
            701 => "double precision".to_string(),
            1043 => "varchar".to_string(),
            1082 => "date".to_string(),
            1114 => "timestamp".to_string(),
            1184 => "timestamptz".to_string(),
            3802 => "jsonb".to_string(),
            oid => format!("unknown({})", oid),
        }
    }
}

/// 批量 Schema 获取优化
impl PostgresDriver {
    /// 一次性获取多个表的 Schema（减少查询次数）
    pub async fn get_tables_schema_batch(
        &self,
        handle: &ConnectionHandle,
        tables: &[String],
    ) -> Result<HashMap<String, TableSchema>, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        // 单次查询获取所有表的列信息
        let column_rows = sqlx::query(r#"
            SELECT 
                table_name,
                column_name,
                data_type,
                is_nullable,
                column_default,
                character_maximum_length,
                numeric_precision,
                col_description((table_schema || '.' || table_name)::regclass, ordinal_position) as comment
            FROM information_schema.columns
            WHERE table_name = ANY($1) AND table_schema = 'public'
            ORDER BY table_name, ordinal_position
        "#)
        .bind(tables)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        // 单次查询获取所有表的主键
        let pk_rows = sqlx::query(r#"
            SELECT 
                t.relname as table_name,
                a.attname as column_name
            FROM pg_index ix
            JOIN pg_class t ON t.oid = ix.indrelid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE t.relname = ANY($1) AND ix.indisprimary
        "#)
        .bind(tables)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        // 组装结果...
        let mut schemas: HashMap<String, TableSchema> = HashMap::new();
        
        // ... 处理逻辑
        
        Ok(schemas)
    }
}
```

### 1.5 缓存失效策略

```rust
// src-tauri/src/cache/invalidation.rs

use super::*;

/// 缓存失效策略
pub enum InvalidationStrategy {
    /// 时间过期（默认 5 分钟）
    TimeBased(Duration),
    /// 事件驱动（DDL 语句执行后）
    EventDriven,
    /// 混合模式
    Hybrid(Duration),
}

/// DDL 检测器 - 检测会修改表结构的 SQL
pub struct DdlDetector;

impl DdlDetector {
    /// 检测 SQL 是否为 DDL 语句
    pub fn is_ddl(sql: &str) -> bool {
        let sql_upper = sql.trim().to_uppercase();
        
        sql_upper.starts_with("CREATE TABLE")
            || sql_upper.starts_with("ALTER TABLE")
            || sql_upper.starts_with("DROP TABLE")
            || sql_upper.starts_with("CREATE INDEX")
            || sql_upper.starts_with("DROP INDEX")
            || sql_upper.starts_with("CREATE VIEW")
            || sql_upper.starts_with("DROP VIEW")
    }
    
    /// 从 DDL 语句中提取表名
    pub fn extract_table_name(sql: &str) -> Option<String> {
        let sql_upper = sql.trim().to_uppercase();
        
        // 简单实现，实际应使用 SQL 解析器
        if sql_upper.starts_with("ALTER TABLE") || sql_upper.starts_with("DROP TABLE") {
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() > 2 {
                let table_name = parts[2].trim_matches('"').trim_matches('`');
                return Some(table_name.to_string());
            }
        }
        
        None
    }
}

/// 在查询执行后自动处理缓存失效
impl QueryExecutor {
    pub async fn execute_with_cache_invalidation(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        sql: &str,
        connection_id: &str,
        database: &str,
    ) -> Result<QueryResult, DriverError> {
        // 执行 SQL
        let result = driver.query(handle, sql).await?;
        
        // 检测是否为 DDL，如果是则使相关缓存失效
        if DdlDetector::is_ddl(sql) {
            if let Some(table_name) = DdlDetector::extract_table_name(sql) {
                tracing::info!("DDL detected, invalidating cache for table: {}", table_name);
                self.schema_cache.invalidate(connection_id, database, Some(&table_name)).await;
            } else {
                // 无法确定具体表，清除整个数据库缓存
                self.schema_cache.invalidate(connection_id, database, None).await;
            }
        }
        
        Ok(result)
    }
}
```

### 1.6 性能对比

| 操作 | 传统方式 | 优化后 | 提升 |
|------|----------|--------|------|
| 首次查询表数据 | 3-5 次 SQL（获取列、主键、索引等） | 1 次 SQL | **80%↓** |
| 后续查询表数据 | 3-5 次 SQL（每次重复查询） | 1 次 SQL | **80%↓** |
| 缓存命中查询 | 3-5 次 SQL | 1 次 SQL | **80%↓** |
| 执行简单 SELECT | 1 次 SQL（无额外开销） | 1 次 SQL | 无变化 |
| 批量获取 10 个表 Schema | 30-50 次 SQL | 2-3 次 SQL | **95%↓** |

```
优化前执行流程（每次 SELECT）:
┌─────────────┐
│ SELECT *    │
│ FROM users  │
└──────┬──────┘
       │
       ▼
┌─────────────────────┐
│ 查询 information_   │  ← 额外查询 1
│ schema.columns      │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ 查询 pg_index       │  ← 额外查询 2
│ 获取主键            │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ 查询 pg_indexes     │  ← 额外查询 3
│ 获取索引            │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ 返回结果            │
└─────────────────────┘

优化后执行流程:
┌─────────────┐     ┌─────────────────────┐
│ SELECT *    │     │ 从缓存读取列信息     │ ← 内存操作
│ FROM users  │ ──► │ (首次从 DB 获取并缓存)│
└─────────────┘     └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │ 返回结果            │
                    └─────────────────────┘
```

### 1.7 内存开销估算

```
单个 TableSchema 内存占用:
- 表名: ~50 bytes
- 列信息: 10 列 × 100 bytes = 1000 bytes
- 主键: ~50 bytes
- 索引: 3 个 × 150 bytes = 450 bytes
- 外键: ~200 bytes
--------------------------------
总计: ~1.75 KB / 表

1000 个表的缓存:
1000 × 1.75 KB = 1.75 MB

结论: 内存开销极小，完全可以接受
```
