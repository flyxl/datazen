//! Async-host lease lifecycle. Physical handles stay in the adapter; this ledger
//! drives actual opening, execution, cleanup and pool policy decisions.
use super::{
    observability::{
        model_str, pool_key_digest, PoolObservabilityEntry, RuntimeObservabilityReport,
    },
    DriverCleanVerdict, HostConditionSnapshot, LeasePurpose, LeaseState, PoolKeyGeneration,
    PoolManager, PoolPolicy, ResourceError,
};
use crate::execution::runtime::budget::{DenialReason, InProcessBudgetCoordinator};
use crate::execution::runtime::connection::ResourceId;
use crate::shared::id::{ConnectionId, DbSessionId, OrganizationId, PrincipalId};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

pub struct LogicalSessionPermit {
    budget: Arc<InProcessBudgetCoordinator>,
    ticket: DbSessionId,
}
impl LogicalSessionPermit {
    pub fn reserve(
        budget: Arc<InProcessBudgetCoordinator>,
        organization: &OrganizationId,
        principal: &PrincipalId,
        connection: &ConnectionId,
        editor: bool,
    ) -> Result<Self, DenialReason> {
        let ticket = budget.mutate_ledger(|ledger| {
            let ticket = ledger.open_session(organization, principal, connection)?;
            if editor {
                if let Err(error) = ledger.connect_editor(&ticket) {
                    let _ = ledger.close_session(&ticket);
                    return Err(error);
                }
            }
            Ok(ticket)
        })?;
        Ok(Self { budget, ticket })
    }
}
impl Drop for LogicalSessionPermit {
    fn drop(&mut self) {
        let _ = self
            .budget
            .mutate_ledger(|ledger| ledger.close_session(&self.ticket));
    }
}
struct Row {
    key: PoolKeyGeneration,
    connection: ConnectionId,
    purpose: LeasePurpose,
    state: LeaseState,
    idle_since: Option<std::time::Instant>,
    verify_after: Option<std::time::Instant>,
}
#[derive(Default)]
struct State {
    rows: HashMap<ResourceId, Row>,
    pools: PoolManager,
}
#[derive(Default)]
pub struct ProductionLeaseManager {
    state: Mutex<State>,
}
impl ProductionLeaseManager {
    pub fn opened(
        &self,
        id: ResourceId,
        key: PoolKeyGeneration,
        connection: ConnectionId,
        purpose: LeasePurpose,
        policy: PoolPolicy,
    ) -> Result<(), ResourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceError::TransportRefused("lease ledger unavailable"))?;
        if state.rows.contains_key(&id) {
            return Err(ResourceError::TransportRefused("duplicate physical lease"));
        }
        state.pools.declare(key.clone(), policy)?;
        state.rows.insert(
            id,
            Row {
                key,
                connection,
                purpose,
                state: LeaseState::Acquired,
                idle_since: None,
                verify_after: None,
            },
        );
        Ok(())
    }
    pub fn executing(&self, id: &ResourceId) -> Result<(), ResourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceError::TransportRefused("lease ledger unavailable"))?;
        let row = state
            .rows
            .get_mut(id)
            .ok_or(ResourceError::TransportRefused("lease not registered"))?;
        match row.state {
            LeaseState::Acquired => row.state = row.state.transition(LeaseState::InUse)?,
            LeaseState::InUse => {}
            _ => return Err(ResourceError::TransportRefused("lease cannot execute")),
        }
        Ok(())
    }
    /// Driver Clean is a supplied conclusion, never reconstructed here.
    pub fn returning(
        &self,
        id: &ResourceId,
        driver: DriverCleanVerdict,
        host: &HostConditionSnapshot,
    ) -> Result<bool, ResourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceError::TransportRefused("lease ledger unavailable"))?;
        let row = state
            .rows
            .get(id)
            .ok_or(ResourceError::TransportRefused("lease not registered"))?;
        let key = row.key.clone();
        let purpose = row.purpose;
        let idle = state
            .rows
            .iter()
            .filter(|(other, row)| *other != id && row.key == key && row.idle_since.is_some())
            .count();
        let park = driver.is_clean()
            && host.admits_to_pool()
            && state.pools.decide(&key, purpose, idle).park;
        let row = state
            .rows
            .get_mut(id)
            .ok_or(ResourceError::TransportRefused("lease not registered"))?;
        if park {
            if row.state != LeaseState::Acquired {
                row.state = row.state.transition(LeaseState::Acquired)?;
            }
            row.idle_since = Some(std::time::Instant::now());
        } else if row.state != LeaseState::Closing && row.state != LeaseState::Quarantined {
            row.state = row.state.transition(LeaseState::Closing)?;
        }
        Ok(park)
    }
    pub fn closed(&self, id: &ResourceId, confirmed: bool) -> Result<(), ResourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceError::TransportRefused("lease ledger unavailable"))?;
        let row = state
            .rows
            .get_mut(id)
            .ok_or(ResourceError::TransportRefused("lease not registered"))?;
        let next = if confirmed {
            LeaseState::Closed
        } else {
            LeaseState::Quarantined
        };
        if row.state != next {
            row.state = row.state.transition(next)?;
        }
        if !confirmed {
            row.idle_since = None;
            row.verify_after =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(300));
        }
        if confirmed {
            let key = row.key.clone();
            state.rows.remove(id);
            if !state.rows.values().any(|row| row.key == key) {
                state.pools.forget(&key);
            }
        }
        Ok(())
    }
    /// Claim a close-only verification attempt without making quarantine acquirable.
    pub fn quarantine_to_verify(&self) -> Vec<ResourceId> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        let now = std::time::Instant::now();
        state
            .rows
            .iter_mut()
            .filter_map(|(id, row)| {
                if row.state != LeaseState::Quarantined
                    || !row.verify_after.is_some_and(|at| at <= now)
                {
                    return None;
                }
                // Claim under the ledger lock; another reclaimer cannot close concurrently.
                row.verify_after = Some(now + std::time::Duration::from_secs(300));
                Some(id.clone())
            })
            .collect()
    }
    pub fn claim_idle(&self, key: &PoolKeyGeneration) -> Result<Option<ResourceId>, ResourceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceError::TransportRefused("lease ledger unavailable"))?;
        let id = state
            .rows
            .iter()
            .find(|(_, row)| {
                row.key == *key
                    && row.state == LeaseState::Acquired
                    && row
                        .idle_since
                        .is_some_and(|at| at.elapsed().as_secs() < super::IDLE_POOL_TTL_SECONDS)
            })
            .map(|(id, _)| id.clone());
        if let Some(id) = &id {
            let row = state
                .rows
                .get_mut(id)
                .ok_or(ResourceError::TransportRefused("idle lease disappeared"))?;
            row.state = row.state.transition(LeaseState::InUse)?;
            row.idle_since = None;
        }
        Ok(id)
    }
    pub fn idle_to_retire(
        &self,
        current: Option<(&ConnectionId, &PoolKeyGeneration)>,
    ) -> Vec<ResourceId> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        let ids: Vec<_> = state
            .rows
            .iter()
            .filter(|(_, row)| {
                row.idle_since
                    .is_some_and(|at| at.elapsed().as_secs() >= super::IDLE_POOL_TTL_SECONDS)
                    || (row.idle_since.is_some()
                        && current.is_some_and(|(connection, key)| {
                            row.connection == *connection && row.key != *key
                        }))
            })
            .map(|(id, _)| id.clone())
            .collect();
        // Claim retirement while holding the same lock as claim_idle. Otherwise
        // a checkout can race a snapshot and receive a resource being closed.
        for id in &ids {
            if let Some(row) = state.rows.get_mut(id) {
                if let Ok(next) = row.state.transition(LeaseState::Closing) {
                    row.state = next;
                    row.idle_since = None;
                }
            }
        }
        ids
    }
    pub fn state(&self, id: &ResourceId) -> Option<LeaseState> {
        self.state.lock().ok()?.rows.get(id).map(|row| row.state)
    }

    /// Only-read snapshot of the lease ledger: per pool key in-use / idle /
    /// closing / quarantined, plus the physical totals.
    ///
    /// Every number is derived from the ledger at read time and the pool policy
    /// is read from the `PoolManager`; this type keeps **no** second counter
    /// (same rule as `resource/table.rs`). A poisoned lock yields `None` rather
    /// than a zeroed report, because "cannot read the ledger" and "no resources"
    /// must not look alike.
    pub fn snapshot(&self) -> Option<RuntimeObservabilityReport> {
        let state = self.state.lock().ok()?;
        let physical_total = state.rows.len() as u64;
        let mut by_key: BTreeMap<String, (PoolKeyGeneration, Counts)> = BTreeMap::new();
        for row in state.rows.values() {
            let (_, counts) = by_key
                .entry(pool_key_digest(&row.key))
                .or_insert_with(|| (row.key.clone(), Counts::default()));
            counts.add(row.state, row.idle_since.is_some());
        }
        let mut pools = Vec::with_capacity(by_key.len());
        let mut totals = Counts::default();
        for (digest, (key, counts)) in by_key {
            let settled = state.pools.snapshot(
                key,
                counts.idle as usize,
                counts.issued() as usize,
                physical_total as usize,
            );
            totals.in_use += counts.in_use;
            totals.idle += counts.idle;
            totals.closing += counts.closing;
            totals.quarantined += counts.quarantined;
            pools.push(PoolObservabilityEntry {
                pool_key_digest: digest,
                model: model_str(settled.model),
                max_idle: settled.max_idle as u64,
                in_use: counts.in_use,
                idle: counts.idle,
                closing: counts.closing,
                quarantined: counts.quarantined,
                issued: counts.issued(),
            });
        }
        Some(RuntimeObservabilityReport::from_ledger(
            pools,
            physical_total,
            totals.in_use,
            totals.idle,
            totals.closing,
            totals.quarantined,
        ))
    }
}

/// Per-key lease counts. Folded straight into the report; never stored.
#[derive(Default)]
struct Counts {
    in_use: u64,
    idle: u64,
    closing: u64,
    quarantined: u64,
}

impl Counts {
    fn add(&mut self, state: LeaseState, has_idle_since: bool) {
        match state {
            // A settled lease waiting for the next request from the same
            // generation: this is exactly `PoolSnapshot.idle_leases`.
            LeaseState::Acquired if has_idle_since => self.idle += 1,
            // `Acquired` without `idle_since` is "just opened, nothing attached
            // yet"; it still holds a physical slot and budget, so it counts as
            // in use. `Closed` never remains in the ledger (a confirmed close
            // removes the row), but folding it here keeps the match exhaustive
            // without inventing a fifth bucket.
            LeaseState::Acquired | LeaseState::InUse | LeaseState::Closed => self.in_use += 1,
            LeaseState::Closing => self.closing += 1,
            LeaseState::Quarantined => self.quarantined += 1,
        }
    }

    const fn issued(&self) -> u64 {
        self.in_use + self.idle + self.closing + self.quarantined
    }
}
impl super::LeaseManager {
    /// Async provider hosts share the same lifecycle/pool policies without blocking
    /// an executor thread on the legacy synchronous PhysicalTransport seam.
    pub fn production() -> ProductionLeaseManager {
        ProductionLeaseManager::default()
    }
}
