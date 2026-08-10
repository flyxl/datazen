# IPC 命令层

> [返回架构总览](../README.md)

## 1. 命令定义 (AppState + Commands)

### 1.1 命令定义

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

## 2. 数据流向

### 2.1 完整数据流

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

### 2.2 连接生命周期

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

## 3. 结构化错误处理 (CommandError)

### 3.1 CommandError 枚举

所有 IPC 命令使用统一的 `CommandError` 枚举（`src-tauri/src/commands/error.rs`）：

```rust
pub enum CommandError {
    Store(StoreError),
    Connection(ConnectionError),
    Driver(DriverError),
    Ai(AiError),
    Io(std::io::Error),
    Json(serde_json::Error),
    NotFound(String),
    NotConfigured(String),
    Validation(String),
    Internal(String),
}
```

- 实现 `serde::Serialize` 序列化为纯字符串，保持前端兼容
- 通过 `From<T>` 实现自动类型转换
- `CmdExt` trait 提供 `.cmd_err(cmd)` 方法统一日志记录和错误转换

### 3.2 AppState 结构

```rust
pub struct AppState {
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
    pub sync_adapters: Arc<SyncAdapterRegistry>,
    pub ai_registry: Arc<AiProviderRegistry>,
    pub schema_context_builder: Arc<SchemaContextBuilder>,
    pub prompt_resolver: Arc<PromptResolver>,
    pub workflow_registry: Arc<WorkflowRegistry>,
    pub workflow_history: Arc<WorkflowHistory>,
    pub mcp_client_manager: Arc<McpClientManager>,
}
```

`build_app_state()` 函数统一初始化，GUI 和 headless MCP 模式共享。

### 3.3 IPC 命令模块清单

| 模块 | 文件 | 关键命令 |
|------|------|---------|
| 连接管理 | `connection.rs` | `get_connections`, `save_connection`, `test_connection`, `connect`, `disconnect`, `ping`, `get_server_info`, `available_drivers` |
| SQL 查询 | `query.rs` | `execute_query`, `get_explain`, `cancel_query`, `get_query_history`, `favorite_query` |
| Schema | `schema.rs` | `get_databases`, `get_tables`, `get_columns`, `get_table_schema`, `get_table_data`, `get_er_data` |
| 表编辑 | `data.rs` | `commit_edits`（批量行 UPDATE） |
| Redis | `kv.rs` | `scan_keys`, `get_key_detail` |
| 备份 | `backup.rs` | `backup_database`, `restore_database` |
| 同步 | `sync.rs` | `compare_databases`, `compare_table_schemas`, `sync_tables`, … |
| Schema Diff Deploy | `schema_diff.rs` | `prepare_schema_diff_plan`, `execute_schema_diff_deploy` |
| 配置 | `config.rs` | `get_settings`, `save_settings`, `get_groups`, `get_log_path`, `export_connections`, `import_connections` |
| 主题包 | `theme.rs` | `list_theme_packs`, `install_theme_pack_with_dialog`, `remove_theme_pack`, `read_theme_pack_file` |
| AI | `ai.rs` | `ai_generate_sql`, `ai_chat`, `ai_diagnose_error`, `ai_analyze_explain`, `ai_parse_filter`, `workflow_*`, `prompt_*`（约 30 个命令） |
| 上下文 | `context.rs` | `context_get_dir`, `context_list_files`, `context_read_files` |
| MCP | `mcp.rs` | `mcp_start`, `mcp_stop`, `mcp_status`, `mcp_client_connect`, `mcp_client_call_tool` |
| 文件 | `file.rs` | `read_file`, `write_file`, `show_editor_context_menu` |
| 窗口 | `window.rs` | `create_sub_window` |
| ADB | `adb.rs` | `adb_list_packages`, `adb_list_databases`, `adb_pull_database` |

## 4. 安全措施总结

| 安全措施 | 实现方式 | 位置 |
|----------|----------|------|
| **密码加密存储** | AES-256-GCM + 系统密钥链 | `Store::encrypt/decrypt` |
| **连接池管理** | sqlx 连接池 + 超时清理 | `PostgresDriver::pools` |
| **空闲连接清理** | 定时任务 (每5分钟) | `ConnectionManager::start_cleanup_task` |
| **连接泄露检测** | 守卫模式 + 超时警告 | `GuardManager` |
| **内存限制** | 结果集大小检查 | `QueryResultLimiter` |
| **SQL 注入防护** | 参数化查询 | `query_with_params` |
| **敏感信息清除** | 内存安全清零 | 密码字段使用 `Zeroize` |

## 5. 依赖清单

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
