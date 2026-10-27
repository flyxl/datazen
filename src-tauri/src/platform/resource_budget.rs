//! Driver budget adapter: each scope uses the shared four-class runtime ledger.
use async_trait::async_trait;
use datazen_driver_api::resource::{BudgetPermit, BudgetPort, ResourceError};
use datazen_modules::execution::runtime::budget::{
    BudgetClaim, InProcessBudgetCoordinator, ResourceClass,
};
use datazen_modules::shared::id::{ConnectionId, OrganizationId, PrincipalId};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct DesktopResourceBudget {
    coordinator: Arc<InProcessBudgetCoordinator>,
    claim: BudgetClaim,
    expected_cost: u32,
    permits: Mutex<
        HashMap<
            String,
            (
                u32,
                Vec<datazen_modules::execution::runtime::budget::BudgetPermit>,
            ),
        >,
    >,
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
    fn capacity(&self) -> (u32, u32) {
        self.coordinator.with_ledger(|ledger| {
            (
                ledger.config().service_physical_connections,
                ledger.physical(&self.claim.connection_id),
            )
        })
    }
    fn denied(&self, requested: u32) -> ResourceError {
        let (limit, used) = self.capacity();
        ResourceError::BudgetDenied {
            requested,
            reason: format!("physical connection limit {limit}, in use {used}"),
        }
    }
}
#[async_trait]
impl BudgetPort for DesktopResourceBudget {
    async fn acquire_control_connection(&self) -> Result<BudgetPermit, ResourceError> {
        let mut claim = self.claim.clone();
        claim.class = ResourceClass::Control;
        claim.slots = 1;
        let record = self
            .coordinator
            .acquire_claim_with(&claim, 10_000)
            .await
            .map_err(|_| self.denied(1))?;
        let id = uuid::Uuid::new_v4().to_string();
        match self.permits.lock() {
            Ok(mut permits) => {
                permits.insert(id.clone(), (1, vec![record]));
            }
            Err(_) => {
                self.coordinator.release_permit(&record, false);
                return Err(self.denied(1));
            }
        }
        Ok(BudgetPermit {
            permit_id: id,
            physical_connections: 1,
        })
    }

    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        if requested == 0 || requested != self.expected_cost {
            let (limit, used) = self.capacity();
            tracing::warn!(source = "physical_scope", category = "cost_mismatch", requested, expected = self.expected_cost, limit, used, class = ?self.claim.class, "resource admission refused");
            return Err(self.denied(requested));
        }
        let records = if requested == 1 {
            vec![self
                .coordinator
                .acquire_claim_with(&self.claim, 10_000)
                .await
                .map_err(|_| {
                    let (limit, used) = self.capacity();
                    tracing::warn!(source = "physical_scope", category = "single_claim_unknown", requested, limit, used, class = ?self.claim.class, "resource admission refused");
                    self.denied(requested)
                })?]
        } else {
            self.coordinator
                .acquire_cost_now(&self.claim)
                .map_err(|_| {
                    let (limit, used) = self.capacity();
                    tracing::warn!(source = "physical_scope", category = "atomic_claim", requested, limit, used, class = ?self.claim.class, "resource admission refused");
                    self.denied(requested)
                })?
        };
        let id = uuid::Uuid::new_v4().to_string();
        let mut permits = match self.permits.lock() {
            Ok(permits) => permits,
            Err(_) => {
                let (limit, used) = self.capacity();
                tracing::warn!(
                    source = "physical_scope",
                    category = "permit_storage_unavailable",
                    requested,
                    limit,
                    used,
                    "resource admission refused"
                );
                for record in &records {
                    self.coordinator.release_permit(record, false);
                }
                return Err(self.denied(requested));
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
        let mut permits = self.permits.lock().map_err(|_| self.denied(0))?;
        if let Some((count, record)) = permits.get(&permit.permit_id) {
            if *count != permit.physical_connections {
                return Err(self.denied(0));
            }
            for permit in record {
                match self.coordinator.release_permit(permit, true) {
                    datazen_modules::execution::runtime::budget::ReleaseResult::Released { .. }
                    | datazen_modules::execution::runtime::budget::ReleaseResult::AlreadyReleased { .. } => {}
                    _ => return Err(self.denied(0)),
                }
            }
            permits.remove(&permit.permit_id);
        }
        Ok(())
    }
}

/// 作用域被丢弃而许可仍挂着时，把许可还给账本。
///
/// 许可的**唯一记账方**是 `InProcessBudgetCoordinator`，而许可的持有者是本作用域。
/// 只要有一条路径让作用域先于显式 [`BudgetPort::release_physical_connections`] 消失
/// ——池中资源被替换、租约表在关停时整体析构、宿主重建会话后端——许可就**永久**留在
/// 账本里直到进程结束：预算只会越来越紧，而且没有任何日志指向这个原因（实测现象是
/// `physical` 单调抬升、`shared_headroom` 只剩 1 而申请要 2 槽）。
///
/// 这里按「归还」而不是「核销」记账：显式释放是资源被确认关闭后的核销（`consumed`），
/// 析构是持有者消失时的兜底归还；两者在账本里必须分得开，否则核销数会被凭空抬高。
/// 显式释放过的许可已从 `permits` 移除，析构时自然无操作。
impl Drop for DesktopResourceBudget {
    fn drop(&mut self) {
        let outstanding = match self.permits.get_mut() {
            Ok(permits) => std::mem::take(permits),
            // 中毒锁里的记录仍然是真记录：取其内容而不是连带丢掉许可。
            Err(poisoned) => std::mem::take(poisoned.into_inner()),
        };
        if outstanding.is_empty() {
            return;
        }
        let mut returned = 0usize;
        for (_, (_, records)) in outstanding {
            for record in &records {
                match self.coordinator.release_permit(record, false) {
                    datazen_modules::execution::runtime::budget::ReleaseResult::Released { .. }
                    | datazen_modules::execution::runtime::budget::ReleaseResult::AlreadyReleased { .. } => {
                        returned += 1;
                    }
                    datazen_modules::execution::runtime::budget::ReleaseResult::Unknown { .. } => {}
                }
            }
        }
        tracing::warn!(
            source = "physical_scope",
            category = "scope_dropped_with_permits",
            returned,
            class = ?self.claim.class,
            "resource admission permits returned by scope drop"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_modules::execution::runtime::budget::BudgetConfig;

    /// 小到「漏一次就看得出来」的桌面口径：control 1 / interactive 2 / metadata 2 / job 0。
    const TEST_TOTAL: u32 = 12;
    const TEST_RESERVED: [u32; 4] = [1, 2, 2, 0];

    struct Fixture {
        coordinator: Arc<InProcessBudgetCoordinator>,
        organization: OrganizationId,
        principal: PrincipalId,
        connection: ConnectionId,
    }

    impl Fixture {
        fn new() -> Self {
            let organization = OrganizationId::new("org-scope-drop");
            Self {
                coordinator: Arc::new(InProcessBudgetCoordinator::new(
                    BudgetConfig::from_parts(TEST_TOTAL, TEST_RESERVED),
                    organization.clone(),
                )),
                organization,
                principal: PrincipalId::new("principal-scope-drop"),
                connection: ConnectionId::new("conn-scope-drop"),
            }
        }

        fn scope(&self, class: ResourceClass, cost: u32) -> DesktopResourceBudget {
            DesktopResourceBudget::scoped(
                self.coordinator.clone(),
                self.organization.clone(),
                self.principal.clone(),
                self.connection.clone(),
                class,
                cost,
            )
        }

        fn physical(&self) -> u32 {
            self.coordinator
                .with_ledger(|ledger| ledger.physical(&self.connection))
        }

        fn shared_used(&self) -> u32 {
            self.coordinator
                .with_ledger(|ledger| ledger.shared_used(&self.connection))
        }
    }

    #[tokio::test]
    async fn control_uses_its_own_reserved_slot_and_ignores_data_cost() {
        let fixture = Fixture::new();
        let scope = fixture.scope(ResourceClass::Metadata, 2);
        let data = scope.acquire_physical_connections(2).await.unwrap();
        let control = scope.acquire_control_connection().await.unwrap();
        assert_eq!(control.physical_connections, 1);
        assert_eq!(fixture.physical(), 3);
        assert_eq!(
            fixture.shared_used(),
            0,
            "control uses its reserved class slot"
        );
        scope.release_control_connection(&control).await.unwrap();
        assert_eq!(fixture.physical(), 2);
        scope.release_physical_connections(&data).await.unwrap();
        assert_eq!(fixture.physical(), 0);
    }

    #[tokio::test]
    async fn a_scope_dropped_with_permits_returns_them() {
        let fixture = Fixture::new();
        {
            let scope = fixture.scope(ResourceClass::Metadata, 2);
            let permit = scope
                .acquire_physical_connections(2)
                .await
                .expect("metadata 保留额足够容纳一次 2 槽许可");
            assert_eq!(permit.physical_connections, 2);
            assert_eq!(fixture.physical(), 2, "许可占用必须落在物理台账上");
            // 作用域在此丢弃：没有调用 `release_physical_connections`。
        }
        assert_eq!(fixture.physical(), 0, "丢弃作用域必须把许可还给账本");
        assert_eq!(fixture.shared_used(), 0, "共享槽必须一并退回");
    }

    #[tokio::test]
    async fn an_explicitly_released_scope_does_not_double_release() {
        let fixture = Fixture::new();
        {
            let scope = fixture.scope(ResourceClass::Metadata, 2);
            let permit = scope
                .acquire_physical_connections(2)
                .await
                .expect("metadata 保留额足够容纳一次 2 槽许可");
            scope
                .release_physical_connections(&permit)
                .await
                .expect("已登记的许可必须能显式释放");
            assert_eq!(fixture.physical(), 0);
            // 显式释放之后析构：不得二次记账。
        }
        assert_eq!(fixture.physical(), 0, "析构不得重复释放已归还的许可");
        assert_eq!(fixture.shared_used(), 0);
    }

    #[tokio::test]
    async fn a_dropped_scope_frees_headroom_for_the_next_claim() {
        let fixture = Fixture::new();
        let first = fixture.scope(ResourceClass::Metadata, 2);
        first
            .acquire_physical_connections(2)
            .await
            .expect("metadata 保留额足够容纳一次 2 槽许可");
        assert_eq!(fixture.physical(), 2);
        // 丢弃持有许可的作用域，而不是丢弃 `BudgetPermit` 句柄：句柄丢弃不放额度。
        drop(first);
        assert_eq!(fixture.physical(), 0, "丢弃后额度必须回到零");
        let retry = fixture.scope(ResourceClass::Metadata, 2);
        retry
            .acquire_physical_connections(2)
            .await
            .expect("归还后同一额度必须可以再次准入");
    }
}
