use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;

use crate::resume::{ResumeTableProgress, TransferResumeCheckpoint as ResumeHook};
use crate::TransferError;

use super::port::TransferPlanRepositoryPort;
use super::StoredTransferPlan;

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
    /// The organization that owns this checkpoint. A scope that does not match
    /// it sees the token as non-existent (CM-05).
    pub organization_id: OrganizationId,
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

/// The refusal every unknown-token path uses. A token owned by another
/// organization is answered exactly like an unknown one (CM-05).
pub(super) fn unknown_token() -> TransferError {
    TransferError::validation("resume token is unknown, consumed, or expired")
}

/// The resume hook of one transfer execution.
///
/// It holds the injected port instance and the organization scope instead of
/// reaching for a process-wide store: the session's lifetime is bound to the
/// host's instance, and a session can never touch another organization's
/// records.
pub struct TransferCheckpointSession {
    store: Arc<dyn TransferPlanRepositoryPort>,
    scope: OrganizationScope,
    plan_id: String,
    selected_tables: Vec<String>,
    completed_tables: Vec<String>,
    token: Option<String>,
    invalidated: bool,
    finalized: bool,
}

impl TransferCheckpointSession {
    pub fn new(
        store: Arc<dyn TransferPlanRepositoryPort>,
        scope: OrganizationScope,
        plan_id: impl Into<String>,
        selected_tables: Vec<String>,
        completed_tables: Vec<String>,
        resume_token: Option<String>,
    ) -> Self {
        Self {
            store,
            scope,
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
            self.store
                .finish_execution(&self.scope, &self.plan_id, false);
            self.finalized = true;
            return Ok(None);
        }
        if let Some(token) = self.token.as_deref() {
            self.store
                .update_checkpoint(&self.scope, token, completed_tables, !partial)?;
            self.finalized = true;
            return Ok(partial.then(|| token.to_string()));
        }
        if partial {
            let token = self.store.create_checkpoint(
                &self.scope,
                &self.plan_id,
                self.selected_tables.clone(),
                completed_tables,
            )?;
            self.store
                .finish_execution(&self.scope, &self.plan_id, true);
            self.token = Some(token.clone());
            self.finalized = true;
            return Ok(Some(token));
        }
        self.store
            .finish_execution(&self.scope, &self.plan_id, false);
        self.finalized = true;
        Ok(None)
    }

    pub fn abort(&mut self) {
        if let Some(token) = self.token.take() {
            self.store.invalidate_checkpoint(&self.scope, &token);
        }
        self.store
            .finish_execution(&self.scope, &self.plan_id, false);
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
        self.store
            .renew_execution(&self.scope, &self.plan_id, self.token.as_deref())
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
            Some(token) => self
                .store
                .prepare_checkpoint_table(&self.scope, token, expected),
            None => {
                let token = self.store.start_chunk_checkpoint(
                    &self.scope,
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
        self.store
            .advance_checkpoint_table(&self.scope, token, source_table, cursor, rows_seen)
    }

    fn token(&self) -> Option<String> {
        (!self.invalidated).then(|| self.token.clone()).flatten()
    }

    fn has_table_progress(&self, source_table: &str) -> bool {
        self.token.as_deref().is_some_and(|token| {
            self.store
                .checkpoint_has_table_progress(&self.scope, token, source_table)
        })
    }

    fn table_has_committed_chunks(&self, source_table: &str) -> bool {
        self.token.as_deref().is_some_and(|token| {
            self.store
                .checkpoint_table_has_committed_chunks(&self.scope, token, source_table)
        })
    }

    fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    fn invalidate(&mut self) {
        self.invalidated = true;
        if let Some(token) = self.token.take() {
            self.store.invalidate_checkpoint(&self.scope, &token);
        }
    }
}

impl Drop for TransferCheckpointSession {
    fn drop(&mut self) {
        if self.finalized {
            return;
        }
        if let Some(token) = self.token.take() {
            self.store.invalidate_checkpoint(&self.scope, &token);
        }
        self.store
            .finish_execution(&self.scope, &self.plan_id, false);
    }
}
