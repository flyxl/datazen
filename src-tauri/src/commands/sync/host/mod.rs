//! Desktop implementation of the handler's host port.
//!
//! One rule shapes the whole file: a Data Sync Job talks to drivers through
//! exactly one `ConnectionManager` for its whole lifetime (§9), and the apply
//! Job keeps exactly one target lease for every batch (§5.3). Everything else
//! — relation metadata — is bookkeeping around those two leases. Durable Job
//! admission owns the physical-endpoint concurrency budget. The cancel bit is
//! not bookkeeping at all: it is not held here, only handed straight through
//! from the stage's own `CancelToken`.

mod executor;
pub(crate) mod recording;
pub(super) mod select;
pub(crate) mod selection;
pub(crate) mod state;
mod statements;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use datazen_driver_api::{
    DatabaseType, SyncKeyContract, SyncSourceAdapter, SyncTargetAdapter, TableSchema,
};
use datazen_runtime::job::CancelToken;

use crate::data_sync::job::artifact::{ChangeSetArtifact, RelationIdentity};
use crate::data_sync::job::host::{
    ArtifactStoreGuard, DataSyncHost, EndpointSession, KeysetPageSource, TableSchemaPair,
    TargetExecutor, TransactionScope,
};
use crate::data_sync::pairing::family_of;
use crate::data_sync::{
    require_data_sync_family, DataSyncError, Endpoint, RowChange, SqlStatement, SyncOptions,
    SyncSourceFilter, TableMapping,
};
use crate::services::{metadata_schema, ConnectionManager};
use crate::transfer::adapter_registry::SyncAdapterRegistry;

use super::super::{resolve_key_contracts, validate_filter_endpoints, validate_filter_schemas};
use super::keyset_source::DriverKeysetSource;
use super::types::resolve_db_name;
use executor::{HostKeysetSource, HostTargetExecutor};

/// Everything the handler needs to read a relation, cached at
/// `mapped_table_schemas` time so no port call has to re-derive SQL.
pub(crate) struct RelationMeta {
    pub(crate) is_source: bool,
    pub(crate) db_type: DatabaseType,
    pub(crate) family: String,
    pub(crate) database: String,
    pub(crate) schema: Option<String>,
    pub(crate) table: String,
    pub(crate) quote: char,
    pub(crate) columns: Vec<String>,
    pub(crate) column_types: HashMap<String, String>,
    pub(crate) pk_columns: Vec<String>,
    pub(crate) key_contracts: Vec<SyncKeyContract>,
    pub(crate) recordset_limit: Option<u64>,
    pub(crate) select_by_key_sql: String,
    pub(crate) schema_obj: TableSchema,
}

/// One opened endpoint lease owned by the durable Job worker.
#[derive(Clone)]
struct Slot {
    is_source: bool,
    session: EndpointSession,
    db_type: DatabaseType,
    read_only: bool,
}

pub(crate) struct HostDataSync {
    connections: Arc<ConnectionManager>,
    adapters: Arc<SyncAdapterRegistry>,
    source: Endpoint,
    target: Endpoint,
    filters: HashMap<String, SyncSourceFilter>,
    slots: Mutex<Vec<Slot>>,
    meta: Mutex<HashMap<(bool, String), RelationMeta>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn invalid(message: impl Into<String>) -> DataSyncError {
    DataSyncError::validation(message.into())
}

impl HostDataSync {
    pub(crate) fn new(
        connections: Arc<ConnectionManager>,
        adapters: Arc<SyncAdapterRegistry>,
        source: Endpoint,
        target: Endpoint,
        filters: HashMap<String, SyncSourceFilter>,
    ) -> Self {
        Self {
            connections,
            adapters,
            source,
            target,
            filters,
            slots: Mutex::new(Vec::new()),
            meta: Mutex::new(HashMap::new()),
        }
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
            endpoint.connection_id
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
    ) -> Result<Arc<dyn SyncSourceAdapter>, DataSyncError> {
        self.adapters
            .get_source(db_type)
            .ok_or_else(|| invalid(format!("no Data Sync key adapter for {db_type}")))
    }

    /// Literal renderer / relation qualifier for the live target dialect.
    pub(crate) fn sync_target_adapter(
        &self,
        db_type: &DatabaseType,
    ) -> Result<Arc<dyn SyncTargetAdapter>, DataSyncError> {
        self.adapters
            .get_target(db_type)
            .ok_or_else(|| invalid(format!("no Data Sync SQL adapter for {db_type}")))
    }

    pub(crate) fn sync_source_adapter(
        &self,
        db_type: &DatabaseType,
    ) -> Result<Arc<dyn SyncSourceAdapter>, DataSyncError> {
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
        if self
            .connections
            .get_session(&endpoint.connection_id)
            .await
            .is_err()
        {
            return Err(invalid(format!(
                "cannot open source/target: database session {} is no longer available; reconnect and compare again",
                endpoint.connection_id
            )));
        }
        self.open_locked(&endpoint.connection_id, endpoint, is_source)
            .await
    }

    async fn open_locked(
        &self,
        db_session_id: &str,
        endpoint: &Endpoint,
        is_source: bool,
    ) -> Result<EndpointSession, DataSyncError> {
        let config = self
            .connections
            .get_session_config(db_session_id)
            .await
            .map_err(|error| invalid(format!("cannot read connection config: {error}")))?;
        let (driver, handle) = self
            .connections
            .get_session(db_session_id)
            .await
            .map_err(|error| invalid(format!("cannot open database session: {error}")))?;
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
        if !is_source && config.read_only {
            return Err(invalid(
                "target connection is read-only; return to comparison",
            ));
        }
        let database = resolve_db_name(Some(&endpoint.database), config.database.as_deref());
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
        });
        Ok(session)
    }
}

#[async_trait]
impl DataSyncHost for HostDataSync {
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
                        select_by_key_sql: select::select_by_key_sql(
                            slot.session.driver.as_ref(),
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
        Ok(Box::new(HostKeysetSource::new(inner, cancel.flag())))
    }

    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactStoreGuard, DataSyncError> {
        selection::store_artifact(artifact)
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        Ok(state::load_artifact(plan_id))
    }

    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<crate::data_sync::job::ChangeBlock>, DataSyncError> {
        selection::selection(plan_id, selection_revision)
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
        Ok(scope_for(&slot.session.family))
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
        Ok(Box::new(HostTargetExecutor::new(
            slot.session.driver.clone(),
            slot.session.handle.clone(),
            cancel.flag(),
            read_by_key_sql,
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
        statements::generate(self, relation, source_table, target_table, changes, options)
    }

    fn now(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    /// 阶段自身的判定只落日志：真正面向用户的失败记录由外层 `RecordingHost`
    /// 写入 `state::FAILURES`（它持有 Job id，而本结构刻意不持有）。
    fn record_stage_failure(&self, stage: &str, reason: String) {
        tracing::warn!(
            stage,
            reason,
            "data-sync stage rejected before any port call"
        );
    }
}

/// Only families with a proven per-statement transaction boundary may be
/// written; anything else reports `NonAtomic` and the apply Job refuses to run
/// (§5.3: refuse when batch transactionality is unprovable).
pub(crate) fn scope_for(family: &str) -> TransactionScope {
    match family {
        "mysql" | "postgresql" | "sqlserver" => TransactionScope::Batch,
        _ => TransactionScope::NonAtomic,
    }
}
