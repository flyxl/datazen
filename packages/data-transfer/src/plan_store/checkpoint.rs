use std::collections::HashMap;
use std::time::Instant;

use uuid::Uuid;

use crate::resume::{ResumeTableProgress, TransferResumeCheckpoint as ResumeHook};
use crate::TransferError;

use super::{
    global_store, source_boundary_fingerprint, target_boundary_fingerprint, PlanState,
    StoredTransferPlan, TransferPlanStore, TRANSFER_ACTIVE_LEASE, TRANSFER_CHECKPOINT_TTL,
    TRANSFER_PLAN_TTL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResumeCheckpointState {
    Available,
    InFlight,
}

/// Server-owned progress for the bounded resumability contract.
///
/// The completed set contains committed source tables only. It deliberately
/// has no row offset or client-provided payload: a resume reuses the immutable
/// plan's source boundary and starts the next table transaction from its
/// beginning.
#[derive(Debug, Clone)]
pub struct TransferResumeCheckpoint {
    pub token: String,
    pub plan_id: String,
    pub source_boundary: String,
    pub target_boundary: String,
    pub selected_tables: Vec<String>,
    pub completed_tables: Vec<String>,
    pub resume_tables: HashMap<String, ResumeTableProgress>,
    pub(super) expires_at: Instant,
    pub(super) state: ResumeCheckpointState,
}

/// Remove expired checkpoint entries when a new plan/checkpoint is issued.
/// An active execution may briefly have an expired token between lease
/// renewal calls, so keep its in-flight state until the execution lease ends.
pub(super) fn retain_live_checkpoints(
    checkpoints: &mut HashMap<String, TransferResumeCheckpoint>,
    plans: &HashMap<String, StoredTransferPlan>,
    now: Instant,
) {
    checkpoints.retain(|_, checkpoint| {
        checkpoint.expires_at > now
            || (checkpoint.state == ResumeCheckpointState::InFlight
                && plans
                    .get(&checkpoint.plan_id)
                    .and_then(|plan| plan.active_until)
                    .is_some_and(|until| until > now))
    });
}

impl TransferPlanStore {
    pub fn create_checkpoint(
        &self,
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
        let plan = plans.get(plan_id).ok_or_else(|| {
            TransferError::validation("transfer plan is unknown or has expired; return to preview")
        })?;
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
        let plan = plans.get_mut(plan_id).ok_or_else(|| {
            TransferError::validation("transfer plan is unknown or has expired; return to preview")
        })?;
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

    pub(super) fn start_chunk_checkpoint(
        &self,
        plan_id: &str,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
        progress: ResumeTableProgress,
    ) -> Result<String, TransferError> {
        let token = self.create_checkpoint(plan_id, selected_tables, completed_tables)?;
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

    pub(super) fn prepare_checkpoint_table(
        &self,
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
        let saved = checkpoints.get_mut(token).ok_or_else(|| {
            TransferError::validation("resume token is unknown, consumed, or expired")
        })?;
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
        let plan = plans.get_mut(&saved.plan_id).ok_or_else(|| {
            TransferError::validation("transfer plan is unknown or has expired; return to preview")
        })?;
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        plan.expires_at = renewed;
        Ok(progress)
    }

    pub(super) fn advance_checkpoint_table(
        &self,
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

    pub(super) fn checkpoint_has_table_progress(&self, token: &str, source_table: &str) -> bool {
        self.checkpoints
            .lock()
            .map(|checkpoints| {
                checkpoints
                    .get(token)
                    .is_some_and(|saved| saved.resume_tables.contains_key(source_table))
            })
            // If the store cannot be inspected, fail closed as though a
            // cursor existed; callers must not fall back to whole-table replay.
            .unwrap_or(true)
    }

    pub(super) fn checkpoint_table_has_committed_chunks(
        &self,
        token: &str,
        source_table: &str,
    ) -> bool {
        self.checkpoints
            .lock()
            .map(|checkpoints| {
                checkpoints
                    .get(token)
                    .and_then(|saved| saved.resume_tables.get(source_table))
                    .is_some_and(|progress| progress.rows_seen > 0)
            })
            .unwrap_or(true)
    }

    pub(super) fn renew_execution(
        &self,
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

    pub(super) fn finish_execution(&self, plan_id: &str, keep_checkpoint: bool) {
        if let Ok(mut plans) = self.plans.lock() {
            if let Some(plan) = plans.get_mut(plan_id) {
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

    pub fn peek_checkpoint(
        &self,
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
        let saved = checkpoint.get(token).ok_or_else(|| {
            TransferError::validation("resume token is unknown, consumed, or expired")
        })?;
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
        let plan = plans.get(plan_id).ok_or_else(|| {
            TransferError::validation("transfer plan is unknown or has expired; return to preview")
        })?;
        if plan.expires_at <= Instant::now()
            && !plan
                .active_until
                .is_some_and(|until| until > Instant::now())
        {
            return Err(TransferError::validation(
                "transfer plan has expired; return to preview",
            ));
        }
        Ok((plan.clone(), saved.clone()))
    }

    pub fn claim_checkpoint(
        &self,
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
        let saved = checkpoints.get_mut(token).ok_or_else(|| {
            TransferError::validation("resume token is unknown, consumed, or expired")
        })?;
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
        let plan = plans.get_mut(plan_id).ok_or_else(|| {
            TransferError::validation("transfer plan is unknown or has expired; return to preview")
        })?;
        plan.state = PlanState::Executing;
        plan.active_until = Some(Instant::now() + TRANSFER_ACTIVE_LEASE);
        Ok((plan.clone(), saved.clone()))
    }

    pub fn update_checkpoint(
        &self,
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
            let saved = checkpoints.remove(token).ok_or_else(|| {
                TransferError::validation("resume token is unknown, consumed, or expired")
            })?;
            if let Some(plan) = plans.get_mut(&saved.plan_id) {
                plan.state = PlanState::Consumed;
                plan.active_until = None;
                plan.expires_at = now + TRANSFER_PLAN_TTL;
            }
            return Ok(());
        }
        let saved = checkpoints.get_mut(token).ok_or_else(|| {
            TransferError::validation("resume token is unknown, consumed, or expired")
        })?;
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
    pub fn invalidate_checkpoint(&self, token: &str) {
        if let Ok(mut checkpoints) = self.checkpoints.lock() {
            checkpoints.remove(token);
        }
    }
}

pub struct TransferCheckpointSession {
    plan_id: String,
    selected_tables: Vec<String>,
    completed_tables: Vec<String>,
    token: Option<String>,
    invalidated: bool,
    finalized: bool,
}

impl TransferCheckpointSession {
    pub fn new(
        plan_id: impl Into<String>,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
        resume_token: Option<String>,
    ) -> Self {
        Self {
            plan_id: plan_id.into(),
            selected_tables,
            completed_tables,
            token: resume_token,
            invalidated: false,
            finalized: false,
        }
    }

    pub fn finish(
        &mut self,
        completed_tables: Vec<String>,
        partial: bool,
    ) -> Result<Option<String>, TransferError> {
        if self.invalidated {
            global_store().finish_execution(&self.plan_id, false);
            self.finalized = true;
            return Ok(None);
        }
        if let Some(token) = self.token.as_deref() {
            global_store().update_checkpoint(token, completed_tables, !partial)?;
            self.finalized = true;
            return Ok(partial.then(|| token.to_string()));
        }
        if partial {
            let token = global_store().create_checkpoint(
                &self.plan_id,
                self.selected_tables.clone(),
                completed_tables,
            )?;
            global_store().finish_execution(&self.plan_id, true);
            self.token = Some(token.clone());
            self.finalized = true;
            return Ok(Some(token));
        }
        global_store().finish_execution(&self.plan_id, false);
        self.finalized = true;
        Ok(None)
    }

    pub fn abort(&mut self) {
        if let Some(token) = self.token.take() {
            global_store().invalidate_checkpoint(&token);
        }
        global_store().finish_execution(&self.plan_id, false);
        self.finalized = true;
    }
}

impl ResumeHook for TransferCheckpointSession {
    fn renew(&mut self) -> Result<(), TransferError> {
        if self.invalidated {
            return Err(TransferError::validation(
                "resume checkpoint was invalidated; return to preview",
            ));
        }
        global_store().renew_execution(&self.plan_id, self.token.as_deref())
    }

    fn prepare_table(
        &mut self,
        source_table: &str,
        target_table: &str,
        key_columns: &[String],
        chunk_size: u32,
        source_fingerprint: &str,
    ) -> Result<ResumeTableProgress, TransferError> {
        self.renew()?;
        let expected = ResumeTableProgress {
            source_table: source_table.to_string(),
            target_table: target_table.to_string(),
            key_columns: key_columns.to_vec(),
            chunk_size,
            source_fingerprint: source_fingerprint.to_string(),
            cursor: None,
            rows_seen: 0,
        };
        let result = match self.token.as_deref() {
            Some(token) => global_store().prepare_checkpoint_table(token, expected),
            None => {
                let token = global_store().start_chunk_checkpoint(
                    &self.plan_id,
                    self.selected_tables.clone(),
                    self.completed_tables.clone(),
                    expected.clone(),
                )?;
                self.token = Some(token);
                Ok(expected)
            }
        };
        if result.is_err() {
            self.invalidate();
        }
        result
    }

    fn advance_table(
        &mut self,
        source_table: &str,
        cursor: Vec<datazen_driver_api::Value>,
        rows_seen: u64,
    ) -> Result<(), TransferError> {
        let token = self.token.as_deref().ok_or_else(|| {
            TransferError::validation("chunk resume checkpoint was not created before writing")
        })?;
        global_store().advance_checkpoint_table(token, source_table, cursor, rows_seen)
    }

    fn token(&self) -> Option<String> {
        (!self.invalidated).then(|| self.token.clone()).flatten()
    }

    fn has_table_progress(&self, source_table: &str) -> bool {
        self.token
            .as_deref()
            .is_some_and(|token| global_store().checkpoint_has_table_progress(token, source_table))
    }

    fn table_has_committed_chunks(&self, source_table: &str) -> bool {
        self.token.as_deref().is_some_and(|token| {
            global_store().checkpoint_table_has_committed_chunks(token, source_table)
        })
    }

    fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    fn invalidate(&mut self) {
        self.invalidated = true;
        if let Some(token) = self.token.take() {
            global_store().invalidate_checkpoint(&token);
        }
    }
}

impl Drop for TransferCheckpointSession {
    fn drop(&mut self) {
        if self.finalized {
            return;
        }
        if let Some(token) = self.token.take() {
            global_store().invalidate_checkpoint(&token);
        }
        global_store().finish_execution(&self.plan_id, false);
    }
}
