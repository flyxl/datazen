//! JobRuntime：阶段调度、取消意图、进度与效果结局聚合（§10.1.1 / §2.3）。
//!
//! 边界：
//!
//! * runtime 负责接受记录、claim、预算、阶段调度、取消意图、事件和 cleanup；
//!   handler 负责 validatePlan/runStage/verifyRecovery（§10.1.1）。
//! * 取消请求是独立意图，不提前改 state；排队取消在调度器确认 notStarted 后终结；
//!   running 取消禁止新增阶段/批次，按实际终态核验。
//! * 效果结局聚合：未派发 notStarted；全部确认完成 completed；全部确认回滚 rolledBack；
//!   存在已提交且未完成 partiallyApplied；任一不可核验 unknown（保留已确认部分）。

use std::sync::{Arc, Mutex};

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{Checkpoint, JobProgress, JobState, StageRecord};
use datazen_platform_api::id::JobStateVersion;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{JobId, WorkerId};

use datazen_platform_api::ports::job::JobRepository;

use crate::budget::ledger::BudgetLedger;
use crate::job::budget::{EndpointRef, MultiEndpointPermits};
use crate::job::handler::{CancelToken, HandlerRegistry, StageTerminal};
use crate::job::plan::project_frozen_plan;
use crate::job::repository::InMemoryJobRepository;
use crate::job::time::JobClock;

/// Job 执行的最终投影。`state` 是 JobState（successful/failed/cancelled），
/// `effect_outcome` 独立表达提交边界，二者正交（§13.1 思想在 Job 上对齐）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobResult {
    pub state: JobState,
    pub effect_outcome: EffectOutcome,
    pub progress: JobProgress,
    pub error: Option<String>,
}

/// Job 运行时编排器。
pub struct JobRuntime {
    repo: Arc<InMemoryJobRepository>,
    handlers: Arc<HandlerRegistry>,
    ledger: Arc<Mutex<BudgetLedger>>,
    clock: Arc<dyn JobClock>,
    clock_ms: u64,
}

impl JobRuntime {
    pub fn new(
        repo: Arc<InMemoryJobRepository>,
        handlers: Arc<HandlerRegistry>,
        ledger: Arc<Mutex<BudgetLedger>>,
        clock: Arc<dyn JobClock>,
        clock_ms: u64,
    ) -> Self {
        Self {
            repo,
            handlers,
            ledger,
            clock,
            clock_ms,
        }
    }

    /// 执行一个 Job 直到终态或待核验。调用前 Job 必须已经被 accept（queued）。
    pub async fn run(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        worker: &WorkerId,
        endpoints: &[EndpointRef],
    ) -> Result<JobResult, PortError> {
        let job = self.repo.get(ctx, job_id.clone()).await?;
        if job.view.state != JobState::Queued {
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().into(),
            });
        }
        let plan = project_frozen_plan(&job.definition.kind, &job.definition.payload)
            .map_err(PortError::from)?;
        let handler = match self
            .handlers
            .resolve(&job.definition.kind, plan.handler_version)
        {
            Some(h) => h,
            None => {
                let _ = self
                    .repo
                    .mark_pending_verification(ctx, job_id, "handlerNotRegistered");
                return Err(PortError::UnsupportedVersion(format!(
                    "handler {}@{} not registered",
                    job.definition.kind, plan.handler_version
                )));
            }
        };
        // 版本/能力/指纹复验：失败即停止派发，不持有任何预算（§3 条 5）。
        let stages = match handler.validate_plan(&plan) {
            Ok(s) => s,
            Err(e) => {
                self.repo
                    .mark_failed_unstarted(ctx, job_id, &e.to_string()).await?;
                return Err(PortError::from(e));
            }
        };
        // 排队取消已登记：调度器确认 notStarted 后终结，不进入预算/claim。
        let latest = self.repo.get(ctx, job_id.clone()).await?;
        if latest.view.cancel_requested {
            let cancelled = self
                .repo
                .confirm_cancelled_not_started(ctx, job_id, EffectOutcome::NotStarted)?;
            return Ok(JobResult {
                state: cancelled.view.state,
                effect_outcome: EffectOutcome::NotStarted,
                progress: JobProgress::default(),
                error: None,
            });
        }
        // §3 多端原子预算：重叠/不足 → 无许可持有。
        let permits = match MultiEndpointPermits::reserve(
            self.ledger.clone(),
            endpoints,
            datazen_platform_api::ports::budget::ResourceClass::Job,
            &ctx.organization_id,
            &ctx.principal_id,
            self.clock_ms,
        ) {
            Ok(p) => p,
            Err(e) => return Err(PortError::from(e)),
        };
        let claim = self.repo.claim(ctx, job_id.clone(), worker.clone()).await?;
        let cancel = CancelToken::new();
        let mut progress = JobProgress::default();
        let mut any_boundary = false;
        let mut saw_unknown = false;
        let mut saw_failed = false;
        let mut saw_cancelled = false;
        for spec in &stages {
            let latest = self.repo.get(ctx, job_id.clone()).await?;
            if latest.view.cancel_requested {
                cancel.cancel();
            }
            let stage_record = StageRecord {
                job_id: job_id.clone(),
                stage_id: spec.stage_id.clone(),
                kind: spec.kind.clone(),
                claimed_by: Some(worker.clone()),
                execution_ids: Vec::new(),
                started_at: Some(self.clock.now()),
                finished_at: None,
            };
            self.repo.record_stage(ctx, &claim, stage_record).await?;
            let outcome = handler
                .run_stage(spec, &cancel)
                .await
                .map_err(|e| PortError::BackendUnavailable(e.to_string()))?;
            progress = accumulate(progress, outcome.progress);
            any_boundary |= !outcome.commit_boundaries.is_empty();
            saw_unknown |= outcome.terminal == StageTerminal::Unknown
                || outcome.effect_outcome == EffectOutcome::Unknown;
            saw_failed |= outcome.terminal == StageTerminal::Failed;
            saw_cancelled |= outcome.terminal == StageTerminal::Cancelled;
            for b in &outcome.commit_boundaries {
                self.repo
                    .record_commit_boundary(ctx, &claim, b.clone())
                    .await?;
                let checkpoint = Checkpoint {
                    job_id: job_id.clone(),
                    state_version: JobStateVersion::new(
                        self.repo
                            .get(ctx, job_id.clone())
                            .await?
                            .state_version
                            .get(),
                    ),
                    stable_target_fingerprint: b.stable_target_fingerprint.clone(),
                    committed: vec![b.clone()],
                    verification_evidence: b.evidence.clone(),
                    recovery_policy: "resumeAfterVerify".to_string(),
                };
                let _ = self.repo.save_checkpoint(ctx, &claim, checkpoint).await;
            }
            if outcome.terminal != StageTerminal::Succeeded {
                break;
            }
        }
        let (state, effect) = match (saw_unknown, saw_failed, saw_cancelled, any_boundary) {
            (true, ..) => (JobState::Failed, EffectOutcome::Unknown),
            (_, true, _, true) => (JobState::Failed, EffectOutcome::PartiallyApplied),
            (_, true, _, false) => (JobState::Failed, EffectOutcome::RolledBack),
            (_, _, true, true) => (JobState::Cancelled, EffectOutcome::PartiallyApplied),
            (_, _, true, false) => (JobState::Cancelled, EffectOutcome::RolledBack),
            _ => (JobState::Succeeded, EffectOutcome::Completed),
        };
        let latest = self.repo.get(ctx, job_id.clone()).await?;
        self.repo
            .compare_and_set_state(ctx, &claim, latest.state_version, state)
            .await?;
        self.repo.set_effect_outcome(ctx, &claim, effect)?;
        if saw_unknown {
            let _ = self
                .repo
                .mark_pending_verification(ctx, job_id, "unknownCommitBoundary");
        }
        permits.release(true);
        Ok(JobResult {
            state,
            effect_outcome: effect,
            progress,
            error: None,
        })
    }
}

fn accumulate(mut acc: JobProgress, delta: JobProgress) -> JobProgress {
    acc.read = acc.read.saturating_increment_by(delta.read.get());
    acc.converted = acc.converted.saturating_increment_by(delta.converted.get());
    acc.attempted = acc.attempted.saturating_increment_by(delta.attempted.get());
    acc.committed = acc.committed.saturating_increment_by(delta.committed.get());
    acc.unknown = acc.unknown.saturating_increment_by(delta.unknown.get());
    acc
}
