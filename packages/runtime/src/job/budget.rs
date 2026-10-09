//! 多端原子预算预留（§3 / §9.5）：源/目标/控制一次性全组预留或全部排队。
//!
//! * 资源申请发生在取资源**之前**；endpoint 重叠（读写对象不可排除）先拒绝、
//!   不持有任何许可（§10.1.1 决策表「 cleanup 未确认 → 预算占用」之外的路径）。
//! * `try_admit_many` 失败时整体回滚，一个名额都不占（BudgetLedger 已有保证）。
//! * 身份不可证明时不开放危险自覆盖：端点给不出 `service_key` 时拒绝。

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
/// 两个字段回答两个**不同**的问题，混用是危险自覆盖漏检的根源：
///
/// * `service_key`——「是否同一台物理服务器上的同一对象」。它必须由端点真实位置
///   （host/port/database/…）导出，所以两个指向同一台服务器的不同连接配置会共享它。
///   **重叠检测只认它**。
/// * `connection_id`——持久化连接配置 id，即 AGENTS.md 的 `connectionId`。它回答的是
///   「这些许可记到哪本账上」（§6.2 `ensure_service` 按它注册），与物理位置无关：
///   一条保存连接可以被 `connect_dedicated` 按库覆盖成多个会话
///   （`effective_config.database` 被改写而 `id` 不变），此时两个数据库**不是**同一
///   个物理端点。把 `connection_id` 当重叠键会让合法的跨库搬运带着一句事实错误的
///   拒绝信息（见 [`detect_endpoint_overlap`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRef {
    pub connection_id: ConnectionId,
    pub service_key: String,
    pub objects: Vec<String>,
    pub role: EndpointRole,
}

/// 端点的物理服务身份；空串意味着身份不可证明。
///
/// 归一化（trim + 小写）：调用方拼出的键不该因为大小写或空白差异而漏判。
///
/// **刻意只此一种键。** 曾并集过 `connection_id`，那是把「记账键」当成「位置键」：
/// 它只能把一次 accept 变成 reject，永远不能把 reject 变成 accept——即它只会误伤
/// 合法搬运而对真实自覆盖零增益，因为它看见的「同一连接」已被 `database` 维度拆开。
fn identity_key(ep: &EndpointRef) -> Option<String> {
    let service_key = ep.service_key.trim();
    (!service_key.is_empty()).then(|| service_key.to_ascii_lowercase())
}

/// 检测危险重叠：同一物理服务下，任一 reader 的读对象与任一 writer 的写对象存在
/// 交集即 `EndpointOverlap`。「同一物理服务」= 两端共享同一个 `service_key`
/// （见 [`identity_key`]），因此同名对象落在**两个不同**物理端点上是允许的。
///
/// 任一端给不出 `service_key`（身份不可证明）且存在 writer 时同样拒绝——不开放
/// 危险自覆盖。
pub fn detect_endpoint_overlap(endpoints: &[EndpointRef]) -> Result<(), JobError> {
    // (身份键, 对象, 读端点连接 id)：读端点 id 随读条目一起存下来，命中时才能
    // 在错误里点名「哪两端」。
    let mut reads: HashSet<(String, String, ConnectionId)> = HashSet::new();
    let mut has_writer = false;
    let mut any_unprovable = false;
    for ep in endpoints {
        let key = identity_key(ep);
        if key.is_none() {
            any_unprovable = true;
        }
        match ep.role {
            EndpointRole::SourceReader => {
                for obj in &ep.objects {
                    if let Some(key) = &key {
                        reads.insert((key.clone(), obj.clone(), ep.connection_id.clone()));
                    }
                }
            }
            EndpointRole::TargetWriter => {
                has_writer = true;
                for obj in &ep.objects {
                    if let Some(key) = &key {
                        let hit = reads
                            .iter()
                            .find(|(read_key, read_obj, _)| read_key == key && read_obj == obj)
                            .map(|(_, _, reader)| reader);
                        if let Some(reader) = hit {
                            return Err(JobError::EndpointOverlap(format!(
                                "object `{obj}` is both read and written on the same physical \
                                 endpoint (read by connection `{}`, written by connection `{}`)",
                                reader.as_str(),
                                ep.connection_id.as_str()
                            )));
                        }
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
            .map(|record| guard.release(&record.to_port_permit(), consumed, self.now_ms))
            .collect()
    }
}
