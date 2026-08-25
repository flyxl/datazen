# 服务层 — 连接管理与资源安全

> [返回架构总览](../README.md)

## 连接 ID 约定

| 名称 | 含义 | 示例来源 |
|------|------|----------|
| **`connection_id`** | 持久化连接配置 ID（`ConnectionConfig.id`，存于 `connections.json`） | GUI 保存的连接、`connect` IPC 入参、MCP tools / AI db tools 入参 |
| **`db_session_id`** | 运行时数据库会话 ID（`ConnectionManager` 分配，断开即失效） | `connect` 返回值、大多数查询/Schema IPC |

规则：**GUI 先 `connect(connection_id)` → 得到 `db_session_id`，后续 SQL/Schema IPC 传 `db_session_id`。** MCP tools / AI db tools 与 prompts 直接传 **`connection_id`**（`list_connections` 返回值）；内部通过 `db_tools::resolve_connection` 按需连接。`resolve_connection` 底层走 `ConnectionManager::resolve_session` 双模解析（先按 `db_session_id` 查找活动会话，再回退到 `connection_id` 建立新会话），但 MCP/API 调用方应只传 connection_id。

> 历史演进：早期版本中持久化配置 ID 叫 `config_id`、运行时会话句柄叫 `connection_id`；现已统一为上表术语（旧键名不做兼容别名）。

---

## 1. 连接管理服务

### 1.1 连接池管理器

> **注意**：下方代码为设计文档示例，实际实现位于 `src-tauri/src/services/connection_manager.rs`，已包含连接去重锁（`connect_locks`）等增强功能。

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
    
    /// 建立新连接（入参为持久化配置连接 id，返回运行时会话 id）
    pub async fn connect(&self, connection_id: &str) -> Result<String, ConnectionError> {
        // 获取配置
        let config = self.config_store
            .get_connection(connection_id)
            .await?
            .ok_or(ConnectionError::ConnectionConfigNotFound(connection_id.to_string()))?;
        
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
        
        let db_session_id = handle.id.clone();
        
        // 记录活动连接
        let mut connections = self.connections.write().await;
        connections.insert(db_session_id.clone(), ActiveConnection {
            handle,
            config,
            created_at: Instant::now(),
            last_used: Instant::now(),
        });
        
        Ok(db_session_id)
    }
    
    /// 断开连接
    pub async fn disconnect(&self, db_session_id: &str) -> Result<(), ConnectionError> {
        let mut connections = self.connections.write().await;
        
        if let Some(active) = connections.remove(db_session_id) {
            let driver = self.registry.get(&active.config.database_type).await;
            if let Some(driver) = driver {
                driver.disconnect(active.handle).await?;
            }
        }
        
        Ok(())
    }
    
    /// 获取会话
    pub async fn get_session(&self, db_session_id: &str) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let mut connections = self.connections.write().await;
        
        let active = connections
            .get_mut(db_session_id)
            .ok_or(ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        
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
    #[error("Connection config not found: {0}")]
    ConnectionConfigNotFound(String),
    
    #[error("DB session not found: {0}")]
    DbSessionNotFound(String),
    
    #[error("Driver not found for type: {0:?}")]
    DriverNotFound(DatabaseType),
    
    #[error("Driver error: {0}")]
    DriverError(#[from] DriverError),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}
```

### 1.2 连接去重锁

`ConnectionManager` 使用 `connect_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>` 防止对同一连接配置的并发连接请求创建重复连接。当多个窗口同时请求同一连接时，只有第一个请求实际执行连接，后续请求等待并复用已建立的连接。

### 1.3 QueryExecutor

`src-tauri/src/services/query_executor.rs` 负责表数据浏览的 SQL 构建：
- `build_select_sql` — 根据驱动元数据构建 SELECT 语句（含分页、筛选、排序）
- 条件性 `OFFSET`：通过 `DatabaseDriver::supports_offset()` 控制（Presto/Trino 不支持 OFFSET）
- 条件性 `COUNT` 查询：通过 `skip_count_query()` 控制

### 1.4 DbTools

`src-tauri/src/services/db_tools.rs` — 共享数据库操作工具，被 AI Chat 工具调用和 MCP Server 复用：
- `resolve_connection(connection_id)` — 从持久化配置连接 ID 解析驱动和句柄（底层 `ConnectionManager::resolve_session` 双模：先匹配活动 `db_session_id`，再回退按 `connection_id` 建会话）
- `list_connections()` — 列出所有可用连接（返回 connection_id）
- `query(connection_id, …)` / `list_databases` / `list_tables` / `get_table_schema` — MCP 与 AI tools 入参均为 **connection_id**

---

## 2. 资源安全与防泄露

### 2.1 连接泄露防护

> **注意**：以下为早期资源安全设计的示意代码（当前源码树中已无独立的 `guard.rs` 模块）；标识符按现行术语书写，`db_session_id` 指运行时会话 ID。

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
    db_session_id: String,
    /// 是否已归还
    returned: bool,
}

impl ConnectionGuard {
    /// 创建连接守卫
    pub fn new(db_session_id: String, operation: String) -> Self {
        Self {
            check_out_time: Instant::now(),
            operation,
            db_session_id,
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
                db_session_id: self.db_session_id.clone(),
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
    pub db_session_id: String,
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
                            leak.db_session_id,
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
    pub async fn check_out(&self, db_session_id: String, operation: String) {
        let guard = ConnectionGuard::new(db_session_id.clone(), operation);
        let mut guards = self.guards.write().await;
        guards.insert(db_session_id, guard);
    }
    
    /// 标记连接归还
    pub async fn check_in(&self, db_session_id: &str) {
        let mut guards = self.guards.write().await;
        if let Some(guard) = guards.get_mut(db_session_id) {
            guard.mark_returned();
        }
    }
}

/// RAII 连接守卫
pub struct ScopedConnectionGuard {
    db_session_id: String,
    guard_manager: Arc<GuardManager>,
}

impl ScopedConnectionGuard {
    pub fn new(db_session_id: String, guard_manager: Arc<GuardManager>) -> Self {
        // 在同步上下文中使用 block_on
        let gm = guard_manager.clone();
        let cid = db_session_id.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                gm.check_out(cid, "query".to_string()).await;
            });
        });
        
        Self {
            db_session_id,
            guard_manager,
        }
    }
}

impl Drop for ScopedConnectionGuard {
    fn drop(&mut self) {
        let gm = self.guard_manager.clone();
        let cid = self.db_session_id.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                gm.check_in(&cid).await;
            });
        });
    }
}
```

### 2.2 内存管理

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
