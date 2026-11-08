//! Runtime 形态中立的 Data Sync 执行宿主（W8 刀3e-4a 下沉）。
//!
//! 桌面对照 `datazen-desktop/src/commands/sync/host/`（mod.rs 的
//! [`HostDataSync`]、executor.rs 的两个驱动端口实现、recording.rs 的失败
//! 记录装饰器——本刀整删，桌面改为薄适配）。下沉动机与 B2 域刀的先例
//! （`keyset_source.rs` / `artifact_view.rs`）同一条规则：**会话→端点借出、
//! 适配器解析、结构缓存、keyset 读取、批次执行、SQL 装配上下文**全部
//! 形态中立——桌面与团队服务器共用同一份实现，形态差异只留三个端口：
//!
//! - [`HostConnectionConfigs`] —— 连接档案回读（桌面单租户 store /
//!   服务端组织隔离目录）；
//! - [`SyncArtifactStore`] —— ChangeSet 工件与已确认选区的进程内注册表
//!   （桌面静态表 / 服务端组织键隔离的 plan 仓储扩展）；
//! - [`FailureSink`] —— 宿主侧失败消息的落点（桌面静态 FAILURES 表 /
//!   服务端装配态 `SyncRuntimeState`）。
//!
//! 既有纪律随迁：一个 Data Sync Job 的整个生命周期只通过**一条** runtime
//! 会话与其活连接租约对话（§9）；apply Job 的每个批次只有**一个**目标
//! 租约（§5.3）；取消位不是簿记——它由阶段的 `CancelToken` 原样递给
//! keyset 读取与批次执行（长遍历因此能在 stage 运行中被 cancel 打断）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use datazen_driver_api::{ConnectionConfig, DatabaseType, TableSchema};

use crate::execution::application::RuntimeConnectionUseCases;
use crate::execution::application::{LiveConnectionLease, LivePurpose};
use crate::jobs::CancelToken;
use crate::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry;
use crate::services::schema_scope::metadata_schema;
use crate::shared::context::RequestContext;
use datazen_driver_api::LegacySqlDialect;

use crate::migration::data_sync::api::{
    generate_reviewed_batch_sql, BatchSqlContext, BatchSqlHost,
};
use crate::migration::data_sync::compare::RowPageSource;
use crate::migration::data_sync::error::DataSyncError;
use crate::migration::data_sync::filter::SyncSourceFilter;
use crate::migration::data_sync::filter_validation::{
    resolve_key_contracts, validate_filter_endpoints, validate_filter_schemas,
};
use crate::migration::data_sync::job::artifact::{
    ChangeBlock, ChangeSetArtifact, RelationIdentity,
};
use crate::migration::data_sync::job::host::{
    ArtifactRepositoryGuard, DataSyncHost, EndpointSession, KeysetPageSource, TableSchemaPair,
    TargetExecutor, TransactionScope,
};
use crate::migration::data_sync::keyset_source::DriverKeysetSource;
use crate::migration::data_sync::model::{Endpoint, Row, RowChange, SyncOptions, TableMapping};
use crate::migration::data_sync::pairing::family_of;
use crate::migration::data_sync::sql::SqlStatement;
use crate::migration::data_sync::{require_data_sync_family, select_by_key_sql};

/// 连接档案回读端口：宿主唯一的形态差异点之一。
///
/// 桌面实现包一层单租户 store（忽略 ctx）；服务端实现按 `ctx.organization_id`
/// 走组织隔离目录。回读 miss 即「连接档案已不在」——打开端点按 fail-closed
/// 拒绝。
#[async_trait]
pub trait HostConnectionConfigs: Send + Sync {
    async fn connection_config(
        &self,
        ctx: &RequestContext,
        connection_id: &str,
    ) -> Option<ConnectionConfig>;
}

/// ChangeSet 工件与已确认选区的注册表端口：宿主形态差异点之二。
///
/// 语义（桌面 `host/state.rs` + `host/selection.rs` 的不变式）：
/// 工件按 planId 冻结存储；选区在 apply 受理后安装、**读取即消费**
/// （§2.1 / CM-41——同一 planId 的第二次认领一无所获）；选区版本必须
/// 与用户复核时的版本一致。
#[async_trait]
pub trait SyncArtifactStore: Send + Sync {
    /// 冻结 prepare 产出的不可变 ChangeSet 工件。
    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactRepositoryGuard, DataSyncError>;
    /// apply 加载工件（miss → `None`，调用方按「计划不可用」拒绝）。
    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError>;
    /// 消费已确认的块子集（读取即消费；版本不匹配 → 拒绝）。
    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError>;
}

/// 宿主侧失败消息落点：形态差异点之三（装饰器 [`RecordingDataSyncHost`]
/// 的唯一生产通道）。
pub trait FailureSink: Send + Sync {
    fn record(&self, job_id: &str, message: String);
}

fn invalid(message: impl Into<String>) -> DataSyncError {
    DataSyncError::validation(message.into())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Everything the handler needs to read a relation, cached at
/// `mapped_table_schemas` time so no port call has to re-derive SQL.
pub struct RelationMeta {
    pub is_source: bool,
    pub db_type: DatabaseType,
    pub family: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub quote: char,
    pub columns: Vec<String>,
    pub column_types: HashMap<String, String>,
    pub pk_columns: Vec<String>,
    pub key_contracts: Vec<datazen_driver_api::SyncKeyContract>,
    pub recordset_limit: Option<u64>,
    pub select_by_key_sql: String,
    pub schema_obj: TableSchema,
}

/// One opened endpoint lease owned by the durable Job worker.
#[derive(Clone)]
struct Slot {
    is_source: bool,
    session: EndpointSession,
    db_type: DatabaseType,
    read_only: bool,
    _lease: Arc<LiveConnectionLease>,
}

/// Runtime 形态中立的 [`DataSyncHost`] 实现（桌面 `HostDataSync` 的下沉名）。
pub struct RuntimeDataSyncHost {
    runtime: Arc<RuntimeConnectionUseCases>,
    call_context: RequestContext,
    configs: Arc<dyn HostConnectionConfigs>,
    artifacts: Arc<dyn SyncArtifactStore>,
    adapters: Arc<SyncAdapterRegistry>,
    source: Endpoint,
    target: Endpoint,
    filters: HashMap<String, SyncSourceFilter>,
    slots: Mutex<Vec<Slot>>,
    meta: Mutex<HashMap<(bool, String), RelationMeta>>,
    /// webdriver 专属的提交故障注入（生产恒 `None`；桌面 e2e 构建注入）。
    commit_fault: Option<Arc<dyn CommitFaultHook>>,
}

impl RuntimeDataSyncHost {
    pub fn new(
        runtime: Arc<RuntimeConnectionUseCases>,
        call_context: RequestContext,
        configs: Arc<dyn HostConnectionConfigs>,
        artifacts: Arc<dyn SyncArtifactStore>,
        adapters: Arc<SyncAdapterRegistry>,
        source: Endpoint,
        target: Endpoint,
        filters: HashMap<String, SyncSourceFilter>,
    ) -> Self {
        Self {
            runtime,
            call_context,
            configs,
            artifacts,
            adapters,
            source,
            target,
            filters,
            slots: Mutex::new(Vec::new()),
            meta: Mutex::new(HashMap::new()),
            commit_fault: None,
        }
    }

    /// 注入提交故障钩子（webdriver e2e 专属；普通构建不调用）。
    pub fn with_commit_fault(mut self, hook: Arc<dyn CommitFaultHook>) -> Self {
        self.commit_fault = Some(hook);
        self
    }

    /// Which side of the pair an endpoint belongs to. Sides are claimed in
    /// open order (prepare opens source first), so a self-sync pair cannot
    /// collapse into one side by comparing connection ids.
    fn next_side(&self, endpoint: &Endpoint) -> Result<bool, DataSyncError> {
        let (have_source, have_target) = {
            let slots = lock(&self.slots);
            (
                slots.iter().any(|slot| slot.is_source),
                slots.iter().any(|slot| !slot.is_source),
            )
        };
        if !have_source {
            return Ok(true);
        }
        if !have_target {
            return Ok(false);
        }
        if endpoint.same_database_as(&self.target) {
            return Ok(false);
        }
        if endpoint.same_database_as(&self.source) {
            return Ok(true);
        }
        Err(invalid(format!(
            "endpoint {} is not part of this Data Sync plan",
            endpoint.db_session_id
        )))
    }

    /// Filter for one mapping: the request's filter wins, then the mapping's
    /// frozen copy, then "no filter" (an all-empty filter is the same thing).
    fn filter_for(
        &self,
        mapping: &TableMapping,
    ) -> Result<Option<SyncSourceFilter>, DataSyncError> {
        let candidate = self
            .filters
            .get(&mapping.source_table)
            .cloned()
            .or_else(|| mapping.source_filter.clone());
        match candidate {
            Some(filter)
                if filter
                    .is_empty()
                    .map_err(|error| invalid(error.to_string()))? =>
            {
                Ok(None)
            }
            other => Ok(other),
        }
    }

    fn slot_of(&self, session: &EndpointSession) -> Result<Slot, DataSyncError> {
        lock(&self.slots)
            .iter()
            .find(|slot| slot.session.handle.id == session.handle.id)
            .map(|slot| Slot {
                is_source: slot.is_source,
                session: EndpointSession {
                    driver: session.driver.clone(),
                    handle: session.handle.clone(),
                    family: session.family.clone(),
                    database: session.database.clone(),
                    schema: session.schema.clone(),
                },
                db_type: slot.db_type.clone(),
                read_only: slot.read_only,
                _lease: slot._lease.clone(),
            })
            .ok_or_else(|| invalid("endpoint session is not owned by this Data Sync Job"))
    }

    fn relation_meta(&self, is_source: bool, table: &str) -> Result<RelationMeta, DataSyncError> {
        let meta = lock(&self.meta)
            .get(&(is_source, table.to_string()))
            .map(|meta| RelationMeta {
                is_source: meta.is_source,
                db_type: meta.db_type.clone(),
                family: meta.family.clone(),
                database: meta.database.clone(),
                schema: meta.schema.clone(),
                table: meta.table.clone(),
                quote: meta.quote,
                columns: meta.columns.clone(),
                column_types: meta.column_types.clone(),
                pk_columns: meta.pk_columns.clone(),
                key_contracts: meta.key_contracts.clone(),
                recordset_limit: meta.recordset_limit,
                select_by_key_sql: meta.select_by_key_sql.clone(),
                schema_obj: meta.schema_obj.clone(),
            });
        meta.ok_or_else(|| {
            invalid(format!(
                "relation {table} was not part of the compared plan; compare again"
            ))
        })
    }

    fn adapter_for(
        &self,
        db_type: &DatabaseType,
    ) -> Result<Arc<dyn datazen_driver_api::SyncSourceAdapter>, DataSyncError> {
        self.adapters
            .get_source(db_type)
            .ok_or_else(|| invalid(format!("no Data Sync key adapter for {db_type}")))
    }

    /// Literal renderer / relation qualifier for the live target dialect.
    pub fn sync_target_adapter(
        &self,
        db_type: &DatabaseType,
    ) -> Result<Arc<dyn datazen_driver_api::SyncTargetAdapter>, DataSyncError> {
        self.adapters
            .get_target(db_type)
            .ok_or_else(|| invalid(format!("no Data Sync SQL adapter for {db_type}")))
    }

    pub fn sync_source_adapter(
        &self,
        db_type: &DatabaseType,
    ) -> Result<Arc<dyn datazen_driver_api::SyncSourceAdapter>, DataSyncError> {
        self.adapter_for(db_type)
    }

    /// The opened target lease. SQL generation needs the live driver for
    /// placeholder syntax and identity-insert toggles, so it never reads a
    /// cached connection.
    fn target_slot(&self) -> Result<Slot, DataSyncError> {
        let slots = lock(&self.slots);
        slots
            .iter()
            .find(|slot| !slot.is_source)
            .cloned()
            .ok_or_else(|| invalid("target endpoint is not open"))
    }

    /// Open one endpoint lease.
    ///
    /// The endpoint names a dedicated, job-owned `dbSessionId`, so this host
    /// opens that exact session rather than resolving by connection config.
    /// The Job lifecycle owns that session and the physical endpoint budget;
    /// this adapter only retains a driver handle while the stage runs.
    async fn open(&self, endpoint: &Endpoint) -> Result<EndpointSession, DataSyncError> {
        let is_source = self.next_side(endpoint)?;
        self.open_locked(&endpoint.db_session_id, endpoint, is_source)
            .await
    }

    async fn open_locked(
        &self,
        db_session_id: &str,
        endpoint: &Endpoint,
        is_source: bool,
    ) -> Result<EndpointSession, DataSyncError> {
        let (view, lease) = self
            .runtime
            .borrow_live_connection_by_id(
                &self.call_context,
                db_session_id,
                LivePurpose::LongLivedHostWork,
            )
            .await
            .map_err(|error| invalid(format!("cannot open database session: {error}")))?;
        let connection = lease
            .connection()
            .map_err(|error| invalid(error.to_string()))?;
        let driver = Arc::clone(connection.driver());
        let handle = connection.handle().clone();
        let config = self
            .configs
            .connection_config(&self.call_context, view.connection_id.as_str())
            .await
            .ok_or_else(|| invalid("connection profile is no longer available"))?;
        // Family pairing is checked as soon as the second endpoint opens, so
        // the user sees the cross-family explanation before any work happens.
        let peer = {
            let slots = lock(&self.slots);
            slots
                .iter()
                .find(|slot| slot.is_source != is_source)
                .map(|slot| slot.db_type.clone())
        };
        // Both endpoints carry the **canonical** family, never the raw driver
        // type: the handler compares `source.family == target.family`, and two
        // profiles of the same family may spell it differently (`postgres` vs
        // `postgresql`, `cloudberry` → `postgresql`). Normalizing here also keeps
        // `scope_for` on a name it actually knows.
        let family = match peer {
            Some(peer) => require_data_sync_family(
                if is_source {
                    &config.database_type
                } else {
                    &peer
                },
                if is_source {
                    &peer
                } else {
                    &config.database_type
                },
            )?,
            None => family_of(&config.database_type),
        };
        // Prepare only reads the target. Read-only admission belongs to the
        // apply permission gate, so users can still inspect and compare it.
        let database = crate::migration::data_sync::api::resolve_db_name(
            Some(&endpoint.database),
            config.database.as_deref(),
        );
        let schema = metadata_schema(
            driver.as_ref(),
            endpoint.schema.as_deref(),
            None,
            config.schema.as_deref(),
        );
        let session = EndpointSession {
            driver,
            handle,
            family,
            database: database.clone(),
            schema: schema.clone(),
        };
        lock(&self.slots).push(Slot {
            is_source,
            session: session.clone(),
            db_type: config.database_type.clone(),
            read_only: config.read_only,
            _lease: Arc::new(lease),
        });
        Ok(session)
    }
}

#[async_trait]
impl DataSyncHost for RuntimeDataSyncHost {
    async fn open_endpoint(&self, endpoint: &Endpoint) -> Result<EndpointSession, DataSyncError> {
        self.open(endpoint).await
    }

    async fn close_endpoint(&self, session: EndpointSession) {
        let popped = {
            let mut slots = lock(&self.slots);
            slots
                .iter()
                .position(|slot| slot.session.handle.id == session.handle.id)
                .map(|index| slots.swap_remove(index))
        };
        drop(popped);
    }

    async fn mapped_table_schemas(
        &self,
        source: &EndpointSession,
        target: &EndpointSession,
        mappings: &[TableMapping],
    ) -> Result<Vec<TableSchemaPair>, DataSyncError> {
        let source_slot = self.slot_of(source)?;
        let target_slot = self.slot_of(target)?;
        self.adapters
            .ensure_pair(&source_slot.db_type, &target_slot.db_type)
            .map_err(invalid)?;
        let src_adapter = self.adapter_for(&source_slot.db_type)?;
        let tgt_adapter = self.adapter_for(&target_slot.db_type)?;
        let mut pairs = Vec::with_capacity(mappings.len());
        for mapping in mappings.iter().filter(|mapping| mapping.enabled) {
            let source_schema = source_slot
                .session
                .driver
                .get_table_schema(
                    &source_slot.session.handle,
                    &mapping.source_table,
                    &source_slot.session.database,
                    source_slot.session.schema.as_deref(),
                )
                .await
                .map_err(|error| {
                    invalid(format!(
                        "table {}: cannot read source schema: {error}",
                        mapping.source_table
                    ))
                })?;
            let target_schema = target_slot
                .session
                .driver
                .get_table_schema(
                    &target_slot.session.handle,
                    &mapping.target_table,
                    &target_slot.session.database,
                    target_slot.session.schema.as_deref(),
                )
                .await
                .map_err(|error| {
                    invalid(format!(
                        "table {}: cannot read target schema: {error}",
                        mapping.target_table
                    ))
                })?;
            let pk_columns = source_schema.effective_primary_keys();
            let filter = self.filter_for(mapping)?;
            if let Some(filter) = filter.as_ref() {
                validate_filter_schemas(
                    filter,
                    &source_schema,
                    &target_schema,
                    &mapping.source_table,
                    &mapping.target_table,
                )
                .map_err(|error| invalid(error.to_string()))?;
            }
            let (source_contracts, target_contracts) = resolve_key_contracts(
                &pk_columns,
                src_adapter.as_ref(),
                tgt_adapter.as_ref(),
                &source_schema,
                &target_schema,
                &mapping.source_table,
                &mapping.target_table,
            )
            .map_err(|error| invalid(error.to_string()))?;
            if let Some(filter) = filter.as_ref() {
                validate_filter_endpoints(
                    filter,
                    &pk_columns,
                    source_slot.session.driver.as_ref(),
                    target_slot.session.driver.as_ref(),
                    src_adapter.as_ref(),
                    tgt_adapter.as_ref(),
                    &source_schema,
                    &target_schema,
                    &source_contracts,
                    &target_contracts,
                    &mapping.source_table,
                )
                .map_err(|error| invalid(error.to_string()))?;
            }
            let recordset_limit = match filter.as_ref() {
                Some(filter) => filter
                    .recordset_limit(&source_schema)
                    .map_err(|error| invalid(error.to_string()))?,
                None => None,
            };
            let mut meta = lock(&self.meta);
            for (is_source, slot, schema, contracts, table) in [
                (
                    true,
                    &source_slot,
                    &source_schema,
                    &source_contracts,
                    mapping.source_table.clone(),
                ),
                (
                    false,
                    &target_slot,
                    &target_schema,
                    &target_contracts,
                    mapping.target_table.clone(),
                ),
            ] {
                let quote = slot.session.driver.quote_char();
                // 占位符规则来自 SqlDialect 契约，旧驱动 API 逐条转发。
                let dialect = LegacySqlDialect::new(slot.session.driver.as_ref());
                // Each side keys on its own declared primary key. The pair
                // shares a keyset contract (checked above), but the predicate
                // that is actually sent to a connection must be built from that
                // connection's own key columns.
                let side_pk = schema.effective_primary_keys();
                let columns = schema
                    .columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect::<Vec<_>>();
                let column_types = schema
                    .columns
                    .iter()
                    .map(|column| (column.name.clone(), column.data_type.clone()))
                    .collect::<HashMap<String, String>>();
                meta.insert(
                    (is_source, table.clone()),
                    RelationMeta {
                        is_source,
                        db_type: slot.db_type.clone(),
                        family: slot.session.family.clone(),
                        database: slot.session.database.clone(),
                        schema: slot.session.schema.clone(),
                        table: table.clone(),
                        quote,
                        columns: columns.clone(),
                        column_types: column_types.clone(),
                        pk_columns: side_pk.clone(),
                        key_contracts: contracts.clone(),
                        recordset_limit,
                        select_by_key_sql: select_by_key_sql(
                            &dialect,
                            &quote,
                            &slot.session.database,
                            slot.session.schema.as_deref(),
                            &table,
                            &columns,
                            &side_pk,
                            &column_types,
                        )?,
                        schema_obj: schema.clone(),
                    },
                );
            }
            drop(meta);
            pairs.push(TableSchemaPair {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                source: source_schema,
                target: target_schema,
            });
        }
        Ok(pairs)
    }

    async fn table_reader(
        &self,
        session: &EndpointSession,
        table: &str,
        filter: Option<&SyncSourceFilter>,
        cancel: &CancelToken,
    ) -> Result<Box<dyn KeysetPageSource>, DataSyncError> {
        let slot = self.slot_of(session)?;
        let meta = self.relation_meta(slot.is_source, table)?;
        let adapter = self.adapter_for(&slot.db_type)?;
        let effective = match filter.cloned() {
            Some(filter) => Some(filter),
            None if slot.is_source => match self.filters.get(table).cloned() {
                Some(filter)
                    if !filter
                        .is_empty()
                        .map_err(|error| invalid(error.to_string()))? =>
                {
                    Some(filter)
                }
                _ => None,
            },
            None => None,
        };
        let recordset_limit = match effective.as_ref() {
            Some(filter) => filter
                .recordset_limit(&meta.schema_obj)
                .map_err(|error| invalid(error.to_string()))?,
            None => None,
        };
        let inner = DriverKeysetSource::new(
            session.driver.clone(),
            session.handle.clone(),
            meta.table.clone(),
            Some(meta.database.clone()),
            meta.schema.clone(),
            meta.columns.clone(),
            meta.pk_columns.clone(),
            meta.quote,
            &meta.family,
            adapter,
            meta.key_contracts.clone(),
            effective,
            meta.column_types.clone(),
            recordset_limit,
        )?;
        Ok(Box::new(DriverKeysetPageSource::new(inner, cancel.flag())))
    }

    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactRepositoryGuard, DataSyncError> {
        self.artifacts.store_artifact(artifact).await
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        self.artifacts.load_artifact(plan_id).await
    }

    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError> {
        self.artifacts.selection(plan_id, selection_revision).await
    }

    async fn check_permissions(&self, session: &EndpointSession) -> Result<(), DataSyncError> {
        let slot = self.slot_of(session)?;
        if !slot.is_source && slot.read_only {
            // Re-verified at the write gate: a target that turned read-only
            // after the plan was reviewed must not be written to.
            return Err(invalid(
                "target connection is now read-only; return to comparison",
            ));
        }
        Ok(())
    }

    async fn transaction_scope(
        &self,
        session: &EndpointSession,
    ) -> Result<TransactionScope, DataSyncError> {
        let slot = self.slot_of(session)?;
        Ok(transaction_scope_for(&slot.session.family))
    }

    async fn target_executor(
        &self,
        session: &EndpointSession,
        cancel: &CancelToken,
    ) -> Result<Box<dyn TargetExecutor>, DataSyncError> {
        let slot = self.slot_of(session)?;
        if slot.is_source {
            return Err(invalid("target executor requested for the source endpoint"));
        }
        let read_by_key_sql = lock(&self.meta)
            .iter()
            .filter(|((is_source, _), _)| !is_source)
            .map(|((_, table), meta)| (table.clone(), meta.select_by_key_sql.clone()))
            .collect::<HashMap<String, String>>();
        Ok(Box::new(DriverTargetExecutor::new(
            slot.session.driver.clone(),
            slot.session.handle.clone(),
            cancel.flag(),
            read_by_key_sql,
            self.commit_fault.clone(),
        )))
    }

    async fn generate_statements(
        &self,
        relation: &RelationIdentity,
        source_table: &str,
        target_table: &str,
        changes: Vec<RowChange>,
        options: &SyncOptions,
    ) -> Result<Vec<SqlStatement>, DataSyncError> {
        // 复核门禁（空批次/未复核行/关系匹配）在 api 内先于本宿主的任何
        // 查询执行；本实现只负责把租约、注册表与比较期缓存拼成上下文。
        generate_reviewed_batch_sql(self, relation, source_table, target_table, changes, options)
    }

    fn now(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    /// 阶段自身的判定只落日志：真正面向用户的失败记录由外层
    /// [`RecordingDataSyncHost`] 写入 [`FailureSink`]（它持有 Job id，而本
    /// 结构刻意不持有）。
    fn record_stage_failure(&self, stage: &str, reason: String) {
        tracing::warn!(
            stage,
            reason,
            "data-sync stage rejected before any port call"
        );
    }
}

/// F4c-1 端口实现：把比较期缓存（`mapped_table_schemas` 落的
/// `RelationMeta`）、适配器注册表与活动目标租约拼成装配上下文。
///
/// 查找顺序与下沉前的桌面实现一致：relation_meta → target_slot →
/// source/target 适配器；失败路径与文本不变。
impl BatchSqlHost for RuntimeDataSyncHost {
    fn write_sql_context(&self, target_table: &str) -> Result<BatchSqlContext, DataSyncError> {
        let meta = self.relation_meta(false, target_table)?;
        let slot = self.target_slot()?;
        let preview_source = self.sync_source_adapter(&meta.db_type)?;
        let preview_target = self.sync_target_adapter(&meta.db_type)?;
        Ok(BatchSqlContext {
            family: slot.session.family.clone(),
            database: meta.database.clone(),
            schema: meta.schema.clone(),
            schema_obj: meta.schema_obj.clone(),
            columns: meta.columns.clone(),
            column_types: meta.column_types.clone(),
            pk_columns: meta.pk_columns.clone(),
            preview_source,
            preview_target,
            target_driver: slot.session.driver.clone(),
        })
    }
}

/// Only families with a proven per-statement transaction boundary may be
/// written; anything else reports `NonAtomic` and the apply Job refuses to run
/// (§5.3: refuse when batch transactionality is unprovable).
pub fn transaction_scope_for(family: &str) -> TransactionScope {
    match family {
        "mysql" | "postgresql" | "sqlserver" => TransactionScope::Batch,
        _ => TransactionScope::NonAtomic,
    }
}

/// webdriver 提交故障注入钩子（桌面 e2e 构建专属；生产与服务端恒 None）。
pub trait CommitFaultHook: Send + Sync {
    /// 取走一次 armed 故障：0 = 无；1 = commit 后丢响应；2 = commit 前丢响应。
    fn take(&self) -> u8;
}

/// `DriverKeysetSource` adapted to the handler port. The port is foreign to
/// the host, so the existing driver reader is wrapped instead of rewritten.
pub struct DriverKeysetPageSource {
    inner: DriverKeysetSource,
    cancel: Arc<AtomicBool>,
}

impl DriverKeysetPageSource {
    pub fn new(inner: DriverKeysetSource, cancel: Arc<AtomicBool>) -> Self {
        Self { inner, cancel }
    }
}

#[async_trait]
impl KeysetPageSource for DriverKeysetPageSource {
    async fn next_page(
        &mut self,
        after_key: Option<&[datazen_driver_api::Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        // A long keyset walk has to stop between pages, so the page loop checks
        // the same bit the runtime flips (§5.3).
        if self.cancel.load(Ordering::SeqCst) {
            return Err(cancelled("compare cancelled before page"));
        }
        self.inner.next_page(after_key, limit).await
    }

    fn normalize_key(
        &self,
        key: &[datazen_driver_api::Value],
    ) -> Result<Vec<datazen_driver_api::SyncKeyValue>, DataSyncError> {
        self.inner.normalize_key(key)
    }
}

/// One fixed target lease for the whole apply Job: every batch begins,
/// verifies, writes and commits on this single session (§5.3).
pub struct DriverTargetExecutor {
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    handle: datazen_driver_api::ConnectionHandle,
    tx: Option<datazen_driver_api::TransactionHandle>,
    cancel: Arc<AtomicBool>,
    /// Precomputed `SELECT … WHERE pk IS NOT NULL` per table name.
    read_by_key_sql: HashMap<String, String>,
    /// webdriver 提交故障钩子（e2e 专属；`None` 时 commit 不受影响）。
    commit_fault: Option<Arc<dyn CommitFaultHook>>,
}

impl DriverTargetExecutor {
    pub fn new(
        driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
        handle: datazen_driver_api::ConnectionHandle,
        cancel: Arc<AtomicBool>,
        read_by_key_sql: HashMap<String, String>,
        commit_fault: Option<Arc<dyn CommitFaultHook>>,
    ) -> Self {
        Self {
            driver,
            handle,
            tx: None,
            cancel,
            read_by_key_sql,
            commit_fault,
        }
    }
}

/// A cancel observed before the commit point still rolls back only the
/// current batch: these checkpoints read the kernel's own bit, which the
/// per-stage cancel watcher flips mid-run (§5.3).
fn cancelled(what: &str) -> DataSyncError {
    DataSyncError::cancelled(format!("execute cancelled: {what}"))
}

#[async_trait]
impl TargetExecutor for DriverTargetExecutor {
    async fn begin(&mut self) -> Result<(), DataSyncError> {
        self.tx = Some(
            self.driver
                .begin_transaction(&self.handle)
                .await
                .map_err(|error| DataSyncError::outcome_unknown(format!("begin: {error}")))?,
        );
        Ok(())
    }

    async fn read_by_key(
        &mut self,
        relation: &RelationIdentity,
        key: &[datazen_driver_api::Value],
    ) -> Result<Option<Row>, DataSyncError> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(cancelled("batch cancelled before verify"));
        }
        let sql = self
            .read_by_key_sql
            .get(&relation.table)
            .ok_or_else(|| {
                DataSyncError::validation(
                    "target relation metadata missing; compare again to refresh the plan",
                )
            })?
            .clone();
        let result = self
            .driver
            .query_with_params(&self.handle, &sql, key)
            .await
            .map_err(|error| DataSyncError::outcome_unknown(format!("read_by_key: {error}")))?;
        Ok(result.rows.into_iter().next())
    }

    async fn execute(&mut self, statement: &SqlStatement) -> Result<u64, DataSyncError> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(cancelled("batch cancelled before write"));
        }
        if let Some(target) = statement.identity_insert.as_ref() {
            self.driver
                .set_identity_insert(
                    &self.handle,
                    &target.database,
                    target.schema.as_deref(),
                    &target.table,
                    true,
                )
                .await
                .map_err(|error| DataSyncError::validation(error.to_string()))?;
        }
        let affected = self
            .driver
            .execute_with_params(&self.handle, &statement.sql, &statement.parameters)
            .await
            .map_err(|error| DataSyncError::outcome_unknown(format!("execute: {error}")))?;
        if let Some(target) = statement.identity_insert.as_ref() {
            let _ = self
                .driver
                .set_identity_insert(
                    &self.handle,
                    &target.database,
                    target.schema.as_deref(),
                    &target.table,
                    false,
                )
                .await;
        }
        Ok(affected)
    }

    async fn commit(&mut self) -> Result<(), DataSyncError> {
        if let Some(tx) = self.tx.take() {
            match self
                .commit_fault
                .as_ref()
                .map(|hook| hook.take())
                .unwrap_or(0)
            {
                1 => {
                    self.driver.commit(tx).await.map_err(|error| {
                        DataSyncError::outcome_unknown(format!("commit: {error}"))
                    })?;
                    return Err(DataSyncError::outcome_unknown(
                        "E2E injected commit response loss after commit",
                    ));
                }
                2 => {
                    // Drop the rollback acknowledgement too, modelling a lost
                    // response at the transaction boundary. This branch exists
                    // only in the webdriver build and consumes one armed fault.
                    let _ = self.driver.rollback(tx).await;
                    return Err(DataSyncError::outcome_unknown(
                        "E2E injected transaction boundary response loss before commit",
                    ));
                }
                _ => {}
            }
            self.driver
                .commit(tx)
                .await
                .map_err(|error| DataSyncError::outcome_unknown(format!("commit: {error}")))?;
        }
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), DataSyncError> {
        if let Some(tx) = self.tx.take() {
            self.driver
                .rollback(tx)
                .await
                .map_err(|error| DataSyncError::validation(error.to_string()))?;
        }
        Ok(())
    }

    async fn discard(&mut self) -> Result<(), DataSyncError> {
        self.tx = None;
        self.driver
            .discard_connection(&self.handle)
            .await
            .map_err(|error| DataSyncError::validation(error.to_string()))
    }
}

/// 失败记录装饰器（桌面 `RecordingHost` 的下沉名）。
///
/// runtime 刻意不携带 handler 错误文本（§2.3：handler 从不改写 Job 状态），
/// `StageRecord` 也没有 outcome/error 字段——宿主是唯一知道 Job **为什么**
/// 失败的层。本装饰器是 [`FailureSink`] 的唯一生产写点，读回侧（桌面
/// IPC / 服务端 `get`·`details`）据此重建用户可见的失败消息，而不是
/// 现编一句。
pub struct RecordingDataSyncHost {
    inner: Arc<dyn DataSyncHost>,
    job_id: String,
    sink: Arc<dyn FailureSink>,
}

impl RecordingDataSyncHost {
    pub fn new(
        inner: Arc<dyn DataSyncHost>,
        job_id: impl Into<String>,
        sink: Arc<dyn FailureSink>,
    ) -> Self {
        Self {
            inner,
            job_id: job_id.into(),
            sink,
        }
    }

    /// Persist a host rejection, then hand the error back unchanged so the
    /// handler keeps its own failure classification (`NotStarted` vs
    /// `OutcomeUnknown` vs conflict).
    fn record<T>(&self, outcome: Result<T, DataSyncError>) -> Result<T, DataSyncError> {
        if let Err(error) = &outcome {
            self.sink.record(&self.job_id, error.to_string());
        }
        outcome
    }
}

#[async_trait]
impl DataSyncHost for RecordingDataSyncHost {
    async fn open_endpoint(&self, endpoint: &Endpoint) -> Result<EndpointSession, DataSyncError> {
        self.record(self.inner.open_endpoint(endpoint).await)
    }

    async fn close_endpoint(&self, session: EndpointSession) {
        self.inner.close_endpoint(session).await;
    }

    async fn mapped_table_schemas(
        &self,
        source: &EndpointSession,
        target: &EndpointSession,
        mappings: &[TableMapping],
    ) -> Result<Vec<TableSchemaPair>, DataSyncError> {
        self.record(
            self.inner
                .mapped_table_schemas(source, target, mappings)
                .await,
        )
    }

    async fn table_reader(
        &self,
        session: &EndpointSession,
        table: &str,
        filter: Option<&SyncSourceFilter>,
        cancel: &CancelToken,
    ) -> Result<Box<dyn KeysetPageSource>, DataSyncError> {
        self.record(
            self.inner
                .table_reader(session, table, filter, cancel)
                .await,
        )
    }

    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactRepositoryGuard, DataSyncError> {
        self.record(self.inner.store_artifact(artifact).await)
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        self.record(self.inner.load_artifact(plan_id).await)
    }

    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError> {
        self.record(self.inner.selection(plan_id, selection_revision).await)
    }

    async fn check_permissions(&self, session: &EndpointSession) -> Result<(), DataSyncError> {
        self.record(self.inner.check_permissions(session).await)
    }

    async fn transaction_scope(
        &self,
        session: &EndpointSession,
    ) -> Result<TransactionScope, DataSyncError> {
        self.record(self.inner.transaction_scope(session).await)
    }

    async fn target_executor(
        &self,
        session: &EndpointSession,
        cancel: &CancelToken,
    ) -> Result<Box<dyn TargetExecutor>, DataSyncError> {
        self.record(self.inner.target_executor(session, cancel).await)
    }

    async fn generate_statements(
        &self,
        relation: &RelationIdentity,
        source_table: &str,
        target_table: &str,
        changes: Vec<RowChange>,
        options: &SyncOptions,
    ) -> Result<Vec<SqlStatement>, DataSyncError> {
        self.record(
            self.inner
                .generate_statements(relation, source_table, target_table, changes, options)
                .await,
        )
    }

    fn now(&self) -> String {
        self.inner.now()
    }

    /// 阶段自身的拒绝（结构门闸、keyset 契约……）也必须落到同一个 Job 的失败
    /// 记录里，否则 runtime 只会交回一个没有任何原因的 `Failed`。
    fn record_stage_failure(&self, stage: &str, reason: String) {
        self.inner.record_stage_failure(stage, reason.clone());
        self.sink.record(&self.job_id, reason);
    }
}
