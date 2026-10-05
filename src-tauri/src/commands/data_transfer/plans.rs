//! Server-owned immutable Data Transfer plans.
//!
//! A preview is the authority for a later run. The client receives only the
//! opaque id and may choose a subset of already planned tables plus the final
//! destructive confirmation. Endpoints, mappings, DDL and schema snapshots
//! remain private to this registry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

mod checkpoint;
use checkpoint::retain_live_checkpoints;
pub(crate) use checkpoint::{TransferCheckpointSession, TransferResumeCheckpoint};

use datazen_driver_api::{iter_driver_factories, DatabaseDriver, TableSchema, PROTOCOL_VERSION};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::data_transfer::{
    DdlPreviewItem, TargetTableDependency, TransferError, TransferJob, TransferPreview,
};

pub(crate) const TRANSFER_PLAN_TTL: Duration = Duration::from_secs(15 * 60);
pub(crate) const TRANSFER_CHECKPOINT_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const TRANSFER_ACTIVE_LEASE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// `Executing` is an in-flight apply; `Consumed` is a terminal single-use
/// marker. A consumed plan is never claimable again, including after an unknown
/// commit outcome (§2.1/§8: one planId is consumable by exactly one apply Job).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanState {
    Available,
    Executing,
    Consumed,
}

/// Data needed to reproduce and validate the server-side execution plan.
/// This type is deliberately not serialized into the IPC response.
#[derive(Debug, Clone)]
pub(crate) struct StoredTransferPlan {
    pub(crate) id: String,
    pub(crate) job: TransferJob,
    pub(crate) can_execute: bool,
    pub(crate) source_driver_type: String,
    pub(crate) target_driver_type: String,
    pub(crate) source_driver_protocol: u32,
    pub(crate) target_driver_protocol: u32,
    pub(crate) source_schema_fingerprint: String,
    pub(crate) target_schema_fingerprint: String,
    /// Reserved for the filter contract. Until parameterized filters are
    /// supported by Transfer, all plans explicitly bind `None` here.
    pub(crate) filter: Option<String>,
    /// Fingerprint of the SQL-file dialect, output format and target namespace
    /// qualifiers.
    /// This keeps catalog/schema scope review-bound alongside the immutable
    /// structure sequence and prevents later renderer changes from silently
    /// changing the output namespace.
    pub(crate) target_scope_fingerprint: Option<String>,
    pub(crate) target_read_only_at_preview: bool,
    /// SQL-file structure statements captured at preview time. The execution
    /// path consumes this immutable sequence instead of re-rendering DDL from
    /// a second inspection, so object ordering and target mappings cannot
    /// drift between preview and publish.
    pub(crate) sql_file_structure: Option<Vec<DdlPreviewItem>>,
    /// Database-target structure statements captured by preview. The DB
    /// executor consumes this exact sequence so a changed source catalog or
    /// adapter cannot silently alter object mappings after review.
    pub(crate) database_structure: Option<Vec<DdlPreviewItem>>,
    /// Target foreign-key edges captured from the same schema snapshot used
    /// by the target-schema fingerprint. They are applied only to the user's
    /// final table selection immediately before execution.
    pub(crate) target_table_dependencies: Vec<TargetTableDependency>,
    /// Monotonic selection revision. The apply request must echo it, so a plan
    /// re-minted after re-review cannot be applied with a stale review.
    pub(crate) revision: u64,
    /// Wall-clock expiry in epoch millis, published to the client so an apply
    /// can be refused as expired without trusting the client clock.
    pub(crate) expires_at_millis: i64,
    expires_at: Instant,
    active_until: Option<Instant>,
    pub(crate) state: PlanState,
}

#[derive(Debug, Serialize)]
struct SchemaFingerprintEntry {
    relation: String,
    schema: Option<TableSchema>,
}

/// Deterministic, lossless fingerprint of the schemas used by one plan.
/// Missing target schemas are included as `null`, so a table appearing after
/// preview invalidates a create-new plan as well.
pub(crate) fn fingerprint_schemas(
    entries: impl IntoIterator<Item = (String, Option<TableSchema>)>,
) -> Result<String, TransferError> {
    let mut entries: Vec<SchemaFingerprintEntry> = entries
        .into_iter()
        .map(|(relation, schema)| SchemaFingerprintEntry { relation, schema })
        .collect();
    entries.sort_by(|a, b| a.relation.cmp(&b.relation));
    let bytes = serde_json::to_vec(&entries).map_err(|error| {
        TransferError::validation(format!("cannot fingerprint schema: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Fingerprint the complete structured source scope so the immutable plan
/// binds the reviewed filters and recordset bounds as well as the source
/// schemas.
pub(crate) fn filter_fingerprint(job: &TransferJob) -> Result<Option<String>, TransferError> {
    let filters: Vec<_> = participating_tables(job)
        .filter_map(|table| {
            if table.source_filter.is_none() && table.recordset.is_none() {
                return None;
            }
            Some((
                table.source_table.clone(),
                table.source_filter.as_ref(),
                table.recordset.as_ref(),
            ))
        })
        .collect();
    if filters.is_empty() {
        return Ok(None);
    }
    let bytes = serde_json::to_vec(&filters).map_err(|error| {
        TransferError::validation(format!("cannot fingerprint source filters: {error}"))
    })?;
    Ok(Some(format!("{:x}", Sha256::digest(bytes))))
}

pub(crate) fn target_scope_fingerprint(job: &TransferJob) -> Result<Option<String>, TransferError> {
    let Some(target) = job.sql_file_target.as_ref() else {
        return Ok(None);
    };
    target.validate_qualifiers()?;
    let scope = (
        target.normalized_database_type(),
        target.normalized_database(),
        target.normalized_schema(),
        target.normalized_encoding(),
        target.normalized_compression(),
    );
    let bytes = serde_json::to_vec(&scope).map_err(|error| {
        TransferError::validation(format!("cannot fingerprint SQL-file target scope: {error}"))
    })?;
    Ok(Some(format!("{:x}", Sha256::digest(bytes))))
}

/// Return the relations that are part of the immutable execution snapshot.
///
/// Disabled mappings are a UI choice that the preview deliberately does not
/// inspect or execute. Keeping this scope in one helper is important: both
/// plan issuance and the preflight revalidation must hash the same relation
/// set, otherwise an existing disabled target can make an unchanged plan
/// stale merely because it was readable during execution.
pub(crate) fn participating_tables(
    job: &TransferJob,
) -> impl Iterator<Item = &crate::data_transfer::model::TableMapping> {
    job.tables.iter().filter(|table| table.enabled)
}

pub(crate) fn driver_protocol_version(driver: &dyn DatabaseDriver) -> u32 {
    let driver_type = driver.driver_type();
    iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == driver_type)
        .map(|factory| factory.protocol_version())
        .unwrap_or(PROTOCOL_VERSION)
}

pub(crate) struct TransferPlanStore {
    plans: Mutex<HashMap<String, StoredTransferPlan>>,
    checkpoints: Mutex<HashMap<String, TransferResumeCheckpoint>>,
    /// Monotonic plan-revision source, so every issued plan is uniquely ordered.
    revision_seq: AtomicU64,
}

impl TransferPlanStore {
    pub(crate) fn new() -> Self {
        Self {
            plans: Mutex::new(HashMap::new()),
            checkpoints: Mutex::new(HashMap::new()),
            revision_seq: AtomicU64::new(0),
        }
    }

    pub(crate) fn issue(
        &self,
        job: TransferJob,
        preview: &TransferPreview,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        source_schemas: &HashMap<String, TableSchema>,
        target_schemas: &HashMap<String, TableSchema>,
        target_read_only: bool,
    ) -> Result<String, TransferError> {
        self.issue_with_ttl(
            job,
            preview,
            source_driver,
            target_driver,
            source_schemas,
            target_schemas,
            target_read_only,
            TRANSFER_PLAN_TTL,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn issue_with_ttl(
        &self,
        job: TransferJob,
        preview: &TransferPreview,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        source_schemas: &HashMap<String, TableSchema>,
        target_schemas: &HashMap<String, TableSchema>,
        target_read_only: bool,
        ttl: Duration,
    ) -> Result<String, TransferError> {
        self.issue_with_dependencies(
            job,
            preview,
            source_driver,
            target_driver,
            source_schemas,
            target_schemas,
            target_read_only,
            Vec::new(),
            ttl,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn issue_with_target_dependencies(
        &self,
        job: TransferJob,
        preview: &TransferPreview,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        source_schemas: &HashMap<String, TableSchema>,
        target_schemas: &HashMap<String, TableSchema>,
        target_read_only: bool,
        target_table_dependencies: Vec<TargetTableDependency>,
    ) -> Result<String, TransferError> {
        self.issue_with_dependencies(
            job,
            preview,
            source_driver,
            target_driver,
            source_schemas,
            target_schemas,
            target_read_only,
            target_table_dependencies,
            TRANSFER_PLAN_TTL,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn issue_with_dependencies(
        &self,
        job: TransferJob,
        preview: &TransferPreview,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        source_schemas: &HashMap<String, TableSchema>,
        target_schemas: &HashMap<String, TableSchema>,
        target_read_only: bool,
        target_table_dependencies: Vec<TargetTableDependency>,
        ttl: Duration,
    ) -> Result<String, TransferError> {
        let source_entries = participating_tables(&job).map(|table| {
            (
                table.source_table.clone(),
                source_schemas.get(&table.source_table).cloned(),
            )
        });
        let target_entries = participating_tables(&job).map(|table| {
            (
                table.target_table.clone(),
                target_schemas.get(&table.target_table).cloned(),
            )
        });
        let source_schema_fingerprint = fingerprint_schemas(source_entries)?;
        let target_schema_fingerprint = fingerprint_schemas(target_entries)?;
        let filter = filter_fingerprint(&job)?;
        let target_scope_fingerprint = target_scope_fingerprint(&job)?;
        let sql_file_structure = job
            .sql_file_target
            .as_ref()
            .filter(|_| !preview.ddl.is_empty())
            .map(|_| preview.ddl.clone());
        let database_structure = job
            .target
            .as_ref()
            .filter(|_| !preview.ddl.is_empty())
            .map(|_| preview.ddl.clone());
        let id = Uuid::new_v4().to_string();
        let plan = StoredTransferPlan {
            id: id.clone(),
            job,
            can_execute: preview.can_execute,
            source_driver_type: source_driver.driver_type(),
            target_driver_type: target_driver.driver_type(),
            source_driver_protocol: driver_protocol_version(source_driver),
            target_driver_protocol: driver_protocol_version(target_driver),
            source_schema_fingerprint,
            target_schema_fingerprint,
            filter,
            target_scope_fingerprint,
            target_read_only_at_preview: target_read_only,
            sql_file_structure,
            database_structure,
            target_table_dependencies,
            revision: self
                .revision_seq
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1),
            expires_at_millis: now_millis().saturating_add(
                i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX),
            ),
            expires_at: Instant::now() + ttl,
            active_until: None,
            state: PlanState::Available,
        };
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let now = Instant::now();
        plans.retain(|_, existing| {
            existing.expires_at > now || existing.active_until.is_some_and(|until| until > now)
        });
        retain_live_checkpoints(&mut checkpoints, &plans, now);
        plans.insert(id.clone(), plan);
        Ok(id)
    }

    pub(crate) fn peek(&self, id: &str) -> Result<StoredTransferPlan, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get(id) else {
            return Err(TransferError::validation(
                "transfer plan is unknown or has expired; return to preview",
            ));
        };
        if plan.expires_at <= Instant::now() {
            plans.remove(id);
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        if plan.state != PlanState::Available {
            return Err(TransferError::validation(
                "transfer plan was already consumed; return to preview",
            ));
        }
        Ok(plan.clone())
    }

    /// Read a plan without requiring it to be claimable. The apply Job uses this
    /// to report *why* a plan cannot run (claimed, consumed, expired) instead of
    /// a generic refusal, while `peek` keeps its Available-only semantics.
    pub(crate) fn peek_any(&self, id: &str) -> Result<StoredTransferPlan, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get(id) else {
            return Err(TransferError::validation(
                "transfer plan is unknown or has expired; return to preview",
            ));
        };
        if plan.expires_at <= Instant::now() {
            plans.remove(id);
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        Ok(plan.clone())
    }

    /// Mark a plan consumed after its single apply Job reached a terminal success.
    /// A failed apply leaves the lease in place so recovery can resume it under
    /// the same planId instead of minting a second apply.
    pub(crate) fn mark_consumed(&self, id: &str) -> Result<(), TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get_mut(id) else {
            return Ok(());
        };
        plan.state = PlanState::Consumed;
        plan.active_until = None;
        Ok(())
    }

    /// Atomically consume a plan. A consumed plan is never claimable again,
    /// including after an unknown commit/rollback outcome.
    pub(crate) fn claim(&self, id: &str) -> Result<StoredTransferPlan, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get_mut(id) else {
            return Err(TransferError::validation(
                "transfer plan is unknown or has expired; return to preview",
            ));
        };
        if plan.expires_at <= Instant::now() {
            plans.remove(id);
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        if plan.state != PlanState::Available {
            return Err(TransferError::validation(
                "transfer plan was already consumed; return to preview",
            ));
        }
        plan.state = PlanState::Executing;
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        Ok(plan.clone())
    }
}

impl Default for TransferPlanStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Wall-clock millis used for plan expiry publication. Uses `unwrap_or` only:
/// a clock before the epoch cannot panic the migration path.
pub(crate) fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn global_store() -> &'static TransferPlanStore {
    static STORE: OnceLock<TransferPlanStore> = OnceLock::new();
    STORE.get_or_init(TransferPlanStore::new)
}

pub(crate) fn issue_plan(
    job: TransferJob,
    preview: &TransferPreview,
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    source_schemas: &HashMap<String, TableSchema>,
    target_schemas: &HashMap<String, TableSchema>,
    target_read_only: bool,
) -> Result<String, TransferError> {
    global_store().issue(
        job,
        preview,
        source_driver,
        target_driver,
        source_schemas,
        target_schemas,
        target_read_only,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn issue_plan_with_target_dependencies(
    job: TransferJob,
    preview: &TransferPreview,
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    source_schemas: &HashMap<String, TableSchema>,
    target_schemas: &HashMap<String, TableSchema>,
    target_read_only: bool,
    target_table_dependencies: Vec<TargetTableDependency>,
) -> Result<String, TransferError> {
    global_store().issue_with_target_dependencies(
        job,
        preview,
        source_driver,
        target_driver,
        source_schemas,
        target_schemas,
        target_read_only,
        target_table_dependencies,
    )
}

pub(crate) fn peek_plan(id: &str) -> Result<StoredTransferPlan, TransferError> {
    global_store().peek(id)
}

pub(crate) fn claim_plan(id: &str) -> Result<StoredTransferPlan, TransferError> {
    global_store().claim(id)
}

/// Read a plan regardless of its claim state; used by the apply Job admission.
pub(crate) fn peek_plan_any(id: &str) -> Result<StoredTransferPlan, TransferError> {
    global_store().peek_any(id)
}

/// Terminal single-use marker, written only after a successful apply Job.
pub(crate) fn mark_plan_consumed(id: &str) -> Result<(), TransferError> {
    global_store().mark_consumed(id)
}

pub(crate) fn source_boundary_fingerprint(
    plan: &StoredTransferPlan,
) -> Result<String, TransferError> {
    let bytes = serde_json::to_vec(&(plan.source_schema_fingerprint.as_str(), &plan.filter))
        .map_err(|error| {
            TransferError::validation(format!("cannot fingerprint source boundary: {error}"))
        })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn target_boundary_fingerprint(
    plan: &StoredTransferPlan,
) -> Result<String, TransferError> {
    let bytes = serde_json::to_vec(&(
        plan.target_schema_fingerprint.as_str(),
        &plan.target_scope_fingerprint,
    ))
    .map_err(|error| {
        TransferError::validation(format!("cannot fingerprint target boundary: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn peek_checkpoint(
    token: &str,
    plan_id: &str,
) -> Result<(StoredTransferPlan, TransferResumeCheckpoint), TransferError> {
    global_store().peek_checkpoint(token, plan_id)
}

pub(crate) fn claim_checkpoint(
    token: &str,
    plan_id: &str,
) -> Result<(StoredTransferPlan, TransferResumeCheckpoint), TransferError> {
    global_store().claim_checkpoint(token, plan_id)
}

pub(crate) fn create_checkpoint(
    plan_id: &str,
    selected_tables: Vec<String>,
    completed_tables: Vec<String>,
) -> Result<String, TransferError> {
    global_store().create_checkpoint(plan_id, selected_tables, completed_tables)
}

pub(crate) fn update_checkpoint(
    token: &str,
    completed_tables: Vec<String>,
    finished: bool,
) -> Result<(), TransferError> {
    global_store().update_checkpoint(token, completed_tables, finished)
}

pub(crate) fn invalidate_checkpoint(token: &str) {
    global_store().invalidate_checkpoint(token)
}

#[cfg(test)]
mod tests;
