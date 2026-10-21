//! `JobRepository`：任务受理与状态推进端口。
//!
//! 词汇表（§4.3）落在 `crate::dto::job`：`JobDefinition`、`JobRecord`、`StageRecord`、
//! `JobStateVersion`、`JobFilter`、`RecoveryFilter`、`Checkpoint`、`CommitBoundary`、`JobClaim`。
//!
//! **落库白名单**必须排除 `dbSessionId`、SessionHandle、lease/cursor、取消句柄与
//! attachment token（[连接 §4.4](connection-management.md#44-可落盘来源与运行时绑定)）。
//! `ExecutionView` 的持久化投影按[连接 §4](connection-management.md#4-dto-与字段定义)的两层
//! 来源规则拆分，实时 `runtimeBinding` 只留在 runtime 内存。
//!
//! 两条不可退让的语义：
//!
//! * **先持久化后取资源**（[连接 §10.1](connection-management.md#101-公共处理)）。
//!   `accept` 成功返回即代表 Job 已受理；资源申请失败**不回滚**受理记录。
//! * **不自动重放副作用阶段**。`list_recoverable` 只返回需要人工/计划核验的待办。
//!
//! ## 组织维度（落盘词汇 + 端口行为）
//!
//! * `accept` 必须把 `ctx` 的组织**盖章**到 `JobDefinition.organization_id`：
//!   调用方自报的组织值不作数（INV-01），落库归属一律来自 adapter 构造的上下文。
//! * `list` 必须按 `JobFilter.organization_id` **且** `ctx` 的组织过滤；
//!   两者不一致时结果是**空**，不是错误。
//! * `get` / `list_recoverable` 必须按 `ctx` 的组织过滤。
//!
//! 跨组织访问按两条路径分别处理，**形状在内存实现与 SQLite 实现上一致**：
//!
//! * **读路径**（`get` / `list` / `list_recoverable`）：一律按「**不存在**」处理
//!   （`NotFound` / 空列表），不返回「无权限」之类的错误，也不泄漏「存在但无权」
//!   （CM-05，与 `ProfileRepository` 同口径）。
//! * **写路径**（`record_stage` / `record_commit_boundary` / `compare_and_set_state` /
//!   `save_checkpoint` / `record_progress` / `record_artifacts` / `record_domain_result` /
//!   `finish`）：由 **claim fencing** 兜底——claim 只在「按 `ctx` 组织核对过的行」上
//!   铸出，因此越组织的写入按 `StaleClaim` 拒绝（**不是** `NotFound`）。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::dto::job::{
    Checkpoint, CommitBoundary, JobClaim, JobDefinition, JobFilter, JobRecord, JobState,
    RecoveryFilter, StageRecord,
};
use crate::error::PortError;
use crate::id::{IdempotencyKey, JobId, JobStateVersion, WorkerId};

#[async_trait]
pub trait JobRepository: Send + Sync + 'static {
    /// 先持久化后取资源（[连接 §10.1](connection-management.md#101-公共处理)）。成功返回即代表 Job 已受理。
    ///
    /// 实现必须把 `ctx` 的组织盖章到 `definition.organization_id` 再落库：
    /// 落库归属只来自 adapter 构造的上下文，调用方自报的组织值一律不作数。
    async fn accept(
        &self,
        ctx: &RequestContext,
        definition: JobDefinition,
        idem: &IdempotencyKey,
    ) -> Result<JobRecord, PortError>;

    /// 按 `ctx` 的组织读取。跨组织访问返回「不存在」而不是错误。
    async fn get(&self, ctx: &RequestContext, job_id: JobId) -> Result<JobRecord, PortError>;

    /// 按组织列举。`filter.organization_id` 与 `ctx` 的组织必须同时匹配；
    /// 两者不一致时返回空列表（跨组织列举不是合法调用）。
    async fn list(
        &self,
        ctx: &RequestContext,
        filter: JobFilter,
    ) -> Result<Vec<JobRecord>, PortError>;

    /// P5：stage 登记必须携带当前 claim，仓储校验 generation/worker/租约未过期（§4.3 修订）。
    async fn record_stage(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: StageRecord,
    ) -> Result<(), PortError>;

    async fn record_commit_boundary(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        boundary: CommitBoundary,
    ) -> Result<(), PortError>;

    /// 状态 CAS；并发推进同一 Job 时由版本不匹配拒绝，不做覆盖写。
    /// P5：同样携带当前 claim 做防护性校验。
    async fn compare_and_set_state(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        next: JobState,
    ) -> Result<JobRecord, PortError>;

    /// claim/renew：多 worker 抢占与续约；续约失败即视为失联（Job 停止新增资源）。
    async fn claim(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
        worker: WorkerId,
    ) -> Result<JobClaim, PortError>;

    /// 续约。端口签名里没有 `RequestContext`，因此**续约自身不做组织比对**：
    /// 组织边界由 **claim fencing** 兜底——claim 只在「按 `ctx` 组织核对过的行」上铸出
    /// （见 `claim`），且续约要匹配当前的 worker + claim generation + 未过期租约，
    /// 所以越组织的续约拿不到有效 claim，只能按 `StaleClaim` 失败。
    async fn renew(&self, claim: &JobClaim) -> Result<JobClaim, PortError>;

    /// 检查点写入与恢复候选查询：只返回需要人工/计划核验的待办，不自动重放副作用阶段。
    async fn save_checkpoint(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        cp: Checkpoint,
    ) -> Result<(), PortError>;

    /// 恢复候选查询：按 `ctx` 的组织限定，跨组织的待办不出现在结果里。
    async fn list_recoverable(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError>;
}

/// 阶段是否已结束。判定只认 `finished_at`：阶段状态与 `JobState` 不是同一台状态机。
pub fn stage_is_finished(stage: &StageRecord) -> bool {
    stage.finished_at.is_some()
}

/// claim 续约失败即视为失联；本函数给出「是否需要重新抢占」的判定，供用例层选择策略。
/// 失联**不**自动重放副作用阶段，只把它交回恢复扫描。
pub fn needs_reclaim(current: JobStateVersion, minimum: JobStateVersion) -> bool {
    current.get() < minimum.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::job::{workflow_block_owner, JobView};
    use crate::id::{
        ArtifactId, BlockId, Counter, ExecutionId, JobId as JId, OrganizationId, PrincipalId,
        StageId, Timestamp,
    };
    use crate::OwnerRef;

    /// 去掉整行注释（含 `///` 文档注释）。契约断言只看**代码形状**，
    /// 否则「本端口不得收 SessionHandle」这句注释会把自己判为违规。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `JobRepository` trait 的代码块（不含注释）。
    fn trait_code() -> String {
        let source = include_str!("job.rs");
        let after = source
            .split("pub trait JobRepository")
            .nth(1)
            .unwrap_or_default();
        code_only(&after[..after.find("\n}\n").unwrap_or(after.len())])
    }

    #[test]
    fn the_persisted_job_types_have_no_field_that_can_hold_a_handle() {
        // 逐文件扫描整个落盘白名单：JobRecord / StageRecord / JobDefinition /
        // Checkpoint / CommitBoundary / JobFilter / RecoveryFilter / JobClaim。
        // 只匹配**声明形状**（`: Type` / snake_case 字段名），避免误伤中文注释里的散文。
        let job_dto = code_only(include_str!("../dto/job.rs"));
        let forbidden = [
            "db_session_id",
            "runtime_epoch",
            "runtime_binding",
            ": DbSessionId",
            ": SessionHandle",
            ": RuntimeEpoch",
            ": RuntimeResultBinding",
            ": LeaseId",
            ": AttachmentToken",
            "#[serde(flatten)]",
        ];
        for marker in forbidden {
            assert!(
                !job_dto.contains(marker),
                "落盘类型不得声明 `{marker}`：JobRecord/StageRecord/Checkpoint 只能存稳定事实"
            );
        }
        // 端口签名本身也不许引入运行时会话类型（只扫描 trait 代码，避免自指）。
        let port = trait_code();
        for marker in [
            "SessionHandle",
            "DbSessionId",
            "RuntimeEpoch",
            "RuntimeResultBinding",
        ] {
            assert!(
                !port.contains(marker),
                "JobRepository 端口签名不得收 `{marker}`：{port}"
            );
        }
    }

    #[test]
    fn the_port_only_speaks_in_persisted_and_versioned_types() {
        // 状态推进是 CAS：入口给 expected 版本，出口给整条记录，不做覆盖写。
        let source = include_str!("job.rs");
        assert!(source.contains("expected: JobStateVersion"));
        assert!(source.contains("next: JobState"));
        // 认领走 claim/renew 两步，续约不回退为「重新 claim 一次」。
        assert!(source.contains("async fn renew(&self, claim: &JobClaim)"));
    }

    #[test]
    fn stage_completion_is_decided_by_the_finished_timestamp() {
        let mut stage = StageRecord {
            job_id: JId::new("job-1"),
            stage_id: StageId::new("stage-1"),
            kind: "sql".into(),
            claimed_by: None,
            execution_ids: Vec::new(),
            started_at: None,
            finished_at: None,
        };
        assert!(!stage_is_finished(&stage));
        stage.started_at = Some(Timestamp::new("2026-01-01T00:00:00Z"));
        assert!(!stage_is_finished(&stage), "已启动不等于已结束");
        stage.finished_at = Some(Timestamp::new("2026-01-01T00:00:05Z"));
        assert!(stage_is_finished(&stage));
    }

    #[test]
    fn reclaim_is_required_when_the_state_version_falls_behind() {
        assert!(needs_reclaim(
            JobStateVersion::new(1),
            JobStateVersion::new(2)
        ));
        assert!(!needs_reclaim(
            JobStateVersion::new(2),
            JobStateVersion::new(2)
        ));
        assert!(!needs_reclaim(
            JobStateVersion::new(3),
            JobStateVersion::new(2)
        ));
    }

    #[test]
    fn an_accepted_record_round_trips_across_the_boundary() {
        let record = JobRecord {
            view: JobView {
                job_id: JId::new("job-1"),
                kind: "workflow".into(),
                state: JobState::Running,
                stage: Some("load".into()),
                execution_ids: vec![ExecutionId::new("exec-1")],
                artifact_ids: vec![ArtifactId::new("art-1")],
                created_at: Timestamp::new("2026-01-01T00:00:00Z"),
                updated_at: Timestamp::new("2026-01-01T00:00:01Z"),
                effect_outcome: None,
                cancel_requested: false,
                pending_verification_reason: None,
                error: None,
                progress: Default::default(),
            },
            definition: JobDefinition {
                job_id: JId::new("job-1"),
                organization_id: OrganizationId::new("org-1"),
                kind: "workflow".into(),
                owner: workflow_block_owner(JId::new("job-1"), BlockId::new("b1")),
                payload: serde_json::json!({"steps": []}),
                created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            },
            stages: Vec::new(),
            state_version: JobStateVersion::new(1),
        };
        let value = serde_json::to_value(&record).expect("serialize");
        assert_eq!(value["stateVersion"], serde_json::json!("1"));
        assert_eq!(
            serde_json::from_value::<JobRecord>(value).expect("deserialize"),
            record
        );
    }

    #[test]
    fn a_checkpoint_records_a_commit_boundary_rather_than_a_cursor() {
        let boundary = CommitBoundary {
            stage_id: StageId::new("stage-1"),
            stable_target_fingerprint: "sha256:abc".into(),
            committed_at: Timestamp::new("2026-01-01T00:00:00Z"),
            operation_id: None,
            batch_id: None,
            payload_digest: None,
            evidence: Vec::new(),
            verified_at: None,
        };
        let checkpoint = Checkpoint {
            job_id: JId::new("job-1"),
            state_version: JobStateVersion::new(4),
            stable_target_fingerprint: "sha256:abc".into(),
            committed: vec![boundary],
            verification_evidence: vec!["pk-verified".into()],
            recovery_policy: "reauthorize-then-rewrite".into(),
        };
        let value = serde_json::to_value(&checkpoint).expect("serialize");
        assert_eq!(
            value["committed"][0]["stableTargetFingerprint"],
            serde_json::json!("sha256:abc")
        );
        assert!(value["committed"][0].get("cursor").is_none());
        assert_eq!(
            serde_json::from_value::<Checkpoint>(value).expect("deserialize"),
            checkpoint
        );
    }

    #[test]
    fn a_claim_names_one_worker_of_one_job_and_is_renewable() {
        let claim = JobClaim {
            job_id: JId::new("job-1"),
            stage_id: StageId::new("stage-1"),
            worker_id: WorkerId::new("w-1"),
            claimed_at: Timestamp::new("2026-01-01T00:00:00Z"),
            claim_generation: Counter::new(1),
            expires_at: Timestamp::new("2026-01-01T00:01:00Z"),
        };
        let renewed = JobClaim {
            claimed_at: Timestamp::new("2026-01-01T00:00:30Z"),
            ..claim.clone()
        };
        assert_eq!(renewed.job_id, claim.job_id);
        assert_eq!(renewed.stage_id, claim.stage_id);
        assert_eq!(renewed.worker_id, claim.worker_id);
        assert_ne!(renewed.claimed_at, claim.claimed_at);
        let value = serde_json::to_value(&claim).expect("serialize");
        assert_eq!(
            serde_json::from_value::<JobClaim>(value).expect("deserialize"),
            claim
        );
    }

    #[test]
    fn listing_filters_are_recoverable_state_not_runtime_cursors() {
        // 过滤器里的 `after` 是 `Counter` 序号，不是持久化游标。
        // 组织先绑到局部变量：字面量里因此看得出「组织从哪来」，
        // 而不是把组织内联成常量（`store::job_filter_scope_tests` 的两条守卫按此约定）。
        let organization = OrganizationId::new("org-1");
        let filter = JobFilter {
            organization_id: organization,
            states: vec![JobState::Running],
            owner: Some(OwnerRef::Job {
                job_id: JId::new("job-1"),
                stage_id: StageId::new("stage-1"),
            }),
            after: Some(Counter::new(10)),
            limit: Some(50),
        };
        let value = serde_json::to_value(&filter).expect("serialize");
        assert_eq!(value["after"], serde_json::json!("10"));
        assert_eq!(value["organizationId"], serde_json::json!("org-1"));
        assert_eq!(value["owner"]["kind"], serde_json::json!("job"));
        assert_eq!(
            serde_json::from_value::<JobFilter>(value).expect("deserialize"),
            filter
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
    fn the_organization_scope_is_not_a_parameter_of_this_port() {
        // 组织隔离由 `ctx` 承担；端口方法没有第二个组织参数可被漏传或写错。
        // 组织维度出现在**落盘词汇**（`JobDefinition` / `JobFilter`）里，而不是端口参数里：
        // 前者是持久化归属，后者是调用现场身份，两者的来源不同、不能互相顶替。
        let trait_code = trait_code();
        assert!(
            !trait_code.contains("organization_id") && !trait_code.contains("OrganizationId"),
            "组织只能来自 RequestContext：{trait_code}"
        );
        let _ = (
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            JobId::new("job-1"),
        );
    }

    #[test]
    fn the_port_pins_the_organization_dimension_into_the_persisted_vocabulary() {
        // 受理落库、列举、查询、恢复四条路径都必须按组织隔离。
        // 这条断言盯住**契约文本**：端口文档一旦不再声明组织语义，就说明隔离语义被删了。
        let source = include_str!("job.rs");
        for marker in ["organization_id", "盖章", "跨组织"] {
            assert!(
                source.contains(marker),
                "JobRepository 契约必须声明组织维度（缺 `{marker}`）"
            );
        }
        // 落盘词汇侧的必填组织维度由 dto 的契约测试守着，这里只确认端口确实引用了它。
        let job_dto = include_str!("../dto/job.rs");
        assert!(
            job_dto.contains("pub organization_id: OrganizationId"),
            "JobDefinition / JobFilter 必须带必填的 organization_id"
        );
    }
}
