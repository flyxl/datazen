# IPC 命令层

> [返回架构总览](../README.md)

## 1. 命令定义 (AppState + Commands)

### 1.1 命令定义

> **注意**：下方为简化示意代码，实际实现位于 `src-tauri/src/commands/`。ID 术语：入参 `connection_id` = 持久化配置连接 id；`db_session_id` = 运行时会话 id（由 `connect` 返回）。SQL/Schema 类命令一律接收 `db_session_id`。

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

/// 建立连接（入参为持久化配置连接 id，返回运行时 db_session_id）
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<String, String> {
    state.connection_manager.connect(&connection_id).await
        .map_err(|e| e.to_string())
}

/// 断开连接
#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), String> {
    state.connection_manager.disconnect(&db_session_id).await
        .map_err(|e| e.to_string())
}

// ============== 数据库操作命令 ==============

/// 获取数据库列表
#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<String>, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
        .map_err(|e| e.to_string())?;
    
    driver.get_databases(&handle).await
        .map_err(|e| e.to_string())
}

/// 获取表列表
#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<Vec<TableInfo>, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
        .map_err(|e| e.to_string())?;
    
    driver.get_tables(&handle, &database).await
        .map_err(|e| e.to_string())
}

/// 获取表结构
#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
) -> Result<TableSchema, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
        .map_err(|e| e.to_string())?;
    
    driver.get_table_schema(&handle, &table).await
        .map_err(|e| e.to_string())
}

/// 执行查询
#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
) -> Result<QueryResult, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
        .map_err(|e| e.to_string())?;
    
    let result = driver.query(&handle, &sql).await
        .map_err(|e| e.to_string())?;
    
    // 记录查询历史：connection_id 字段存归属的持久化配置连接 id，
    // 由运行时会话解析（见 ConnectionManager::owner_connection_id）。
    let history_entry = QueryHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id: state.connection_manager
            .owner_connection_id(&db_session_id).await
            .unwrap_or_default(),
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
    db_session_id: String,
    sql: String,
) -> Result<ExplainResult, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
        .map_err(|e| e.to_string())?;
    
    driver.explain(&handle, &sql).await
        .map_err(|e| e.to_string())
}

// ============== 表数据命令 ==============

/// 获取表数据（带分页、筛选、排序）
#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
) -> Result<TableDataResult, String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
        .map_err(|e| e.to_string())?;
    
    let executor = QueryExecutor { schema_cache: state.schema_cache.clone() };
    executor.get_table_data(
        &driver, &handle,
        &db_session_id, "", &table,
        page, page_size, filters, 
        sorts.map(|s| s.first().cloned()).flatten(),
    ).await.map_err(|e| e.to_string())
}

/// 取消正在执行的查询
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&db_session_id).await
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
| 连接管理 | `connection.rs` | `get_connections`, `save_connection`, `delete_connection`, `reorder_connections`, `test_connection`, `connect`, `disconnect`, `release_connection`, `ping_connection`, `get_connection_info`, `get_available_drivers` |
| **Driver Command** | `driver_command.rs` | `execute_driver_command`, `execute_driver_command_stream`, `get_connection_commands`, `get_driver_commands` — SQL `query`/`query_stream`/`execute`、Workflow、Redis `scan_keys`/`get_key`、Schema 对象 `list_objects`、SQLite 驱动内建 ADB 命令（`adb_list_packages` 等，`requiresConnection = false`，决策 2 自 Host 迁入）等统一入口 |
| SQL 查询 | `query.rs` | `execute_query`, `execute_query_stream`（委托 `execute_driver_command_stream`）, `get_explain`, `cancel_query`, `get_query_history`, `clear_query_history`, `purge_history`, `get/add/delete_favorite_query` —— 三查询命令均带可选 `database` 定位参数（见 §3.4） |
| Schema | `schema.rs` | `get_databases`, `get_tables`, `get_columns`, `get_table_schema`, `get_table_data`（可选 `database` 定位）, `get_er_data`, `get_database_objects`, `get_object_ddl`, `get_privileges`（对象/权限命令内部经 Driver Command 执行）；结构变更走 `structure.rs` 的 `plan_table_structure_changes`（可选 `database` 定位） |
| 表编辑 | `data.rs` | `commit_row_updates`, `commit_row_deletes`（批量行 UPDATE / DELETE） |
| 备份 | `backup.rs` | 直连路径（webdriver 门控）：`backup_database`, `restore_database`, `execute_sql_file`；对话框：`backup_database_with_dialog`, `restore_database_with_dialog`, `execute_sql_file_with_dialog`。决策 3+6 将双轨合并为 `backup_database` + `restore_sql_file`（`override_path` 仅 webdriver 构建生效），见 [ipc-refactor-plan.md](./ipc-refactor-plan.md)；合并代码在 `feature/f3-backup-merge` 分支，待随 F4 合入 |
| 同步 | `commands/sync/` | `inspect_data_sync`, `compare_data_sync`, `execute_data_sync`；已移除：`compare_databases`/`sync_table`/`sync_tables`（legacy）、`classify_sync_pair`（前端 `syncPairing.ts` 镜像同逻辑） |
| Schema Diff Deploy | `schema_diff.rs` | `prepare_schema_diff_plan`, `execute_schema_diff_deploy`, `compare_table_schemas`（`compare_table_data` 未上线已移除） |
| 配置 | `config.rs` | `get_settings`, `save_settings`, `get_groups`, `save_groups`, `get_log_path`, `open_log_dir/open_workflows_dir/open_context_dir/open_path`, `restart_app`；导入导出族：`export_connections(_with_dialog)`, `import_connections_preview/_with_dialog`, `detect_connection_import_path`, `pick_connection_import_path_with_dialog`, `import_connections_from_app`, `export_app_data(_with_dialog)`, `import_app_data(_with_dialog)`, `save_encryption_key_with_dialog` |
| 主题包 | `theme.rs` | `list_theme_packs`, `install_theme_pack_with_dialog`, `remove_theme_pack`, `read_theme_pack_file` |
| AI | `ai.rs` | `ai_generate_sql`, `ai_chat`, `ai_diagnose_error`, `ai_analyze_explain`, `ai_parse_filter`, `workflow_*`, `prompt_*`（约 30 个命令） |
| 上下文 | `context.rs` | `context_get_dir`, `context_list_files`, `context_read_files` |
| MCP | `mcp.rs` | `mcp_start_stdio`, `mcp_stop`, `mcp_get_status`, `mcp_reload`, `mcp_list_all_tools`, `mcp_client_connect/call_tool/disconnect/list/tools` |
| 文件 | `file.rs` | 对话框系列：`save_text_with_dialog`, `save_base64_with_dialog`, `begin_save_with_dialog`, `append_save_text`, `finish_save`, `abort_save`, `open_text_with_dialog`, `open_base64_with_dialog`, `export_tables_stream`（纯路径读写 IPC 已删除） |
| 窗口 | `window.rs` | `create_sub_window` |

### 3.4 库 / Schema 定位机制（IPC 重构终态）

`use_database` IPC 已废弃删除（决策 1），目标库定位统一为「请求显式传参」，分两层：

**宿主会话 pin（会话维度，所有命令可用）**

- `execute_query` / `execute_query_stream` / `get_explain` 及 `execute_driver_command(_stream)` /
  `get_table_data` / `plan_table_structure_changes` 均接收可选 `database`
- 宿主在执行前经共享助手 `ensure_session_database`（`commands/query.rs`）前置切库并更新会话活动库记录；
  `None` / 空白 / 与当前相同 → 零次切换，保持 not-connected 错误语义
- pin 持久生效：切库后，后续不带定位参数的未限定命令自然落在目标库

**驱动方言内联重写（语句维度，F7 六驱动 SQL 定位重写）**

- driver-command 信封另带可选 `schema`（PG 系 = database + schema 两维；MySQL 系仅 database）
- 驱动可覆写 `DatabaseDriver::qualify_sql_target`，基于 driver-api 的 `SqlTarget` / `qualify_sql_with`
  （sqlparser AST 改写）把未限定表引用重写为方言限定名：MySQL/MariaDB `` `db`.`t` ``（真跨库内联）、
  PostgreSQL/DuckDB `"schema"."t"`（database 维沿用连接池切换）、SQL Server `db.schema.t` 三段名、
  ClickHouse `db.t`、SQLite 仅 ATTACH 别名场景（通常 no-op）
- 只改定位语境（FROM / JOIN / INSERT INTO / UPDATE / DELETE FROM / TRUNCATE / CREATE|DROP|ALTER TABLE /
  CREATE INDEX ON）；跳过 CTE 名、子查询别名、字符串字面量与已限定引用；幂等；解析失败原样放行并记日志
- 双保险正交并存：pin（会话维度）与 rewrite（语句维度）互不冲突；旧驱动无重写能力时由 pin 兜底

**连带删除面（决策 2 / 4 / 5）**

- ADB 三命令自 Host `commands/adb.rs` 迁入 sqlite 驱动 Command API（`requiresConnection = false`），
  原生保存对话框由 `DriverSaveDialogSpec` 元数据驱动的宿主薄壳 `finish_save_dialog` 完成
- `write_file` / `write_file_base64` / `read_file` 已删除，E2E fixture 改 Node.js fs；对话框系 API 全部保留
- `get_monitor_paused` / `set_monitor_paused` / `compare_table_data` / `classify_sync_pair` 已删除

> backup/restore 路径/对话框双轨的合并形态见 [ipc-refactor-plan.md](./ipc-refactor-plan.md) 决策 3+6。

## 4. 安全措施总结

| 安全措施 | 实现方式 | 位置 |
|----------|----------|------|
| **密码加密存储** | AES-256-GCM；主密钥在 OS 钥匙串或 `{appData}/.key` | `Store::encrypt/decrypt` + `key_store` |
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
