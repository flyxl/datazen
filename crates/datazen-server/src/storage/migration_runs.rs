//! 迁移运行历史的服务端 SQLite 适配器（W8 刀3b；v12
//! `migration_run_history` 表）。
//!
//! 词汇（[`MigrationRunRecord`] / [`MigrationRunFilter`] /
//! [`MigrationRunPage`]）唯一定义在共享层 `datazen::history`（Data Sync /
//! Data Transfer / Schema Diff 三类运行共用，`operation` 区分）；桌面实现
//! 是本机 `history.sqlite`（单租户），web 形态改存服务端管理库（PK 升
//! `(organization_id, id)`）。
//!
//! 桌面语义对齐（`history_db/migration_run.rs`）：
//!
//! - **save = upsert**（`ON CONFLICT(id) DO UPDATE`）：start 先落一行
//!   `running`，finish 用同 id 覆写终态——「同一 run 的两段落账」。
//! - **追加后截断**：按本组织 `started_at` 倒序保留
//!   [`MAX_MIGRATION_RUN_HISTORY`] 行（桌面同一条业务规则；防无界增长）。
//! - **list = 过滤 + 计数 + 分页**：`limit` clamp 到 1..=100（桌面同款）。
//! - **启动行改 interrupted**：桌面在 schema 初始化时把遗留 `running` 行
//!   批量为 `interrupted`；服务端进程重启后的遗留行由读取面如实回答
//!   （改写时机属于进程启动切片，本件不隐式触发）。
//!
//! 组织隔离（CM-05）：所有 WHERE 带组织键；隔离键由调用方按认证链事实
//! 装配（INV-01）。

use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};

use datazen::history::{MigrationRunFilter, MigrationRunPage, MigrationRunRecord};

use crate::storage::AdminStore;

/// 桌面 `history_db` 的同款保留上限（超出按 `started_at` 倒序截断）。
pub const MAX_MIGRATION_RUN_HISTORY: usize = 500;

/// 存储面错误（调用方决定降级语义：部署编排里落账失败只 warn，不阻断
/// 执行应答——桌面 `start_migration_run` / `finish_migration_run` 同款）。
#[derive(Debug, thiserror::Error)]
pub enum MigrationRunStoreError {
    #[error("migration run store: {0}")]
    Backend(String),
}

fn backend(error: impl std::fmt::Display) -> MigrationRunStoreError {
    MigrationRunStoreError::Backend(error.to_string())
}

/// v12 表的行存取。
#[derive(Clone)]
pub struct ServerMigrationRuns {
    store: Arc<AdminStore>,
}

impl ServerMigrationRuns {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    /// upsert 一行（start/finish 两段共用；桌面 `save_migration_run`）。
    pub async fn save(
        &self,
        organization_id: &str,
        run: MigrationRunRecord,
    ) -> Result<(), MigrationRunStoreError> {
        let store = self.store.clone();
        let organization_id = organization_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            Self::save_sync(&conn, &organization_id, &run)
        })
        .await
        .map_err(|error| backend(format!("migration run task: {error}")))?
    }

    /// 单行读取（W8 刀3e-3 `get_migration_run` 的存储半边）：本组织 +
    /// run id；miss / 越组织 → `Ok(None)`（调用方按 404 收口——统一 404
    /// 纪律，不区分「不存在」与「不可见」）。
    pub async fn get(
        &self,
        organization_id: &str,
        run_id: &str,
    ) -> Result<Option<MigrationRunRecord>, MigrationRunStoreError> {
        let store = self.store.clone();
        let organization_id = organization_id.to_owned();
        let run_id = run_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            conn.query_row(
                "SELECT id, operation, status, outcome, phase, profile_id, profile_revision, \
                 source_connection_id, target_connection_id, started_at, finished_at, \
                 selected_count, committed_count, failed_count, conflict_count, \
                 cancelled, rollback_outcome, error_summary \
                 FROM migration_run_history WHERE organization_id = ?1 AND id = ?2",
                params![organization_id, run_id],
                run_from_row,
            )
            .optional()
            .map_err(backend)
        })
        .await
        .map_err(|error| backend(format!("migration run get task: {error}")))?
    }

    fn save_sync(
        conn: &Connection,
        organization_id: &str,
        run: &MigrationRunRecord,
    ) -> Result<(), MigrationRunStoreError> {
        conn.execute(
            "INSERT INTO migration_run_history (
               organization_id, id, operation, status, outcome, phase,
               profile_id, profile_revision, source_connection_id, target_connection_id,
               started_at, finished_at,
               selected_count, committed_count, failed_count, conflict_count,
               cancelled, rollback_outcome, error_summary
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
             ON CONFLICT (organization_id, id) DO UPDATE SET
               operation = excluded.operation, status = excluded.status,
               outcome = excluded.outcome, phase = excluded.phase,
               profile_id = excluded.profile_id,
               profile_revision = excluded.profile_revision,
               source_connection_id = excluded.source_connection_id,
               target_connection_id = excluded.target_connection_id,
               started_at = excluded.started_at, finished_at = excluded.finished_at,
               selected_count = excluded.selected_count,
               committed_count = excluded.committed_count,
               failed_count = excluded.failed_count,
               conflict_count = excluded.conflict_count,
               cancelled = excluded.cancelled,
               rollback_outcome = excluded.rollback_outcome,
               error_summary = excluded.error_summary",
            params![
                organization_id,
                run.id,
                run.operation,
                run.status,
                run.outcome,
                run.phase,
                run.profile_id,
                run.profile_revision,
                run.source_connection_id,
                run.target_connection_id,
                run.started_at,
                run.finished_at,
                run.selected_count as i64,
                run.committed_count as i64,
                run.failed_count as i64,
                run.conflict_count as i64,
                run.cancelled as i64,
                run.rollback_outcome,
                run.error_summary,
            ],
        )
        .map_err(backend)?;
        // 截断：按 started_at 倒序保留上限行（桌面同款）。
        conn.execute(
            "DELETE FROM migration_run_history
             WHERE organization_id = ?1 AND id NOT IN (
               SELECT id FROM migration_run_history WHERE organization_id = ?1
               ORDER BY started_at DESC, id LIMIT ?2
             )",
            params![organization_id, MAX_MIGRATION_RUN_HISTORY as i64],
        )
        .map_err(backend)?;
        Ok(())
    }

    /// 过滤 + 计数 + 分页（桌面 `list_migration_runs` 同语义；
    /// `limit` clamp 1..=100）。
    pub async fn list(
        &self,
        organization_id: &str,
        filter: MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, MigrationRunStoreError> {
        let store = self.store.clone();
        let organization_id = organization_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            Self::list_sync(&conn, &organization_id, &filter, offset, limit)
        })
        .await
        .map_err(|error| backend(format!("migration run list task: {error}")))?
    }

    fn list_sync(
        conn: &Connection,
        organization_id: &str,
        filter: &MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, MigrationRunStoreError> {
        let mut where_sql = String::from("WHERE organization_id = ?1");
        let mut bind: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(organization_id.to_owned())];
        if let Some(operation) = filter.operation.as_deref() {
            bind.push(Box::new(operation.to_owned()));
            where_sql.push_str(&format!(" AND operation = ?{}", bind.len()));
        }
        if let Some(status) = filter.status.as_deref() {
            bind.push(Box::new(status.to_owned()));
            where_sql.push_str(&format!(" AND status = ?{}", bind.len()));
        }
        if let Some(profile_id) = filter.profile_id.as_deref() {
            bind.push(Box::new(profile_id.to_owned()));
            where_sql.push_str(&format!(" AND profile_id = ?{}", bind.len()));
        }
        if let Some(connection_id) = filter.connection_id.as_deref() {
            bind.push(Box::new(connection_id.to_owned()));
            where_sql.push_str(&format!(
                " AND (source_connection_id = ?{index} OR target_connection_id = ?{index})",
                index = bind.len()
            ));
        }
        let total: u64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM migration_run_history {where_sql}"),
                rusqlite::params_from_iter(bind.iter()),
                |row| row.get::<_, i64>(0),
            )
            .map_err(backend)?
            .try_into()
            .unwrap_or(0);
        let limit = limit.clamp(1, 100);
        bind.push(Box::new((offset + limit) as i64));
        bind.push(Box::new(offset as i64));
        let page_sql = format!(
            "SELECT id, operation, status, outcome, phase, profile_id, profile_revision, \
             source_connection_id, target_connection_id, started_at, finished_at, \
             selected_count, committed_count, failed_count, conflict_count, \
             cancelled, rollback_outcome, error_summary \
             FROM migration_run_history {where_sql} \
             ORDER BY started_at DESC, id LIMIT ?{} OFFSET ?{}",
            bind.len() - 1,
            bind.len()
        );
        let mut stmt = conn.prepare(&page_sql).map_err(backend)?;
        let items = stmt
            .query_map(rusqlite::params_from_iter(bind.iter()), run_from_row)
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(MigrationRunPage {
            items,
            total,
            offset,
            limit,
        })
    }
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MigrationRunRecord> {
    let selected_count: i64 = row.get(11)?;
    let committed_count: i64 = row.get(12)?;
    let failed_count: i64 = row.get(13)?;
    let conflict_count: i64 = row.get(14)?;
    let cancelled: i64 = row.get(15)?;
    Ok(MigrationRunRecord {
        id: row.get(0)?,
        operation: row.get(1)?,
        status: row.get(2)?,
        outcome: row.get(3)?,
        phase: row.get(4)?,
        profile_id: row.get(5)?,
        profile_revision: row.get(6)?,
        source_connection_id: row.get(7)?,
        target_connection_id: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
        selected_count: selected_count.try_into().unwrap_or(0),
        committed_count: committed_count.try_into().unwrap_or(0),
        failed_count: failed_count.try_into().unwrap_or(0),
        conflict_count: conflict_count.try_into().unwrap_or(0),
        cancelled: cancelled != 0,
        rollback_outcome: row.get(16)?,
        error_summary: row.get(17)?,
    })
}
