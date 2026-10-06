//! `DataSyncHost` 的内存实现：CM-43/44/45 的假宿主。
//!
//! 端点会话按 `src{n}` / `tgt{n}` 命名，因此测试既可断言「开/关一一配对」，也可断言
//! 执行器绑定的 Lease 在整个 Job 内唯一。所有 state 访问都在同步块内完成（无跨 await 持锁）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_data_sync::error::DataSyncError;
use datazen_data_sync::filter::SyncSourceFilter;
use datazen_data_sync::job::host::{ArtifactStoreGuard, TableSchemaPair};
use datazen_data_sync::job::{
    ChangeBlock, ChangeSetArtifact, DataSyncHost, EndpointSession, KeysetPageSource,
    RelationIdentity, TargetExecutor, TransactionScope,
};
use datazen_data_sync::model::{ChangeOperation, Endpoint, Row, RowChange, SyncOptions};
use datazen_data_sync::sql::SqlStatement;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, TableSchema, Value};

use super::executor::{ExecHook, FakeExecutor, SharedStore, StatementRows, TargetStore};
use super::page_source::FakePageSource;
use super::rows::{endpoint, key_repr, schema as table_schema, FAMILY, TABLE};

pub const SOURCE_CONN: &str = "conn-src";
pub const TARGET_CONN: &str = "conn-tgt";

pub fn source_endpoint() -> Endpoint {
    endpoint(SOURCE_CONN)
}

pub fn target_endpoint() -> Endpoint {
    endpoint(TARGET_CONN)
}

fn role(connection_id: &str) -> &'static str {
    if connection_id.contains("src") {
        "src"
    } else {
        "tgt"
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SchemaMode {
    Compatible,
    Drifted,
}

struct HostState {
    session_seq: usize,
    opened: Vec<String>,
    closed: Vec<String>,
    /// close 路径判定为「资源已销毁但会话句柄未归还池」的会话（CM-45）。
    leaked: Vec<String>,
    artifacts: HashMap<String, ChangeSetArtifact>,
    selections: HashMap<(String, u64), Vec<ChangeBlock>>,
    calls: Vec<String>,
    source_rows: HashMap<String, Vec<Row>>,
    target_rows: HashMap<String, Vec<Row>>,
    permissions_ok: bool,
    scope: TransactionScope,
    fail_open: Option<String>,
    fail_close_once: bool,
    schema_mode: SchemaMode,
    statement_seq: usize,
    hook: ExecHook,
    affected_override: Option<u64>,
    executor_leases: Vec<String>,
    /// 阶段自身的拒绝理由：runtime 不携带错误文本，host 端口是唯一的证据通道。
    stage_failures: Vec<(String, String)>,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            session_seq: 0,
            opened: Vec::new(),
            closed: Vec::new(),
            leaked: Vec::new(),
            artifacts: HashMap::new(),
            selections: HashMap::new(),
            calls: Vec::new(),
            source_rows: HashMap::new(),
            target_rows: HashMap::new(),
            permissions_ok: true,
            scope: TransactionScope::Batch,
            fail_open: None,
            fail_close_once: false,
            schema_mode: SchemaMode::Compatible,
            statement_seq: 0,
            hook: ExecHook::default(),
            affected_override: None,
            executor_leases: Vec::new(),
            stage_failures: Vec::new(),
        }
    }
}

pub struct FakeHost {
    state: Mutex<HostState>,
    store: SharedStore,
    plan_rows: StatementRows,
    events: Arc<Mutex<Vec<String>>>,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeHost {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HostState::default()),
            store: Arc::new(Mutex::new(TargetStore::default())),
            plan_rows: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // ----- 配置（构造期调用，链式） -----

    pub fn source_rows(&self, table: &str, rows: Vec<Row>) -> &Self {
        self.write(|s| {
            s.source_rows.insert(table.to_string(), rows);
        });
        self
    }

    /// 目标表快照（prepare 比较时的目标侧读源）。
    pub fn target_rows(&self, table: &str, rows: Vec<Row>) -> &Self {
        self.write(|s| {
            s.target_rows.insert(table.to_string(), rows);
        });
        self
    }

    /// 预置目标表已提交态（CM-43 的 review 基线）。
    pub fn seed_target(&self, rows: Vec<(Vec<Value>, Row)>) -> &Self {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        for (key, value) in rows {
            store.concurrent_write(&key, value);
        }
        self
    }

    pub fn artifact(&self, artifact: ChangeSetArtifact) -> &Self {
        self.write(|s| {
            s.artifacts.insert(artifact.plan_id.clone(), artifact);
        });
        self
    }

    pub fn selection(&self, plan_id: &str, revision: u64, blocks: Vec<ChangeBlock>) -> &Self {
        self.write(|s| {
            s.selections.insert((plan_id.to_string(), revision), blocks);
        });
        self
    }

    pub fn fail_open(&self, connection_id: &str) -> &Self {
        self.write(|s| s.fail_open = Some(connection_id.to_string()));
        self
    }

    /// 下一次 close 走「资源销毁失败」分支（CM-45）。
    pub fn fail_close_once(&self) -> &Self {
        self.write(|s| s.fail_close_once = true);
        self
    }

    pub fn deny_permissions(&self) -> &Self {
        self.write(|s| s.permissions_ok = false);
        self
    }

    pub fn scope(&self, scope: TransactionScope) -> &Self {
        self.write(|s| s.scope = scope);
        self
    }

    pub fn exec_hook(&self, hook: ExecHook) -> &Self {
        self.write(|s| s.hook = hook);
        self
    }

    pub fn affected_override(&self, affected: u64) -> &Self {
        self.write(|s| s.affected_override = Some(affected));
        self
    }

    pub fn schema_mode(&self, mode: SchemaMode) -> &Self {
        self.write(|s| s.schema_mode = mode);
        self
    }

    // ----- 断言用的观察口 -----

    pub fn opened(&self) -> Vec<String> {
        self.read(|s| s.opened.clone())
    }

    pub fn closed(&self) -> Vec<String> {
        self.read(|s| s.closed.clone())
    }

    pub fn leaked(&self) -> Vec<String> {
        self.read(|s| s.leaked.clone())
    }

    /// 开了却没关掉的会话（应为空；CM-45 的泄漏例外除外）。
    pub fn live(&self) -> Vec<String> {
        self.read(|s| {
            s.opened
                .iter()
                .filter(|id| !s.closed.contains(id) && !s.leaked.contains(id))
                .cloned()
                .collect()
        })
    }

    pub fn calls(&self) -> Vec<String> {
        self.read(|s| s.calls.clone())
    }

    pub fn call_count(&self, name: &str) -> usize {
        self.calls().iter().filter(|c| c.as_str() == name).count()
    }

    pub fn executor_leases(&self) -> Vec<String> {
        self.read(|s| s.executor_leases.clone())
    }

    /// 执行器事件：`begin#0` / `execute#0#stmt-1` / `commit#0` / `rollback#0` …
    pub fn events(&self) -> Vec<String> {
        self.events
            .lock()
            .map(|log| log.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    pub fn has_event(&self, needle: &str) -> bool {
        self.events().iter().any(|e| e.as_str() == needle)
    }

    /// 阶段自身写下的失败理由（`(stage, reason)`）。
    pub fn stage_failures(&self) -> Vec<(String, String)> {
        self.read(|s| s.stage_failures.clone())
    }

    /// 最近一条阶段失败理由；`has_stage_failure` 先确认真的写过。
    pub fn last_stage_failure(&self) -> Option<String> {
        self.read(|s| s.stage_failures.last().map(|(_, reason)| reason.clone()))
    }

    pub fn count_events(&self, needle: &str) -> usize {
        self.events()
            .iter()
            .filter(|e| e.as_str() == needle)
            .count()
    }

    pub fn committed(&self, key: &[Value]) -> Option<Row> {
        self.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .committed(key)
    }

    pub fn pending_writes(&self) -> usize {
        self.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_writes()
    }

    /// 模拟「另一个连接」在 review 之后改了目标行（CM-43）。
    pub fn concurrent_write(&self, key: &[Value], row: Row) {
        self.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .concurrent_write(key, row);
    }

    // ----- 内部 -----

    fn write<F: FnOnce(&mut HostState)>(&self, f: F) {
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
    }

    fn write_and_return<T, F: FnOnce(&mut HostState) -> T>(&self, f: F) -> T {
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    fn read<T, F: FnOnce(&HostState) -> T>(&self, f: F) -> T {
        let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    fn call(&self, name: &str) {
        self.write(|s| s.calls.push(name.to_string()));
    }
}

#[async_trait]
impl DataSyncHost for FakeHost {
    async fn open_endpoint(&self, endpoint: &Endpoint) -> Result<EndpointSession, DataSyncError> {
        self.call("open_endpoint");
        if self.read(|s| s.fail_open.as_deref() == Some(endpoint.connection_id.as_str())) {
            return Err(DataSyncError::not_started(format!(
                "fake host refuses to open {}",
                endpoint.connection_id
            )));
        }
        let id = self.write_and_return(|s| {
            s.session_seq += 1;
            let id = format!("{}-{}", role(&endpoint.connection_id), s.session_seq);
            s.opened.push(id.clone());
            id
        });
        let driver: Arc<dyn DatabaseDriver> = MockDriver::new(FAMILY, MockDriverOptions::default());
        Ok(EndpointSession {
            driver,
            handle: ConnectionHandle {
                id,
                pool_id: format!("pool-{}", endpoint.connection_id),
            },
            family: FAMILY.to_string(),
            database: endpoint.database.clone(),
            schema: endpoint.schema.clone(),
        })
    }

    /// 端口不返回错误：会话资源在此无条件销毁，连接模式不回流到池。
    /// `fail_close_once` 造出「资源已销毁、句柄未归还」的 CM-45 分支。
    async fn close_endpoint(&self, session: EndpointSession) {
        self.call("close_endpoint");
        let id = session.handle.id.clone();
        self.write(|s| {
            if s.fail_close_once {
                s.fail_close_once = false;
                s.calls
                    .push(format!("close_endpoint:resource-destroyed({id})"));
                s.leaked.push(id);
            } else {
                s.closed.push(id);
            }
        });
    }

    async fn mapped_table_schemas(
        &self,
        _source: &EndpointSession,
        _target: &EndpointSession,
        mappings: &[datazen_data_sync::model::TableMapping],
    ) -> Result<Vec<TableSchemaPair>, DataSyncError> {
        self.call("mapped_table_schemas");
        let mode = self.read(|s| s.schema_mode);
        Ok(mappings
            .iter()
            .map(|m| TableSchemaPair {
                source_table: m.source_table.clone(),
                target_table: m.target_table.clone(),
                source: table_schema(&m.source_table),
                target: match mode {
                    SchemaMode::Compatible => table_schema(&m.target_table),
                    SchemaMode::Drifted => super::rows::drifted_schema(&m.target_table),
                },
            })
            .collect())
    }

    async fn table_reader(
        &self,
        session: &EndpointSession,
        table: &str,
        _filter: Option<&SyncSourceFilter>,
    ) -> Result<Box<dyn KeysetPageSource>, DataSyncError> {
        self.call("table_reader");
        let is_source = session.handle.id.starts_with("src");
        let rows = self
            .read(|s| {
                if is_source {
                    s.source_rows.get(table).cloned()
                } else {
                    s.target_rows.get(table).cloned()
                }
            })
            .ok_or_else(|| DataSyncError::not_started(format!("no fixture rows for {table}")))?;
        Ok(Box::new(FakePageSource::new(rows, vec![0])))
    }

    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactStoreGuard, DataSyncError> {
        self.call("store_artifact");
        let id = format!("artifact-{}", artifact.plan_id);
        self.write(|s| {
            s.artifacts.insert(artifact.plan_id.clone(), artifact);
        });
        Ok(ArtifactStoreGuard { artifact_id: id })
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        self.call("load_artifact");
        Ok(self.read(|s| s.artifacts.get(plan_id).cloned()))
    }

    async fn selection(
        &self,
        plan_id: &str,
        revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError> {
        self.call("selection");
        Ok(self
            .read(|s| s.selections.get(&(plan_id.to_string(), revision)).cloned())
            .unwrap_or_default())
    }

    async fn check_permissions(&self, _session: &EndpointSession) -> Result<(), DataSyncError> {
        self.call("check_permissions");
        if self.read(|s| s.permissions_ok) {
            Ok(())
        } else {
            Err(DataSyncError::conflict("fake host denies target write"))
        }
    }

    async fn transaction_scope(
        &self,
        _session: &EndpointSession,
    ) -> Result<TransactionScope, DataSyncError> {
        self.call("transaction_scope");
        Ok(self.read(|s| s.scope))
    }

    async fn target_executor(
        &self,
        session: &EndpointSession,
    ) -> Result<Box<dyn TargetExecutor>, DataSyncError> {
        self.call("target_executor");
        let (hook, affected) = self.read(|s| (s.hook, s.affected_override));
        self.write(|s| s.executor_leases.push(session.handle.id.clone()));
        Ok(Box::new(FakeExecutor::new(
            session.handle.id.clone(),
            Arc::clone(&self.store),
            Arc::clone(&self.plan_rows),
            Arc::clone(&self.events),
            hook,
            affected,
        )))
    }

    async fn generate_statements(
        &self,
        _relation: &RelationIdentity,
        _source_table: &str,
        target_table: &str,
        changes: Vec<RowChange>,
        _options: &SyncOptions,
    ) -> Result<Vec<SqlStatement>, DataSyncError> {
        self.call("generate_statements");
        let mut planned: Vec<(String, Option<Row>)> = Vec::with_capacity(changes.len());
        let mut out = Vec::with_capacity(changes.len());
        for change in changes {
            let sql = self.write_and_return(|s| {
                s.statement_seq += 1;
                format!("stmt-{}", s.statement_seq)
            });
            // 私有协议：`sql` 字段是结果行的取回键，None = 删除。
            let result = match change.operation {
                ChangeOperation::Delete => None,
                ChangeOperation::Insert | ChangeOperation::Update => change.source_row.clone(),
                // 冻结 ChangeSet 里不应出现 Unchanged；这里不静默造数据。
                ChangeOperation::Unchanged => {
                    return Err(DataSyncError::conflict(
                        "frozen ChangeSet carries an Unchanged block",
                    ))
                }
            };
            planned.push((sql.clone(), result));
            out.push(SqlStatement {
                table: target_table.to_string(),
                operation: change.operation,
                sql,
                preview_sql: format!("{TABLE} -> {target_table}"),
                parameters: change.key.clone(),
                row_key: change.key.clone(),
                identity_insert: None,
            });
        }
        let mut registry = self.plan_rows.lock().unwrap_or_else(|e| e.into_inner());
        for (sql, result) in planned {
            let _ = registry.insert(sql, result);
        }
        drop(registry);
        Ok(out)
    }

    fn now(&self) -> String {
        "2026-01-01T00:00:00Z".to_string()
    }

    fn record_stage_failure(&self, stage: &str, reason: String) {
        self.write(|s| s.stage_failures.push((stage.to_string(), reason)));
    }
}

// `MutexGuard` 不是 `Send`：以上所有 helper 都在同步块内取放 state，不跨 `.await`。

/// 便于测试断言 key 形态。
pub fn key_of(values: &[Value]) -> String {
    key_repr(values)
}

/// 与 `mapped_table_schemas(Compatible)` 一致的目标表结构。
pub fn target_table_schema() -> TableSchema {
    table_schema(TABLE)
}
