//! History 仓储端口（§4.1 第 5 条：仓储 trait 在 domain，实现由宿主完成）。
//!
//! * 桌面：`src-tauri/src/store/history_db.rs` 的 SQLite 实现适配本端口；
//! * Web：服务端实现同一契约，用例代码一行不改（04-repositories §7.3）。
//!
//! 语义口径（§7.1）：
//! * [`QueryHistoryRepository::delete`] `Ok(0)` 表示 id 本就不存在，不是错误；
//! * [`QueryHistoryRepository::recent`] 是窄读取投影（热列表），
//!   [`QueryHistoryRepository::page`] 是带 `total` 的全功能分页读取；
//! * 追加后的保留截断（`MAX_*` 常量）由存储实现原子完成；
//! * [`QueryHistoryRepository::purge`] 是跨 query/workflow 两域的保留策略
//!   动作（按作用域清除、可选保留天数），留在同一端口避免两个半套接口。

use async_trait::async_trait;

use super::migration_run::{MigrationRunFilter, MigrationRunPage, MigrationRunRecord};
use super::query::QueryHistoryEntry;
use super::scope::{HistoryScope, QueryHistoryFilter, QueryHistoryPage};

/// 存储失败：可重试与否由调用方按既有口径处理；不携带 SQL/宿主路径。
#[derive(Debug, thiserror::Error)]
#[error("history storage error: {message}")]
pub struct HistoryError {
    pub message: String,
}

impl HistoryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// 查询历史仓储（追加 / 窄读取 / 分页 / 删除 / 保留策略）。
#[async_trait]
pub trait QueryHistoryRepository: Send + Sync {
    /// 追加一条历史；存储侧负责保留截断与去重口径（既有行为不变）。
    async fn add(&self, entry: QueryHistoryEntry) -> Result<(), HistoryError>;

    /// 窄读取：最近的若干条，可按连接/库/schema 过滤。
    async fn recent(
        &self,
        limit: usize,
        connection_id: Option<&str>,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Result<Vec<QueryHistoryEntry>, HistoryError>;

    /// 分页读取：过滤器在 `limit` 之前生效，返回含截断前 `total` 的页。
    async fn page(&self, filter: &QueryHistoryFilter<'_>) -> Result<QueryHistoryPage, HistoryError>;

    /// 删除一条；`Ok(0)` 表示 id 已不存在（幂等，不是错误）。
    async fn delete(&self, id: &str) -> Result<u64, HistoryError>;

    /// 按作用域清除历史；`retain_days` 为 `None` 时全清。
    /// 返回受影响行数。
    async fn purge(&self, scope: HistoryScope, retain_days: Option<u32>) -> Result<u64, HistoryError>;
}

/// 迁移运行历史仓储（Data Sync / Data Transfer / Schema Diff 共享）。
#[async_trait]
pub trait MigrationRunRepository: Send + Sync {
    /// 保存（插入或更新）一条运行记录。
    async fn save(&self, run: &MigrationRunRecord) -> Result<(), HistoryError>;

    /// 取单条；`None` = 不存在。
    async fn get(&self, id: &str) -> Result<Option<MigrationRunRecord>, HistoryError>;

    /// 分页列举；`total` 为过滤后全部行数。
    async fn list(
        &self,
        filter: &MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, HistoryError>;
}
