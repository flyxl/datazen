# 服务层 — 连接管理与资源安全

> [返回架构总览](../README.md)

## 1、连接管理服务

### 1.1 连接池管理器

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

## 2、资源安全与防泄露

### 2.1 连接泄露防护

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
