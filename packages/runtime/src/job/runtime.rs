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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{Checkpoint, JobProgress, JobState, StageRecord};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{JobId, WorkerId};

use datazen_platform_api::ports::job::JobRepository;

use crate::budget::ledger::BudgetLedger;
use crate::job::budget::{EndpointRef, MultiEndpointPermits};
use crate::job::handler::{CancelToken, HandlerRegistry, StageOutcome, StageSpec, StageTerminal};
use crate::job::plan::project_frozen_plan;
use crate::job::repository::InMemoryJobRepository;
use crate::job::time::JobClock;

/// 阶段内取消看守者的轮询间隔。
///
/// 代价不是「进程内字典查找」：`InMemoryJobRepository` 的全部索引共用一把全局
/// `Mutex`，而 `JobRepository::get` 在**持锁期间**深拷贝整条 `JobRecord`
/// （`definition.payload` 是 `serde_json::Value`，另有 `Vec<StageRecord>`），
/// 每个运行中的阶段每秒会因此拷贝 20 次，并与 `request_cancel`、`record_stage`
/// 和进度写入争锁。所以看守者走 [`InMemoryJobRepository::cancel_poll`]：同一个锁，
/// 只读两个 bool，锁内不做任何拷贝。
///
/// 50ms 是响应延迟与争锁次数之间的折中：取消是低频的用户动作，这个延迟用户感知不到。
///
/// **这是该间隔的唯一真源（single source of truth）。** 它是 `pub` 的，因为
/// 两侧测试的观察窗口必须从它推导，而不是各自写死一个「大概差不多」的毫秒数：
/// 写死的观察窗口在间隔被调大后会**静默失效**——窗口比一个轮询周期还短时，
/// 「读次数不再增长」这句话恒成立，看门狗是死是活都测不出来。
/// 改这里之前先看 `packages/runtime/tests/job_cancel_watch/poll_interval.rs`
/// 里的两条需求边界断言。
pub const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
    /// 当前在飞的阶段内取消看守者数量。每个阶段 +1，阶段收尾时 -1；
    /// 恒为 0 是「没有游离任务」的运行时判据。
    watchers: Arc<AtomicUsize>,
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
            watchers: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// 在飞的阶段内取消看守者数量。`dispatch` 收尾后必须为 0：每个阶段都会先把
    /// 自己的看守者 abort + await 掉才进入下一个阶段。
    pub fn active_cancel_watchers(&self) -> usize {
        self.watchers.load(Ordering::SeqCst)
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
        let plan = match project_frozen_plan(&job.definition.kind, &job.definition.payload) {
            Ok(plan) => plan,
            Err(error) => {
                self.repo
                    .mark_failed_unstarted(ctx, job_id, "invalidFrozenPlan")
                    .await?;
                return Err(PortError::from(error));
            }
        };
        let handler = match self
            .handlers
            .resolve(&job.definition.kind, plan.handler_version)
        {
            Some(h) => h,
            None => {
                self.repo
                    .mark_failed_unstarted(ctx, job_id, "handlerNotRegistered")
                    .await?;
                return Err(PortError::UnsupportedVersion(format!(
                    "handler {}@{} not registered",
                    job.definition.kind, plan.handler_version
                )));
            }
        };
        // 版本/能力/指纹复验：失败即停止派发，不持有任何预算（§3 条 5）。
        let validation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler.validate_plan(&plan)
        }));
        let stages = match validation {
            Err(_) => {
                self.repo
                    .mark_failed_unstarted(ctx, job_id, "handlerValidationPanicked")
                    .await?;
                return Err(PortError::BackendUnavailable(
                    "handlerValidationPanicked".into(),
                ));
            }
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                self.repo
                    .mark_failed_unstarted(ctx, job_id, "handlerValidationFailed")
                    .await?;
                return Err(PortError::from(e));
            }
        };
        // 排队取消已登记：调度器确认 notStarted 后终结，不进入预算/claim。
        let latest = self.repo.get(ctx, job_id.clone()).await?;
        if latest.view.cancel_requested {
            let cancelled =
                self.repo
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
            Err(e) => {
                self.repo
                    .mark_failed_unstarted(ctx, job_id, "jobBudgetRejected")
                    .await?;
                return Err(PortError::from(e));
            }
        };
        // D1：派发/收尾任一路径失败，必须释放全部许可；成功路径才按 consumed 核销。
        let dispatched = self.dispatch(ctx, job_id, worker, handler, &stages).await;
        match dispatched {
            Ok(result) => {
                permits.release(true);
                Ok(result)
            }
            Err(err) => {
                permits.release(false);
                Err(err)
            }
        }
    }

    /// claim 之后的派发与收尾。错误会向上抛给 [`Self::run`]，由其统一释放预算许可。
    async fn dispatch(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        worker: &WorkerId,
        handler: Arc<dyn crate::job::handler::JobHandler>,
        stages: &[crate::job::handler::StageSpec],
    ) -> Result<JobResult, PortError> {
        let claim = self.repo.claim(ctx, job_id.clone(), worker.clone()).await?;
        let result = self
            .dispatch_claimed(ctx, job_id, worker, handler, stages, &claim)
            .await;
        if result.is_err() {
            // Fenced atomic terminal write: a stale worker cannot overwrite a new owner.
            self.repo.fail_claimed(ctx, &claim, "jobDispatchFailed")?;
        }
        result
    }

    async fn dispatch_claimed(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        worker: &WorkerId,
        handler: Arc<dyn crate::job::handler::JobHandler>,
        stages: &[StageSpec],
        claim: &datazen_platform_api::dto::job::JobClaim,
    ) -> Result<JobResult, PortError> {
        let cancel = CancelToken::new();
        let mut progress = JobProgress::default();
        let mut any_boundary = false;
        let mut saw_unknown = false;
        let mut saw_failed = false;
        let mut saw_cancelled = false;
        let mut committed_all = Vec::new();
        let mut failure_reason = None;
        let mut failure_effect = EffectOutcome::NotStarted;
        for spec in stages {
            let latest = self.repo.get(ctx, job_id.clone()).await?;
            if latest.view.cancel_requested {
                cancel.cancel();
            }
            if cancel.is_cancelled() {
                saw_cancelled = true;
                break;
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
            self.repo.record_stage(ctx, claim, stage_record).await?;
            let outcome = match self
                .run_stage_watched(ctx, job_id, handler.as_ref(), spec, &cancel)
                .await
            {
                Ok(outcome) => outcome,
                Err(_) => {
                    saw_failed = true;
                    saw_unknown = true;
                    failure_reason = Some("handlerStageFailedOrPanicked".to_string());
                    break;
                }
            };
            if outcome.terminal != StageTerminal::Succeeded {
                failure_effect = outcome.effect_outcome;
                failure_reason = Some(
                    if outcome.terminal == StageTerminal::Unknown {
                        "unknownCommitBoundary"
                    } else {
                        "handlerStageTerminated"
                    }
                    .to_string(),
                );
            }
            progress = accumulate(progress, outcome.progress);
            any_boundary |= !outcome.commit_boundaries.is_empty();
            saw_unknown |= outcome.terminal == StageTerminal::Unknown
                || outcome.effect_outcome == EffectOutcome::Unknown;
            saw_failed |= outcome.terminal == StageTerminal::Failed;
            saw_cancelled |= outcome.terminal == StageTerminal::Cancelled;
            for b in &outcome.commit_boundaries {
                self.repo
                    .record_commit_boundary(ctx, claim, b.clone())
                    .await?;
                committed_all.push(b.clone());
            }
            if outcome.terminal != StageTerminal::Succeeded {
                break;
            }
        }
        // D2：边界逐条已落库；checkpoint 聚合一次性写入且错误不被吞。
        if !committed_all.is_empty() {
            let latest_job = self.repo.get(ctx, job_id.clone()).await?;
            let fingerprint = committed_all
                .first()
                .map(|b| b.stable_target_fingerprint.clone())
                .unwrap_or_default();
            let evidence: Vec<String> = committed_all
                .iter()
                .flat_map(|b| b.evidence.iter().cloned())
                .collect();
            let checkpoint = Checkpoint {
                job_id: job_id.clone(),
                state_version: latest_job.state_version,
                stable_target_fingerprint: fingerprint,
                committed: committed_all,
                verification_evidence: evidence,
                recovery_policy: "resumeAfterVerify".to_string(),
            };
            self.repo.save_checkpoint(ctx, claim, checkpoint).await?;
        }
        let (state, effect) = match (saw_unknown, saw_failed, saw_cancelled, any_boundary) {
            (true, ..) => (JobState::Failed, EffectOutcome::Unknown),
            (_, true, _, true) => (JobState::Failed, EffectOutcome::PartiallyApplied),
            (_, true, _, false) => (JobState::Failed, failure_effect),
            (_, _, true, true) => (JobState::Cancelled, EffectOutcome::PartiallyApplied),
            (_, _, true, false) => (JobState::Cancelled, failure_effect),
            _ => (JobState::Succeeded, EffectOutcome::Completed),
        };
        let latest = self.repo.get(ctx, job_id.clone()).await?;
        // 终态 CAS 会清 claim，因此先在仍有效的 claim 下写效果结局，再落终态。
        self.repo.set_effect_outcome(ctx, claim, effect)?;
        self.repo
            .compare_and_set_state(ctx, claim, latest.state_version, state)
            .await?;
        if let Some(reason) = &failure_reason {
            self.repo.mark_pending_verification(ctx, job_id, reason)?;
        }
        Ok(JobResult {
            state,
            effect_outcome: effect,
            progress,
            error: failure_reason,
        })
    }

    /// Executes in the dispatch task; stage faults are caught before watcher abort + await.
    async fn run_stage_watched(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        handler: &dyn crate::job::handler::JobHandler,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, PortError> {
        let watch = CancelWatch::spawn(
            self.repo.clone(),
            ctx.clone(),
            job_id.clone(),
            cancel.clone(),
            self.watchers.clone(),
        );
        // Catch each poll in this task: no detached handler may continue writing after return.
        // Never copy a panic payload or backend error into a user-visible reason.
        let mut stage = handler.run_stage(spec, cancel);
        let outcome = std::future::poll_fn(|cx| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stage.as_mut().poll(cx)))
            {
                Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
                Ok(std::task::Poll::Ready(result)) => {
                    std::task::Poll::Ready(result.map_err(|_| ()))
                }
                Err(_) => std::task::Poll::Ready(Err(())),
            }
        })
        .await;
        // 先 abort 再 await：await 返回之后这条任务确定已经结束，不会在
        // dispatch 返回后继续读仓储、继续持有请求上下文。
        watch.join().await;
        outcome.map_err(|_| PortError::BackendUnavailable("handlerStageFailedOrPanicked".into()))
    }
}

/// 阶段内取消看守者的句柄：正常路径 `join`（abort + await），异常展开时 `Drop` 兜底
/// abort，两者都不会留下游离任务。计数器在两条路径上都要减，所以放在 `Drop`。
struct CancelWatch {
    handle: Option<tokio::task::JoinHandle<()>>,
    live: Arc<AtomicUsize>,
}

impl CancelWatch {
    fn spawn(
        repo: Arc<InMemoryJobRepository>,
        ctx: RequestContext,
        job_id: JobId,
        cancel: CancelToken,
        live: Arc<AtomicUsize>,
    ) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        let handle = tokio::spawn(watch_cancel_request(repo, ctx, job_id, cancel));
        Self {
            handle: Some(handle),
            live,
        }
    }

    /// 终止并等待看守者结束。调用方必须 await 它，`dispatch` 才允许继续。
    async fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for CancelWatch {
    fn drop(&mut self) {
        // 阶段 panic 导致 `join` 没跑到时，这里仍然 abort；走到 `join` 时句柄已被取走。
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
        // 计数只在 Drop 里归还，所以 `join`（正常路径）与展开（异常路径）两条路都不会漏减。
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 阶段内取消看守者：与 `run_stage` 并发轮询仓储的取消意图。
///
/// 取消请求只能落在 `request_cancel`（§2.3：取消是意图，不改 state），而阶段执行
/// 期间没有别的读点，所以阶段内要看到取消就必须有这么一个观察者。它读到
/// `cancel_requested` 为真就翻转内核那一个 [`CancelToken`] 并立即退出——此后内核
/// 的 `cancel()` 调用有且只有这一个来源，handler 不再需要自己那一条通道。
async fn watch_cancel_request(
    repo: Arc<InMemoryJobRepository>,
    ctx: RequestContext,
    job_id: JobId,
    cancel: CancelToken,
) {
    loop {
        match repo.cancel_poll(&ctx, &job_id) {
            Ok(snapshot) => {
                if snapshot.cancel_requested {
                    cancel.cancel();
                    return;
                }
                // Job 已终结：派发已经收尾，继续轮询没有意义。
                if snapshot.terminal {
                    return;
                }
            }
            Err(error) => {
                // CANCEL_POLL_FAILED：轮询失败时的**刻意决策**——记告警后停止轮询，
                // 让当前阶段跑完，而不是把读失败升级成阶段失败。
                //
                // 理由：
                // * 读失败不会造出取消，只会让我们错过取消；误取消则不可挽回——一个
                //   健康的迁移 Job 被写成 Cancelled，用户没有任何手段改回来。宁可漏，
                //   不可错。
                // * 漏掉的取消不是被丢弃，而是**不再被本阶段感知**：如果这个 Job 还有
                //   下一个阶段，`dispatch` 开跑前会重读到它，在那个边界收敛。data-transfer
                //   侧真正的单阶段 Job 是 `prepare`、SQL 文件目标的 `apply`，以及
                //   `structure` / `data` 模式的 `apply`；`structureAndData` 模式的 `apply`
                //   展开为 structure → data → foreignKeys 三个阶段，因此**有**下一个边界，
                //   取消会在这些阶段边界被重新读到，不会在完全不感知取消的情况下走完。
                // * fail-closed 代价更高：一次瞬时故障就打断一个已经提交了若干批、
                //   带 checkpoint 可续跑的长任务，而故障通常与迁移本身无关。
                // * 绝不允许静默吞掉：告警带上 jobId 与错误原文，排障时可定位。
                tracing::warn!(
                    job_id = job_id.as_str(),
                    error = %error,
                    "cancel watch stopped polling after a job repository read failure; \
                     the running stage is left alone and the cancel intent is re-read at \
                     the next stage boundary, if there is one"
                );
                return;
            }
        }
        tokio::time::sleep(CANCEL_POLL_INTERVAL).await;
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
