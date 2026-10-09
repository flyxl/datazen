//! 任务契约：视图、内部记录与 checkpoint。
//!
//! §4.2 的四张记录表里，Job 侧的必要字段直接落成本模块的类型。
//! **硬规则**：checkpoint 只保存稳定目标/版本/映射指纹、已提交边界、核验证据与恢复策略，
//! **不保存 live session / lease / cursor**（§4.2 `JobCheckpoint`）。

use serde::{Deserialize, Serialize};

use crate::dto::execution::EffectOutcome;
use crate::id::{ArtifactId, BlockId, Counter, ExecutionId, JobId, StageId, Timestamp, WorkerId};
use crate::OwnerRef;

/// 任务状态。与 `ExecutionState` 是**不同**的状态机：任务状态不含 `cancelRequested`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// P5 阶段进度：五类计数彼此独立，未确认 commit 的行不得计入 `committed`（§2.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub read: Counter,
    pub converted: Counter,
    pub attempted: Counter,
    pub committed: Counter,
    pub unknown: Counter,
}

/// 任务视图。`executionIds` 只收**已建立**的执行记录；接受记录提交失败时不应出现在这里。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub job_id: JobId,
    pub kind: String,
    pub state: JobState,
    pub stage: Option<String>,
    pub execution_ids: Vec<ExecutionId>,
    pub artifact_ids: Vec<ArtifactId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// P5 派生效果结局（§10.1.1）。与 `state` 独立：失败/取消不抹掉已提交范围。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_outcome: Option<EffectOutcome>,
    /// 取消请求是独立事实，不把 `state` 提前改成 cancelled（§10.1.1）。
    #[serde(default)]
    pub cancel_requested: bool,
    /// 进入待核验时的原因（如 `outcomeUnknown` / `cleanupNotConfirmed`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_verification_reason: Option<String>,
    /// Safe, bounded terminal result detail; raw SQL, driver errors and secrets are not stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// P5 五类进度计数；缺省为全零。
    #[serde(default)]
    pub progress: JobProgress,
}

/// 任务定义：一次提交要做什么。**不含**任何运行时会话信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDefinition {
    pub job_id: JobId,
    pub kind: String,
    pub owner: OwnerRef,
    /// 计划正文（工作流步骤 / 任务体）。敏感内容只允许引用 `SecretRef`，禁止内联凭据。
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
}

/// 阶段记录。`claimed_by` 为空表示尚未被任何 worker 认领。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageRecord {
    pub job_id: JobId,
    pub stage_id: StageId,
    pub kind: String,
    pub claimed_by: Option<WorkerId>,
    pub execution_ids: Vec<ExecutionId>,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

/// 任务记录：视图 + 认领与恢复所需的最小额外字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub view: JobView,
    pub definition: JobDefinition,
    pub stages: Vec<StageRecord>,
    /// 状态版本，任务状态 CAS 的比较基准。
    pub state_version: crate::id::JobStateVersion,
}

/// Safe recovery summary returned by the desktop job details query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobRecoveryVerdict {
    /// Durable admission was committed but no worker ever claimed the Job.
    NotExecuted,
    PendingVerification,
    ResumeAfterVerify,
    Reject,
    RequireManualReview,
}

/// Recovery decision persisted by the host after a handler has inspected its checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecoveryResult {
    pub verdict: JobRecoveryVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_through: Option<u64>,
    /// Stable code only; arbitrary handler error text is not durable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

/// Detailed read model for a single Job. Bulk list responses remain small.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDetails {
    pub job: JobView,
    /// CAS version captured with this details snapshot.
    pub state_version: crate::id::JobStateVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_revision: Option<u64>,
    #[serde(default)]
    pub commit_boundaries: Vec<CommitBoundary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<JobRecoveryResult>,
    /// Bounded handler receipts, one per completed stage. Free-form text is excluded.
    #[serde(default)]
    pub domain_results: Vec<JobDomainResult>,
    /// Stable connection/object identity used only to scope explicit post-restart verification.
    #[serde(default)]
    pub recovery_targets: Vec<JobRecoveryTarget>,
    /// Optional stable before-state fingerprint for explicit post-restart verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_before_fingerprint: Option<String>,
    /// Stable recovery policy code; never a free-form instruction or SQL fragment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_policy: Option<String>,
}

/// Safe persistent target identity. It is not a live session or a resource lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecoveryTarget {
    pub connection_id: String,
    pub object_ids: Vec<String>,
}

/// Persistable handler summary. The host validates every identifier and applies a size cap.
/// It deliberately has no field for SQL, driver messages, credentials, or runtime handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDomainResult {
    pub stage_id: StageId,
    pub result_code: String,
    pub outcome_code: String,
    #[serde(default)]
    pub counters: Vec<JobResultCounter>,
    #[serde(default)]
    pub items: Vec<JobResultItem>,
    #[serde(default)]
    pub artifact_ids: Vec<ArtifactId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResultCounter {
    pub code: String,
    pub value: Counter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResultItem {
    /// Opaque operation/object identifier, never a statement or user-supplied label.
    pub item_id: String,
    pub outcome_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

/// Input passed to an explicit recovery verifier after the caller has re-authorized resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecoveryRequest {
    pub details: JobDetails,
    pub checkpoint: Option<Checkpoint>,
}

/// Safe facts returned by an explicit read-only verifier. The host persists these atomically;
/// the type carries no request to dispatch or replay a Job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecoveryVerification {
    pub result: JobRecoveryResult,
    #[serde(default)]
    pub confirmed_boundaries: Vec<CommitBoundary>,
    #[serde(default)]
    pub domain_results: Vec<JobDomainResult>,
}

/// 已提交边界：checkpoint 里**唯一**关于「已经生效到哪里」的事实。
/// 它是稳定指纹，不是活租约；进程重启后据此核验，而不是据此续跑旧会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitBoundary {
    pub stage_id: StageId,
    /// 已提交内容的稳定目标指纹（规范化目标 + 版本 + 映射指纹）。
    pub stable_target_fingerprint: String,
    pub committed_at: Timestamp,
    /// P5：Operation 粒度。Schema Diff 以 operationId 标记（§7）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// P5：批次粒度。Data Sync/Transfer 以 batchId 标记（§7）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    /// P5：冻结载荷摘要；恢复核验用它区分「重放了新载荷」与「原批已提交」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_digest: Option<String>,
    /// P5：真实提交 evidence（目标批次记录 / 只读核验结果摘要）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    /// P5：证据被核验确认的时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<Timestamp>,
}

/// 任务 checkpoint。**禁止**包含 live session、lease、cursor（§4.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub job_id: JobId,
    pub state_version: crate::id::JobStateVersion,
    /// 稳定目标/版本/映射指纹。
    pub stable_target_fingerprint: String,
    /// 已提交边界，按提交顺序递增。
    pub committed: Vec<CommitBoundary>,
    /// 核验证据：主键/版本校验、对象映射校验的结果摘要。
    pub verification_evidence: Vec<String>,
    /// 恢复策略标识（如 `reauthorize-then-rewrite` / `read-only-replay`）。
    pub recovery_policy: String,
}

/// worker 认领。P5：`claim_generation` 是持久化单调计数（由仓储单调发放），
/// 初次认领与每次接管 +1，续约不变；`expires_at` 到期即失租，旧 claim 的写入一律拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobClaim {
    pub job_id: JobId,
    pub stage_id: StageId,
    pub worker_id: WorkerId,
    pub claimed_at: Timestamp,
    pub claim_generation: Counter,
    pub expires_at: Timestamp,
}

/// 任务查询过滤条件。分页游标**不落盘、不跨重启**，因此用 `Counter` 而非持久化游标。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFilter {
    pub states: Vec<JobState>,
    pub owner: Option<OwnerRef>,
    pub after: Option<crate::id::Counter>,
    pub limit: Option<u32>,
}

/// 恢复扫描过滤条件。只筛持久化状态，**不筛运行时资源**。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryFilter {
    pub kinds: Vec<String>,
    pub older_than: Option<Timestamp>,
    pub limit: Option<u32>,
}

/// 工作流块归属的 `OwnerRef::WorkflowBlock` 便捷构造。
pub fn workflow_block_owner(job_id: JobId, block_id: BlockId) -> OwnerRef {
    OwnerRef::WorkflowBlock { job_id, block_id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{Counter, JobId, StageId};
    use serde_json::json;

    fn job_view() -> JobView {
        JobView {
            job_id: JobId::new("job-1"),
            kind: "workflow".into(),
            state: JobState::Running,
            stage: Some("load".into()),
            execution_ids: vec![ExecutionId::new("exec-1")],
            artifact_ids: vec![ArtifactId::new("art-1")],
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            updated_at: Timestamp::new("2026-01-01T00:00:05Z"),
            effect_outcome: Some(EffectOutcome::Completed),
            cancel_requested: false,
            pending_verification_reason: None,
            error: None,
            progress: JobProgress::default(),
        }
    }

    #[test]
    fn empty_job_details_keep_client_collection_contract() {
        let details = JobDetails {
            job: job_view(),
            state_version: crate::id::JobStateVersion::new(1),
            plan_id: None,
            plan_digest: None,
            selection_revision: None,
            commit_boundaries: Vec::new(),
            recovery: None,
            domain_results: Vec::new(),
            recovery_targets: Vec::new(),
            target_before_fingerprint: None,
            recovery_policy: None,
        };
        let mut value = serde_json::to_value(&details).expect("serialize details");
        for key in ["commitBoundaries", "domainResults", "recoveryTargets"] {
            assert_eq!(
                value[key],
                json!([]),
                "client requires {key} even before a stage completes"
            );
        }
        // Old persisted records may omit these collections; reads remain compatible.
        value
            .as_object_mut()
            .expect("details object")
            .remove("domainResults");
        value
            .as_object_mut()
            .expect("details object")
            .remove("recoveryTargets");
        assert_eq!(
            serde_json::from_value::<JobDetails>(value).expect("legacy details"),
            details
        );
    }

    #[test]
    fn job_view_round_trips() {
        let original = job_view();
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(value["state"], json!("running"));
        assert_eq!(value["stage"], json!("load"));
        assert_eq!(
            serde_json::from_value::<JobView>(value).expect("deserialize"),
            original
        );
    }

    #[test]
    fn checkpoint_carries_no_live_session_or_lease() {
        let checkpoint = Checkpoint {
            job_id: JobId::new("job-1"),
            state_version: crate::id::JobStateVersion::new(4),
            stable_target_fingerprint: "sha256:abc".into(),
            committed: vec![CommitBoundary {
                stage_id: StageId::new("stage-1"),
                stable_target_fingerprint: "sha256:abc".into(),
                committed_at: Timestamp::new("2026-01-01T00:00:00Z"),
                operation_id: None,
                batch_id: None,
                payload_digest: None,
                evidence: Vec::new(),
                verified_at: None,
            }],
            verification_evidence: vec!["pk-verified".into()],
            recovery_policy: "reauthorize-then-rewrite".into(),
        };
        let value = serde_json::to_value(&checkpoint).expect("serialize");
        assert_eq!(value["stateVersion"], json!("4"));
        assert_eq!(
            value["committed"][0]["stableTargetFingerprint"],
            json!("sha256:abc")
        );
        assert_eq!(
            serde_json::from_value::<Checkpoint>(value).expect("deserialize"),
            checkpoint
        );
    }

    #[test]
    fn job_record_and_stage_record_round_trip() {
        let record = JobRecord {
            view: job_view(),
            definition: JobDefinition {
                job_id: JobId::new("job-1"),
                kind: "workflow".into(),
                owner: workflow_block_owner(JobId::new("job-1"), BlockId::new("b1")),
                payload: json!({"steps": []}),
                created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            },
            stages: vec![StageRecord {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-1"),
                kind: "sql".into(),
                claimed_by: Some(WorkerId::new("w-1")),
                execution_ids: vec![ExecutionId::new("exec-1")],
                started_at: None,
                finished_at: None,
            }],
            state_version: crate::id::JobStateVersion::new(1),
        };
        let value = serde_json::to_value(&record).expect("serialize");
        assert_eq!(value["definition"]["owner"]["kind"], json!("workflowBlock"));
        assert_eq!(value["definition"]["owner"]["blockId"], json!("b1"));
        assert_eq!(value["stages"][0]["claimedBy"], json!("w-1"));
        assert_eq!(
            serde_json::from_value::<JobRecord>(value).expect("deserialize"),
            record
        );
    }

    #[test]
    fn filters_and_claims_round_trip() {
        let filter = JobFilter {
            states: vec![JobState::Running, JobState::Queued],
            owner: None,
            after: Some(Counter::new(10)),
            limit: Some(50),
        };
        let value = serde_json::to_value(&filter).expect("serialize");
        assert_eq!(value["states"], json!(["running", "queued"]));
        assert_eq!(value["after"], json!("10"));
        assert_eq!(
            serde_json::from_value::<JobFilter>(value).expect("deserialize"),
            filter
        );

        let claim = JobClaim {
            job_id: JobId::new("job-1"),
            stage_id: StageId::new("stage-1"),
            worker_id: WorkerId::new("w-1"),
            claimed_at: Timestamp::new("2026-01-01T00:00:00Z"),
            claim_generation: Counter::new(1),
            expires_at: Timestamp::new("2026-01-01T00:01:00Z"),
        };
        let value = serde_json::to_value(&claim).expect("serialize");
        assert_eq!(
            serde_json::from_value::<JobClaim>(value).expect("deserialize"),
            claim
        );

        let recovery = RecoveryFilter {
            kinds: vec!["workflow".into()],
            older_than: Some(Timestamp::new("2025-12-31T00:00:00Z")),
            limit: Some(10),
        };
        let value = serde_json::to_value(&recovery).expect("serialize");
        assert_eq!(
            serde_json::from_value::<RecoveryFilter>(value).expect("deserialize"),
            recovery
        );
    }

    #[test]
    fn terminal_job_states() {
        assert!(JobState::Succeeded.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(!JobState::Queued.is_terminal());
    }
}
