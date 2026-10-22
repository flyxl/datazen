//! The in-memory implementation of the plan storage port.
//!
//! The `HashMap` implementation **stays in this package**: the host injects an
//! instance of it, it does not reimplement it. A different backing store only
//! has to implement [`TransferPlanRepositoryPort`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use datazen_driver_api::driver_protocol_version;
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;
use uuid::Uuid;

use crate::resume::ResumeTableProgress;
use crate::TransferError;

use super::checkpoint::{retain_live_checkpoints, unknown_token, ResumeCheckpointState};
use super::port::{PlanIssueRequest, TransferPlanRepositoryPort};
use super::{
    filter_fingerprint, fingerprint_schemas, source_boundary_fingerprint,
    target_boundary_fingerprint, target_scope_fingerprint, PlanState, StoredTransferPlan,
    TransferResumeCheckpoint, TRANSFER_ACTIVE_LEASE, TRANSFER_CHECKPOINT_TTL, TRANSFER_PLAN_TTL,
};

/// The refusal every "not found" path uses.
///
/// Cross-organization access must be **indistinguishable** from an unknown id
/// (CM-05): a caller must not learn that a plan exists in another
/// organization. Therefore the scope check and the missing-id check return the
/// very same message.
pub(super) fn unknown_plan() -> TransferError {
    TransferError::validation("transfer plan is unknown or has expired; return to preview")
}

/// Process-local plan registry.
///
/// One instance per host process, created during assembly and injected as
/// `Arc<dyn TransferPlanRepositoryPort>`.
pub struct InMemoryTransferPlanRepository {
    pub(super) plans: Mutex<HashMap<String, StoredTransferPlan>>,
    pub(super) checkpoints: Mutex<HashMap<String, super::TransferResumeCheckpoint>>,
    /// Monotonic plan-revision source, so every issued plan is uniquely ordered.
    revision_seq: AtomicU64,
}

impl InMemoryTransferPlanRepository {
    pub fn new() -> Self {
        Self {
            plans: Mutex::new(HashMap::new()),
            checkpoints: Mutex::new(HashMap::new()),
            revision_seq: AtomicU64::new(0),
        }
    }

    /// True when `scope` owns the given record.
    ///
    /// The organization id is the **only** dimension: a plan issued by another
    /// organization is simply not there for this caller.
    pub(super) fn owns(scope: &OrganizationScope, owner: &OrganizationId) -> bool {
        scope.organization_id() == owner
    }
}

impl Default for InMemoryTransferPlanRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferPlanRepositoryPort for InMemoryTransferPlanRepository {
    fn issue(
        &self,
        scope: &OrganizationScope,
        request: PlanIssueRequest<'_>,
    ) -> Result<String, TransferError> {
        let PlanIssueRequest {
            job,
            preview,
            source_driver,
            target_driver,
            source_schemas,
            target_schemas,
            target_read_only,
            target_table_dependencies,
            ttl,
        } = request;
        let source_entries = super::participating_tables(&job).map(|table| {
            (
                table.source_table.clone(),
                source_schemas.get(&table.source_table).cloned(),
            )
        });
        let target_entries = super::participating_tables(&job).map(|table| {
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
            organization_id: scope.organization_id().clone(),
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
            expires_at_millis: super::now_millis()
                .saturating_add(i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX)),
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

    fn peek(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<StoredTransferPlan, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get(id) else {
            return Err(unknown_plan());
        };
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
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

    fn peek_any(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<StoredTransferPlan, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get(id) else {
            return Err(unknown_plan());
        };
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
        if plan.expires_at <= Instant::now() {
            plans.remove(id);
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        Ok(plan.clone())
    }

    fn claim(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<StoredTransferPlan, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get_mut(id) else {
            return Err(unknown_plan());
        };
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
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
        plan.active_until = Some(Instant::now() + super::TRANSFER_ACTIVE_LEASE);
        Ok(plan.clone())
    }

    fn mark_consumed(&self, scope: &OrganizationScope, id: &str) -> Result<(), TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let Some(plan) = plans.get_mut(id) else {
            return Ok(());
        };
        if !Self::owns(scope, &plan.organization_id) {
            return Ok(());
        }
        plan.state = PlanState::Consumed;
        plan.active_until = None;
        Ok(())
    }
    fn create_checkpoint(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
    ) -> Result<String, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        retain_live_checkpoints(&mut checkpoints, &plans, Instant::now());
        let plan = plans.get(plan_id).ok_or_else(unknown_plan)?;
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
        if plan.expires_at <= Instant::now()
            && !(plan.state == PlanState::Executing
                && plan
                    .active_until
                    .is_some_and(|until| until > Instant::now()))
        {
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        if plan.state != PlanState::Executing {
            return Err(TransferError::validation(
                "transfer plan was not claimed; restart from preview",
            ));
        }
        let checkpoint_expiry = Instant::now() + TRANSFER_CHECKPOINT_TTL;
        let plan = plans.get_mut(plan_id).ok_or_else(unknown_plan)?;
        plan.expires_at = checkpoint_expiry;
        // Keep the execution lease live from checkpoint creation through the
        // first page commit and cursor advancement. Long source snapshots or
        // first-page writes must not lose their plan lease in this gap.
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        let source_boundary = source_boundary_fingerprint(plan)?;
        let target_boundary = target_boundary_fingerprint(plan)?;
        let token = Uuid::new_v4().to_string();
        let checkpoint = TransferResumeCheckpoint {
            token: token.clone(),
            plan_id: plan_id.to_string(),
            organization_id: scope.organization_id().clone(),
            source_boundary,
            target_boundary,
            selected_tables,
            completed_tables,
            resume_tables: HashMap::new(),
            expires_at: checkpoint_expiry,
            state: ResumeCheckpointState::Available,
        };
        checkpoints.insert(token.clone(), checkpoint);
        Ok(token)
    }

    fn start_chunk_checkpoint(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
        progress: ResumeTableProgress,
    ) -> Result<String, TransferError> {
        let token = self.create_checkpoint(scope, plan_id, selected_tables, completed_tables)?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let saved = checkpoints
            .get_mut(&token)
            .ok_or_else(|| TransferError::validation("new resume checkpoint was not stored"))?;
        saved
            .resume_tables
            .insert(progress.source_table.clone(), progress);
        saved.state = ResumeCheckpointState::InFlight;
        Ok(token)
    }

    fn prepare_checkpoint_table(
        &self,
        scope: &OrganizationScope,
        token: &str,
        expected: ResumeTableProgress,
    ) -> Result<ResumeTableProgress, TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let saved = checkpoints.get_mut(token).ok_or_else(unknown_token)?;
        if !Self::owns(scope, &saved.organization_id) {
            return Err(unknown_token());
        }
        if saved.state != ResumeCheckpointState::InFlight || saved.expires_at <= Instant::now() {
            return Err(TransferError::validation(
                "resume checkpoint is no longer active; return to preview",
            ));
        }
        let progress = match saved.resume_tables.get(&expected.source_table) {
            Some(existing)
                if existing.source_table == expected.source_table
                    && existing.target_table == expected.target_table
                    && existing.key_columns == expected.key_columns
                    && existing.chunk_size == expected.chunk_size
                    && existing.source_fingerprint == expected.source_fingerprint =>
            {
                existing.clone()
            }
            Some(_) => {
                return Err(TransferError::validation(
                    "source data, table mapping, key order, or chunk size changed since the checkpoint; return to preview",
                ));
            }
            None => {
                saved
                    .resume_tables
                    .insert(expected.source_table.clone(), expected.clone());
                expected
            }
        };
        let renewed = Instant::now() + TRANSFER_CHECKPOINT_TTL;
        saved.expires_at = renewed;
        let plan = plans.get_mut(&saved.plan_id).ok_or_else(unknown_plan)?;
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        plan.expires_at = renewed;
        Ok(progress)
    }

    fn advance_checkpoint_table(
        &self,
        scope: &OrganizationScope,
        token: &str,
        source_table: &str,
        cursor: Vec<datazen_driver_api::Value>,
        rows_seen: u64,
    ) -> Result<(), TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let saved = checkpoints.get_mut(token).ok_or_else(|| {
            TransferError::validation("resume checkpoint disappeared after target commit")
        })?;
        if !Self::owns(scope, &saved.organization_id) {
            return Err(unknown_token());
        }
        if saved.state != ResumeCheckpointState::InFlight || saved.expires_at <= Instant::now() {
            return Err(TransferError::validation(
                "resume checkpoint cannot advance after target commit",
            ));
        }
        let progress = saved.resume_tables.get_mut(source_table).ok_or_else(|| {
            TransferError::validation("resume table state disappeared after target commit")
        })?;
        if cursor.len() != progress.key_columns.len() || rows_seen < progress.rows_seen {
            return Err(TransferError::validation(
                "resume cursor does not match the immutable primary-key order",
            ));
        }
        progress.cursor = Some(cursor);
        progress.rows_seen = rows_seen;
        let renewed = Instant::now() + TRANSFER_CHECKPOINT_TTL;
        saved.expires_at = renewed;
        let plan = plans.get_mut(&saved.plan_id).ok_or_else(|| {
            TransferError::validation("transfer plan disappeared after target commit")
        })?;
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        plan.expires_at = renewed;
        Ok(())
    }

    fn checkpoint_has_table_progress(
        &self,
        scope: &OrganizationScope,
        token: &str,
        source_table: &str,
    ) -> bool {
        self.checkpoints
            .lock()
            .map(|checkpoints| {
                checkpoints
                    .get(token)
                    .filter(|saved| Self::owns(scope, &saved.organization_id))
                    .is_some_and(|saved| saved.resume_tables.contains_key(source_table))
            })
            // If the store cannot be inspected, fail closed as though a
            // cursor existed; callers must not fall back to whole-table replay.
            .unwrap_or(true)
    }

    fn checkpoint_table_has_committed_chunks(
        &self,
        scope: &OrganizationScope,
        token: &str,
        source_table: &str,
    ) -> bool {
        self.checkpoints
            .lock()
            .map(|checkpoints| {
                checkpoints
                    .get(token)
                    .filter(|saved| Self::owns(scope, &saved.organization_id))
                    .and_then(|saved| saved.resume_tables.get(source_table))
                    .is_some_and(|progress| progress.rows_seen > 0)
            })
            .unwrap_or(true)
    }

    fn renew_execution(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        token: Option<&str>,
    ) -> Result<(), TransferError> {
        let now = Instant::now();
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let plan = plans.get_mut(plan_id).ok_or_else(|| {
            TransferError::validation("active transfer plan disappeared; return to preview")
        })?;
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
        if plan.state != PlanState::Executing {
            return Err(TransferError::validation(
                "transfer execution lease is no longer active; return to preview",
            ));
        }
        plan.active_until = Some(now + TRANSFER_ACTIVE_LEASE);
        if let Some(token) = token {
            let mut checkpoints = self
                .checkpoints
                .lock()
                .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
            let saved = checkpoints
                .get_mut(token)
                .ok_or_else(|| TransferError::validation("active resume checkpoint disappeared"))?;
            if !Self::owns(scope, &saved.organization_id) {
                return Err(unknown_token());
            }
            if saved.state != ResumeCheckpointState::InFlight {
                return Err(TransferError::validation(
                    "active resume checkpoint is no longer in flight",
                ));
            }
            saved.expires_at = now + TRANSFER_CHECKPOINT_TTL;
            plan.expires_at = saved.expires_at;
        }
        Ok(())
    }

    fn finish_execution(&self, scope: &OrganizationScope, plan_id: &str, keep_checkpoint: bool) {
        if let Ok(mut plans) = self.plans.lock() {
            if let Some(plan) = plans.get_mut(plan_id) {
                if !Self::owns(scope, &plan.organization_id) {
                    return;
                }
                plan.state = PlanState::Consumed;
                plan.active_until = None;
                plan.expires_at = Instant::now()
                    + if keep_checkpoint {
                        TRANSFER_CHECKPOINT_TTL
                    } else {
                        TRANSFER_PLAN_TTL
                    };
            }
        }
    }

    fn peek_checkpoint(
        &self,
        scope: &OrganizationScope,
        token: &str,
        plan_id: &str,
    ) -> Result<(StoredTransferPlan, TransferResumeCheckpoint), TransferError> {
        let plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let checkpoint = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let saved = checkpoint.get(token).ok_or_else(unknown_token)?;
        if !Self::owns(scope, &saved.organization_id) {
            return Err(unknown_token());
        }
        if saved.expires_at <= Instant::now() {
            return Err(TransferError::validation(
                "resume token expired; return to preview and compare again",
            ));
        }
        if saved.token != token {
            return Err(TransferError::validation(
                "resume token registry entry is invalid",
            ));
        }
        if saved.plan_id != plan_id {
            return Err(TransferError::validation(
                "resume token does not belong to this transfer plan",
            ));
        }
        if saved.state != ResumeCheckpointState::Available {
            return Err(TransferError::validation(
                "resume token is already running or its prior outcome is unknown",
            ));
        }
        let plan = plans.get(plan_id).ok_or_else(unknown_plan)?;
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
        if plan.expires_at <= Instant::now()
            && plan
                .active_until
                .is_none_or(|until| until <= Instant::now())
        {
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        Ok((plan.clone(), saved.clone()))
    }

    fn claim_checkpoint(
        &self,
        scope: &OrganizationScope,
        token: &str,
        plan_id: &str,
    ) -> Result<(StoredTransferPlan, TransferResumeCheckpoint), TransferError> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let saved = checkpoints.get_mut(token).ok_or_else(unknown_token)?;
        if !Self::owns(scope, &saved.organization_id) {
            return Err(unknown_token());
        }
        if saved.expires_at <= Instant::now() {
            checkpoints.remove(token);
            return Err(TransferError::validation(
                "resume token expired; return to preview and compare again",
            ));
        }
        if saved.token != token {
            return Err(TransferError::validation(
                "resume token registry entry is invalid",
            ));
        }
        if saved.plan_id != plan_id {
            return Err(TransferError::validation(
                "resume token does not belong to this transfer plan",
            ));
        }
        if saved.state != ResumeCheckpointState::Available {
            return Err(TransferError::validation(
                "resume token is already running or its prior outcome is unknown",
            ));
        }
        saved.state = ResumeCheckpointState::InFlight;
        let plan = plans.get_mut(plan_id).ok_or_else(unknown_plan)?;
        if !Self::owns(scope, &plan.organization_id) {
            return Err(unknown_plan());
        }
        plan.state = PlanState::Executing;
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        Ok((plan.clone(), saved.clone()))
    }

    fn update_checkpoint(
        &self,
        scope: &OrganizationScope,
        token: &str,
        completed_tables: Vec<String>,
        finished: bool,
    ) -> Result<(), TransferError> {
        let now = Instant::now();
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        let mut checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| TransferError::validation("transfer plan registry is unavailable"))?;
        if finished {
            let saved = checkpoints.remove(token).ok_or_else(unknown_token)?;
            if !Self::owns(scope, &saved.organization_id) {
                return Ok(());
            }
            if let Some(plan) = plans.get_mut(&saved.plan_id) {
                plan.state = PlanState::Consumed;
                plan.active_until = None;
                plan.expires_at = now + TRANSFER_PLAN_TTL;
            }
            return Ok(());
        }
        let saved = checkpoints.get_mut(token).ok_or_else(unknown_token)?;
        if !Self::owns(scope, &saved.organization_id) {
            return Err(unknown_token());
        }
        if saved.state != ResumeCheckpointState::InFlight {
            return Err(TransferError::validation(
                "resume checkpoint is not in flight",
            ));
        }
        if saved.expires_at <= now {
            checkpoints.remove(token);
            return Err(TransferError::validation(
                "resume checkpoint expired during execution; restart from preview",
            ));
        }
        saved.completed_tables = completed_tables;
        saved.expires_at = now + TRANSFER_CHECKPOINT_TTL;
        saved.state = ResumeCheckpointState::Available;
        if let Some(plan) = plans.get_mut(&saved.plan_id) {
            plan.state = PlanState::Consumed;
            plan.active_until = None;
            plan.expires_at = saved.expires_at;
        }
        Ok(())
    }

    /// Unknown commit/rollback outcomes are never resumable. Removing the
    /// checkpoint forces a fresh preview instead of risking duplicate writes.
    fn invalidate_checkpoint(&self, scope: &OrganizationScope, token: &str) {
        if let Ok(mut checkpoints) = self.checkpoints.lock() {
            let owned = checkpoints
                .get(token)
                .is_some_and(|saved| Self::owns(scope, &saved.organization_id));
            if owned {
                checkpoints.remove(token);
            }
        }
    }
}
