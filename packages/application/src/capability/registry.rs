//! 运行时能力注册表：声明 → 降低 → 取用。缺能力一律 [`ApiError`]，绝不 no-op 成功。
//!
//! ## 三段式
//!
//! 1. **声明**（[`CapabilityRegistry::declare`]）：provider 注册表形状与 driver 静态声明
//!    在此汇合。默认注册表**全空**——所有键落在 [`CapabilityState::Unavailable`]。
//! 2. **降低**（[`CapabilityRegistry::lower`]）：运行期探测、握手结果、弱保证档位在这里
//!    落账，每降低一次都写一条 [`DegradationRecord`] 并抬高 `revision`。
//!    **探测只能降低**（`system-overview.md:296`）：[`CapabilityRegistry::confirm`] 对
//!    [`DegradationCause::RuntimeProbeLowered`] 直接报错，不给抬回去的入口。
//! 3. **取用**（[`CapabilityRegistry::resolve`]）：唯一能把能力变成 [`CapabilityGrant`]
//!    的地方。不可用 ⇒ [`ApiErrorCode::CapabilityUnsupported`]，没有第三条路。
//!
//! ## 为什么不在这里解析 driver-api 的 `CapabilitySet`
//!
//! 依赖图 `APP → PA` 是 P1 固化的；`driver-capability-migration.md` §6.2 也把「把端口
//! 接上实现」判给组装层。因此声明来源做成注入闭包 [`Declarations`]，由组装层把
//! `DatabaseDriverFactory::resource_capabilities()` 桥接进来。本 crate 不认识 SQLx、
//! Tokio、HTTP 或 Tauri 类型，也不因为某个驱动声明了能力就相信它——桥接方负责把
//! 静态声明与探测结果一起喂进来。
//!
//! ## 降级不是成功
//!
//! [`CapabilityRegistry::require`] 是严格入口，连降级都不接受。
//! [`CapabilityRegistry::resolve`] 允许接受降级，但只在 [`DegradePolicy`] **同时点名**
//! 键和 [`DegradationCause`] 时才接受：宽泛的「接受一切降级」会让
//! `RuntimeProbeLowered` 悄悄混过去。接受到的 [`CapabilityGrant::Degraded`] 只能折算出
//! [`CapabilityCompletion::Partial`]，写不出 [`CapabilityCompletion::Full`]。

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::error::{ApiError, ApiErrorCode};

use super::domain::{
    CapabilityCompletion, CapabilityGrant, CapabilityKey, CapabilityRegistrationError,
    CapabilityState, DegradationCause, StateMap,
};
use super::snapshot::{CapabilitySnapshot, DegradationRecord};

/// 声明来源：`connectionId → 该连接实际支持的键集合`。
///
/// 返回 `None` 表示连接不可解析。此时**什么都不注册**——所有键留在
/// [`CapabilityState::Unavailable`]，任何取用都是 `CapabilityUnsupported`。
pub type Declarations = Arc<dyn Fn(&str) -> Option<Vec<CapabilityKey>> + Send + Sync>;

/// 调用方愿意接受哪些降级。
///
/// 必须是 `(键, 原因)` 的**成对**白名单。`DegradePolicy::strict()` 是默认值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DegradePolicy {
    accepted: BTreeSet<(CapabilityKey, DegradationCause)>,
}

impl DegradePolicy {
    /// 不接受任何降级：只有 [`CapabilityState::Available`] 才拿到授权。
    pub fn strict() -> Self {
        Self::default()
    }

    /// 点名接受某一条降级。
    pub fn accepting(mut self, key: CapabilityKey, cause: DegradationCause) -> Self {
        self.accepted.insert((key, cause));
        self
    }

    pub fn allows(&self, key: CapabilityKey, cause: DegradationCause) -> bool {
        self.accepted.contains(&(key, cause))
    }
}

/// 运行期能力注册表。
#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    provider_id: String,
    states: StateMap,
    degradations: Vec<DegradationRecord>,
    revision: u64,
}

impl CapabilityRegistry {
    /// 建一个**全空**注册表：18 个键全部 [`CapabilityState::Unavailable`]。
    ///
    /// 空注册表不是「什么都没配」，是「什么都不支持」——这是失败关闭的默认值。
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            states: StateMap::new(),
            degradations: Vec::new(),
            revision: 0,
        }
    }

    /// 从注入的声明源装配一个连接的全部键。
    ///
    /// 源返回 `None` 时注册表保持全空并 `Ok` 返回：这不是 no-op 成功，而是把
    /// 「不知道」如实落成「全不可用」，后续每次取用都会显式失败。
    pub fn from_declarations(
        provider_id: impl Into<String>,
        connection_id: &str,
        declarations: &Declarations,
    ) -> Result<Self, CapabilityRegistrationError> {
        let mut registry = Self::new(provider_id);
        if let Some(keys) = declarations(connection_id) {
            registry.declare_all(keys)?;
        }
        Ok(registry)
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// 单调递增的修订号。每次状态变更 +1，快照比较靠它定序。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 声明一个键可用。蕴含的前置键必须已经声明，否则整批失败。
    ///
    /// 蕴含缺失时**不**自动补登：自动补登会把「我以为 coordinated 因此也就有
    /// perDatabase」这种推测固化成事实，而计划第 99 行要的正是揭穿这类推测。
    pub fn declare(&mut self, key: CapabilityKey) -> Result<(), CapabilityRegistrationError> {
        if let Some(weak) = key.implied_by() {
            if self.states.get(&weak).copied() != Some(CapabilityState::Available) {
                return Err(CapabilityRegistrationError::IncompleteImplication {
                    strong: key.as_str(),
                    weak: weak.as_str(),
                });
            }
        }
        if self.states.get(&key).copied() == Some(CapabilityState::Available) {
            return Err(CapabilityRegistrationError::AlreadyConfirmed { key: key.as_str() });
        }
        self.states.insert(key, CapabilityState::Available);
        self.revision += 1;
        Ok(())
    }

    /// 批量声明。**全有或全无**：任何一条违反蕴含就整体失败，注册表保持原样。
    pub fn declare_all(
        &mut self,
        keys: impl IntoIterator<Item = CapabilityKey>,
    ) -> Result<(), CapabilityRegistrationError> {
        let mut staged = self.clone();
        for key in keys {
            staged.declare(key)?;
        }
        self.states = staged.states;
        self.revision = staged.revision;
        Ok(())
    }

    /// 把一个已声明的键降级。降级原因必须明确，否则调用方无法标注 `partial`。
    ///
    /// 从未声明过的键不能降级：`Unavailable → Degraded` 是一次**提升**（部分可用强于
    /// 完全不可用），不是降低。这种情况直接报错，状态不变。
    pub fn lower(
        &mut self,
        key: CapabilityKey,
        cause: DegradationCause,
    ) -> Result<(), CapabilityRegistrationError> {
        match self.states.get(&key).copied() {
            None | Some(CapabilityState::Unavailable) => {
                Err(CapabilityRegistrationError::LowerUndeclared { key: key.as_str() })
            }
            Some(_) => {
                self.states.insert(key, CapabilityState::Degraded { cause });
                self.degradations.push(DegradationRecord {
                    key,
                    cause,
                    at_revision: self.revision + 1,
                });
                self.revision += 1;
                Ok(())
            }
        }
    }

    /// 凭运行期证据把降级重新确认到 Full。
    ///
    /// 被 [`DegradationCause::RuntimeProbeLowered`] 降过的键**不可**抬回：
    /// `system-overview.md:296` 明文「运行时探测可降低静态声明，不能无依据提升权限」。
    /// 从未声明过的键同样拒绝——那是从零跳到 Full。
    pub fn confirm(&mut self, key: CapabilityKey) -> Result<(), CapabilityRegistrationError> {
        match self.states.get(&key).copied() {
            None | Some(CapabilityState::Unavailable) => {
                Err(CapabilityRegistrationError::ConfirmUndeclared { key: key.as_str() })
            }
            Some(CapabilityState::Available) => {
                Err(CapabilityRegistrationError::AlreadyConfirmed { key: key.as_str() })
            }
            Some(CapabilityState::Degraded { cause }) if cause.is_reconfirmable() => {
                self.states.insert(key, CapabilityState::Available);
                self.revision += 1;
                Ok(())
            }
            Some(CapabilityState::Degraded { .. }) => {
                Err(CapabilityRegistrationError::Unreconfirmable { key: key.as_str() })
            }
        }
    }

    pub fn state(&self, key: CapabilityKey) -> CapabilityState {
        self.states.get(&key).copied().unwrap_or_default()
    }

    /// 已登记的键（不含从未登记的键——它们就是 `Unavailable`）。
    pub fn states(&self) -> impl Iterator<Item = (CapabilityKey, CapabilityState)> + '_ {
        self.states.iter().map(|(key, state)| (*key, *state))
    }

    /// 降级流水，按发生顺序。
    pub fn degradations(&self) -> &[DegradationRecord] {
        &self.degradations
    }

    /// 取一份可记录、可比较的快照。
    pub fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot {
            provider_id: self.provider_id.clone(),
            revision: self.revision,
            states: self
                .states
                .iter()
                .map(|(key, state)| (key.as_str().to_string(), *state))
                .collect(),
            degradations: self.degradations.clone(),
        }
    }

    /// **严格**取用：只有 [`CapabilityState::Available`] 给 Full 授权。
    ///
    /// 降级与不可用一律 [`ApiErrorCode::CapabilityUnsupported`]。
    /// 调用方用它就等于承诺「这条路径不接受弱保证」。
    pub fn require(&self, key: CapabilityKey) -> Result<CapabilityGrant, ApiError> {
        self.resolve(key, &DegradePolicy::strict())
    }

    /// 按策略取用。
    ///
    /// [`CapabilityState::Unavailable`] 永远失败——`DegradePolicy` 里没有、
    /// 也**不能**有「未声明即忽略」的选项。
    pub fn resolve(
        &self,
        key: CapabilityKey,
        policy: &DegradePolicy,
    ) -> Result<CapabilityGrant, ApiError> {
        match self.state(key) {
            CapabilityState::Available => Ok(CapabilityGrant::Full { key }),
            CapabilityState::Degraded { cause } => self.grant(key, cause, policy),
            CapabilityState::Unavailable => Err(self.unsupported(key, None)),
        }
    }

    /// 对某个类别做一次整体闸门：类别里只要有一个键不可用就整体失败。
    ///
    /// 存在的理由是「批量操作」——例如一次备份 Job 需要
    /// [`CapabilityKey::RowRead`] + [`CapabilityKey::BackupArtifact`]，
    /// 缺一个却按降级继续产出会产出一个不完整的备份，而且**看起来是成功的**。
    pub fn require_all(&self, keys: &[CapabilityKey]) -> Result<Vec<CapabilityGrant>, ApiError> {
        let mut grants = Vec::with_capacity(keys.len());
        for key in keys {
            grants.push(self.require(*key)?);
        }
        Ok(grants)
    }

    fn grant(
        &self,
        key: CapabilityKey,
        cause: DegradationCause,
        policy: &DegradePolicy,
    ) -> Result<CapabilityGrant, ApiError> {
        if policy.allows(key, cause) {
            Ok(CapabilityGrant::Degraded { key, cause })
        } else {
            Err(self.unsupported(key, Some(cause)))
        }
    }

    fn unsupported(&self, key: CapabilityKey, cause: Option<DegradationCause>) -> ApiError {
        // 脱敏文案：只拼 provider 标识与能力字面值，不带驱动内部错误、不带连接串。
        let detail = match cause {
            Some(cause) => format!("{key}（降级原因 {cause:?}，调用方未接受）"),
            None => format!("{key}（未声明或已被运行期否决）"),
        };
        ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            format!("provider {} 不提供所需能力 {detail}", self.provider_id),
        )
    }
}

/// 把一组授权折算成唯一的完成态。任一降级授权都会把整体拉成 `Partial`。
///
/// 这是「降级不得呈现为成功」的收口处：批量路径即使全拿到授权，只要有一个是降级，
/// 完成态就是 [`CapabilityCompletion::Partial`]。
pub fn completion_of(grants: &[CapabilityGrant]) -> CapabilityCompletion {
    grants
        .iter()
        .filter_map(|grant| grant.cause())
        .min()
        .map_or(CapabilityCompletion::Full, CapabilityCompletion::Partial)
}
