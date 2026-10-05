//! 多端原子预算预留（§3 / §9.5）：源/目标/控制一次性全组预留或全部排队。
//!
//! * 资源申请发生在取资源**之前**；endpoint 重叠（读写对象不可排除）先拒绝、
//!   不持有任何许可（§10.1.1 决策表「 cleanup 未确认 → 预算占用」之外的路径）。
//! * `try_admit_many` 失败时整体回滚，一个名额都不占（BudgetLedger 已有保证）。
//! * 身份不可证明时不开放危险自覆盖：缺 service_key / connectionId 的端点拒绝。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use datazen_platform_api::id::{ConnectionId, OrganizationId, PrincipalId};
use datazen_platform_api::ports::budget::ResourceClass;

use crate::budget::classes::BudgetClaim;
use crate::budget::ledger::BudgetLedger;
use crate::budget::records::{PermitRecord, ReleaseResult};

use crate::job::error::JobError;

/// 端点在迁移任务中的角色。同一服务上整个 Job 周期占一个 reader/writer（§6.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointRole {
    SourceReader,
    TargetWriter,
    Control,
}

/// 一个迁移端点：归属连接、角色、参与对象、物理服务身份。
///
/// `service_key` 是「同一物理服务」的比较键——不同 profile 可能指向同一对象，
/// 同一 profile 也可能指向不同库，单靠 connectionId 判断自覆盖会漏报/误报。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRef {
    pub connection_id: ConnectionId,
    pub service_key: String,
    pub objects: Vec<String>,
    pub role: EndpointRole,
}

/// 检测危险重叠：同 service_key 下，任一 reader 的读对象与任一 writer 的写对象
/// 存在交集即 `EndpointOverlap`；service_key 为空（身份不可证明）且同时存在
/// reader 与 writer 时同样拒绝（不开放危险自覆盖）。
pub fn detect_endpoint_overlap(endpoints: &[EndpointRef]) -> Result<(), JobError> {
    let mut reads: HashSet<(String, String)> = HashSet::new();
    let mut has_writer = false;
    let mut any_unprovable = false;
    for ep in endpoints {
        if ep.service_key.trim().is_empty() {
            any_unprovable = true;
        }
        match ep.role {
            EndpointRole::SourceReader => {
                for obj in &ep.objects {
                    reads.insert((ep.service_key.clone(), obj.clone()));
                }
            }
            EndpointRole::TargetWriter => {
                has_writer = true;
                for obj in &ep.objects {
                    if reads.contains(&(ep.service_key.clone(), obj.clone())) {
                        return Err(JobError::EndpointOverlap(format!(
                            "service `{}` object `{obj}` is both read and written",
                            ep.service_key
                        )));
                    }
                }
            }
            EndpointRole::Control => {}
        }
    }
    if any_unprovable && has_writer {
        return Err(JobError::EndpointOverlap(
            "endpoint identity unprovable while both reader and writer present".into(),
        ));
    }
    Ok(())
}

/// 一次 Job 的所有端点许可：全有或全无。
///
/// 构造即完成 `detect_endpoint_overlap` + 一次 `try_admit_many`。
/// 许可在 `Drop` 时**不**自动释放——资源生命周期由 runtime 的 cleanup 显式驱动
/// （关闭窗口只取消订阅，不释放 Job 资源，§8）。
pub struct MultiEndpointPermits {
    ledger: Arc<Mutex<BudgetLedger>>,
    permits: Vec<PermitRecord>,
    now_ms: u64,
}

impl MultiEndpointPermits {
    /// 原子全组预留。重叠 → `EndpointOverlap`（未持有任何许可）；
    /// 预算不足 → `BudgetDenied`（未持有任何许可）。
    pub fn reserve(
        ledger: Arc<Mutex<BudgetLedger>>,
        endpoints: &[EndpointRef],
        class: ResourceClass,
        organization_id: &OrganizationId,
        principal: &PrincipalId,
        now_ms: u64,
    ) -> Result<Self, JobError> {
        detect_endpoint_overlap(endpoints)?;
        // 按 service_key（物理服务身份）为键、connection_id 次键稳定排序，避免 AB/BA 的许可获取顺序差异（§3）。
        let mut endpoints: Vec<&EndpointRef> = endpoints.iter().collect();
        endpoints.sort_by(|a, b| {
            a.service_key
                .cmp(&b.service_key)
                .then_with(|| a.connection_id.as_str().cmp(b.connection_id.as_str()))
        });
        let claims: Vec<BudgetClaim> = endpoints
            .iter()
            .map(|ep| BudgetClaim {
                organization_id: organization_id.clone(),
                connection_id: ep.connection_id.clone(),
                principal: principal.clone(),
                class,
                pinned: true,
                slots: 1,
                worker: None,
            })
            .collect();
        let permits = {
            let mut guard = ledger
                .lock()
                .map_err(|_| JobError::BudgetDenied("lock".into()))?;
            guard
                .try_admit_many(&claims, now_ms)
                .map_err(|reason| JobError::BudgetDenied(format!("{reason:?}")))?
        };
        Ok(Self {
            ledger,
            permits,
            now_ms,
        })
    }

    pub fn permits(&self) -> &[PermitRecord] {
        &self.permits
    }

    /// 显式核销。幂等：重复释放返回 `AlreadyReleased`（BudgetLedger 保证）。
    pub fn release(self, consumed: bool) -> Vec<ReleaseResult> {
        let mut guard = match self.ledger.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        self.permits
            .iter()
            .map(|record| {
                guard.release(&record.to_port_permit(), consumed, self.now_ms)
            })
            .collect()
    }
}
