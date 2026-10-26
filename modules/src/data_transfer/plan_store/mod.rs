//! Server-owned immutable Data Transfer plans.
//!
//! A preview is the authority for a later run. The client receives only the
//! opaque id and may choose a subset of already planned tables plus the final
//! destructive confirmation. Endpoints, mappings, DDL and schema snapshots
//! remain private to this registry.
//!
//! ## Storage is a port, not a global
//!
//! The registry itself is a port ([`TransferPlanRepositoryPort`]): the host creates
//! one instance during assembly and injects it. The **plan store** has no
//! process-wide global any more — a plan lives in the instance the host handed
//! to the call site, so the storage lifetime is the host's decision rather
//! than the package's. (Other parts of the package keep their own process-wide
//! registries, e.g. the SQL file path table in `crate::data_transfer::sql_file`.)
//!
//! The rules (pairing, fingerprints, single-use consumption, resume semantics,
//! commit boundaries) stay in this package; only the storage is a port. The
//! in-memory implementation stays here too ([`InMemoryTransferPlanRepository`]).

use std::time::{Duration, Instant};

mod checkpoint;
mod memory;
mod port;

#[cfg(test)]
mod tests;

pub use checkpoint::{TransferCheckpointSession, TransferResumeCheckpoint};
pub use memory::InMemoryTransferPlanRepository;
pub use port::{PlanIssueRequest, TransferPlanRepositoryPort};

use crate::shared::id::OrganizationId;
use datazen_driver_api::TableSchema;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::data_transfer::{DdlPreviewItem, TargetTableDependency, TransferError, TransferJob};

pub const TRANSFER_PLAN_TTL: Duration = Duration::from_secs(15 * 60);
pub const TRANSFER_CHECKPOINT_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const TRANSFER_ACTIVE_LEASE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// `Executing` is an in-flight apply; `Consumed` is a terminal single-use
/// marker. A consumed plan is never claimable again, including after an unknown
/// commit outcome (§2.1/§8: one planId is consumable by exactly one apply Job).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    Available,
    Executing,
    Consumed,
}

/// Data needed to reproduce and validate the server-side execution plan.
/// This type is deliberately not serialized into the IPC response.
#[derive(Debug, Clone)]
pub struct StoredTransferPlan {
    pub id: String,
    /// The organization that owns this plan. A scope that does not match it
    /// sees the plan as non-existent (CM-05).
    pub organization_id: OrganizationId,
    pub job: TransferJob,
    pub can_execute: bool,
    pub source_driver_type: String,
    pub target_driver_type: String,
    pub source_driver_protocol: u32,
    pub target_driver_protocol: u32,
    pub source_schema_fingerprint: String,
    pub target_schema_fingerprint: String,
    /// Reserved for the filter contract. Until parameterized filters are
    /// supported by Transfer, all plans explicitly bind `None` here.
    pub filter: Option<String>,
    /// Fingerprint of the SQL-file dialect, output format and target namespace
    /// qualifiers.
    /// This keeps catalog/schema scope review-bound alongside the immutable
    /// structure sequence and prevents later renderer changes from silently
    /// changing the output namespace.
    pub target_scope_fingerprint: Option<String>,
    pub target_read_only_at_preview: bool,
    /// SQL-file structure statements captured at preview time. The execution
    /// path consumes this immutable sequence instead of re-rendering DDL from
    /// a second inspection, so object ordering and target mappings cannot
    /// drift between preview and publish.
    pub sql_file_structure: Option<Vec<DdlPreviewItem>>,
    /// Database-target structure statements captured by preview. The DB
    /// executor consumes this exact sequence so a changed source catalog or
    /// adapter cannot silently alter object mappings after review.
    pub database_structure: Option<Vec<DdlPreviewItem>>,
    /// Target foreign-key edges captured from the same schema snapshot used
    /// by the target-schema fingerprint. They are applied only to the user's
    /// final table selection immediately before execution.
    pub target_table_dependencies: Vec<TargetTableDependency>,
    /// Monotonic selection revision. The apply request must echo it, so a plan
    /// re-minted after re-review cannot be applied with a stale review.
    pub revision: u64,
    /// Wall-clock expiry in epoch millis, published to the client so an apply
    /// can be refused as expired without trusting the client clock.
    pub expires_at_millis: i64,
    expires_at: Instant,
    active_until: Option<Instant>,
    pub state: PlanState,
}

#[derive(Debug, Serialize)]
struct SchemaFingerprintEntry {
    relation: String,
    schema: Option<TableSchema>,
}

/// Deterministic, lossless fingerprint of the schemas used by one plan.
/// Missing target schemas are included as `null`, so a table appearing after
/// preview invalidates a create-new plan as well.
pub fn fingerprint_schemas(
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
pub fn filter_fingerprint(job: &TransferJob) -> Result<Option<String>, TransferError> {
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

pub fn target_scope_fingerprint(job: &TransferJob) -> Result<Option<String>, TransferError> {
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
pub fn participating_tables(
    job: &TransferJob,
) -> impl Iterator<Item = &crate::data_transfer::model::TableMapping> {
    job.tables.iter().filter(|table| table.enabled)
}

/// Wall-clock millis used for plan expiry publication. Uses `unwrap_or` only:
/// a clock before the epoch cannot panic the migration path.
pub fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn source_boundary_fingerprint(plan: &StoredTransferPlan) -> Result<String, TransferError> {
    let bytes = serde_json::to_vec(&(plan.source_schema_fingerprint.as_str(), &plan.filter))
        .map_err(|error| {
            TransferError::validation(format!("cannot fingerprint source boundary: {error}"))
        })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn target_boundary_fingerprint(plan: &StoredTransferPlan) -> Result<String, TransferError> {
    let bytes = serde_json::to_vec(&(
        plan.target_schema_fingerprint.as_str(),
        &plan.target_scope_fingerprint,
    ))
    .map_err(|error| {
        TransferError::validation(format!("cannot fingerprint target boundary: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
