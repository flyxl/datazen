//! 六类可选能力的词汇表（P2 计划 `platform-development-plan.md:93`）。
//!
//! ## 为什么这六类
//!
//! 计划第 93 行把 P2 的第二项交付写成「namespace / session / transaction /
//! snapshot / data / backup 可选能力注册与运行时能力降低机制」。这是**用例层**要判定
//! 「能不能做」的对象清单，因此词汇表落在本 crate，而不是 driver-api。
//!
//! 注意与 `system-overview.md:301` 的九项清单（namespace/session/transaction/catalog/
//! migration/snapshot/data-read/data-write/backup）的关系：后者是底座视角的粗粒度枚举。
//! 本表按计划第 93 行收成六类，其中 `data-read` / `data-write` 合并为 [`CapabilityDomain::Data`]。
//! `catalog` / `migration` 在六类里没有对应键——它们由 P3 的 JobRuntime 与
//! `shared-boundaries-and-ports.md` 的 Command 契约承担，不在本轨的判定面上。
//! 两者不一致这一点已上报 Lead 裁决，本文件不擅自扩张到九类。
//!
//! ## 失败关闭的三条编码规则
//!
//! 1. [`CapabilityState`] 的 [`Default`] 是 [`CapabilityState::Unavailable`]。
//!    任何**没被显式声明**的能力都落在「不可用」上，而不是「可用」。
//! 2. [`CapabilityState::enables_feature`] 只对 [`CapabilityState::Available`] 为真。
//!    [`CapabilityState::Degraded`] **不**开启功能——降级路径必须由调用方显式接受
//!    （[`CapabilityRegistry::resolve`]），不接受就是错误。依据 `driver-capability-migration.md`
//!    §6.1：降级路径「必须在 UI 与 effectOutcome 上标出 partial 或 unknown，不得呈现为成功」。
//! 3. [`CapabilityCompletion`] 没有 `Default`，构造它的唯一入口是 [`CapabilityGrant`]。
//!    没有 Full 授权就写不出 `Full` 完成态。
//!
//! 依据：连接 §5.2 能力粒度表、`driver-capability-migration.md` §6.1、计划第 93/99 行。

use std::collections::BTreeMap;
use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// 六类可选能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityDomain {
    Namespace,
    Session,
    Transaction,
    Snapshot,
    Data,
    Backup,
}

impl CapabilityDomain {
    /// 全部六类。顺序即计划第 93 行的书写顺序，不得重排（测试固化）。
    pub const ALL: [CapabilityDomain; 6] = [
        Self::Namespace,
        Self::Session,
        Self::Transaction,
        Self::Snapshot,
        Self::Data,
        Self::Backup,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Session => "session",
            Self::Transaction => "transaction",
            Self::Snapshot => "snapshot",
            Self::Data => "data",
            Self::Backup => "backup",
        }
    }
}

/// 一条具体的可选能力。
///
/// 封闭枚举是刻意的：计划第 99 行要求「新增能力缺失不会 no-op 成功」。新增一个键时，
/// 默认注册表里它必然是 [`CapabilityState::Unavailable`]，任何忘记实现降级的新调用点
/// 会在第一次测试时炸成 `CapabilityUnsupported`，而不是悄悄 `Ok(())`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapabilityKey {
    // ── namespace ──
    /// 同一会话原地切换命名空间，不重建底层连接（连接 §5.2 `namespaceSwitch = inPlace`）。
    NamespaceInPlaceSwitch,
    /// 可切换，但必须替换底层资源（连接 §5.2 `namespaceSwitch = requiresReplacement`）。
    NamespaceReplacementSwitch,
    // ── session ──
    /// 驱动真的持有会话级物理连接（连接 §5.2 `statefulSession`）。
    ///
    /// 「池大小为 1」**不是**证据（`driver-capability-migration.md` §6.1 禁止条款）。
    StatefulSession,
    /// 能完整读回会话上下文（连接 §5.2 `contextObservation = full`）。
    ContextObservation,
    /// 交出会话级句柄：事务、游标、服务端预处理句柄（连接 §5.2 `sessionScopedHandles`）。
    SessionScopedHandles,
    /// 能把资源复位到初始化基线并给出验证结论（连接 §5.2 `resetForReuse = verified`）。
    ResetForReuse,
    /// 精确取消单次执行（连接 §5.2 `preciseCancel`）。会话级 cancel 不是它。
    PreciseCancel,
    // ── transaction ──
    /// 能报告事务状态（连接 §5.2 `transactionObservation = full`）。
    TransactionObservation,
    /// 接受调用方指定的隔离级别（连接 §5.2 `transactions` 的隔离级别部分）。
    IsolationLevel,
    /// 开启事务后支持保存点（连接 §5.2 `transactions` 的 savepoint 部分）。
    Savepoints,
    // ── snapshot ──
    /// 单表一致性快照（连接 §5.2 `snapshots = perTable`）。
    SnapshotPerTable,
    /// 库级一致性快照（连接 §5.2 `snapshots = perDatabase`）。
    SnapshotPerDatabase,
    /// 跨库协调快照（连接 §5.2 `snapshots = coordinated`）。
    SnapshotCoordinated,
    // ── data ──
    /// 读取数据行（`system-overview.md:301` 的 `data-read`）。
    RowRead,
    /// 写入数据行（`system-overview.md:301` 的 `data-write`）。
    RowWrite,
    /// 流式返回结果集（`factory.rs:41` 的 `supports_streaming_results`）。
    StreamingResults,
    // ── backup ──
    /// 能由 Job 产出备份产物并进 ArtifactStore（`team-server-and-auth.md:629`）。
    BackupArtifact,
    /// 能从备份产物恢复。**从不**意味着活动数据库会话可恢复
    /// （`persistence-model.md` §7.2 与发布门槛原文）。
    RestoreFromArtifact,
}

impl CapabilityKey {
    /// 全部可选能力。新增键必须同时加进这个数组，否则 [`Self::domain`] 与快照覆盖面会漏。
    pub const ALL: [CapabilityKey; 18] = [
        Self::NamespaceInPlaceSwitch,
        Self::NamespaceReplacementSwitch,
        Self::StatefulSession,
        Self::ContextObservation,
        Self::SessionScopedHandles,
        Self::ResetForReuse,
        Self::PreciseCancel,
        Self::TransactionObservation,
        Self::IsolationLevel,
        Self::Savepoints,
        Self::SnapshotPerTable,
        Self::SnapshotPerDatabase,
        Self::SnapshotCoordinated,
        Self::RowRead,
        Self::RowWrite,
        Self::StreamingResults,
        Self::BackupArtifact,
        Self::RestoreFromArtifact,
    ];

    /// 该键归属的类别。`CapabilityKey::ALL` 必须覆盖全部六类（测试固化）。
    pub const fn domain(self) -> CapabilityDomain {
        match self {
            Self::NamespaceInPlaceSwitch | Self::NamespaceReplacementSwitch => {
                CapabilityDomain::Namespace
            }
            Self::StatefulSession
            | Self::ContextObservation
            | Self::SessionScopedHandles
            | Self::ResetForReuse
            | Self::PreciseCancel => CapabilityDomain::Session,
            Self::TransactionObservation | Self::IsolationLevel | Self::Savepoints => {
                CapabilityDomain::Transaction
            }
            Self::SnapshotPerTable | Self::SnapshotPerDatabase | Self::SnapshotCoordinated => {
                CapabilityDomain::Snapshot
            }
            Self::RowRead | Self::RowWrite | Self::StreamingResults => CapabilityDomain::Data,
            Self::BackupArtifact | Self::RestoreFromArtifact => CapabilityDomain::Backup,
        }
    }

    /// 协议字面值（快照与差异记录用它做键）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NamespaceInPlaceSwitch => "namespace.inPlaceSwitch",
            Self::NamespaceReplacementSwitch => "namespace.replacementSwitch",
            Self::StatefulSession => "session.stateful",
            Self::ContextObservation => "session.contextObservation",
            Self::SessionScopedHandles => "session.scopedHandles",
            Self::ResetForReuse => "session.resetForReuse",
            Self::PreciseCancel => "session.preciseCancel",
            Self::TransactionObservation => "transaction.observation",
            Self::IsolationLevel => "transaction.isolationLevel",
            Self::Savepoints => "transaction.savepoints",
            Self::SnapshotPerTable => "snapshot.perTable",
            Self::SnapshotPerDatabase => "snapshot.perDatabase",
            Self::SnapshotCoordinated => "snapshot.coordinated",
            Self::RowRead => "data.rowRead",
            Self::RowWrite => "data.rowWrite",
            Self::StreamingResults => "data.streamingResults",
            Self::BackupArtifact => "backup.artifact",
            Self::RestoreFromArtifact => "backup.restoreFromArtifact",
        }
    }

    /// 反查。未登记的字面量返回 `None`，调用方必须失败关闭而不是跳过。
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.as_str() == value)
    }

    /// 声明该键时必须一并声明的前置键（蕴含关系）。
    ///
    /// 蕴含是**单调**的：拿到强保证必然拿到弱保证。缺失蕴含登记不是「宽松一点」，
    /// 而是让快照能说谎（例如声称 coordinated 却查不到 perDatabase），
    /// 因此 [`CapabilityRegistry::declare`](super::CapabilityRegistry::declare) 直接拒绝。
    pub const fn implied_by(self) -> Option<CapabilityKey> {
        match self {
            // 原地切库要求会话真的被固定住，否则"原地"没有意义。
            Self::NamespaceInPlaceSwitch => Some(Self::StatefulSession),
            // 句柄的生存期是资源生存期，没有会话就没有句柄。
            Self::SessionScopedHandles => Some(Self::StatefulSession),
            // snapshot 档位单调：coordinated ⊃ perDatabase ⊃ perTable。
            Self::SnapshotPerDatabase => Some(Self::SnapshotPerTable),
            Self::SnapshotCoordinated => Some(Self::SnapshotPerDatabase),
            _ => None,
        }
    }

    /// 该键的弱一档兄弟（供降低时定位下一步）。没有更强档位时为 `None`。
    pub const fn weaker(self) -> Option<CapabilityKey> {
        match self {
            Self::NamespaceInPlaceSwitch => Some(Self::NamespaceReplacementSwitch),
            Self::SnapshotCoordinated => Some(Self::SnapshotPerDatabase),
            Self::SnapshotPerDatabase => Some(Self::SnapshotPerTable),
            _ => None,
        }
    }
}

impl fmt::Display for CapabilityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 能力键在线路上的形状就是 [`CapabilityKey::as_str`]，不是变体名。
///
/// 变体名是 Rust 私有细节；线上要能被别的进程、别的版本读懂。反序列化遇到未知字面量
/// **报错**而不是落到某个默认变体——落到默认变体等于凭空造出一项能力。
impl Serialize for CapabilityKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CapabilityKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <&str as Deserialize>::deserialize(deserializer)?;
        Self::from_wire(raw)
            .ok_or_else(|| de::Error::custom(format!("unknown capability key `{raw}`")))
    }
}

/// 降级原因。降级**必须**带原因，否则调用方无法在 UI 与 `effectOutcome` 上标注。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DegradationCause {
    /// 只有静态声明，尚无运行期证据（`describeResource` 说有，实际连接尚未验过）。
    DeclaredOnly,
    /// 运行期探测把静态声明降低了。探测**只能**降低，不能提升
    /// （`system-overview.md:296`）。
    RuntimeProbeLowered,
    /// 只拿到弱一档保证（例如 `snapshots` 只到 `perDatabase`）。
    WeakerGuarantee,
}

impl DegradationCause {
    /// **不得**提供默认；需要默认时按最严格的一档处理。
    ///
    /// 理由：[`CapabilityRegistry::confirm`] 依据 cause 决定能否重新确认到
    /// [`CapabilityState::Available`]。来源不明的降级若默认成 `DeclaredOnly`，
    /// 就会被当作「只是还没探测」而重新抬回 Full——正是「无依据提升权限」。
    pub const fn strict_default() -> Self {
        Self::RuntimeProbeLowered
    }

    /// 该原因是否允许后续由运行期证据重新确认到 Full。
    pub const fn is_reconfirmable(self) -> bool {
        matches!(self, Self::DeclaredOnly | Self::WeakerGuarantee)
    }
}

/// 单条能力的运行期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityState {
    /// 静态声明 + 运行期确认，完整保证。
    Available,
    /// 声明过，但运行期只能给出弱保证。**不是成功**。
    Degraded { cause: DegradationCause },
    /// 没有声明，或声明被运行期否决。
    Unavailable,
}

impl CapabilityState {
    /// 只有完整保证才开启功能。`Degraded` 不算。
    pub const fn enables_feature(self) -> bool {
        matches!(self, Self::Available)
    }

    pub const fn is_degraded(self) -> bool {
        matches!(self, Self::Degraded { .. })
    }

    pub const fn is_unavailable(self) -> bool {
        matches!(self, Self::Unavailable)
    }

    pub const fn cause(self) -> Option<DegradationCause> {
        match self {
            Self::Degraded { cause } => Some(cause),
            _ => None,
        }
    }
}

impl Default for CapabilityState {
    /// 失败关闭：默认不可用。
    fn default() -> Self {
        Self::Unavailable
    }
}

/// 拿到授权后允许上报的完成态。
///
/// **没有 `Default`**：构造它的唯一入口是 [`CapabilityGrant::completion`]。
/// 写代码时想直接造一个「完成」是造不出来的——必须先有授权。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityCompletion {
    /// 完整完成。
    Full,
    /// 部分完成，必须带上原因。依据 `driver-capability-migration.md` §6.1：
    /// 降级路径要标 `partial` 或 `unknown`，不得呈现为成功。
    Partial(DegradationCause),
}

impl CapabilityCompletion {
    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// 一次能力取用的授权结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityGrant {
    /// 完整授权。
    Full { key: CapabilityKey },
    /// 显式接受了降级。`cause` 必须一路带到完成态。
    Degraded {
        key: CapabilityKey,
        cause: DegradationCause,
    },
}

impl CapabilityGrant {
    pub const fn key(self) -> CapabilityKey {
        match self {
            Self::Full { key } | Self::Degraded { key, .. } => key,
        }
    }

    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full { .. })
    }

    pub const fn cause(self) -> Option<DegradationCause> {
        match self {
            Self::Full { .. } => None,
            Self::Degraded { cause, .. } => Some(cause),
        }
    }

    /// 授权折算成完成态。降级授权**不可能**产出 `Full`。
    pub const fn completion(self) -> CapabilityCompletion {
        match self {
            Self::Full { .. } => CapabilityCompletion::Full,
            Self::Degraded { cause, .. } => CapabilityCompletion::Partial(cause),
        }
    }
}

/// 注册期的不变量违反。这些是**编程错误**（注册表建错了），不是运行期请求失败，
/// 因此不混进 [`ApiError`](crate::error::ApiError)。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityRegistrationError {
    /// 试图把从未声明过的键降级为「降级」——那是把不可用抬高成部分可用。
    #[error("cannot lower capability `{key}`: it was never declared (state stays unavailable)")]
    LowerUndeclared {
        /// 目标键。
        key: &'static str,
    },
    /// 蕴含缺失。强保证必须连带声明弱保证。
    #[error("capability `{strong}` implies `{weak}`: declare `{weak}` first or not at all")]
    IncompleteImplication {
        /// 声明了的那一个。
        strong: &'static str,
        /// 被蕴含但未声明的那一个。
        weak: &'static str,
    },
    /// 试图把从未声明过的键直接确认为 Full——那是「无依据提升权限」。
    #[error("cannot confirm capability `{key}`: it was never declared")]
    ConfirmUndeclared {
        /// 被确认的键。
        key: &'static str,
    },
    /// 运行期探测已经否决过的键，不允许再凭后续证据抬回 Full。
    #[error("capability `{key}` was lowered by runtime probe and cannot be raised again")]
    Unreconfirmable {
        /// 被抬回的键。
        key: &'static str,
    },
    /// 键已经是 Full，重复确认是注册表建错了。
    #[error("capability `{key}` is already fully confirmed")]
    AlreadyConfirmed {
        /// 重复确认的键。
        key: &'static str,
    },
}

/// `CapabilityKey → 状态` 的类型化视图，快照内部用它。
pub(crate) type StateMap = BTreeMap<CapabilityKey, CapabilityState>;
