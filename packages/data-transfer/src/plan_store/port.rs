//! The Data Transfer plan storage port.
//!
//! ## What stays in this package and what becomes a port
//!
//! The *rules* that make a transfer plan safe stay here: pairing validation,
//! schema/filter/target-scope fingerprints, single-use consumption, the resume
//! checkpoint contract and the commit boundary. Only the **storage** is a port:
//! the host creates an instance at assembly time and injects it, instead of the
//! package reaching for a process-wide global.
//!
//! The in-memory [`HashMap`](std::collections::HashMap) implementation also stays
//! in this package ([`super::InMemoryTransferPlanStore`]). A host that wants a
//! durable plan registry only has to provide another implementation of this
//! trait; it never has to move the rules.
//!
//! ## Organization scope
//!
//! Every method takes an [`OrganizationScope`]. A scope that does not own a
//! record is answered as **"not found"** — never as "forbidden" (CM-05): a
//! cross-organization caller must not be able to tell "exists but not yours"
//! from "does not exist", because that difference is itself a leak.
//!
//! ## Why the trait is synchronous
//!
//! Claiming a plan is a single atomic state transition
//! (`Available → Executing`), and a checkpoint token is claimed exactly once.
//! Keeping the port synchronous makes that transition one non-interruptible
//! step: an `async` signature would allow an unrelated `await` to sit between
//! the check and the write.
//!
//! ## Why the trait is **not** split into a plan half and a checkpoint half
//!
//! The checkpoint side and the plan side are **one aggregate**, and the split
//! would break the single-writer atomicity they share. Every checkpoint
//! operation also rewrites the *plan's* state in the same call:
//! `create_checkpoint` mints a token **and** extends the plan's lease,
//! `prepare_checkpoint_table` / `advance_checkpoint_table` renew the plan's
//! lease **and** move the checkpoint's cursor, `claim_checkpoint` flips the
//! plan to `Executing` **and** takes the token in flight,
//! `update_checkpoint(finished)` consumes the plan **and** retires the token,
//! and `renew_execution` extends both. An implementation therefore has to
//! serialize those writes against each other, no matter how the trait is cut
//! up — splitting it would only move the boundary that the atomicity depends
//! on, and would let a future implementor satisfy one half while the other
//! half quietly drops the plan-state half of the write.
//!
//! (The "split by aggregate, do not merge traits" rule of platform-api §4.2
//! governs the *platform-api* port family — `#[async_trait]`, `PortError`,
//! platform ID newtypes. This is a **domain package** port: synchronous,
//! `TransferError`, opaque `String` ids. It is outside that family, and its
//! methods all belong to the same aggregate root anyway.)

use std::collections::HashMap;
use std::time::Duration;

use datazen_driver_api::{DatabaseDriver, TableSchema, Value};
use datazen_platform_api::ports::scope::OrganizationScope;

use crate::model::{TransferJob, TransferPreview};
use crate::resume::ResumeTableProgress;
use crate::{TargetTableDependency, TransferError};

use super::{StoredTransferPlan, TransferResumeCheckpoint};

/// Everything one plan issuance needs.
///
/// Bundled into one struct so the port signature does not grow a positional
/// argument per rule: the issuance rules (which schemas participate, how long
/// the plan lives) are package facts, not transport details.
pub struct PlanIssueRequest<'a> {
    /// The reviewed job. Its body is stored verbatim; the client never gets to
    /// replace it later.
    pub job: TransferJob,
    /// The preview this plan is minted from.
    pub preview: &'a TransferPreview,
    /// The source driver, captured by type and protocol version.
    pub source_driver: &'a dyn DatabaseDriver,
    /// The target driver, captured by type and protocol version.
    pub target_driver: &'a dyn DatabaseDriver,
    /// Source schemas participating in the plan.
    pub source_schemas: &'a HashMap<String, TableSchema>,
    /// Target schemas participating in the plan.
    pub target_schemas: &'a HashMap<String, TableSchema>,
    /// Whether the target was read-only at preview time.
    pub target_read_only: bool,
    /// Target foreign-key edges captured from the same schema snapshot.
    pub target_table_dependencies: Vec<TargetTableDependency>,
    /// How long the plan stays claimable.
    pub ttl: Duration,
}

impl<'a> PlanIssueRequest<'a> {
    /// The default issuance: the package plan TTL, no target dependencies.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        job: TransferJob,
        preview: &'a TransferPreview,
        source_driver: &'a dyn DatabaseDriver,
        target_driver: &'a dyn DatabaseDriver,
        source_schemas: &'a HashMap<String, TableSchema>,
        target_schemas: &'a HashMap<String, TableSchema>,
        target_read_only: bool,
    ) -> Self {
        Self {
            job,
            preview,
            source_driver,
            target_driver,
            source_schemas,
            target_schemas,
            target_read_only,
            target_table_dependencies: Vec::new(),
            ttl: super::TRANSFER_PLAN_TTL,
        }
    }
}

/// Storage port for immutable transfer plans and their resume checkpoints.
///
/// Implementations must be safe to share: the host injects one instance per
/// process and every apply Job races against the others for the same plan.
pub trait TransferPlanStorePort: Send + Sync + 'static {
    /// Mint one immutable plan owned by `scope` and return its opaque id.
    fn issue(
        &self,
        scope: &OrganizationScope,
        request: PlanIssueRequest<'_>,
    ) -> Result<String, TransferError>;

    /// Read a plan that may still be claimed.
    ///
    /// A plan that was already claimed or consumed is refused here; use
    /// [`Self::peek_any`] when the caller needs to report *why* it cannot run.
    fn peek(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<StoredTransferPlan, TransferError>;

    /// Read a plan regardless of its claim state.
    fn peek_any(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<StoredTransferPlan, TransferError>;

    /// Atomically consume a plan: `Available → Executing`.
    ///
    /// Exactly one caller wins; a consumed plan is never claimable again,
    /// including after an unknown commit outcome.
    fn claim(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<StoredTransferPlan, TransferError>;

    /// Write the terminal single-use marker after a successful apply Job.
    fn mark_consumed(&self, scope: &OrganizationScope, plan_id: &str) -> Result<(), TransferError>;

    /// Create a resume checkpoint for a claimed plan.
    fn create_checkpoint(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
    ) -> Result<String, TransferError>;

    /// Read a checkpoint that is available for claiming.
    fn peek_checkpoint(
        &self,
        scope: &OrganizationScope,
        token: &str,
        plan_id: &str,
    ) -> Result<(StoredTransferPlan, TransferResumeCheckpoint), TransferError>;

    /// Atomically claim a checkpoint token. Exactly one caller wins.
    fn claim_checkpoint(
        &self,
        scope: &OrganizationScope,
        token: &str,
        plan_id: &str,
    ) -> Result<(StoredTransferPlan, TransferResumeCheckpoint), TransferError>;

    /// Release an in-flight checkpoint back to the available state, or finish
    /// it (`finished = true`), which consumes it.
    fn update_checkpoint(
        &self,
        scope: &OrganizationScope,
        token: &str,
        completed_tables: Vec<String>,
        finished: bool,
    ) -> Result<(), TransferError>;

    /// Drop a checkpoint token: an unknown commit/rollback outcome is never
    /// resumable.
    fn invalidate_checkpoint(&self, scope: &OrganizationScope, token: &str);

    /// Renew the execution lease of a claimed plan (and of its checkpoint).
    fn renew_execution(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        token: Option<&str>,
    ) -> Result<(), TransferError>;

    /// Close out a plan after its execution finished.
    fn finish_execution(&self, scope: &OrganizationScope, plan_id: &str, keep_checkpoint: bool);

    /// Bind the progress of one table into an in-flight checkpoint.
    ///
    /// A table whose source digest, mapping, key order or chunk size changed
    /// since the checkpoint is refused: the checkpoint may not be reused.
    fn prepare_checkpoint_table(
        &self,
        scope: &OrganizationScope,
        token: &str,
        expected: ResumeTableProgress,
    ) -> Result<ResumeTableProgress, TransferError>;

    /// Advance the committed cursor of one table.
    fn advance_checkpoint_table(
        &self,
        scope: &OrganizationScope,
        token: &str,
        source_table: &str,
        cursor: Vec<Value>,
        rows_seen: u64,
    ) -> Result<(), TransferError>;

    /// Whether a checkpoint already carries progress for a table.
    fn checkpoint_has_table_progress(
        &self,
        scope: &OrganizationScope,
        token: &str,
        source_table: &str,
    ) -> bool;

    /// Whether a checkpoint table already committed chunks.
    fn checkpoint_table_has_committed_chunks(
        &self,
        scope: &OrganizationScope,
        token: &str,
        source_table: &str,
    ) -> bool;

    /// Create a checkpoint that already carries the first chunk's progress.
    fn start_chunk_checkpoint(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
        progress: ResumeTableProgress,
    ) -> Result<String, TransferError>;
}
