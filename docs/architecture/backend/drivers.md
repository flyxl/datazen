# 数据库驱动层

> [返回架构总览](../README.md)

## 1、核心模块设计

### 1.1 数据库驱动抽象层

```rust
// src-tauri/src/db/mod.rs

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// 数据库类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    MariaDB,
    SQLite,
    Redis,
}

/// 统一的数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,  // 加密存储
    pub ssl_mode: SslMode,
    pub connection_timeout: u32,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    pub color_tag: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: Option<String>,
    pub password: Option<String>,
}

/// 查询结果
#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// 统一的值类型
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(serde_json::Value),
}

/// 数据库驱动 Trait - 核心抽象
#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    /// 获取驱动类型
    fn driver_type(&self) -> DatabaseType;
    
    /// 建立连接
    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError>;
    
    /// 测试连接
    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError>;
    
    /// 断开连接
    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError>;
    
    /// 获取数据库列表
    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;
    
    /// 获取表列表
    async fn get_tables(&self, handle: &ConnectionHandle, database: &str) -> Result<Vec<TableInfo>, DriverError>;
    
    /// 获取表结构
    async fn get_table_schema(&self, handle: &ConnectionHandle, table: &str) -> Result<TableSchema, DriverError>;
    
    /// 执行查询
    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError>;
    
    /// 执行带参数的查询
    async fn query_with_params(
        &self, 
        handle: &ConnectionHandle, 
        sql: &str, 
        params: &[Value]
    ) -> Result<QueryResult, DriverError>;
    
    /// 执行更新/插入/删除
    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;
    
    /// 开始事务
    async fn begin_transaction(&self, handle: &ConnectionHandle) -> Result<TransactionHandle, DriverError>;
    
    /// 提交事务
    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError>;
    
    /// 回滚事务
    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError>;
    
    /// 获取查询执行计划
    async fn explain(&self, handle: &ConnectionHandle, sql: &str) -> Result<ExplainResult, DriverError>;
    
    /// 取消正在执行的查询
    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError>;
}

/// 连接句柄 - 内部包含连接池引用
#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    pub id: String,
    pub pool_id: String,
}

/// 事务句柄
#[derive(Debug)]
pub struct TransactionHandle {
    pub id: String,
    pub connection_id: String,
}

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub server_version: String,
    pub server_type: String,
}

#[derive(Debug, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: TableType,
    pub row_count: Option<i64>,
}

#[derive(Debug, Serialize)]
pub enum TableType {
    Table,
    View,
    MaterializedView,
    SystemTable,
}

#[derive(Debug, Serialize)]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

#[derive(Debug, Serialize)]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

#[derive(Debug, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: String,
}

#[derive(Debug, Serialize)]
pub struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Debug, Serialize)]
pub struct ExplainResult {
    pub plan_text: String,
    pub plan_json: Option<serde_json::Value>,
    pub total_cost: Option<f64>,
    pub estimated_rows: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Query failed: {0}")]
    QueryFailed(String),
    
    #[error("Connection timeout")]
    ConnectionTimeout,
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("SSL error: {0}")]
    SslError(String),
    
    #[error("SSH tunnel error: {0}")]
    SshTunnelError(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Driver not found for type: {0:?}")]
    DriverNotFound(DatabaseType),
    
    #[error("Connection pool exhausted")]
    PoolExhausted,
    
    #[error("Transaction error: {0}")]
    TransactionError(String),
}
```

### 1.2 PostgreSQL 驱动实现

```rust
// src-tauri/src/db/postgres.rs

use super::*;
use sqlx::postgres::{PgPoolOptions, PgPool, PgRow};
use sqlx::{Row, Pool, Postgres, postgres::PgConnectOptions};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// PostgreSQL 驱动实现
pub struct PostgresDriver {
    /// 连接池管理器
    pools: Arc<RwLock<HashMap<String, PgPool>>>,
    /// 最大连接数
    max_connections: u32,
    /// 连接超时
    connection_timeout: Duration,
    /// 空闲超时
    idle_timeout: Duration,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600), // 10分钟
        }
    }
    
    /// 构建 PostgreSQL 连接选项
    fn build_connect_options(&self, config: &ConnectionConfig) -> Result<PgConnectOptions, DriverError> {
        let mut options = PgConnectOptions::new();
        
        // 主机和端口
        if let Some(host) = &config.host {
            options = options.host(host);
        }
        if let Some(port) = config.port {
            options = options.port(port);
        }
        
        // 数据库名
        if let Some(database) = &config.database {
            options = options.database(database);
        }
        
        // 用户名和密码
        if let Some(username) = &config.username {
            options = options.username(username);
        }
        if let Some(password) = &config.password {
            options = options.password(password);
        }
        
        // SSL 配置
        options = match config.ssl_mode {
            SslMode::Disable => options.ssl_mode(sqlx::postgres::PgSslMode::Disable),
            SslMode::Prefer => options.ssl_mode(sqlx::postgres::PgSslMode::Prefer),
            SslMode::Require => options.ssl_mode(sqlx::postgres::PgSslMode::Require),
            SslMode::VerifyCa | SslMode::VerifyFull => {
                // 需要配置 CA 证书路径
                options.ssl_mode(sqlx::postgres::PgSslMode::VerifyFull)
            }
        };
        
        // 连接超时
        options = options.connect_timeout(self.connection_timeout);
        
        Ok(options)
    }
    
    /// 创建连接池
    async fn create_pool(&self, config: &ConnectionConfig) -> Result<PgPool, DriverError> {
        let options = self.build_connect_options(config)?;
        
        let pool = PgPoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(1)
            .acquire_timeout(self.connection_timeout)
            .idle_timeout(self.idle_timeout)
            .max_lifetime(Duration::from_secs(3600)) // 1小时最大生命周期
            .connect_with(options)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        
        Ok(pool)
    }
    
    /// 将 PgRow 转换为 Value
    fn row_to_values(row: &PgRow, columns: &[ColumnInfo]) -> Vec<Option<Value>> {
        columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                // 根据列类型获取值
                match col.data_type.to_lowercase().as_str() {
                    "int2" | "int4" | "int8" => {
                        row.try_get::<Option<i64>, _>(i)
                            .ok()
                            .flatten()
                            .map(Value::Integer)
                    }
                    "float4" | "float8" => {
                        row.try_get::<Option<f64>, _>(i)
                            .ok()
                            .flatten()
                            .map(Value::Float)
                    }
                    "bool" => {
                        row.try_get::<Option<bool>, _>(i)
                            .ok()
                            .flatten()
                            .map(Value::Bool)
                    }
                    "bytea" => {
                        row.try_get::<Option<Vec<u8>>, _>(i)
                            .ok()
                            .flatten()
                            .map(Value::Bytes)
                    }
                    "json" | "jsonb" => {
                        row.try_get::<Option<serde_json::Value>, _>(i)
                            .ok()
                            .flatten()
                            .map(Value::Json)
                    }
                    _ => {
                        // 默认作为字符串处理
                        row.try_get::<Option<String>, _>(i)
                            .ok()
                            .flatten()
                            .map(Value::String)
                    }
                }
            })
            .collect()
    }
}

#[async_trait]
impl DatabaseDriver for PostgresDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }
    
    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let pool = self.create_pool(config).await?;
        
        let handle_id = uuid::Uuid::new_v4().to_string();
        let pool_id = handle_id.clone();
        
        // 存储连接池
        let mut pools = self.pools.write().await;
        pools.insert(pool_id.clone(), pool);
        
        Ok(ConnectionHandle {
            id: handle_id,
            pool_id,
        })
    }
    
    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let pool = self.create_pool(config).await?;
        
        // 执行简单查询获取服务器信息
        let row = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let version: String = row.get(0);
        
        // 立即关闭测试连接池
        pool.close().await;
        
        Ok(ServerInfo {
            server_version: version,
            server_type: "PostgreSQL".to_string(),
        })
    }
    
    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let mut pools = self.pools.write().await;
        
        if let Some(pool) = pools.remove(&handle.pool_id) {
            pool.close().await;
        }
        
        Ok(())
    }
    
    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        let rows = sqlx::query(
            "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let databases: Vec<String> = rows.iter().map(|row| row.get(0)).collect();
        
        Ok(databases)
    }
    
    async fn get_tables(&self, handle: &ConnectionHandle, database: &str) -> Result<Vec<TableInfo>, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        let rows = sqlx::query(r#"
            SELECT 
                table_name,
                table_type,
                (SELECT reltuples::bigint FROM pg_class WHERE relname = t.table_name) as row_count
            FROM information_schema.tables t
            WHERE table_schema = 'public'
            ORDER BY table_name
        "#)
        .bind(database)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let tables: Vec<TableInfo> = rows
            .iter()
            .map(|row| {
                let table_type_str: String = row.get("table_type");
                let table_type = match table_type_str.as_str() {
                    "BASE TABLE" => TableType::Table,
                    "VIEW" => TableType::View,
                    "MATERIALIZED VIEW" => TableType::MaterializedView,
                    _ => TableType::Table,
                };
                
                TableInfo {
                    name: row.get("table_name"),
                    schema: Some("public".to_string()),
                    table_type,
                    row_count: Some(row.get("row_count")),
                }
            })
            .collect();
        
        Ok(tables)
    }
    
    async fn get_table_schema(&self, handle: &ConnectionHandle, table: &str) -> Result<TableSchema, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        // 获取列信息
        let column_rows = sqlx::query(r#"
            SELECT 
                column_name,
                data_type,
                is_nullable,
                column_default,
                character_maximum_length,
                numeric_precision,
                col_description((table_schema || '.' || table_name)::regclass, ordinal_position) as comment
            FROM information_schema.columns
            WHERE table_name = $1 AND table_schema = 'public'
            ORDER BY ordinal_position
        "#)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let columns: Vec<ColumnSchema> = column_rows
            .iter()
            .map(|row| {
                let data_type: String = row.get("data_type");
                let char_len: Option<i32> = row.get("character_maximum_length");
                let num_prec: Option<i32> = row.get("numeric_precision");
                
                let full_type = if let Some(len) = char_len {
                    format!("{}({})", data_type, len)
                } else if let Some(prec) = num_prec {
                    format!("{}({})", data_type, prec)
                } else {
                    data_type
                };
                
                ColumnSchema {
                    name: row.get("column_name"),
                    data_type: full_type,
                    nullable: row.get::<String, _>("is_nullable") == "YES",
                    default_value: row.get("column_default"),
                    comment: row.get("comment"),
                    is_primary_key: false, // 稍后更新
                    is_auto_increment: false,
                }
            })
            .collect();
        
        // 获取主键
        let pk_rows = sqlx::query(r#"
            SELECT a.attname
            FROM pg_index i
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
            WHERE i.indrelid = $1::regclass AND i.indisprimary
        "#)
        .bind(format!("public.{}", table))
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let primary_keys: Vec<String> = pk_rows.iter().map(|row| row.get(0)).collect();
        
        // 获取索引信息
        let index_rows = sqlx::query(r#"
            SELECT 
                i.relname as index_name,
                array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                ix.indisunique,
                ix.indisprimary,
                am.amname as index_type
            FROM pg_index ix
            JOIN pg_class t ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_am am ON am.oid = i.relam
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE t.relname = $1
            GROUP BY i.relname, ix.indisunique, ix.indisprimary, am.amname
        "#)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let indexes: Vec<IndexInfo> = index_rows
            .iter()
            .map(|row| {
                let columns: Vec<String> = row.get("columns");
                IndexInfo {
                    name: row.get("index_name"),
                    columns,
                    is_unique: row.get("indisunique"),
                    is_primary: row.get("indisprimary"),
                    index_type: row.get("index_type"),
                }
            })
            .collect();
        
        // 获取外键
        let fk_rows = sqlx::query(r#"
            SELECT
                tc.constraint_name,
                kcu.column_name,
                ccu.table_name AS foreign_table_name,
                ccu.column_name AS foreign_column_name,
                rc.update_rule,
                rc.delete_rule
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
                ON tc.constraint_name = kcu.constraint_name
            JOIN information_schema.constraint_column_usage AS ccu
                ON ccu.constraint_name = tc.constraint_name
            JOIN information_schema.referential_constraints AS rc
                ON rc.constraint_name = tc.constraint_name
            WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_name = $1
        "#)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let foreign_keys: Vec<ForeignKeyInfo> = fk_rows
            .iter()
            .map(|row| ForeignKeyInfo {
                name: row.get("constraint_name"),
                columns: vec![row.get("column_name")],
                referenced_table: row.get("foreign_table_name"),
                referenced_columns: vec![row.get("foreign_column_name")],
                on_update: row.get("update_rule"),
                on_delete: row.get("delete_rule"),
            })
            .collect();
        
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes,
            foreign_keys,
        })
    }
    
    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
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
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms,
            });
        }
        
        // 获取列信息
        let first_row = &result[0];
        let columns: Vec<ColumnInfo> = first_row
            .columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().to_string(),
                nullable: true, // PostgreSQL 不直接提供可空信息
            })
            .collect();
        
        // 转换行数据
        let rows: Vec<Vec<Option<Value>>> = result
            .iter()
            .map(|row| Self::row_to_values(row, &columns))
            .collect();
        
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms,
        })
    }
    
    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        let result = sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        Ok(result.rows_affected())
    }
    
    async fn begin_transaction(&self, handle: &ConnectionHandle) -> Result<TransactionHandle, DriverError> {
        // TODO: 实现事务管理
        Ok(TransactionHandle {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: handle.id.clone(),
        })
    }
    
    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        // TODO: 实现事务提交
        Ok(())
    }
    
    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        // TODO: 实现事务回滚
        Ok(())
    }
    
    async fn explain(&self, handle: &ConnectionHandle, sql: &str) -> Result<ExplainResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {}", sql);
        
        let row = sqlx::query(&explain_sql)
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
        let plan_json: serde_json::Value = row.get(0);
        let plan_text = serde_json::to_string_pretty(&plan_json).unwrap_or_default();
        
        // 解析关键指标
        let total_cost = plan_json
            .pointer("/0/Plan/Total Cost")
            .and_then(|v| v.as_f64());
        let estimated_rows = plan_json
            .pointer("/0/Plan/Plan Rows")
            .and_then(|v| v.as_i64());
        
        Ok(ExplainResult {
            plan_text,
            plan_json: Some(plan_json),
            total_cost,
            estimated_rows,
        })
    }
    
    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        // 取消同数据库内所有活跃查询（排除自身连接）
        let rows = sqlx::query(
            "SELECT pg_cancel_backend(pid) \
             FROM pg_stat_activity \
             WHERE pid != pg_backend_pid() \
               AND state = 'active' \
               AND datname = current_database()",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        tracing::info!(cancelled = rows.len(), "pg: cancelled active queries");
        Ok(())
    }
    
    async fn query_with_params(
        &self, 
        handle: &ConnectionHandle, 
        sql: &str, 
        params: &[Value]
    ) -> Result<QueryResult, DriverError> {
        // TODO: 实现参数化查询
        self.query(handle, sql).await
    }
}

/// 实现 Drop 以确保资源清理
impl Drop for PostgresDriver {
    fn drop(&mut self) {
        // 在同步上下文中无法直接调用 async close
        // 使用 tokio runtime 来关闭连接池
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let pools = self.pools.clone();
            rt.spawn(async move {
                let mut pools = pools.write().await;
                for (_, pool) in pools.drain() {
                    pool.close().await;
                }
            });
        }
    }
}
```

### 1.3 驱动注册表

```rust
// src-tauri/src/db/registry.rs

use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 驱动注册表 - 管理所有数据库驱动
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 注册驱动
    pub async fn register(&self, driver: Arc<dyn DatabaseDriver>) {
        let mut drivers = self.drivers.write().await;
        drivers.insert(driver.driver_type(), driver);
    }
    
    /// 获取驱动
    pub async fn get(&self, db_type: &DatabaseType) -> Option<Arc<dyn DatabaseDriver>> {
        let drivers = self.drivers.read().await;
        drivers.get(db_type).cloned()
    }
    
    /// 获取支持的数据库类型列表
    pub async fn supported_types(&self) -> Vec<DatabaseType> {
        let drivers = self.drivers.read().await;
        drivers.keys().cloned().collect()
    }
}

/// 初始化所有驱动
pub async fn init_drivers() -> DriverRegistry {
    let registry = DriverRegistry::new();
    
    // 注册 PostgreSQL 驱动
    registry.register(Arc::new(PostgresDriver::new())).await;
    
    // 注册 MySQL 驱动 (类似实现)
    // registry.register(Arc::new(MySqlDriver::new())).await;
    
    // 注册 SQLite 驱动
    // registry.register(Arc::new(SqliteDriver::new())).await;
    
    registry
}
```

## 2、扩展新数据库类型

### 2.1 扩展步骤

添加新数据库类型只需：

1. **实现 `DatabaseDriver` Trait**

```rust
// src-tauri/src/db/mongodb.rs (示例)

use super::*;

pub struct MongoDbDriver {
    // MongoDB 特定的实现
}

#[async_trait]
impl DatabaseDriver for MongoDbDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::MongoDB  // 新增类型
    }
    
    // 实现所有 trait 方法...
}
```

2. **添加数据库类型枚举**

```rust
// src-tauri/src/db/mod.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    MariaDB,
    SQLite,
    Redis,
    MongoDB,  // 新增
}
```

3. **注册驱动**

```rust
// src-tauri/src/db/registry.rs

pub async fn init_drivers() -> DriverRegistry {
    let registry = DriverRegistry::new();
    
    registry.register(Arc::new(PostgresDriver::new())).await;
    registry.register(Arc::new(MongoDbDriver::new())).await;  // 新增
    
    registry
}
```

### 2.2 驱动开发规范

```rust
/// 驱动开发规范文档
/// 
/// 1. 所有驱动必须实现 DatabaseDriver trait
/// 2. 使用连接池管理连接，避免频繁创建/销毁
/// 3. 实现超时机制，防止查询无限等待
/// 4. 正确处理错误，转换为统一的 DriverError
/// 5. 实现 Drop trait 确保资源释放
/// 6. 编写单元测试覆盖主要功能（写在本驱动 crate 内，不要写到 src-tauri）
/// 
/// 示例：
/// 
/// impl Drop for MyDriver {
///     fn drop(&mut self) {
///         // 清理连接池
///         // 关闭所有连接
///     }
/// }
```

## 3、数据库类型扩展架构（2026-07 重构后）

### 3.1 Commands 模块划分

`src-tauri/src/commands/` 按领域拆分为子模块，`mod.rs` 仅保留 `AppState`、re-export 和 `log_err`：

| 模块 | 职责 |
|------|------|
| `connection.rs` | 连接 CRUD、测试、connect/disconnect |
| `schema.rs` | 数据库/表/列/表数据 |
| `query.rs` | SQL 执行、explain、查询历史/收藏 |
| `data.rs` | 行级更新（`commit_row_updates`） |
| `backup.rs` | 备份/恢复 |
| `commands/sync/` | Data Sync IPC（`inspect_data_sync` / `compare_data_sync` / `execute_data_sync`；legacy sync IPC 已移除） |
| `kiwi.rs` | Kiwi OAuth 登录/实例列表 |
| `config.rs` | 设置、分组、导入导出（含加密） |
| `file.rs` | 对话框系文件保存/打开与流式导出、编辑器右键菜单（纯路径读写 IPC 已随 IPC 重构删除） |

### 3.2 驱动接口隔离

```
DatabaseDriver (SQL 通用)
├── format_sql_literal()     # 值字面量格式化（PG: TRUE/FALSE, MySQL: 1/0）
├── build_update_sql()       # UPDATE 语句构建
└── skip_count_query()       # 是否跳过 COUNT(*)（Kiwi 覆盖为 true）

KeyValueDriver (Redis 驱动内部实现；UI 经 `scan_keys`/`get_key` Driver Command)
├── scan_keys_with_info()
└── get_key_detail()
```

- `DriverRegistry::get()` — 获取 `Arc<dyn DatabaseDriver>`
- Redis KV 浏览器经 `execute_driver_command`（`scan_keys` / `get_key`），不再走 Host 专用 IPC

### 3.3 添加新 DB 类型检查清单

1. `db/mod.rs` — `DatabaseType` 枚举添加变体
2. `db/` — 实现 `DatabaseDriver`（+ 可选 `KeyValueDriver`）
3. `db/registry.rs` — `init_drivers()` 注册；KV 类型在 `get_kv_driver()` 映射
4. 按需覆盖：`skip_count_query`、`format_sql_literal`、`build_update_sql`
5. 新型 IPC 命令放入对应 `commands/*.rs` 子模块
6. `lib.rs` — `generate_handler` 注册新命令

## 4. 查询结果流式传输

SQL 编辑器走 `execute_query_stream`（Tauri `Channel`），按批发送行，避免一次性把整个结果集序列化进 IPC。

`query_multi` / Driver Command `query` 仍一次性返回完整 `MultiQueryResult`，供 MCP、Workflow、Schema 缓存等需要完整结果的路径使用。

### 与「限制 SELECT 结果行数」的关系

这两件事互相独立，禁止混用：

| | 限制 SELECT 结果行数 | 流式传输 |
|---|---|---|
| 作用 | 改写 SQL（`LIMIT n+1`）或在驱动侧截断 | 传输与解码分批进行 |
| 开关 | `AppSettings.limit_select_results` | 编辑器路径始终开启 |
| 关闭开关时 | `limit = None`，返回全部行 | 仍然分批推送全部行 |
| 批大小 | 不是行数上限 | `QUERY_STREAM_BATCH_SIZE`（默认 500），只影响 IPC 分片 |

各 path 驱动覆盖 `DatabaseDriver::query_stream`，按协议能力分三档：

| 档次 | 驱动 | 行为 |
|---|---|---|
| 逐行 fetch | Postgres / MySQL / SQLite（sqlx `.fetch()`）、DuckDB（`rows.next()`）、SQL Server（tiberius `QueryItem`）、MongoDB（cursor `try_next`） | 驱动侧不攒完整结果集 |
| 服务端分页 | Elasticsearch（`/_sql` cursor）、HBase（scanner 循环） | 按批向服务端取下一页 |
| 解析后分批 emit | ClickHouse / Turso / rqlite / InfluxDB / Vector / VictoriaMetrics / Redis | HTTP/RESP 通常一次返回 JSON，解码后用 `QueryRowBatcher` 分批推给 IPC；SQL 类仍走 `append_select_limit`（`LIMIT n+1`） |

`ReuseDriver` 必须转发 `query_stream`，以便 Doris / MariaDB 等复用驱动拿到内层的真正流式实现。

默认实现（先 `query_multi` 再 `emit_multi_query_as_stream`）只留给未覆盖的 Git 驱动或第三方插件。
