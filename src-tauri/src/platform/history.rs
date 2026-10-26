//! Desktop infrastructure adapter for the module-owned history repositories.
//!
//! D6 第一切片：`datazen-modules::history` 拥有词汇与仓储端口；SQLite 落盘
//! (`history.sqlite`)、保留截断与 schema 迁移环留在宿主的 `HistoryDb`。
//! 本文件只把 `HistoryDb` 的同步方法适配为模块端口——不复制缓存、不引入
//! 第二份存储事实（04-repositories §7.3）。
//!
//! 并发口径与既有 `DesktopRepository::add_query_history` 一致：rusqlite
//! 临界区由 `HistoryDb` 内部的 `Mutex` 保护，适配器不加第二把锁。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_modules::history::{
    HistoryError, HistoryScope, MigrationRunFilter, MigrationRunPage, MigrationRunRecord,
    MigrationRunRepository, QueryHistoryEntry, QueryHistoryFilter, QueryHistoryPage,
    QueryHistoryRepository,
};

use crate::store::history_db::HistoryDb;

/// 查询历史 + 迁移运行历史的桌面仓储适配器（同一 `HistoryDb` 实例）。
#[derive(Clone)]
pub struct DesktopHistoryRepositories {
    db: Arc<HistoryDb>,
}

impl DesktopHistoryRepositories {
    pub fn new(db: Arc<HistoryDb>) -> Self {
        Self { db }
    }

    fn storage_error(error: impl std::fmt::Display) -> HistoryError {
        HistoryError::new(error.to_string())
    }
}

#[async_trait]
impl QueryHistoryRepository for DesktopHistoryRepositories {
    async fn add(&self, entry: QueryHistoryEntry) -> Result<(), HistoryError> {
        self.db
            .add_query_history(entry)
            .map_err(Self::storage_error)
    }

    async fn recent(
        &self,
        limit: usize,
        connection_id: Option<&str>,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Result<Vec<QueryHistoryEntry>, HistoryError> {
        self.db
            .get_query_history(limit, connection_id, database, schema)
            .map_err(Self::storage_error)
    }

    async fn page(
        &self,
        filter: &QueryHistoryFilter<'_>,
    ) -> Result<QueryHistoryPage, HistoryError> {
        self.db
            .query_history_page(filter)
            .map_err(Self::storage_error)
    }

    async fn delete(&self, id: &str) -> Result<u64, HistoryError> {
        self.db
            .delete_query_history(id)
            .map_err(Self::storage_error)
    }

    async fn purge(
        &self,
        scope: HistoryScope,
        retain_days: Option<u32>,
    ) -> Result<u64, HistoryError> {
        self.db
            .purge(scope, retain_days)
            .map_err(Self::storage_error)
    }
}

#[async_trait]
impl MigrationRunRepository for DesktopHistoryRepositories {
    async fn save(&self, run: &MigrationRunRecord) -> Result<(), HistoryError> {
        self.db.save_migration_run(run).map_err(Self::storage_error)
    }

    async fn get(&self, id: &str) -> Result<Option<MigrationRunRecord>, HistoryError> {
        self.db.get_migration_run(id).map_err(Self::storage_error)
    }

    async fn list(
        &self,
        filter: &MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, HistoryError> {
        self.db
            .list_migration_runs(filter, offset, limit)
            .map_err(Self::storage_error)
    }
}
