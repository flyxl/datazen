use async_trait::async_trait;
use datazen_driver_api::resource::{BudgetPermit, BudgetPort, ResourceError};
use std::{collections::HashMap, sync::Mutex};

pub struct DesktopResourceBudget {
    limit: u32,
    permits: Mutex<HashMap<String, u32>>,
}
impl DesktopResourceBudget {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            permits: Mutex::new(HashMap::new()),
        }
    }
}
#[async_trait]
impl BudgetPort for DesktopResourceBudget {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        let mut permits = self
            .permits
            .lock()
            .map_err(|_| ResourceError::BudgetDenied {
                requested,
                reason: "budget unavailable".into(),
            })?;
        let used: u32 = permits.values().sum();
        if requested == 0 || requested > self.limit.saturating_sub(used) {
            return Err(ResourceError::BudgetDenied {
                requested,
                reason: "physical connection limit reached".into(),
            });
        }
        let id = uuid::Uuid::new_v4().to_string();
        permits.insert(id.clone(), requested);
        Ok(BudgetPermit {
            permit_id: id,
            physical_connections: requested,
        })
    }
    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        let mut permits = self
            .permits
            .lock()
            .map_err(|_| ResourceError::BudgetDenied {
                requested: 0,
                reason: "budget unavailable".into(),
            })?;
        if let Some(count) = permits.get(&permit.permit_id) {
            if *count != permit.physical_connections {
                return Err(ResourceError::BudgetDenied {
                    requested: 0,
                    reason: "permit mismatch".into(),
                });
            }
        }
        permits.remove(&permit.permit_id);
        Ok(())
    }
}
