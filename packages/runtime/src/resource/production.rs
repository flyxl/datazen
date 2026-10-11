//! Async-host lease lifecycle. Physical handles stay in the adapter; this ledger
//! drives actual opening, execution, cleanup and pool policy decisions.
use super::{
    DriverCleanVerdict, HostConditionSnapshot, LeasePurpose, LeaseState, PoolKeyGeneration,
    PoolManager, PoolPolicy, ResourceError,
};
use crate::{
    budget::{DenialReason, InProcessBudgetCoordinator},
    connection::ResourceId,
};
use datazen_platform_api::id::{ConnectionId, DbSessionId, OrganizationId, PrincipalId};
use std::{
    collections::HashMap,
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
        row.state = row.state.transition(if confirmed {
            LeaseState::Closed
        } else {
            LeaseState::Quarantined
        })?;
        if confirmed {
            let key = row.key.clone();
            state.rows.remove(id);
            if !state.rows.values().any(|row| row.key == key) {
                state.pools.forget(&key);
            }
        }
        Ok(())
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
}
impl super::LeaseManager {
    /// Async provider hosts share the same lifecycle/pool policies without blocking
    /// an executor thread on the legacy synchronous PhysicalTransport seam.
    pub fn production() -> ProductionLeaseManager {
        ProductionLeaseManager::default()
    }
}
