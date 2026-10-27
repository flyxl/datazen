//! P4 覆盖率补齐：`JobHost::mark_restart_candidates_for_verification` 的
//! **Running 分支**，钉住文档点名的关键不变量（
//! `docs/architecture/platform/data-migration-jobs.md` §2.4：reopen 只标记待核验，
//! **绝不重放 handler**）。
//!
//! 既有覆盖分布（验证轨道逐条核实）：
//! * Queued 分支 → `src-tauri/src/store/app_db_jobs_tests.rs`
//!   `accept_then_restart_before_dispatch_is_durable_not_executed_and_never_replayed`
//! * Running 分支 → 同文件的 AppState 重开旅程断言了 reason + verdict，
//!   但该旅程**没有注册任何 handler**，「绝不调用 handler」无法被计数证明；
//! * 终态分支 → 同文件 `sqlite_runtime_reopens_unknown_boundary_receipt_and_verifies_without_replay`。
//!
//! 本文件补上缺的那颗钉子：在**注册过 handler** 的宿主上跑 reopen 扫描，
//! handler 调用计数必须恒为 0，且随后经 `JobRuntime::run` 的自动重放必须被 CAS 拒绝。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use datazen::execution::EffectOutcome;
use datazen::jobs::domain::job::{
    Checkpoint, JobDefinition, JobProgress, JobRecoveryVerdict, JobState,
};
use datazen::jobs::domain::repository::JobRepository;
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ClientInstanceId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId, StageId,
    Timestamp, WorkerId,
};

use datazen::execution::runtime::budget::{ledger::BudgetLedger, quotas::BudgetConfig};
use datazen::jobs::{
    CancelToken, FrozenPlan, HandlerRegistry, InMemoryJobRepository, JobClock, JobHandler, JobHost,
    JobRuntime, RecoveryVerdict, SharedClock, StageOutcome, StageSpec, StageTerminal,
};

fn ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("user-a"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("restart-verify"),
        None,
    )
}

fn definition(job_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        // The repository stamps the accepting context's organization onto the
        // persisted definition, so this field only has to name the same
        // organization the test drives the job with.
        organization_id: OrganizationId::new("datazen-local"),
        kind: "dataSyncApply".into(),
        owner: datazen::shared::context::OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "migration".into(),
        },
        payload: serde_json::json!({
            "consumedPlanId": "plan-restart",
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "selectionRevision": 1,
        }),
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

/// 只会「数自己被调用了几次」的 handler：任何一次 run_stage 都是重放副作用。
struct CountingReplayHandler {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl JobHandler for CountingReplayHandler {
    fn kind(&self) -> &str {
        "dataSyncApply"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, _plan: &FrozenPlan) -> Result<Vec<StageSpec>, datazen::jobs::JobError> {
        Ok(vec![StageSpec {
            stage_id: StageId::new("s1"),
            kind: "apply".into(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(
        &self,
        spec: &StageSpec,
        _cancel: &CancelToken,
    ) -> Result<StageOutcome, datazen::jobs::JobError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: StageTerminal::Succeeded,
            progress: JobProgress::default(),
            commit_boundaries: Vec::new(),
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            effect_outcome: EffectOutcome::Completed,
            error_code: None,
        })
    }
    fn verify_recovery(&self, _checkpoint: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "test-handler-never-verifies".into(),
        }
    }
}

/// 不变量：reopen 扫描把 Running 作业转成 Failed/Unknown + 待核验，且**绝不调用 handler**。
#[tokio::test]
async fn mark_restart_candidates_marks_running_jobs_for_verification_without_replay() {
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
    let c = ctx();
    let id = JobId::new("job-running-at-restart");

    // 模拟上个进程留下的 Running 作业：durable accept + worker claim。
    JobRepository::accept(
        repo.as_ref(),
        &c,
        definition(id.as_str()),
        &IdempotencyKey::new("restart-key"),
    )
    .await
    .expect("durable accept");
    JobRepository::claim(
        repo.as_ref(),
        &c,
        id.clone(),
        WorkerId::new("worker-before-restart"),
    )
    .await
    .expect("simulate a running worker claim");
    assert_eq!(
        repo.get(&c, id.clone()).await.expect("get").view.state,
        JobState::Running,
        "前置条件：作业应为 Running"
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = HandlerRegistry::new();
    registry.register(Arc::new(CountingReplayHandler {
        calls: calls.clone(),
    }));
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default())));
    let clock_trait: Arc<dyn JobClock> = clock.clone();
    let host = JobHost::new(repo.clone(), ledger.clone(), clock_trait.clone());

    // 重启扫描：Running → Failed/Unknown + restartNeedsVerification + 待核验判决。
    let marked = host
        .mark_restart_candidates_for_verification(&c, Default::default())
        .await
        .expect("restart scan should succeed");
    assert_eq!(marked.len(), 1, "唯一的 Running 候选应被标出");
    assert_eq!(marked[0].view.state, JobState::Failed);
    assert_eq!(
        marked[0].view.effect_outcome,
        Some(EffectOutcome::Unknown),
        "被 claim 过的 Running 作业效果必须是 Unknown，不能谎报 NotStarted"
    );
    assert_eq!(
        marked[0].view.error.as_deref(),
        Some("restartNeedsVerification"),
        "reason 必须是对外稳定字面码 restartNeedsVerification"
    );
    assert_eq!(
        marked[0].view.pending_verification_reason.as_deref(),
        Some("restartNeedsVerification")
    );
    let details = host.details(&c, id.clone()).await.expect("details");
    assert_eq!(
        details.recovery.as_ref().map(|result| result.verdict),
        Some(JobRecoveryVerdict::PendingVerification),
        "扫描必须落一份 PendingVerification 判决"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "核心不变量：reopen 扫描绝不派发、绝不调用 handler"
    );

    // 防呆二：即便此刻有活 handler 注册在案，重走 runtime 也必须被 CAS 拒。
    let runtime = JobRuntime::new(repo.clone(), Arc::new(registry), ledger, clock_trait, 0);
    let replay = runtime
        .run(&c, &id, &WorkerId::new("worker-after-restart"), &[])
        .await;
    assert!(
        matches!(replay, Err(PortError::CasConflict { .. })),
        "已终态的作业不得被重放，实得 {replay:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "重放尝试同样不得调用 handler"
    );
}
