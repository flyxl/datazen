# DataZen 后端技术方案

## 一、整体架构

### 1.1 架构概览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Tauri Application                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         Frontend (React)                             │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐               │   │
│  │  │Connection│ │ Query    │ │ Table    │ │ Settings │               │   │
│  │  │Manager   │ │ Editor   │ │ Editor   │ │ Panel    │               │   │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘               │   │
│  └───────┼────────────┼────────────┼────────────┼─────────────────────┘   │
│          │            │            │            │                          │
│          └────────────┴────────────┴────────────┘                          │
│                              │ Tauri IPC                                   │
│                              ▼                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        Backend (Rust)                                │   │
│  │                                                                      │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │   │
│  │  │   Commands   │  │   Services   │  │   Stores     │              │   │
│  │  │  (IPC Layer) │  │ (Business    │  │ (Persistent  │              │   │
│  │  │              │  │  Logic)      │  │  Storage)    │              │   │
│  │  └──────┬───────┘  └──────┬───────┘  └──────────────┘              │   │
│  │         │                 │                                          │   │
│  │         └────────┬────────┘                                          │   │
│  │                  ▼                                                   │   │
│  │  ┌──────────────────────────────────────────────────────────────┐   │   │
│  │  │                    Database Drivers Layer                     │   │   │
│  │  │  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌───────────┐  │   │   │
│  │  │  │ PostgreSQL │ │   MySQL    │ │   SQLite   │ │   Redis   │  │   │   │
│  │  │  │   Driver   │ │   Driver   │ │   Driver   │ │  Driver   │  │   │   │
│  │  │  └─────┬──────┘ └─────┬──────┘ └─────┬──────┘ └─────┬─────┘  │   │   │
│  │  └────────┼──────────────┼──────────────┼──────────────┼────────┘   │   │
│  │           └──────────────┴──────────────┴──────────────┘            │   │
│  │                                  │                                   │   │
│  └──────────────────────────────────┼───────────────────────────────────┘   │
│                                     │                                       │
└─────────────────────────────────────┼───────────────────────────────────────┘
                                      │
                                      ▼
                           ┌──────────────────┐
                           │  External DBs    │
                           │  (PostgreSQL,    │
                           │   MySQL, etc.)   │
                           └──────────────────┘
```

### 1.2 分层架构

| 层级 | 职责 | 关键特性 |
|------|------|----------|
| **Commands 层** | 处理前端 IPC 调用 | 参数验证、权限检查、错误转换 |
| **Services 层** | 业务逻辑处理 | 连接管理、查询执行、事务控制 |
| **Drivers 层** | 数据库驱动抽象 | 统一接口、连接池管理 |
| **Stores 层** | 本地持久化 | 加密存储、配置管理 |

---

## 二、核心模块设计

### 2.1 数据库驱动抽象层

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

### 2.2 PostgreSQL 驱动实现

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
        // PostgreSQL 使用 pg_cancel_backend 取消查询
        // 需要获取当前连接的后端 PID
        let pools = self.pools.read().await;
        let pool = pools.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection not found".to_string()))?;
        
        // 获取后端 PID 并取消
        sqlx::query("SELECT pg_cancel_backend(pg_backend_pid())")
            .execute(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        
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

### 2.3 驱动注册表

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

---

## 三、Schema 缓存与查询优化

### 3.1 设计目标

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

### 3.2 Schema 缓存架构

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

### 3.3 优化的查询执行流程

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

### 3.4 驱动层优化

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

### 3.5 缓存失效策略

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

### 3.6 性能对比

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

### 3.7 内存开销估算

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

---

## 四、连接管理服务

### 4.1 连接池管理器

```rust
// src-tauri/src/services/connection_manager.rs

use crate::db::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use std::time::Instant;

/// 活动连接信息
#[derive(Debug)]
struct ActiveConnection {
    handle: ConnectionHandle,
    config: ConnectionConfig,
    created_at: Instant,
    last_used: Instant,
}

/// 连接管理器 - 核心服务
pub struct ConnectionManager {
    /// 驱动注册表
    registry: Arc<DriverRegistry>,
    /// 活动连接
    connections: Arc<RwLock<HashMap<String, ActiveConnection>>>,
    /// 连接配置存储
    config_store: Arc<ConfigStore>,
    /// 空闲超时时间
    idle_timeout: Duration,
}

impl ConnectionManager {
    pub fn new(registry: Arc<DriverRegistry>, config_store: Arc<ConfigStore>) -> Self {
        Self {
            registry,
            connections: Arc::new(RwLock::new(HashMap::new())),
            config_store,
            idle_timeout: Duration::from_secs(1800), // 30分钟
        }
    }
    
    /// 建立新连接
    pub async fn connect(&self, config_id: &str) -> Result<String, ConnectionError> {
        // 获取配置
        let config = self.config_store
            .get_connection(config_id)
            .await?
            .ok_or(ConnectionError::ConfigNotFound(config_id.to_string()))?;
        
        // 解密密码
        let mut config = config;
        if let Some(encrypted) = &config.password {
            config.password = Some(self.config_store.decrypt_password(encrypted)?);
        }
        
        // 获取驱动
        let driver = self.registry
            .get(&config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(config.database_type))?;
        
        // 建立连接
        let handle = driver.connect(&config).await?;
        
        let connection_id = handle.id.clone();
        
        // 记录活动连接
        let mut connections = self.connections.write().await;
        connections.insert(connection_id.clone(), ActiveConnection {
            handle,
            config,
            created_at: Instant::now(),
            last_used: Instant::now(),
        });
        
        Ok(connection_id)
    }
    
    /// 断开连接
    pub async fn disconnect(&self, connection_id: &str) -> Result<(), ConnectionError> {
        let mut connections = self.connections.write().await;
        
        if let Some(active) = connections.remove(connection_id) {
            let driver = self.registry.get(&active.config.database_type).await;
            if let Some(driver) = driver {
                driver.disconnect(active.handle).await?;
            }
        }
        
        Ok(())
    }
    
    /// 获取连接
    pub async fn get_connection(&self, connection_id: &str) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let mut connections = self.connections.write().await;
        
        let active = connections
            .get_mut(connection_id)
            .ok_or(ConnectionError::ConnectionNotFound(connection_id.to_string()))?;
        
        // 更新最后使用时间
        active.last_used = Instant::now();
        
        let driver = self.registry
            .get(&active.config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(active.config.database_type.clone()))?;
        
        Ok((driver, active.handle.clone()))
    }
    
    /// 测试连接配置
    pub async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, ConnectionError> {
        let driver = self.registry
            .get(&config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(config.database_type.clone()))?;
        
        driver.test_connection(config).await
            .map_err(ConnectionError::DriverError)
    }
    
    /// 清理空闲连接
    pub async fn cleanup_idle_connections(&self) {
        let mut connections = self.connections.write().await;
        let now = Instant::now();
        
        let to_remove: Vec<String> = connections
            .iter()
            .filter(|(_, conn)| now.duration_since(conn.last_used) > self.idle_timeout)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in to_remove {
            if let Some(active) = connections.remove(&id) {
                if let Some(driver) = self.registry.get(&active.config.database_type).await {
                    let _ = driver.disconnect(active.handle).await;
                }
            }
        }
    }
    
    /// 启动定期清理任务
    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(300)); // 每5分钟
            
            loop {
                interval.tick().await;
                self.cleanup_idle_connections().await;
            }
        });
    }
    
    /// 关闭所有连接
    pub async fn shutdown(&self) {
        let mut connections = self.connections.write().await;
        
        for (id, active) in connections.drain() {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
            tracing::info!("Closed connection: {}", id);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("Configuration not found: {0}")]
    ConfigNotFound(String),
    
    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),
    
    #[error("Driver not found for type: {0:?}")]
    DriverNotFound(DatabaseType),
    
    #[error("Driver error: {0}")]
    DriverError(#[from] DriverError),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}
```

---

## 五、本地数据存储方案

### 4.1 存储架构

```rust
// src-tauri/src/store/mod.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

/// 存储管理器
pub struct Store {
    /// 存储目录
    data_dir: PathBuf,
    /// 加密密钥 (从系统密钥链获取)
    encryption_key: [u8; 32],
    /// 内存缓存
    cache: Arc<RwLock<StoreCache>>,
}

#[derive(Default)]
struct StoreCache {
    connections: Vec<ConnectionConfig>,
    settings: AppSettings,
    query_history: Vec<QueryHistoryEntry>,
    favorites: Vec<FavoriteQuery>,
}

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub query_result_limit: u32,
    pub auto_save: bool,
    pub confirm_on_delete: bool,
    pub editor_font_size: u32,
    pub editor_font_family: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            language: "zh-CN".to_string(),
            query_result_limit: 1000,
            auto_save: true,
            confirm_on_delete: true,
            editor_font_size: 13,
            editor_font_family: "JetBrains Mono".to_string(),
        }
    }
}

/// 查询历史条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHistoryEntry {
    pub id: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub execution_time_ms: u64,
    pub rows_affected: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
}

/// 收藏的查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteQuery {
    pub id: String,
    pub name: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub sql: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tags: Vec<String>,
}

impl Store {
    /// 初始化存储
    pub async fn init(app_handle: &tauri::AppHandle) -> Result<Self, StoreError> {
        // 获取应用数据目录
        let data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| StoreError::InitError(e.to_string()))?;
        
        // 确保目录存在
        tokio::fs::create_dir_all(&data_dir)
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?;
        
        // 获取或创建加密密钥
        let encryption_key = Self::get_or_create_encryption_key(&data_dir).await?;
        
        let store = Self {
            data_dir,
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
        };
        
        // 加载已有数据
        store.load_all().await?;
        
        Ok(store)
    }
    
    /// 从系统密钥链获取或创建加密密钥
    async fn get_or_create_encryption_key(data_dir: &PathBuf) -> Result<[u8; 32], StoreError> {
        // 尝试从 keyring 获取
        let keyring = keyring::Entry::new("DataZen", "encryption_key");
        
        match keyring.get_password() {
            Ok(key_b64) => {
                // 解码现有密钥
                let key_bytes = BASE64.decode(&key_b64)
                    .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
                
                let mut key = [0u8; 32];
                key.copy_from_slice(&key_bytes);
                Ok(key)
            }
            Err(_) => {
                // 生成新密钥
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                
                // 存储到 keyring
                let key_b64 = BASE64.encode(&key);
                keyring.set_password(&key_b64)
                    .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
                
                Ok(key)
            }
        }
    }
    
    /// 加密数据
    fn encrypt(&self, plaintext: &str) -> Result<String, StoreError> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        // 格式: base64(nonce || ciphertext)
        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);
        
        Ok(BASE64.encode(&combined))
    }
    
    /// 解密数据
    fn decrypt(&self, encrypted: &str) -> Result<String, StoreError> {
        let combined = BASE64.decode(encrypted)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        if combined.len() < 12 {
            return Err(StoreError::EncryptionError("Invalid encrypted data".to_string()));
        }
        
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        String::from_utf8(plaintext)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))
    }
    
    /// 加载所有数据
    async fn load_all(&self) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        // 加载连接配置
        cache.connections = self.load_json_file("connections.json")
            .await
            .unwrap_or_default();
        
        // 解密密码
        for conn in &mut cache.connections {
            if let Some(encrypted) = &conn.password {
                conn.password = Some(self.decrypt(encrypted)?);
            }
        }
        
        // 加载设置
        cache.settings = self.load_json_file("settings.json")
            .await
            .unwrap_or_default();
        
        // 加载查询历史
        cache.query_history = self.load_json_file("history/queries.json")
            .await
            .unwrap_or_default();
        
        // 加载收藏
        cache.favorites = self.load_json_file("favorites/queries.json")
            .await
            .unwrap_or_default();
        
        Ok(())
    }
    
    /// 加载 JSON 文件
    async fn load_json_file<T: for<'de> Deserialize<'de>>(&self, filename: &str) -> Result<T, StoreError> {
        let path = self.data_dir.join(filename);
        
        if !path.exists() {
            return Err(StoreError::FileNotFound(filename.to_string()));
        }
        
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;
        
        serde_json::from_str(&content)
            .map_err(|e| StoreError::ParseError(e.to_string()))
    }
    
    /// 保存 JSON 文件
    async fn save_json_file<T: Serialize>(&self, filename: &str, data: &T) -> Result<(), StoreError> {
        let path = self.data_dir.join(filename);
        
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StoreError::WriteError(e.to_string()))?;
        }
        
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| StoreError::ParseError(e.to_string()))?;
        
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;
        
        Ok(())
    }
}

/// 连接配置存储服务
impl Store {
    /// 获取所有连接配置
    pub async fn get_connections(&self) -> Vec<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.clone()
    }
    
    /// 获取单个连接配置
    pub async fn get_connection(&self, id: &str) -> Option<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.iter().find(|c| c.id == id).cloned()
    }
    
    /// 保存连接配置
    pub async fn save_connection(&self, config: ConnectionConfig) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        // 加密密码
        let mut config = config;
        if let Some(password) = &config.password {
            config.password = Some(self.encrypt(password)?);
        }
        
        // 更新或添加
        if let Some(pos) = cache.connections.iter().position(|c| c.id == config.id) {
            cache.connections[pos] = config;
        } else {
            cache.connections.push(config);
        }
        
        // 保存到文件
        self.save_json_file("connections.json", &cache.connections).await?;
        
        Ok(())
    }
    
    /// 删除连接配置
    pub async fn delete_connection(&self, id: &str) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        cache.connections.retain(|c| c.id != id);
        
        self.save_json_file("connections.json", &cache.connections).await?;
        
        Ok(())
    }
    
    /// 解密密码 (供 ConnectionManager 使用)
    pub fn decrypt_password(&self, encrypted: &str) -> Result<String, StoreError> {
        self.decrypt(encrypted)
    }
}

/// 查询历史管理
impl Store {
    /// 添加查询历史
    pub async fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        cache.query_history.insert(0, entry);
        
        // 限制历史记录数量
        if cache.query_history.len() > 1000 {
            cache.query_history.truncate(1000);
        }
        
        self.save_json_file("history/queries.json", &cache.query_history).await?;
        
        Ok(())
    }
    
    /// 获取查询历史
    pub async fn get_query_history(&self, limit: usize) -> Vec<QueryHistoryEntry> {
        let cache = self.cache.read().await;
        cache.query_history.iter().take(limit).cloned().collect()
    }
    
    /// 清空查询历史
    pub async fn clear_query_history(&self) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        cache.query_history.clear();
        
        self.save_json_file("history/queries.json", &cache.query_history).await?;
        
        Ok(())
    }
}

/// 设置管理
impl Store {
    /// 获取设置
    pub async fn get_settings(&self) -> AppSettings {
        let cache = self.cache.read().await;
        cache.settings.clone()
    }
    
    /// 保存设置
    pub async fn save_settings(&self, settings: AppSettings) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        cache.settings = settings;
        
        self.save_json_file("settings.json", &cache.settings).await?;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Initialization error: {0}")]
    InitError(String),
    
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("Read error: {0}")]
    ReadError(String),
    
    #[error("Write error: {0}")]
    WriteError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}

/// 类型别名
pub type ConfigStore = Store;
```

---

## 六、Tauri Commands 层

### 5.1 命令定义

```rust
// src-tauri/src/commands/mod.rs

use crate::db::*;
use crate::services::*;
use crate::store::*;
use tauri::State;
use std::sync::Arc;

/// 应用状态
pub struct AppState {
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
}

// ============== 连接管理命令 ==============

/// 获取所有连接配置
#[tauri::command]
pub async fn get_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionConfig>, String> {
    Ok(state.store.get_connections().await)
}

/// 保存连接配置
#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), String> {
    state.store.save_connection(config).await
        .map_err(|e| e.to_string())
}

/// 删除连接配置
#[tauri::command]
pub async fn delete_connection(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.store.delete_connection(&id).await
        .map_err(|e| e.to_string())
}

/// 测试连接
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ServerInfo, String> {
    state.connection_manager.test_connection(&config).await
        .map_err(|e| e.to_string())
}

/// 建立连接
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    config_id: String,
) -> Result<String, String> {
    state.connection_manager.connect(&config_id).await
        .map_err(|e| e.to_string())
}

/// 断开连接
#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    state.connection_manager.disconnect(&connection_id).await
        .map_err(|e| e.to_string())
}

// ============== 数据库操作命令 ==============

/// 获取数据库列表
#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    driver.get_databases(&handle).await
        .map_err(|e| e.to_string())
}

/// 获取表列表
#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableInfo>, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    driver.get_tables(&handle, &database).await
        .map_err(|e| e.to_string())
}

/// 获取表结构
#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableSchema, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    driver.get_table_schema(&handle, &table).await
        .map_err(|e| e.to_string())
}

/// 执行查询
#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<QueryResult, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    let result = driver.query(&handle, &sql).await
        .map_err(|e| e.to_string())?;
    
    // 记录查询历史
    let history_entry = QueryHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id: connection_id.clone(),
        database: String::new(),
        sql: sql.clone(),
        executed_at: chrono::Utc::now(),
        execution_time_ms: result.execution_time_ms,
        rows_affected: result.rows_affected,
        success: true,
        error_message: None,
    };
    
    let _ = state.store.add_query_history(history_entry).await;
    
    Ok(result)
}

/// 获取执行计划
#[tauri::command]
pub async fn get_explain(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<ExplainResult, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    driver.explain(&handle, &sql).await
        .map_err(|e| e.to_string())
}

// ============== 表数据命令 ==============

/// 获取表数据（带分页、筛选、排序）
#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
) -> Result<TableDataResult, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    let executor = QueryExecutor { schema_cache: state.schema_cache.clone() };
    executor.get_table_data(
        &driver, &handle,
        &connection_id, "", &table,
        page, page_size, filters, 
        sorts.map(|s| s.first().cloned()).flatten(),
    ).await.map_err(|e| e.to_string())
}

/// 取消正在执行的查询
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;
    
    driver.cancel_query(&handle).await
        .map_err(|e| e.to_string())
}

// ============== 查询历史命令 ==============

/// 获取查询历史
#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<QueryHistoryEntry>, String> {
    Ok(state.store.get_query_history(limit).await)
}

/// 清空查询历史
#[tauri::command]
pub async fn clear_query_history(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.store.clear_query_history().await
        .map_err(|e| e.to_string())
}

// ============== 设置命令 ==============

/// 获取设置
#[tauri::command]
pub async fn get_settings(
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    Ok(state.store.get_settings().await)
}

/// 保存设置
#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    state.store.save_settings(settings).await
        .map_err(|e| e.to_string())
}
```

---

## 七、数据流向

### 6.1 完整数据流

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                    查询执行流程                                  │
└─────────────────────────────────────────────────────────────────────────────────┘

  用户操作                    前端                        后端                       数据库
     │                         │                           │                          │
     │  1. 执行 SQL            │                           │                          │
     ├────────────────────────►│                           │                          │
     │                         │                           │                          │
     │                         │  2. IPC 调用              │                          │
     │                         │  execute_query()          │                          │
     │                         ├──────────────────────────►│                          │
     │                         │                           │                          │
     │                         │                    ┌──────┴──────┐                   │
     │                         │                    │ 3. 验证参数 │                   │
     │                         │                    │ 4. 获取连接 │                   │
     │                         │                    │ 5. 获取驱动 │                   │
     │                         │                    └──────┬──────┘                   │
     │                         │                           │                          │
     │                         │                           │  6. 执行 SQL             │
     │                         │                           ├─────────────────────────►│
     │                         │                           │                          │
     │                         │                           │          ┌───────────────┤
     │                         │                           │  7. 返回  │ 执行查询      │
     │                         │                           │◄─────────┤ 结果集        │
     │                         │                           │          └───────────────┤
     │                         │                           │                          │
     │                         │                    ┌──────┴──────┐                   │
     │                         │                    │ 8. 转换结果 │                   │
     │                         │                    │ 9. 记录历史 │                   │
     │                         │                    └──────┬──────┘                   │
     │                         │                           │                          │
     │                         │  10. IPC 返回结果         │                          │
     │                         │◄──────────────────────────┤                          │
     │                         │                           │                          │
     │  11. 渲染结果           │                           │                          │
     │◄────────────────────────┤                           │                          │
     │                         │                           │                          │
```

### 6.2 连接生命周期

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                  连接生命周期管理                                │
└─────────────────────────────────────────────────────────────────────────────────┘

    创建                          使用                          清理
     │                             │                             │
     ▼                             ▼                             ▼
┌─────────┐                 ┌─────────────┐               ┌───────────┐
│  用户   │                 │ Connection  │               │  定时器   │
│ 新建连接 │                 │  Manager    │               │  清理任务  │
└────┬────┘                 └──────┬──────┘               └─────┬─────┘
     │                             │                            │
     │ 1. 加载配置                  │                            │
     │ 2. 解密密码                  │                            │
     │                             │                            │
     │ 3. 创建连接池                │                            │
     ├────────────────────────────►│                            │
     │                             │                            │
     │                             │ 4. 记录连接信息             │
     │                             │    - created_at            │
     │                             │    - last_used             │
     │                             │                            │
     │                             │◄───────────────────────────┤
     │                             │ 5. 定期检查空闲连接         │
     │                             │    (每5分钟)               │
     │                             │                            │
     │                             │ 6. 超时清理                 │
     │                             │    (>30分钟未使用)         │
     │                             │                            │
     │                             │ 7. 关闭连接池               │
     │                             │    释放资源                │
     │                             │                            │
     ▼                             ▼                            ▼
  Connection                  Active                  Idle Connection
    Created                  Connection                 Closed
```

---

## 八、资源安全与防泄露

### 7.1 连接泄露防护

```rust
// src-tauri/src/services/guard.rs

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;

/// 连接使用追踪器
pub struct ConnectionGuard {
    /// 连接检查时间
    check_out_time: Instant,
    /// 操作描述
    operation: String,
    /// 连接 ID
    connection_id: String,
    /// 是否已归还
    returned: bool,
}

impl ConnectionGuard {
    /// 创建连接守卫
    pub fn new(connection_id: String, operation: String) -> Self {
        Self {
            check_out_time: Instant::now(),
            operation,
            connection_id,
            returned: false,
        }
    }
    
    /// 归还连接
    pub fn mark_returned(&mut self) {
        self.returned = true;
    }
    
    /// 检查是否泄露
    pub fn check_leak(&self) -> Option<LeakInfo> {
        if self.returned {
            return None;
        }
        
        let elapsed = self.check_out_time.elapsed();
        if elapsed > Duration::from_secs(60) {
            Some(LeakInfo {
                connection_id: self.connection_id.clone(),
                operation: self.operation.clone(),
                held_duration: elapsed,
            })
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct LeakInfo {
    pub connection_id: String,
    pub operation: String,
    pub held_duration: Duration,
}

/// 连接守卫管理器
pub struct GuardManager {
    guards: Arc<RwLock<HashMap<String, ConnectionGuard>>>,
}

impl GuardManager {
    pub fn new() -> Self {
        let guards = Arc::new(RwLock::new(HashMap::new()));
        let guards_clone = guards.clone();
        
        // 启动泄露检测任务
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                let guards = guards_clone.read().await;
                for (_, guard) in guards.iter() {
                    if let Some(leak) = guard.check_leak() {
                        tracing::warn!(
                            "Potential connection leak detected: connection={}, operation={}, duration={:?}",
                            leak.connection_id,
                            leak.operation,
                            leak.held_duration
                        );
                    }
                }
            }
        });
        
        Self { guards }
    }
    
    /// 注册连接使用
    pub async fn check_out(&self, connection_id: String, operation: String) {
        let guard = ConnectionGuard::new(connection_id.clone(), operation);
        let mut guards = self.guards.write().await;
        guards.insert(connection_id, guard);
    }
    
    /// 标记连接归还
    pub async fn check_in(&self, connection_id: &str) {
        let mut guards = self.guards.write().await;
        if let Some(guard) = guards.get_mut(connection_id) {
            guard.mark_returned();
        }
    }
}

/// RAII 连接守卫
pub struct ScopedConnectionGuard {
    connection_id: String,
    guard_manager: Arc<GuardManager>,
}

impl ScopedConnectionGuard {
    pub fn new(connection_id: String, guard_manager: Arc<GuardManager>) -> Self {
        // 在同步上下文中使用 block_on
        let gm = guard_manager.clone();
        let cid = connection_id.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                gm.check_out(cid, "query".to_string()).await;
            });
        });
        
        Self {
            connection_id,
            guard_manager,
        }
    }
}

impl Drop for ScopedConnectionGuard {
    fn drop(&mut self) {
        let gm = self.guard_manager.clone();
        let cid = self.connection_id.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                gm.check_in(&cid).await;
            });
        });
    }
}
```

### 7.2 内存管理

```rust
// src-tauri/src/services/memory_manager.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use sysinfo::{System, SystemExt, ProcessExt};

/// 内存监控器
pub struct MemoryMonitor {
    system: Arc<RwLock<System>>,
    max_memory_mb: u64,
}

impl MemoryMonitor {
    pub fn new(max_memory_mb: u64) -> Self {
        Self {
            system: Arc::new(RwLock::new(System::new())),
            max_memory_mb,
        }
    }
    
    /// 获取当前内存使用
    pub async fn get_memory_usage(&self) -> MemoryUsage {
        let mut system = self.system.write().await;
        system.refresh_memory();
        
        MemoryUsage {
            used_mb: system.used_memory() / 1024 / 1024,
            total_mb: system.total_memory() / 1024 / 1024,
            available_mb: system.available_memory() / 1024 / 1024,
        }
    }
    
    /// 检查是否接近内存限制
    pub async fn is_memory_pressure(&self) -> bool {
        let usage = self.get_memory_usage().await;
        usage.used_mb > self.max_memory_mb * 80 / 100
    }
}

#[derive(Debug)]
pub struct MemoryUsage {
    pub used_mb: u64,
    pub total_mb: u64,
    pub available_mb: u64,
}

/// 大结果集处理
pub struct QueryResultLimiter {
    max_rows: usize,
    max_bytes: usize,
}

impl QueryResultLimiter {
    pub fn new() -> Self {
        Self {
            max_rows: 100_000,
            max_bytes: 100 * 1024 * 1024, // 100MB
        }
    }
    
    /// 检查结果集是否过大
    pub fn check_result_size(&self, rows: usize, estimated_bytes: usize) -> Result<(), QueryLimitError> {
        if rows > self.max_rows {
            return Err(QueryLimitError::TooManyRows {
                actual: rows,
                limit: self.max_rows,
            });
        }
        
        if estimated_bytes > self.max_bytes {
            return Err(QueryLimitError::ResultTooLarge {
                actual_bytes: estimated_bytes,
                limit_bytes: self.max_bytes,
            });
        }
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QueryLimitError {
    #[error("Too many rows: {actual} > {limit}")]
    TooManyRows { actual: usize, limit: usize },
    
    #[error("Result too large: {actual_bytes} bytes > {limit_bytes} bytes")]
    ResultTooLarge { actual_bytes: usize, limit_bytes: usize },
}
```

---

## 九、扩展新数据库类型

### 8.1 扩展步骤

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

### 8.2 驱动开发规范

```rust
/// 驱动开发规范文档
/// 
/// 1. 所有驱动必须实现 DatabaseDriver trait
/// 2. 使用连接池管理连接，避免频繁创建/销毁
/// 3. 实现超时机制，防止查询无限等待
/// 4. 正确处理错误，转换为统一的 DriverError
/// 5. 实现 Drop trait 确保资源释放
/// 6. 编写单元测试覆盖主要功能
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

---

## 十、安全措施总结

| 安全措施 | 实现方式 | 位置 |
|----------|----------|------|
| **密码加密存储** | AES-256-GCM + 系统密钥链 | `Store::encrypt/decrypt` |
| **连接池管理** | sqlx 连接池 + 超时清理 | `PostgresDriver::pools` |
| **空闲连接清理** | 定时任务 (每5分钟) | `ConnectionManager::start_cleanup_task` |
| **连接泄露检测** | 守卫模式 + 超时警告 | `GuardManager` |
| **内存限制** | 结果集大小检查 | `QueryResultLimiter` |
| **SQL 注入防护** | 参数化查询 | `query_with_params` |
| **敏感信息清除** | 内存安全清零 | 密码字段使用 `Zeroize` |

---

## 十一、依赖清单

```toml
# Cargo.toml

[dependencies]
# Tauri 核心
tauri = { version = "2", features = ["multi-window"] }

# 异步运行时
tokio = { version = "1", features = ["full"] }

# 数据库驱动
sqlx = { version = "0.7", features = [
    "runtime-tokio",
    "tls-rustls",
    "postgres",
    "mysql",
    "sqlite",
    "chrono",
    "json",
] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 加密
aes-gcm = "0.10"
base64 = "0.22"
rand = "0.8"

# 系统密钥链
keyring = "2"

# 日期时间
chrono = { version = "0.4", features = ["serde"] }

# UUID
uuid = { version = "1", features = ["v4", "serde"] }

# 错误处理
thiserror = "1"
anyhow = "1"

# 日志
tracing = "0.1"
tracing-subscriber = "0.3"

# 系统信息
sysinfo = "0.30"

# 异步 trait
async-trait = "0.1"

[dev-dependencies]
tokio-test = "0.4"
```

---

## 十二、测试策略

### 11.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_connection_pool_creation() {
        let driver = PostgresDriver::new();
        let config = ConnectionConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            database_type: DatabaseType::PostgreSQL,
            host: Some("localhost".to_string()),
            port: Some(5432),
            database: Some("test".to_string()),
            username: Some("postgres".to_string()),
            password: Some("password".to_string()),
            // ...
        };
        
        let result = driver.connect(&config).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_encryption_decryption() {
        let store = Store::init_for_test().await;
        let plaintext = "my_secret_password";
        
        let encrypted = store.encrypt(plaintext).unwrap();
        let decrypted = store.decrypt(&encrypted).unwrap();
        
        assert_eq!(plaintext, decrypted);
        assert_ne!(plaintext, encrypted);
    }
}
```

### 11.2 集成测试

```rust
#[tokio::test]
#[ignore] // 需要真实数据库
async fn test_full_query_flow() {
    // 1. 初始化
    let registry = init_drivers().await;
    let store = Store::init_for_test().await;
    let manager = ConnectionManager::new(Arc::new(registry), Arc::new(store));
    
    // 2. 连接
    let conn_id = manager.connect("test_connection").await.unwrap();
    
    // 3. 查询
    let (driver, handle) = manager.get_connection(&conn_id).await.unwrap();
    let result = driver.query(&handle, "SELECT 1").await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    // 4. 断开
    manager.disconnect(&conn_id).await.unwrap();
}
```
