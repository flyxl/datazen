//! JobRepository 的进程内实现（P5 契约：幂等受理、planId 唯一消费、claim fencing）。
//!
//! 不变量镜像 `jobs` / `job_checkpoints` 的 DDL 约束（persistence-model §3.6）：
//!
//! * apply 类 Job 的 `payload.consumedPlanId` 在受理事务中建立唯一消费；
//! * 同一幂等键 + 同指纹 → 原 Job receipt；同键不同指纹 → [`PortError::IdempotencyConflict`]；
//! * claim 单调 `claim_generation`；初次认领与接管 +1，续约不变；
//!   所有 worker 写入校验当前 generation + worker + 租约未过期，否则拒绝；
//! * 终态时 claim 清空（worker/expires 成对置 NULL 的内存等价物）；
//! * checkpoint 以 (job_id, checkpoint_version) 为键，重复版本冲突；
//! * 恢复扫描只产出待核验候选，绝不重放副作用阶段。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobClaim, JobDefinition, JobFilter, JobRecord, JobRecoveryResult,
    JobState, RecoveryFilter, StageRecord,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    Counter, IdempotencyKey, JobId, JobStateVersion, StageId, Timestamp, WorkerId,
};

use crate::connection::types::fnv1a64_hex;
use crate::job::error::JobError;
use crate::job::plan::project_frozen_plan;
use crate::job::time::{after_seconds, JobClock};
use datazen_platform_api::ports::job::JobRepository;

mod runtime_methods;

/// 单个 Job 的可变行。
#[derive(Debug, Clone)]
struct JobRow {
    record: JobRecord,
    claim: Option<JobClaim>,
    cancel_requested_at: Option<Timestamp>,
}

#[derive(Debug, Clone)]
struct IdemReceipt {
    job_id: JobId,
    accept_fingerprint: String,
}

/// 取消看守者每轮只需要两个 bool。这是 [`JobRepository::get`] 的**廉价替身**：
/// `get` 会持锁深拷贝整条 `JobRecord`（含 `payload: serde_json::Value` 与
/// `Vec<StageRecord>`），阶段执行期间每秒被读 20 次并与 `request_cancel` /
/// `record_stage` / 进度写入争同一把全局锁；而看守者只判断「取消了吗」「终结了吗」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelPollSnapshot {
    pub cancel_requested: bool,
    pub terminal: bool,
}

#[derive(Default)]
struct Inner {
    /// 以 job_id 为键（RequestContext 的组织维度由 ctx 校验：内存实现单组织作用域演示，
    /// 组织键的完整隔离语义由服务端仓储落地，详见 persistence-model §3.6）。
    jobs: HashMap<JobId, JobRow>,
    idempotency: HashMap<String, IdemReceipt>, // key
    consumed_plans: HashMap<String, JobId>,    // planId
    checkpoints: HashMap<(JobId, u64), Checkpoint>,
    /// 逐批已确认提交边界（§7）：record_commit_boundary 落库，按产生顺序递增。
    boundaries: HashMap<JobId, Vec<CommitBoundary>>,
    recovery: HashMap<JobId, JobRecoveryResult>,
}

/// `JobRepository` 的内存实现。线程安全：单个 `Mutex` 保护全部索引（同一把锁即受理事务）。
pub struct InMemoryJobRepository {
    inner: Mutex<Inner>,
    clock: Arc<dyn JobClock>,
    claim_ttl: i64,
    /// 测试缝：`get` 故障注入与调用计数。生产构建不保留这两个字段。
    #[cfg(any(test, feature = "test-harness"))]
    get_fault: std::sync::atomic::AtomicBool,
    #[cfg(any(test, feature = "test-harness"))]
    get_calls: std::sync::atomic::AtomicUsize,
}

impl InMemoryJobRepository {
    pub fn new(clock: Arc<dyn JobClock>, claim_ttl_secs: i64) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            clock,
            claim_ttl: claim_ttl_secs,
            #[cfg(any(test, feature = "test-harness"))]
            get_fault: std::sync::atomic::AtomicBool::new(false),
            #[cfg(any(test, feature = "test-harness"))]
            get_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 测试缝：让此后每次 [`JobRepository::get`] 返回 `BackendUnavailable`。
    ///
    /// 内存实现正常只会以 `NotFound` 失败，没有删除 Job 的路径，所以「轮询期间读失败」
    /// 这条分支只能靠注入触发。取消看守者对它的处置是被决策过的（见 `runtime.rs`
    /// 的 `CANCEL_POLL_FAILED` 注释），必须能被独立复现，因此这里是公开开关。
    #[cfg(any(test, feature = "test-harness"))]
    pub fn fail_get(&self, failing: bool) {
        self.get_fault
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }

    /// 测试缝：累计 `get` 调用次数。用来证明看守者在读失败后**停止轮询**，
    /// 而不是把错误吞掉后继续空转。
    #[cfg(any(test, feature = "test-harness"))]
    pub fn get_calls(&self) -> usize {
        self.get_calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 测试缝：把 Job 强制写成终态，用来复现「Job 已经收尾，但某个阶段还在跑」的窗口。
    ///
    /// 正常路径里终态只由 `confirm_cancelled_not_started` / `compare_and_set_state` 落，
    /// 它们都在 `dispatch` 自己的收尾里，阶段执行期间不存在这样的窗口，所以
    /// `watch_cancel_request` 的 `terminal => return` 分支（见 `runtime.rs`）没有别的
    /// 触发方式。测试用它把窗口撑开：阶段仍在阻塞时强制终结 Job，好观察看守者是否
    /// 自己停止轮询。
    ///
    /// 与真实收尾的差别要讲清楚：这里**保留 claim**、并单调推进 `state_version`，
    /// 于是阶段返回后 `dispatch` 的收尾 CAS 仍能按它读到的最新版本正常收敛；真实路径
    /// 会连 claim 一起清空。这正是它只能是测试缝、不能进生产代码的原因。
    #[cfg(any(test, feature = "test-harness"))]
    pub fn force_terminal_for_test(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        state: JobState,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        debug_assert!(state.is_terminal(), "只允许强制成终态");
        let mut inner = lock_inner(&self.inner)?;
        let now = self.clock.now();
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        row.record.view.state = state;
        row.record.view.updated_at = now;
        row.record.state_version = JobStateVersion::new(row.record.state_version.get() + 1);
        Ok(row.record.clone())
    }

    /// 取消看守者的轮询读：只取两个 bool，**不克隆 `JobRecord`**。
    ///
    /// 与 [`JobRepository::get`] 共用同一把全局 `Mutex`，所以它省掉的是锁内的深拷贝
    /// （`definition.payload` 的 `serde_json::Value` + `Vec<StageRecord>`），锁本身省不掉。
    /// 测试缝与 `get` 共用计数与故障开关，看守者的行为断言因此只有一个口径。
    pub fn cancel_poll(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<CancelPollSnapshot, PortError> {
        let _ = ctx;
        #[cfg(any(test, feature = "test-harness"))]
        {
            use std::sync::atomic::Ordering;
            self.get_calls.fetch_add(1, Ordering::SeqCst);
            if self.get_fault.load(Ordering::SeqCst) {
                return Err(PortError::BackendUnavailable(
                    "injected get failure (test seam)".into(),
                ));
            }
        }
        let inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        Ok(CancelPollSnapshot {
            cancel_requested: row.record.view.cancel_requested,
            terminal: row.record.view.state.is_terminal(),
        })
    }

    /// claim fencing：generation/worker 必须匹配当前持有值，且租约未过期。
    fn check_claim(inner: &Inner, claim: &JobClaim, now: &Timestamp) -> Result<(), JobError> {
        let row = inner
            .jobs
            .get(&claim.job_id)
            .ok_or_else(|| JobError::NotFound(claim.job_id.as_str().to_string()))?;
        let current = row.claim.as_ref().ok_or(JobError::StaleClaim)?;
        if current.worker_id != claim.worker_id
            || current.claim_generation != claim.claim_generation
        {
            return Err(JobError::StaleClaim);
        }
        if now.as_str() > current.expires_at.as_str() {
            return Err(JobError::ClaimExpired);
        }
        Ok(())
    }

    /// 取消意图：只记录 `cancel_requested_at` 与视图标记，**不改 state**
    /// （queued 取消由调度器确认 notStarted；running 取消由执行终态裁决，§10.1.1）。
    pub fn request_cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if row.record.view.state.is_terminal() {
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().into(),
            });
        }
        row.record.view.cancel_requested = true;
        row.cancel_requested_at = Some(self.clock.now());
        row.record.view.updated_at = self.clock.now();
        Ok(row.record.clone())
    }

    /// 调度器确认「尚未产生任何外部效果」后终结：cancelled + notStarted。
    pub fn confirm_cancelled_not_started(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        effect_outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if row.record.view.state != JobState::Queued || !row.record.view.cancel_requested {
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().into(),
            });
        }
        row.record.view.state = JobState::Cancelled;
        row.record.view.effect_outcome = Some(effect_outcome);
        row.record.view.updated_at = self.clock.now();
        row.record.state_version = JobStateVersion::new(row.record.state_version.get() + 1);
        row.claim = None;
        Ok(row.record.clone())
    }

    /// 由 runtime 在阶段结束时聚合写入效果结局。
    pub fn set_effect_outcome(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        outcome: EffectOutcome,
    ) -> Result<(), PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        row.record.view.effect_outcome = Some(outcome);
        row.record.view.updated_at = self.clock.now();
        Ok(())
    }

    /// 在从未被 claim 的路径终结 Job：failed + notStarted，不走 claim fencing。
    /// 只在 `row.claim.is_none()` 且 Job 仍为 Queued 时允许。
    pub async fn mark_failed_unstarted(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason: &str,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if row.claim.is_some() || row.record.view.state != JobState::Queued {
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().into(),
            });
        }
        row.record.view.state = JobState::Failed;
        row.record.view.effect_outcome = Some(EffectOutcome::NotStarted);
        row.record.view.error = Some(reason.into());
        row.record.view.pending_verification_reason = None;
        row.record.view.updated_at = self.clock.now();
        row.record.state_version = JobStateVersion::new(row.record.state_version.get() + 1);
        Ok(row.record.clone())
    }

    /// 标记待核验（effectOutcome 保留 unknown，不改 state）。
    pub fn mark_pending_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason: &str,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if !row.record.view.state.is_terminal() {
            row.record.view.state = JobState::Failed;
            row.record.view.effect_outcome = Some(if row.claim.is_some() {
                EffectOutcome::Unknown
            } else {
                EffectOutcome::NotStarted
            });
            row.record.view.error = Some(reason.into());
            row.record.state_version =
                JobStateVersion::new(row.record.state_version.get().saturating_add(1));
            row.claim = None;
        }
        row.record.view.pending_verification_reason = Some(reason.into());
        row.record.view.updated_at = self.clock.now();
        Ok(row.record.clone())
    }

    /// 已落库的提交边界（按产生顺序）。
    pub fn committed_boundaries(&self, job_id: &JobId) -> Vec<CommitBoundary> {
        let inner = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        inner.boundaries.get(job_id).cloned().unwrap_or_default()
    }

    /// checkpoint 读取（恢复核验用）。
    pub fn latest_checkpoint(&self, ctx: &RequestContext, job_id: &JobId) -> Option<Checkpoint> {
        let _ = ctx;
        let inner = self.inner.lock().ok()?;
        inner
            .checkpoints
            .iter()
            .filter(|((id, _), _)| id == job_id)
            .max_by_key(|((_, v), _)| *v)
            .map(|(_, cp)| cp.clone())
    }
}

fn lock_inner(mutex: &Mutex<Inner>) -> Result<std::sync::MutexGuard<'_, Inner>, PortError> {
    mutex
        .lock()
        .map_err(|_| PortError::BackendUnavailable("lock".into()))
}

#[async_trait]
impl JobRepository for InMemoryJobRepository {
    /// 幂等 receipt：同键同指纹 → 原记录；不同指纹 → IdempotencyConflict。
    /// apply 类同时建立 planId 唯一消费；三者在同一锁事务内完成（§2.1）。
    async fn accept(
        &self,
        ctx: &RequestContext,
        definition: JobDefinition,
        idem: &IdempotencyKey,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        let plan =
            project_frozen_plan(&definition.kind, &definition.payload).map_err(PortError::from)?;
        let fingerprint = format!(
            "{}|{}",
            definition.kind,
            fnv1a64_hex(definition.payload.to_string().as_bytes())
        );
        let mut inner = lock_inner(&self.inner)?;
        if let Some(prev) = inner.idempotency.get(idem.as_str()) {
            if prev.accept_fingerprint == fingerprint {
                let row = inner
                    .jobs
                    .get(&prev.job_id)
                    .ok_or_else(|| PortError::NotFound(prev.job_id.as_str().into()))?;
                return Ok(row.record.clone());
            }
            return Err(PortError::IdempotencyConflict);
        }
        if let Some(plan_id) = &plan.consumed_plan_id {
            if inner.consumed_plans.contains_key(plan_id) {
                return Err(PortError::PlanAlreadyConsumed(plan_id.clone()));
            }
        }
        let now = self.clock.now();
        let record = JobRecord {
            view: datazen_platform_api::dto::job::JobView {
                job_id: definition.job_id.clone(),
                kind: definition.kind.clone(),
                state: JobState::Queued,
                stage: None,
                execution_ids: Vec::new(),
                artifact_ids: Vec::new(),
                created_at: now.clone(),
                updated_at: now,
                effect_outcome: None,
                cancel_requested: false,
                pending_verification_reason: None,
                error: None,
                progress: Default::default(),
            },
            definition: definition.clone(),
            stages: Vec::new(),
            state_version: JobStateVersion::new(1),
        };
        inner.jobs.insert(
            definition.job_id.clone(),
            JobRow {
                record: record.clone(),
                claim: None,
                cancel_requested_at: None,
            },
        );
        inner.idempotency.insert(
            idem.as_str().to_string(),
            IdemReceipt {
                job_id: definition.job_id.clone(),
                accept_fingerprint: fingerprint,
            },
        );
        if let Some(plan_id) = plan.consumed_plan_id {
            inner
                .consumed_plans
                .insert(plan_id, definition.job_id.clone());
        }
        Ok(record)
    }

    async fn get(&self, ctx: &RequestContext, job_id: JobId) -> Result<JobRecord, PortError> {
        let _ = ctx;
        #[cfg(any(test, feature = "test-harness"))]
        {
            use std::sync::atomic::Ordering;
            self.get_calls.fetch_add(1, Ordering::SeqCst);
            if self.get_fault.load(Ordering::SeqCst) {
                return Err(PortError::BackendUnavailable(
                    "injected get failure (test seam)".into(),
                ));
            }
        }
        let inner = lock_inner(&self.inner)?;
        inner
            .jobs
            .get(&job_id)
            .map(|row| row.record.clone())
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))
    }

    async fn list(
        &self,
        ctx: &RequestContext,
        filter: JobFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let _ = ctx;
        let inner = lock_inner(&self.inner)?;
        let mut out: Vec<JobRecord> = inner
            .jobs
            .values()
            .filter(|row| {
                filter.states.is_empty() || filter.states.contains(&row.record.view.state)
            })
            .filter(|row| {
                filter.owner.is_none() || filter.owner == Some(row.record.definition.owner.clone())
            })
            .map(|row| row.record.clone())
            .collect();
        out.sort_by(|a, b| a.view.created_at.cmp(&b.view.created_at));
        Ok(out)
    }

    async fn record_stage(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: StageRecord,
    ) -> Result<(), PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        if let Some(existing) = row
            .record
            .stages
            .iter_mut()
            .find(|existing| existing.stage_id == stage.stage_id)
        {
            *existing = stage.clone();
        } else {
            row.record.stages.push(stage.clone());
        }
        for execution_id in &stage.execution_ids {
            if !row.record.view.execution_ids.contains(execution_id) {
                row.record.view.execution_ids.push(execution_id.clone());
            }
        }
        row.record.view.stage = Some(stage.stage_id.as_str().to_string());
        row.record.view.updated_at = self.clock.now();
        Ok(())
    }

    async fn record_commit_boundary(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        boundary: CommitBoundary,
    ) -> Result<(), PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        if !inner.jobs.contains_key(&claim.job_id) {
            return Err(PortError::NotFound(claim.job_id.as_str().into()));
        }
        // 边界的 durable 投影：落库后不再只由 checkpoint 兜底。
        inner
            .boundaries
            .entry(claim.job_id.clone())
            .or_default()
            .push(boundary);
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        row.record.view.updated_at = self.clock.now();
        Ok(())
    }

    async fn compare_and_set_state(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        next: JobState,
    ) -> Result<JobRecord, PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        if row.record.state_version != expected {
            return Err(PortError::CasConflict {
                entity: "job",
                id: claim.job_id.as_str().into(),
            });
        }
        row.record.view.state = next;
        row.record.view.updated_at = self.clock.now();
        row.record.state_version = JobStateVersion::new(row.record.state_version.get() + 1);
        if next.is_terminal() {
            row.claim = None;
        }
        Ok(row.record.clone())
    }

    async fn claim(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
        worker: WorkerId,
    ) -> Result<JobClaim, PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(&job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if row.record.view.state.is_terminal() {
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().into(),
            });
        }
        if let Some(current) = &row.claim {
            let expired = self.clock.now().as_str() > current.expires_at.as_str();
            if row.record.view.state == JobState::Running && !expired {
                return Err(PortError::StaleClaim);
            }
            // Queued 或 claim 已过期：允许接管，generation +1。
        }
        let generation = match &row.claim {
            Some(c) => Counter::new(c.claim_generation.get() + 1),
            None => Counter::new(1),
        };
        let now = self.clock.now();
        let expires_at = after_seconds(&now, self.claim_ttl).map_err(PortError::from)?;
        let stage_id = row
            .record
            .stages
            .first()
            .map(|s| s.stage_id.clone())
            .unwrap_or_else(|| StageId::new("*"));
        let claim = JobClaim {
            job_id: job_id.clone(),
            stage_id,
            worker_id: worker,
            claimed_at: now,
            claim_generation: generation,
            expires_at,
        };
        row.claim = Some(claim.clone());
        row.record.view.state = JobState::Running;
        row.record.view.updated_at = self.clock.now();
        row.record.state_version = JobStateVersion::new(row.record.state_version.get() + 1);
        Ok(claim)
    }

    async fn renew(&self, claim: &JobClaim) -> Result<JobClaim, PortError> {
        let mut inner = lock_inner(&self.inner)?;
        let now = self.clock.now();
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        let current = row.claim.as_ref().ok_or(PortError::StaleClaim)?;
        if current.worker_id != claim.worker_id
            || current.claim_generation != claim.claim_generation
        {
            return Err(PortError::StaleClaim);
        }
        if now.as_str() > current.expires_at.as_str() {
            return Err(PortError::StaleClaim);
        }
        let expires_at = after_seconds(&now, self.claim_ttl).map_err(PortError::from)?;
        let renewed = JobClaim {
            claimed_at: now,
            expires_at,
            ..claim.clone()
        };
        row.claim = Some(renewed.clone());
        Ok(renewed)
    }

    async fn save_checkpoint(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        cp: Checkpoint,
    ) -> Result<(), PortError> {
        let _ = ctx;
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        if inner
            .checkpoints
            .contains_key(&(cp.job_id.clone(), cp.state_version.get()))
        {
            return Err(PortError::CasConflict {
                entity: "job_checkpoint",
                id: cp.job_id.as_str().into(),
            });
        }
        inner
            .checkpoints
            .insert((cp.job_id.clone(), cp.state_version.get()), cp);
        Ok(())
    }

    async fn list_recoverable(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let _ = ctx;
        let inner = lock_inner(&self.inner)?;
        let mut out: Vec<JobRecord> = inner
            .jobs
            .values()
            .filter(|row| {
                matches!(
                    row.record.view.state,
                    JobState::Queued | JobState::Running | JobState::Cancelled
                ) || row.record.view.pending_verification_reason.is_some()
            })
            .map(|row| row.record.clone())
            .collect();
        out.retain(|row| filter.kinds.is_empty() || filter.kinds.contains(&row.definition.kind));
        out.sort_by(|a, b| a.view.created_at.cmp(&b.view.created_at));
        Ok(out)
    }
}
