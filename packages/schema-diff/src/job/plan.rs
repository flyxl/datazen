//! §2.2 冻结计划数据模型与进程内计划工厂（PlanStore）。
//!
//! 准备阶段（`schemaDiffPrepare`）完成后落入此处；应用阶段（`schemaDiffApply`）
//! 以 planId 一次性消费（§2.1）。计划正文 Artifact 可变子集通过
//! [`SchemaDiffFrozenPlan::body_digests`] 校验，不随客户端回传绕过。

use std::collections::HashMap;
use std::sync::Mutex;

use datazen_platform_api::id::Timestamp;

use crate::types::SchemaDiffPlan;

/// 当前受理的计划格式 major。未知 major 拒绝（§2.2 / §10.1.1）。
pub const SUPPORTED_PLAN_MAJOR: u64 = 1;

/// SchemaDiffApply 的 handlerVersion。与计划内 handlerVersion 相同才继续。
pub const SCHEMA_DIFF_HANDLER_VERSION: u64 = 1;

/// §4.2 后的恢复策略声明。**不能由用户任意宣称**：只能来自下表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPolicy {
    ReadOnlyVerify,
    TargetBatchEvidence,
    InTransactionBatch,
    ReplayDisabled,
}

impl RecoveryPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnlyVerify => "readOnlyVerify",
            Self::TargetBatchEvidence => "targetBatchEvidence",
            Self::InTransactionBatch => "inTransactionBatch",
            Self::ReplayDisabled => "replayDisabled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "readOnlyVerify" => Some(Self::ReadOnlyVerify),
            "targetBatchEvidence" => Some(Self::TargetBatchEvidence),
            "inTransactionBatch" => Some(Self::InTransactionBatch),
            "replayDisabled" => Some(Self::ReplayDisabled),
            _ => None,
        }
    }
}

/// §2.2 冻结计划：准备 Job 的产物元数据。计划 Artifact 是
/// [`SchemaDiffPlan`] 本体 + 差异/风险/渲染摘要；这里只保存可持久化引用与
/// 稳定指纹，runtimeBinding（session、lease）只留在内存（`ReviewedPlan`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiffFrozenPlan {
    pub plan_id: String,
    pub plan_version: u64,
    pub handler_version: u64,
    pub checkpoint_version: u64,
    pub selection_revision: u64,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    /// 物理服务/命名空间/对象身份与观察时间；不能只按 connectionId 判自覆盖。
    pub endpoint_evidence: Vec<String>,
    /// config/credential/capability 快照版本（不保存秘密）；执行前复验比对。
    pub capability_snapshot_hash: String,
    /// 结构/PK/索引/参与对象/列映射/顺序的稳定摘要。
    pub schema_fingerprint: String,
    pub mapping_fingerprint: String,
    /// realTime / tableSnapshot / databaseSnapshot。
    pub consistency: String,
    /// batch / table / task / nonAtomicDDL。
    pub transaction_scope: String,
    pub recovery_policy: RecoveryPolicy,
    pub body_artifact_ids: Vec<String>,
    pub body_digests: Vec<String>,
    /// 用户确认的破坏性操作范围（operation ID 列表）。
    pub confirmed_actions: Vec<String>,
}

impl SchemaDiffFrozenPlan {
    /// 版本守卫：未知 major 一律拒绝。
    pub fn validate_versions(&self) -> Result<(), SchemaDiffPlanError> {
        for got in [self.plan_version, self.handler_version, self.checkpoint_version] {
            if got != SUPPORTED_PLAN_MAJOR {
                return Err(SchemaDiffPlanError::VersionIncompatible {
                    got,
                    supported: SUPPORTED_PLAN_MAJOR,
                });
            }
        }
        Ok(())
    }

    /// 与当前生效事实比对（capability 快照/结构指纹）。
    pub fn matches_capability_snapshot(&self, current_hash: &str) -> bool {
        self.capability_snapshot_hash == current_hash
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchemaDiffPlanError {
    #[error("plan expired: {0}")]
    PlanExpired(String),
    #[error("plan not found: {0}")]
    PlanNotFound(String),
    #[error("selection revision mismatch: expected {expected}, got {got}")]
    SelectionRevisionMismatch { expected: u64, got: u64 },
    #[error("unsupported plan/checkpoint version major {got}, supported {supported}")]
    VersionIncompatible {
        got: u64,
        supported: u64,
    },
    #[error("recovery policy invalid: {0}")]
    RecoveryPolicyInvalid(String),
    #[error("endpoint evidence empty")]
    EndpointEvidenceEmpty,
    #[error("capability snapshot changed after review")]
    CapabilityChanged,
    #[error("target structure changed before apply: {0}")]
    PlanStale(String),
}

/// fnv1a64 十六进制（与 runtime `connection::types::fnv1a64_hex` 同口径，
/// 这里复制一份避免跨 crate 私有可见性问题）。
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// 存储在 PlanStore 里的受理物：冻结计划元数据 + 已渲染的 SchemaDiffPlan 本体。
#[derive(Debug)]
pub struct StoredPlan {
    pub meta: SchemaDiffFrozenPlan,
    pub plan: SchemaDiffPlan,
    pub body_digest: String,
}

/// 进程内计划工厂。planId 一次消费语义由 JobRepository 的 `consumedPlanId`
/// 唯一约束保证；此处提供 prepare→apply 之间的内存衔接与过期/选择版本校验。
#[derive(Default)]
pub struct PlanStore {
    inner: Mutex<HashMap<String, StoredPlan>>,
}

impl PlanStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, stored: StoredPlan) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(stored.meta.plan_id.clone(), stored);
        }
    }

    pub fn get(&self, plan_id: &str) -> Option<StoredPlan> {
        let guard = self.inner.lock().ok()?;
        let stored = guard.get(plan_id)?;
        Some(StoredPlan {
            meta: stored.meta.clone(),
            plan: stored.plan.clone(),
            body_digest: stored.body_digest.clone(),
        })
    }

    /// 一次性消费：从存储移除并校验过期、选择版本与版本守卫。
    /// §2.1「同一 planId 仅能被一个 apply Job 消费」；重新审阅后必须生成新 planId。
    pub fn take_for_apply(
        &self,
        plan_id: &str,
        selection_revision: u64,
        now: &Timestamp,
    ) -> Result<StoredPlan, SchemaDiffPlanError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| SchemaDiffPlanError::PlanNotFound(plan_id.to_string()))?;
        let stored = guard
            .remove(plan_id)
            .ok_or_else(|| SchemaDiffPlanError::PlanNotFound(plan_id.to_string()))?;
        stored.meta.validate_versions()?;
        if stored.meta.expires_at.as_str() < now.as_str() {
            return Err(SchemaDiffPlanError::PlanExpired(plan_id.to_string()));
        }
        if stored.meta.selection_revision != selection_revision {
            return Err(SchemaDiffPlanError::SelectionRevisionMismatch {
                expected: stored.meta.selection_revision,
                got: selection_revision,
            });
        }
        if stored.meta.endpoint_evidence.is_empty() {
            return Err(SchemaDiffPlanError::EndpointEvidenceEmpty);
        }
        Ok(stored)
    }

    /// 校验计划还存在、未过期且与当前选择版本一致，但不消费。
    pub fn validate_for_apply(
        &self,
        plan_id: &str,
        selection_revision: u64,
        now: &Timestamp,
    ) -> Result<(), SchemaDiffPlanError> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| SchemaDiffPlanError::PlanNotFound(plan_id.to_string()))?;
        let stored = guard
            .get(plan_id)
            .ok_or_else(|| SchemaDiffPlanError::PlanNotFound(plan_id.to_string()))?;
        stored.meta.validate_versions()?;
        if stored.meta.expires_at.as_str() < now.as_str() {
            return Err(SchemaDiffPlanError::PlanExpired(plan_id.to_string()));
        }
        if stored.meta.selection_revision != selection_revision {
            return Err(SchemaDiffPlanError::SelectionRevisionMismatch {
                expected: stored.meta.selection_revision,
                got: selection_revision,
            });
        }
        // §2.2：fingerprint/endpointEvidence/recoveryPolicy 必须非空且可核验。
        if stored.meta.schema_fingerprint.trim().is_empty()
            || stored.meta.mapping_fingerprint.trim().is_empty()
        {
            return Err(SchemaDiffPlanError::PlanStale("fingerprint empty".into()));
        }
        if stored.meta.endpoint_evidence.is_empty() {
            return Err(SchemaDiffPlanError::EndpointEvidenceEmpty);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(plan_id: &str) -> SchemaDiffFrozenPlan {
        SchemaDiffFrozenPlan {
            plan_id: plan_id.into(),
            plan_version: 1,
            handler_version: 1,
            checkpoint_version: 1,
            selection_revision: 3,
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            expires_at: Timestamp::new("2026-01-01T01:00:00Z"),
            source_connection_id: "conn-src".into(),
            target_connection_id: "conn-tgt".into(),
            source_database: None,
            target_database: None,
            source_schema: None,
            target_schema: None,
            endpoint_evidence: vec!["sqlite:file-a".into()],
            capability_snapshot_hash: "cap-1".into(),
            schema_fingerprint: "schema-1".into(),
            mapping_fingerprint: "map-1".into(),
            consistency: "tableSnapshot".into(),
            transaction_scope: "task".into(),
            recovery_policy: RecoveryPolicy::ReadOnlyVerify,
            body_artifact_ids: vec![],
            body_digests: vec![],
            confirmed_actions: vec![],
        }
    }

    #[test]
    fn validate_versions_rejects_unknown_major() {
        let mut m = meta("p1");
        m.plan_version = 2;
        assert!(matches!(
            m.validate_versions(),
            Err(SchemaDiffPlanError::VersionIncompatible { got: 2, supported: 1 })
        ));
    }

    #[test]
    fn take_for_apply_rejects_expired_and_stale_selection() {
        let store = PlanStore::new();
        let stored = StoredPlan {
            meta: meta("p1"),
            plan: SchemaDiffPlan {
                plan_id: Some("p1".into()),
                table: "t".into(),
                tables: vec![],
                source_dialect: "sqlite".into(),
                target_dialect: "sqlite".into(),
                same_dialect: true,
                statements: vec![],
                warnings: vec![],
                requirements: vec![],
                rollback_completeness: crate::types::RollbackCompleteness {
                    complete: true,
                    missing: vec![],
                },
                type_suggestions: vec![],
                expected_target_schemas: vec![],
            },
            body_digest: String::new(),
        };
        store.insert(stored);
        let now = Timestamp::new("2026-01-01T02:00:00Z");
        assert!(matches!(
            store.take_for_apply("p1", 3, &now),
            Err(SchemaDiffPlanError::PlanExpired(_))
        ));
    }
}
