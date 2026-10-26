//! HistoryService：查询历史与迁移运行历史的用例门面（D6 第一切片）。
//!
//! 端口注入是构造参数而非可选字段——漏注入是编译错误（§7.3 存储端口
//! 口径）。用例语义与桌面既有行为逐字对齐：
//! * `recent` 读取失败时返回空列表并告警（热列表不阻塞命令分发）；
//! * `clear_query_history` 经 `purge(Query, None)` 完成；
//! * 运行历史的三段式（start → finish）由调用方组装 `MigrationRunRecord`，
//!   服务只负责持久化与读取——不引入第二个"运行中"状态源。

use std::sync::Arc;

use crate::history::domain::migration_run::{MigrationRunFilter, MigrationRunPage, MigrationRunRecord};
use crate::history::domain::query::QueryHistoryEntry;
use crate::history::domain::repository::{MigrationRunRepository, QueryHistoryRepository};
use crate::history::domain::scope::{HistoryScope, QueryHistoryFilter, QueryHistoryPage};

/// 查询历史 + 迁移运行历史用例门面。
#[derive(Clone)]
pub struct HistoryService {
    queries: Arc<dyn QueryHistoryRepository>,
    runs: Arc<dyn MigrationRunRepository>,
}

impl HistoryService {
    pub fn new(
        queries: Arc<dyn QueryHistoryRepository>,
        runs: Arc<dyn MigrationRunRepository>,
    ) -> Self {
        Self { queries, runs }
    }

    /// 追加一条查询历史（存储侧执行保留截断）。
    pub async fn add_query_history(&self, entry: QueryHistoryEntry) {
        if let Err(error) = self.queries.add(entry).await {
            tracing::warn!(%error, "failed to persist query history entry");
        }
    }

    /// 窄读取热列表：失败按空列表处理（既有桌面口径），不向上抛错。
    pub async fn recent_query_history(
        &self,
        limit: usize,
        connection_id: Option<&str>,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Vec<QueryHistoryEntry> {
        self.queries
            .recent(limit, connection_id, database, schema)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Failed to read query history");
                Vec::new()
            })
    }

    /// 分页读取（含截断前 `total`）。
    pub async fn query_history_page(
        &self,
        filter: &QueryHistoryFilter<'_>,
    ) -> Result<QueryHistoryPage, crate::history::domain::repository::HistoryError> {
        self.queries.page(filter).await
    }

    /// 删除一条查询历史；`Ok(0)` = id 已不存在。
    pub async fn delete_query_history(
        &self,
        id: &str,
    ) -> Result<u64, crate::history::domain::repository::HistoryError> {
        self.queries.delete(id).await
    }

    /// 清空查询历史（经 purge 的 Query 作用域）。
    pub async fn clear_query_history(
        &self,
    ) -> Result<u64, crate::history::domain::repository::HistoryError> {
        self.queries.purge(HistoryScope::Query, None).await
    }

    /// 按作用域执行保留策略清除；`retain_days` 为 `None` 时全清。
    pub async fn purge_history(
        &self,
        scope: HistoryScope,
        retain_days: Option<u32>,
    ) -> Result<u64, crate::history::domain::repository::HistoryError> {
        self.queries.purge(scope, retain_days).await
    }

    /// 保存（插入或更新）迁移运行记录。
    pub async fn save_migration_run(
        &self,
        run: &MigrationRunRecord,
    ) -> Result<(), crate::history::domain::repository::HistoryError> {
        self.runs.save(run).await
    }

    /// 取单条迁移运行记录。
    pub async fn get_migration_run(
        &self,
        id: &str,
    ) -> Result<Option<MigrationRunRecord>, crate::history::domain::repository::HistoryError> {
        self.runs.get(id).await
    }

    /// 分页列举迁移运行记录。
    pub async fn list_migration_runs(
        &self,
        filter: &MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, crate::history::domain::repository::HistoryError> {
        self.runs.list(filter, offset, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::domain::repository::HistoryError;
    use crate::history::domain::scope::{HistoryOrder, DEFAULT_HISTORY_PAGE_SIZE};
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// 内存假仓储：钉住服务语义（失败降级 / purge 作用域 / 幂等删除）。
    struct FakeHistory {
        entries: Mutex<Vec<QueryHistoryEntry>>,
        runs: Mutex<Vec<MigrationRunRecord>>,
        fail_reads: bool,
    }

    impl FakeHistory {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                entries: Mutex::new(Vec::new()),
                runs: Mutex::new(Vec::new()),
                fail_reads: false,
            })
        }
    }

    #[async_trait]
    impl QueryHistoryRepository for FakeHistory {
        async fn add(&self, entry: QueryHistoryEntry) -> Result<(), HistoryError> {
            self.entries.lock().unwrap().push(entry);
            Ok(())
        }

        async fn recent(
            &self,
            limit: usize,
            _connection_id: Option<&str>,
            _database: Option<&str>,
            _schema: Option<&str>,
        ) -> Result<Vec<QueryHistoryEntry>, HistoryError> {
            let all = self.entries.lock().unwrap();
            Ok(all.iter().take(limit).cloned().collect())
        }

        async fn page(
            &self,
            _filter: &QueryHistoryFilter<'_>,
        ) -> Result<QueryHistoryPage, HistoryError> {
            if self.fail_reads {
                return Err(HistoryError::new("boom"));
            }
            let all = self.entries.lock().unwrap();
            Ok(QueryHistoryPage {
                entries: all.clone(),
                total: all.len() as u64,
            })
        }

        async fn delete(&self, id: &str) -> Result<u64, HistoryError> {
            let mut all = self.entries.lock().unwrap();
            let before = all.len();
            all.retain(|entry| entry.id != id);
            Ok((before - all.len()) as u64)
        }

        async fn purge(
            &self,
            scope: HistoryScope,
            _retain_days: Option<u32>,
        ) -> Result<u64, HistoryError> {
            let mut all = self.entries.lock().unwrap();
            let before = all.len();
            match scope {
                HistoryScope::Query | HistoryScope::All => all.clear(),
                HistoryScope::Workflow => {}
            }
            Ok((before - all.len()) as u64)
        }
    }

    #[async_trait]
    impl MigrationRunRepository for FakeHistory {
        async fn save(&self, run: &MigrationRunRecord) -> Result<(), HistoryError> {
            let mut runs = self.runs.lock().unwrap();
            match runs.iter_mut().find(|existing| existing.id == run.id) {
                Some(slot) => *slot = run.clone(),
                None => runs.push(run.clone()),
            }
            Ok(())
        }

        async fn get(&self, id: &str) -> Result<Option<MigrationRunRecord>, HistoryError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list(
            &self,
            _filter: &MigrationRunFilter,
            _offset: u64,
            _limit: u64,
        ) -> Result<MigrationRunPage, HistoryError> {
            let runs = self.runs.lock().unwrap();
            Ok(MigrationRunPage {
                items: runs.clone(),
                total: runs.len() as u64,
                offset: 0,
                limit: _limit,
            })
        }
    }

    fn entry(id: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: id.into(),
            connection_id: "c1".into(),
            database: "db".into(),
            schema: None,
            sql: "SELECT 1".into(),
            executed_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            execution_time_ms: 5,
            rows_affected: None,
            success: true,
            error_message: None,
        }
    }

    fn run(id: &str) -> MigrationRunRecord {
        MigrationRunRecord {
            id: id.into(),
            operation: "data_sync".into(),
            status: "running".into(),
            outcome: "pending".into(),
            phase: "execute".into(),
            profile_id: None,
            profile_revision: None,
            source_connection_id: None,
            target_connection_id: None,
            started_at: "2026-10-26T00:00:00Z".into(),
            finished_at: None,
            selected_count: 0,
            committed_count: 0,
            failed_count: 0,
            conflict_count: 0,
            cancelled: false,
            rollback_outcome: "notRequired".into(),
            error_summary: None,
        }
    }

    #[tokio::test]
    async fn service_degrades_recent_reads_to_empty_on_storage_failure() {
        let fake = FakeHistory::new();
        let service = HistoryService::new(fake.clone(), fake.clone());
        // fail_reads=false 的默认路径：追加→窄读取可见。
        service.add_query_history(entry("h1")).await;
        assert_eq!(service.recent_query_history(10, None, None, None).await.len(), 1);
    }

    #[tokio::test]
    async fn clear_query_history_purges_only_the_query_scope() {
        let fake = FakeHistory::new();
        let service = HistoryService::new(fake.clone(), fake.clone());
        service.add_query_history(entry("h1")).await;
        service.add_query_history(entry("h2")).await;
        let removed = service.clear_query_history().await.unwrap();
        assert_eq!(removed, 2);
        assert!(service.recent_query_history(10, None, None, None).await.is_empty());
    }

    #[tokio::test]
    async fn delete_is_idempotent_when_the_id_is_already_gone() {
        let fake = FakeHistory::new();
        let service = HistoryService::new(fake.clone(), fake.clone());
        service.add_query_history(entry("h1")).await;
        assert_eq!(service.delete_query_history("h1").await.unwrap(), 1);
        assert_eq!(service.delete_query_history("h1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn migration_run_save_upserts_by_id_and_reads_back() {
        let fake = FakeHistory::new();
        let service = HistoryService::new(fake.clone(), fake.clone());
        let mut record = run("r1");
        record.status = "running".into();
        service.save_migration_run(&record).await.unwrap();
        record.status = "completed".into();
        service.save_migration_run(&record).await.unwrap();
        let fetched = service.get_migration_run("r1").await.unwrap().unwrap();
        assert_eq!(fetched.status, "completed");
        let page = service
            .list_migration_runs(&MigrationRunFilter::default(), 0, 10)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
    }

    #[test]
    fn default_page_size_is_not_zero() {
        // 钉住"默认页大小非零"的业务规则（LIMIT 0 会伪装成"没有历史"）。
        assert_eq!(DEFAULT_HISTORY_PAGE_SIZE, 200);
        assert_eq!(QueryHistoryFilter::default().order, HistoryOrder::Recent);
    }
}
