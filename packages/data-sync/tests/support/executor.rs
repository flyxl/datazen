//! 目标侧内存库 + `TargetExecutor` 实现。
//!
//! `TargetStore` 区分已提交态（`base`）与事务内未提交覆盖（`overlay`），
//! 因此「批次 1 已提交 / 批次 2 回滚 / 批次 3 未开始」可直接从数据读出。

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// 该批次 commit 成功后置位 kernel `CancelToken` 的标志位：模拟取消请求
    /// 在 Job 运行途中抵达，而不是在 stage 开头就已取消。
    pub cancel_token_after_commit: Option<usize>,
    /// 该批次第 N 条 execute 成功后置位 kernel `CancelToken` 的标志位。
    /// 用于把取消请求投递到批次内部的两次写之间，逼出宿主执行器的写前检查点。
    pub cancel_token_after_statement: Option<(usize, usize)>,
}

pub struct FakeExecutor {
    /// 绑定的目标 Lease（= `EndpointSession.handle.id`），全流程唯一。
    pub lease: String,
    store: SharedStore,
    plan_rows: StatementRows,
    events: Arc<Mutex<Vec<String>>>,
    hook: ExecHook,
    affected_override: Option<u64>,
    /// kernel `CancelToken` 的标志位（`CancelToken::flag()`）。
    cancel: Arc<AtomicBool>,
    batch: usize,
    statement: usize,
}

impl FakeExecutor {
    pub fn new(
        lease: String,
        store: SharedStore,
        plan_rows: StatementRows,
        events: Arc<Mutex<Vec<String>>>,
        hook: ExecHook,
        affected_override: Option<u64>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            lease,
            store,
            plan_rows,
            events,
            hook,
            affected_override,
            cancel,
            batch: 0,
            statement: 0,
        }
    }

    fn record(&self, event: String) {
        if let Ok(mut log) = self.events.lock() {
            log.push(event);
        }
    }

    /// 与宿主执行器同样的三处检查点：目标侧每次触碰数据库之前都先看 kernel
    /// 取消标志位，标志位来自 `CancelToken::flag()`，不是宿主自己铸的。
    fn checkpoint(&self, what: &str) -> Result<(), DataSyncError> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(DataSyncError::cancelled(format!("cancelled before {what}")));
        }
        Ok(())
    }
}

#[async_trait]
impl TargetExecutor for FakeExecutor {
    async fn begin(&mut self) -> Result<(), DataSyncError> {
        let tag = self.batch;
        self.record(format!("begin#{tag}"));
        self.batch += 1;
        self.checkpoint("begin")
    }

    async fn read_by_key(
        &mut self,
        _relation: &RelationIdentity,
        key: &[Value],
    ) -> Result<Option<Row>, DataSyncError> {
        self.record(format!("read#{}", key_repr(key)));
        self.checkpoint("read_by_key")?;
        Ok(self.store.lock().map_err(lock_err)?.read(key))
    }

    async fn execute(&mut self, statement: &SqlStatement) -> Result<u64, DataSyncError> {
        let batch = self.batch.saturating_sub(1);
        self.record(format!("execute#{batch}#{}", statement.sql));
        self.checkpoint("execute")?;
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
        if self.hook.cancel_token_after_statement == Some((batch, self.statement)) {
            self.cancel.store(true, Ordering::SeqCst);
        }
        self.statement += 1;
        Ok(self.affected_override.unwrap_or(1))
    }

    async fn commit(&mut self) -> Result<(), DataSyncError> {
        let batch = self.batch.saturating_sub(1);
        self.record(format!("commit#{batch}"));
        if self.hook.unknown_commit_on_batch == Some(batch) {
            return Err(DataSyncError::outcome_unknown("injected unknown commit"));
        }
        self.store.lock().map_err(lock_err)?.commit();
        if self.hook.cancel_token_after_commit == Some(batch) {
            self.cancel.store(true, Ordering::SeqCst);
        }
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
