//! 目标侧内存库 + `TargetExecutor` 实现。
//!
//! `TargetStore` 区分已提交态（`base`）与事务内未提交覆盖（`overlay`），
//! 因此「批次 1 已提交 / 批次 2 回滚 / 批次 3 未开始」可直接从数据读出。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_data_sync::error::DataSyncError;
use datazen_data_sync::job::{RelationIdentity, TargetExecutor};
use datazen_data_sync::model::Row;
use datazen_data_sync::sql::SqlStatement;
use datazen_driver_api::Value;

use super::rows::key_repr;

pub type SharedStore = Arc<Mutex<TargetStore>>;

/// 目标表内存态。`overlay` 为事务内未提交改动，`None` 表示删除。
#[derive(Default)]
pub struct TargetStore {
    base: BTreeMap<String, Row>,
    overlay: BTreeMap<String, Option<Row>>,
}

impl TargetStore {
    pub fn new(rows: Vec<(Vec<Value>, Row)>) -> Self {
        let mut base = BTreeMap::new();
        for (key, value) in rows {
            base.insert(key_repr(&key), value);
        }
        Self {
            base,
            overlay: BTreeMap::new(),
        }
    }

    /// 事务内可见的读取结果。
    pub fn read(&self, key: &[Value]) -> Option<Row> {
        let k = key_repr(key);
        match self.overlay.get(&k) {
            Some(Some(row)) => Some(row.clone()),
            Some(None) => None,
            None => self.base.get(&k).cloned(),
        }
    }

    /// 事务内写入（暂存）。
    pub fn stage_put(&mut self, key: &[Value], row: Row) {
        self.overlay.insert(key_repr(key), Some(row));
    }

    /// 事务内删除（暂存）。
    pub fn stage_delete(&mut self, key: &[Value]) {
        self.overlay.insert(key_repr(key), None);
    }

    /// 提交：覆盖层并入已提交态。
    pub fn commit(&mut self) {
        let overlay = std::mem::take(&mut self.overlay);
        for (key, value) in overlay {
            match value {
                Some(row) => {
                    self.base.insert(key, row);
                }
                None => {
                    self.base.remove(&key);
                }
            }
        }
    }

    /// 回滚/丢弃：覆盖层整体丢弃。
    pub fn discard(&mut self) {
        self.overlay.clear();
    }

    /// 模拟「另一个连接」绕过本执行器直接改库（CM-43 的并发写入）。
    pub fn concurrent_write(&mut self, key: &[Value], row: Row) {
        self.base.insert(key_repr(key), row);
    }

    /// 已提交态读取（断言用）。
    pub fn committed(&self, key: &[Value]) -> Option<Row> {
        self.base.get(&key_repr(key)).cloned()
    }

    pub fn pending_writes(&self) -> usize {
        self.overlay.len()
    }
}

/// `generate_statements` 与执行器之间的私有协议：`statement.sql` 是结果行的取回键，
/// 值为 `None` 表示删除。
pub type StatementRows = Arc<Mutex<HashMap<String, Option<Row>>>>;

/// 可注入的失败钩子，用于造出取消 / 提交结果不可判两条分支。
#[derive(Clone, Copy, Default)]
pub struct ExecHook {
    /// 该批次第一条 execute 返回 `Cancelled`（批次在事务中被取消）。
    pub cancel_on_batch: Option<usize>,
    /// 该批次 commit 返回 `OutcomeUnknown`（提交结果不可判）。
    pub unknown_commit_on_batch: Option<usize>,
}

pub struct FakeExecutor {
    /// 绑定的目标 Lease（= `EndpointSession.handle.id`），全流程唯一。
    pub lease: String,
    store: SharedStore,
    plan_rows: StatementRows,
    events: Arc<Mutex<Vec<String>>>,
    hook: ExecHook,
    affected_override: Option<u64>,
    batch: usize,
}

impl FakeExecutor {
    pub fn new(
        lease: String,
        store: SharedStore,
        plan_rows: StatementRows,
        events: Arc<Mutex<Vec<String>>>,
        hook: ExecHook,
        affected_override: Option<u64>,
    ) -> Self {
        Self {
            lease,
            store,
            plan_rows,
            events,
            hook,
            affected_override,
            batch: 0,
        }
    }

    fn record(&self, event: String) {
        if let Ok(mut log) = self.events.lock() {
            log.push(event);
        }
    }
}

#[async_trait]
impl TargetExecutor for FakeExecutor {
    async fn begin(&mut self) -> Result<(), DataSyncError> {
        let tag = self.batch;
        self.record(format!("begin#{tag}"));
        self.batch += 1;
        Ok(())
    }

    async fn read_by_key(
        &mut self,
        _relation: &RelationIdentity,
        key: &[Value],
    ) -> Result<Option<Row>, DataSyncError> {
        self.record(format!("read#{}", key_repr(key)));
        Ok(self.store.lock().map_err(lock_err)?.read(key))
    }

    async fn execute(&mut self, statement: &SqlStatement) -> Result<u64, DataSyncError> {
        let batch = self.batch.saturating_sub(1);
        self.record(format!("execute#{batch}#{}", statement.sql));
        if self.hook.cancel_on_batch == Some(batch) {
            return Err(DataSyncError::cancelled("injected batch cancel"));
        }
        let planned = self
            .plan_rows
            .lock()
            .map_err(lock_err)?
            .get(statement.sql.as_str())
            .cloned();
        if let Some(planned) = planned {
            let mut store = self.store.lock().map_err(lock_err)?;
            match planned {
                Some(row) => store.stage_put(&statement.row_key, row),
                None => store.stage_delete(&statement.row_key),
            }
        }
        Ok(self.affected_override.unwrap_or(1))
    }

    async fn commit(&mut self) -> Result<(), DataSyncError> {
        let batch = self.batch.saturating_sub(1);
        self.record(format!("commit#{batch}"));
        if self.hook.unknown_commit_on_batch == Some(batch) {
            return Err(DataSyncError::outcome_unknown("injected unknown commit"));
        }
        self.store.lock().map_err(lock_err)?.commit();
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), DataSyncError> {
        let batch = self.batch.saturating_sub(1);
        self.record(format!("rollback#{batch}"));
        self.store.lock().map_err(lock_err)?.discard();
        Ok(())
    }

    async fn discard(&mut self) -> Result<(), DataSyncError> {
        let batch = self.batch.saturating_sub(1);
        self.record(format!("discard#{batch}"));
        self.store.lock().map_err(lock_err)?.discard();
        Ok(())
    }
}

fn lock_err<T>(_: std::sync::PoisonError<T>) -> DataSyncError {
    DataSyncError::validation("fake host mutex poisoned")
}
