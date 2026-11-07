//! WR-F7：`ServerBudgetCoordinator` → `dr::BudgetPort` 的物理预算桥。
//!
//! [`crate::execution::driver_exec::ServerDriverExecBackend`] 的构造注入点
//! `Arc<dyn dr::BudgetPort>`（driver_exec.rs 头部账目表的「物理预算」行）由
//! 本桥接供给：driver-api 端口按**物理连接数**申请/归还（无组织上下文），
//! modules `BudgetCoordinator` 按 (组织, 连接, 用途) 记账——桥接把 n 条物理
//! 连接展开为 n 份 coordinator 许可，映射账目在桥内存里持有（`release` 时
//! 按 `permit_id` 找回逐一归还）。
//!
//! 登记（v1 形态边界）：
//!
//! - 组织归属取构造注入的快照范围（与 coordinator 构造同源）；
//!   多组织分账随 P9 全局预算切片；
//! - 连接维度用合成连接键——物理预算在 v1 是节点级池账（coordinator 的
//!   `node_limit` 才是约束），不是逐连接账；
//! - 用途固定 [`BudgetPurpose::BackgroundJob`]（驱动执行是服务端发起的
//!   非交互负载；交互通道的预算随执行域路由点亮分流）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use datazen::execution::application::budget::{
    BudgetCoordinator, BudgetPermit as CoordinatorPermit, BudgetPurpose, ReleaseOutcome,
};
use datazen::shared::id::{ConnectionId, OrganizationId};
use datazen_driver_api::resource::{self as dr};

/// 桥接的合成连接键（v1 节点级池账，见模块头登记）。
const PHYSICAL_CONNECTION_KEY: &str = "physical:bridge";

fn ledger_guard(
    ledger: &Mutex<HashMap<String, Vec<CoordinatorPermit>>>,
) -> MutexGuard<'_, HashMap<String, Vec<CoordinatorPermit>>> {
    ledger
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 物理预算桥。一个进程一份（与 coordinator 同生命周期）。
pub struct BudgetPortBridge {
    coordinator: Arc<dyn BudgetCoordinator>,
    organization_id: OrganizationId,
    ledger: Mutex<HashMap<String, Vec<CoordinatorPermit>>>,
    sequence: AtomicU64,
}

impl std::fmt::Debug for BudgetPortBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BudgetPortBridge")
            .field("coordinator", &"BudgetCoordinator")
            .finish()
    }
}

impl BudgetPortBridge {
    /// 构造。`coordinator` 与 bootstrap 的额度账同实例；组织取其快照范围。
    pub fn new(coordinator: Arc<dyn BudgetCoordinator>, organization_id: OrganizationId) -> Self {
        Self {
            coordinator,
            organization_id,
            ledger: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
        }
    }

    fn next_permit_id(&self) -> String {
        format!(
            "budget-bridge-{}",
            self.sequence.fetch_add(1, Ordering::Relaxed)
        )
    }

    async fn acquire_one(&self, requested: u32) -> Result<CoordinatorPermit, dr::ResourceError> {
        let request = datazen::execution::application::budget::BudgetRequest::new(
            self.organization_id.clone(),
            ConnectionId::new(PHYSICAL_CONNECTION_KEY),
            BudgetPurpose::BackgroundJob,
        );
        self.coordinator
            .acquire(request)
            .await
            .map_err(|error| dr::ResourceError::BudgetDenied {
                requested,
                reason: error.to_string(),
            })
    }
}

#[async_trait]
impl dr::BudgetPort for BudgetPortBridge {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<dr::BudgetPermit, dr::ResourceError> {
        let mut permits = Vec::with_capacity(requested as usize);
        for _ in 0..requested {
            match self.acquire_one(requested).await {
                Ok(permit) => permits.push(permit),
                // 整体失败：已拿到的许可按 Unused 归还，不留半边（与
                // coordinator `reserve_many` 的全有全无语义对齐）。
                Err(error) => {
                    for held in &permits {
                        let _ = self.coordinator.release(held, ReleaseOutcome::Unused).await;
                    }
                    return Err(error);
                }
            }
        }
        let permit_id = self.next_permit_id();
        ledger_guard(&self.ledger).insert(permit_id.clone(), permits);
        Ok(dr::BudgetPermit {
            permit_id,
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &dr::BudgetPermit,
    ) -> Result<(), dr::ResourceError> {
        let permits = ledger_guard(&self.ledger)
            .remove(permit.permit_id.as_str())
            .ok_or_else(|| dr::ResourceError::BudgetDenied {
                requested: permit.physical_connections,
                reason: format!("unknown budget bridge permit {}", permit.permit_id),
            })?;
        for held in &permits {
            // 逐份归还；单份失败即返（INV-10 幂等核销在 coordinator 侧，
            // 重复归还不重复释放额度；失败原因进错误文本供上游观测）。
            if let Err(error) = self
                .coordinator
                .release(held, ReleaseOutcome::Consumed)
                .await
            {
                return Err(dr::ResourceError::BudgetDenied {
                    requested: permit.physical_connections,
                    reason: format!("release failed: {error}"),
                });
            }
        }
        Ok(())
    }
}
