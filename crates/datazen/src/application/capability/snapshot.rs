//! 能力快照：**可记录、可比较**。
//!
//! ## 为什么必须有它
//!
//! 计划第 99 行的出口判据是「新增能力缺失不会 no-op 成功」。这条判据要能被证明，
//! 就必须回答一个问题：**这次请求到底比上一次多了什么、少了什么**。没有快照就只剩
//! 一句 `CapabilityUnsupported`，既证明不了「少了」，也证明不了「降了」——降级和
//! 缺失在错误文案上会完全一样，于是「新增能力缺失」和「新增能力降级」被混为一谈。
//!
//! 因此快照同时承担三件事：
//!
//! * **记录**：`states` 是当时每个键的运行期状态，`degradations` 是每一次降低的时间线
//!   （带 `at_revision`，可定序）。
//! * **比较**：[`CapabilitySnapshot::difference_from`] 给出并集上的逐键差异。
//! * **可复现**：`states` 的键是 [`CapabilityKey::as_str`] 稳定字面量，
//!   序列化后跨进程、跨版本可比。
//!
//! ## 快照不可作为授权来源
//!
//! 快照是**证据**不是**许可**。回放一份旧快照不能把能力抬回 Full：
//! [`CapabilityState::Degraded`] 在快照里仍然是 `Degraded`。要让降级重新变成 Full，
//! 只能走 [`CapabilityRegistry::confirm`](super::CapabilityRegistry::confirm) 拿到新的运行期证据。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::domain::{CapabilityKey, CapabilityState, DegradationCause};

/// 一次降级的时间线记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DegradationRecord {
    /// 被降级的键。
    pub key: CapabilityKey,
    /// 降级原因。
    pub cause: DegradationCause,
    /// 发生时注册表的修订号（用于定序与去重判断）。
    pub at_revision: u64,
}

/// 两个快照之间某个键的差异。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityChange {
    /// 键字面值。存字面量而不是枚举，是为了让**未知键也能被比较出来**——
    /// 来自更高版本快照的新能力不能因为本 crate 不认识就从差异里消失（那等于 no-op）。
    pub key: String,
    /// 旧状态。旧快照里没有这个键时是 [`CapabilityState::Unavailable`]——
    /// **不**是因为「未知」而是因为「没声明」。
    pub from: CapabilityState,
    pub to: CapabilityState,
}

impl CapabilityChange {
    /// 能被本 crate 识别的键。来自更高版本快照的键返回 `None`。
    pub fn typed_key(&self) -> Option<CapabilityKey> {
        CapabilityKey::from_wire(&self.key)
    }

    /// 这次差异是否让某项能力**退化**（新状态不比旧状态强）。
    pub const fn is_regression(&self) -> bool {
        capability_rank(self.from) >= capability_rank(self.to)
    }
}

/// 状态的强弱序。`Unavailable < Degraded < Available`，不可用最弱。
const fn capability_rank(state: CapabilityState) -> u8 {
    match state {
        CapabilityState::Unavailable => 0,
        CapabilityState::Degraded { .. } => 1,
        CapabilityState::Available => 2,
    }
}

/// 某一时刻的能力快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    /// 被观测的 provider（通常是 `connectionId`）。非敏感标识。
    pub provider_id: String,
    /// 快照对应的注册表修订号。
    pub revision: u64,
    /// 键字面值 → 状态。**只含已登记的键**；缺席即 [`CapabilityState::Unavailable`]。
    pub states: BTreeMap<String, CapabilityState>,
    /// 降级时间线。
    pub degradations: Vec<DegradationRecord>,
}

impl CapabilitySnapshot {
    /// 取某个键的状态。未登记返回 [`CapabilityState::Unavailable`]（失败关闭）。
    pub fn state_of(&self, key: CapabilityKey) -> CapabilityState {
        self.states.get(key.as_str()).copied().unwrap_or_default()
    }

    /// 两个快照的逐键差异，按键字面值排序。
    ///
    /// 方向是 `self → other`：`self` 是基线，`other` 是对照。
    /// 取键的并集，因此**只在其中一侧出现**的键也会被检查到——基线里没有的键在对照里
    /// 出现时读成 `Unavailable → Available`，不会被「基线里没这项」悄悄跳过。
    ///
    /// 只报**真的不同**的那些键：两侧同状态的键不是差异。报出来只会淹没真正的变化。
    pub fn difference_from(&self, other: &Self) -> Vec<CapabilityChange> {
        let keys: BTreeSet<&str> = self
            .states
            .keys()
            .map(String::as_str)
            .chain(other.states.keys().map(String::as_str))
            .collect();
        keys.into_iter()
            .map(|key| CapabilityChange {
                key: key.to_string(),
                from: self.state_of_key(key),
                to: other.state_of_key(key),
            })
            .filter(|change| change.from != change.to)
            .collect()
    }

    fn state_of_key(&self, key: &str) -> CapabilityState {
        self.states.get(key).copied().unwrap_or_default()
    }

    /// 能力形状是否一致（忽略 `revision` 与降级时间线）。
    ///
    /// 用于「换了 provider 或换了版本之后能力有没有变」的判定。
    pub fn same_capabilities_as(&self, other: &Self) -> bool {
        let mut keys: BTreeSet<&str> = self
            .states
            .keys()
            .map(String::as_str)
            .chain(other.states.keys().map(String::as_str))
            .collect();
        keys.retain(|key| self.state_of_key(key) != other.state_of_key(key));
        keys.is_empty()
    }

    /// 本快照相对一个**全不可用**基线，带来了哪些差异。
    ///
    /// 计划第 99 行的直接对应物：把「这个 provider 有能力」与「连基础能力都没有」
    /// 摆在一起比，新增项与缺失项一眼可见。
    pub fn difference_from_nothing(&self) -> Vec<CapabilityChange> {
        let empty = Self {
            provider_id: self.provider_id.clone(),
            revision: 0,
            states: BTreeMap::new(),
            degradations: Vec::new(),
        };
        self.difference_from(&empty)
    }

    /// 快照里所有降级记录，按发生顺序（`at_revision` 升序）。
    pub fn degradation_timeline(&self) -> Vec<DegradationRecord> {
        let mut timeline = self.degradations.clone();
        timeline.sort_by_key(|record| (record.at_revision, record.key));
        timeline
    }

    /// 该快照允许的完整能力集合（严格取用下真正能开功能的键）。
    pub fn confirmed_keys(&self) -> Vec<CapabilityKey> {
        CapabilityKey::ALL
            .into_iter()
            .filter(|key| self.state_of(*key) == CapabilityState::Available)
            .collect()
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
