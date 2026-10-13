//! Driver budget adapter: each scope uses the shared four-class runtime ledger.
use async_trait::async_trait;
use datazen_driver_api::resource::{BudgetPermit, BudgetPort, ResourceError};
use datazen_platform_api::id::{ConnectionId, OrganizationId, PrincipalId};
use datazen_runtime::budget::{BudgetClaim, InProcessBudgetCoordinator, ResourceClass};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct DesktopResourceBudget {
    coordinator: Arc<InProcessBudgetCoordinator>,
    claim: BudgetClaim,
    expected_cost: u32,
    permits: Mutex<HashMap<String, (u32, Vec<datazen_runtime::budget::BudgetPermit>)>>,
}
impl DesktopResourceBudget {
    pub fn scoped(
        coordinator: Arc<InProcessBudgetCoordinator>,
        organization: OrganizationId,
        principal: PrincipalId,
        connection: ConnectionId,
        class: ResourceClass,
        cost: u32,
    ) -> Self {
        Self {
            coordinator,
            claim: BudgetClaim::single(organization, connection, principal, class)
                .with_slots(cost)
                .pinned(),
            expected_cost: cost,
            permits: Mutex::new(HashMap::new()),
        }
    }
    fn denied(requested: u32) -> ResourceError {
        ResourceError::BudgetDenied {
            requested,
            reason: "physical budget refused".into(),
        }
    }
}
#[async_trait]
impl BudgetPort for DesktopResourceBudget {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        if requested == 0 || requested != self.expected_cost {
            tracing::warn!(source = "physical_scope", category = "cost_mismatch", requested, expected = self.expected_cost, class = ?self.claim.class, "resource admission refused");
            return Err(Self::denied(requested));
        }
        let records = if requested == 1 {
            vec![self
                .coordinator
                .acquire_claim_with(&self.claim, 10_000)
                .await
                .map_err(|_| {
                    tracing::warn!(source = "physical_scope", category = "single_claim_unknown", requested, class = ?self.claim.class, "resource admission refused");
                    Self::denied(requested)
                })?]
        } else {
            self.coordinator
                .acquire_cost_now(&self.claim)
                .map_err(|_| {
                    tracing::warn!(source = "physical_scope", category = "atomic_claim", requested, class = ?self.claim.class, "resource admission refused");
                    Self::denied(requested)
                })?
        };
        let id = uuid::Uuid::new_v4().to_string();
        let mut permits = match self.permits.lock() {
            Ok(permits) => permits,
            Err(_) => {
                tracing::warn!(
                    source = "physical_scope",
                    category = "permit_storage_unavailable",
                    requested,
                    "resource admission refused"
                );
                for record in &records {
                    self.coordinator.release_permit(record, false);
                }
                return Err(Self::denied(requested));
            }
        };
        permits.insert(id.clone(), (requested, records));
        Ok(BudgetPermit {
            permit_id: id,
            physical_connections: requested,
        })
    }
    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        let mut permits = self.permits.lock().map_err(|_| Self::denied(0))?;
        if let Some((count, record)) = permits.get(&permit.permit_id) {
            if *count != permit.physical_connections {
                return Err(Self::denied(0));
            }
            for permit in record {
                match self.coordinator.release_permit(permit, true) {
                    datazen_runtime::budget::ReleaseResult::Released { .. }
                    | datazen_runtime::budget::ReleaseResult::AlreadyReleased { .. } => {}
                    _ => return Err(Self::denied(0)),
                }
            }
            permits.remove(&permit.permit_id);
        }
        Ok(())
    }
}
