//! JobRuntime 生命周期旅程（成功路径、许可释放、未知结果、取消）。
//!
//! 从 `job_kernel.rs` 拆出，理由是单文件 ≤800 行纪律（AGENTS.md「单文件规模与模块拆分」）。
//! 拆分只搬运代码：夹具与导入经 `use super::*;` 全部来自父模块，断言与被测调用逐字未改。

use super::*;

// ------------------------------------------------ runtime 生命周期旅程
fn runtime_with(
    repo: Arc<InMemoryJobRepository>,
    handler: Arc<dyn JobHandler>,
    ledger: Arc<Mutex<BudgetLedger>>,
    clock: Arc<SharedClock>,
) -> JobRuntime {
    let mut handlers = HandlerRegistry::new();
    handlers.register(handler);
    JobRuntime::new(repo, Arc::new(handlers), ledger, clock, 0)
}

#[tokio::test]
async fn runtime_success_path_records_boundaries_checkpoints_and_progress() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-r1"), "job-r1"),
        &IdempotencyKey::new("r1"),
    )
    .await
    .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let runtime = runtime_with(
        repo,
        Arc::new(CountingHandler {
            kind: "schemaDiffApply",
            calls: calls.clone(),
            mode: Mode::Success,
        }),
        ledger,
        clock,
    );
    let result = runtime
        .run(
            &c,
            &JobId::new("job-r1"),
            &WorkerId::new("w1"),
            &[EndpointRef {
                connection_id: conn(),
                service_key: "svc".into(),
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            }],
        )
        .await
        .expect("run");
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert_eq!(result.progress.committed.get(), 10);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "handler 只执行一次");
}

/// 失败型 handler：run_stage 直接 Err。
struct ErrHandler {
    panic: bool,
    committed_first: bool,
}

#[async_trait::async_trait]
impl JobHandler for ErrHandler {
    fn kind(&self) -> &str {
        "schemaDiffApply"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(
        &self,
        _plan: &FrozenPlan,
    ) -> Result<Vec<StageSpec>, datazen_runtime::job::JobError> {
        let mut stages = vec![StageSpec {
            stage_id: StageId::new("s1"),
            kind: "apply".into(),
            depends_on: vec![],
        }];
        if self.committed_first {
            stages.push(StageSpec {
                stage_id: StageId::new("s2"),
                kind: "apply".into(),
                depends_on: vec![],
            });
        }
        Ok(stages)
    }
    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, datazen_runtime::job::JobError> {
        if self.committed_first && spec.stage_id == StageId::new("s1") {
            return CountingHandler {
                kind: "schemaDiffApply",
                calls: Arc::new(AtomicUsize::new(0)),
                mode: Mode::TwoBoundaries,
            }
            .run_stage(spec, cancel)
            .await;
        }
        if self.panic {
            panic!("synthetic panic payload");
        }
        Err(datazen_runtime::job::JobError::BudgetDenied(
            "synthetic backend secret".into(),
        ))
    }
    fn verify_recovery(
        &self,
        _checkpoint: &datazen_platform_api::dto::job::Checkpoint,
    ) -> RecoveryVerdict {
        RecoveryVerdict::ResumeAfterVerify { resume_through: 0 }
    }
}

/// Handler Err converges durably; unknown side effects are not called rollback.
#[tokio::test]
async fn run_stage_error_propagates_and_releases_all_permits() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-e1"), "job-e1"),
        &IdempotencyKey::new("e1"),
    )
    .await
    .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let repo = Arc::new(repo);
    let runtime = runtime_with(
        repo.clone(),
        Arc::new(ErrHandler {
            panic: false,
            committed_first: false,
        }),
        ledger.clone(),
        clock,
    );
    let result = runtime
        .run(
            &c,
            &JobId::new("job-e1"),
            &WorkerId::new("w1"),
            &[EndpointRef {
                connection_id: conn(),
                service_key: "svc".into(),
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            }],
        )
        .await
        .expect("converged failure");
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(
        result.error.as_deref(),
        Some("handlerStageFailedOrPanicked")
    );
    assert_eq!(runtime.active_cancel_watchers(), 0);
    let queried = repo.get(&c, JobId::new("job-e1")).await.expect("query");
    assert_eq!(queried.view.state, JobState::Failed);
    assert_eq!(queried.view.effect_outcome, Some(EffectOutcome::Unknown));
    assert_eq!(
        ledger.lock().expect("lock").permits().count(),
        0,
        "失败路径不得残留许可"
    );
}

/// D1/D2：成功路径许可应已核销（count==0）；单 stage 两条边界 → committed_boundaries 读出 2 条。
#[tokio::test]
async fn success_path_releases_permits_and_persists_both_boundaries() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-e2"), "job-e2"),
        &IdempotencyKey::new("e2"),
    )
    .await
    .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let repo_for_read = repo.clone();
    let runtime = runtime_with(
        repo,
        Arc::new(CountingHandler {
            kind: "schemaDiffApply",
            calls,
            mode: Mode::TwoBoundaries,
        }),
        ledger.clone(),
        clock,
    );
    let result = runtime
        .run(
            &c,
            &JobId::new("job-e2"),
            &WorkerId::new("w1"),
            &[EndpointRef {
                connection_id: conn(),
                service_key: "svc".into(),
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            }],
        )
        .await
        .expect("run");
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(
        ledger.lock().expect("lock").permits().count(),
        0,
        "成功路径许可已核销"
    );
    let persisted = repo_for_read.committed_boundaries(&JobId::new("job-e2"));
    assert_eq!(
        persisted.len(),
        2,
        "单 stage 两条边界必须全部落库: {persisted:?}"
    );
    assert_eq!(persisted[0].batch_id.as_deref(), Some("batch-1"));
    assert_eq!(persisted[1].batch_id.as_deref(), Some("batch-2"));
}

#[tokio::test]
async fn runtime_unknown_outcome_preserves_unknown_and_pending_reason() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-r2"), "job-r2"),
        &IdempotencyKey::new("r2"),
    )
    .await
    .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let runtime = runtime_with(
        repo,
        Arc::new(CountingHandler {
            kind: "schemaDiffApply",
            calls: calls.clone(),
            mode: Mode::Unknown,
        }),
        ledger,
        clock,
    );
    let result = runtime
        .run(
            &c,
            &JobId::new("job-r2"),
            &WorkerId::new("w1"),
            &[EndpointRef {
                connection_id: conn(),
                service_key: "svc".into(),
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            }],
        )
        .await
        .expect("run");
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn queued_cancel_before_run_finalizes_not_started_without_budget_or_claim() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-r3"), "job-r3"),
        &IdempotencyKey::new("r3"),
    )
    .await
    .expect("accept");
    repo.request_cancel(&c, &JobId::new("job-r3"))
        .expect("intent");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let runtime = runtime_with(
        repo,
        Arc::new(CountingHandler {
            kind: "schemaDiffApply",
            calls: calls.clone(),
            mode: Mode::Success,
        }),
        ledger.clone(),
        clock,
    );
    let result = runtime
        .run(
            &c,
            &JobId::new("job-r3"),
            &WorkerId::new("w1"),
            &[EndpointRef {
                connection_id: conn(),
                service_key: "svc".into(),
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            }],
        )
        .await
        .expect("run");
    assert_eq!(result.state, JobState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "取消后不派发");
    assert_eq!(
        ledger.lock().expect("lock").permits().count(),
        0,
        "不持有许可"
    );
}

#[tokio::test]
async fn panic_converges_before_return_and_can_be_queried() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("panic-plan"), "panic-job"),
        &IdempotencyKey::new("panic-key"),
    )
    .await
    .expect("accept");
    let repo = Arc::new(repo);
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("ledger").ensure_service(&conn());
    let runtime = runtime_with(
        repo.clone(),
        Arc::new(ErrHandler {
            panic: true,
            committed_first: true,
        }),
        ledger.clone(),
        clock,
    );
    let result = runtime
        .run(
            &c,
            &JobId::new("panic-job"),
            &WorkerId::new("worker"),
            &[EndpointRef {
                connection_id: conn(),
                service_key: "svc".into(),
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            }],
        )
        .await
        .expect("panic contained");
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(runtime.active_cancel_watchers(), 0);
    assert_eq!(ledger.lock().expect("ledger").permits().count(), 0);
    let queried = repo
        .get(&c, JobId::new("panic-job"))
        .await
        .expect("query after panic");
    assert_eq!(queried.view.state, JobState::Failed);
    assert_eq!(repo.committed_boundaries(&JobId::new("panic-job")).len(), 2);
    assert!(repo
        .latest_checkpoint(&c, &JobId::new("panic-job"))
        .is_some());
    assert_eq!(
        queried.view.pending_verification_reason.as_deref(),
        Some("handlerStageFailedOrPanicked")
    );
    assert!(
        runtime
            .run(&c, &JobId::new("panic-job"), &WorkerId::new("worker"), &[])
            .await
            .is_err(),
        "no implicit replay"
    );
}

struct ValidationPanicHandler;
#[async_trait::async_trait]
impl JobHandler for ValidationPanicHandler {
    fn kind(&self) -> &str {
        "schemaDiffApply"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(
        &self,
        _: &FrozenPlan,
    ) -> Result<Vec<StageSpec>, datazen_runtime::job::JobError> {
        panic!("synthetic validation panic");
    }
    async fn run_stage(
        &self,
        _: &StageSpec,
        _: &CancelToken,
    ) -> Result<StageOutcome, datazen_runtime::job::JobError> {
        panic!("validation must prevent dispatch");
    }
    fn verify_recovery(&self, _: &datazen_platform_api::dto::job::Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "test".into(),
        }
    }
}

#[tokio::test]
async fn validation_panic_is_unstarted_failure_and_never_claims_budget() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(
        &c,
        definition(
            "schemaDiffApply",
            apply_payload("validation-plan"),
            "validation-job",
        ),
        &IdempotencyKey::new("validation-key"),
    )
    .await
    .expect("accept");
    let repo = Arc::new(repo);
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    let runtime = runtime_with(
        repo.clone(),
        Arc::new(ValidationPanicHandler),
        ledger.clone(),
        clock,
    );
    assert!(runtime
        .run(
            &c,
            &JobId::new("validation-job"),
            &WorkerId::new("worker"),
            &[]
        )
        .await
        .is_err());
    let queried = repo
        .get(&c, JobId::new("validation-job"))
        .await
        .expect("query");
    assert_eq!(queried.view.state, JobState::Failed);
    assert_eq!(queried.view.effect_outcome, Some(EffectOutcome::NotStarted));
    assert_eq!(
        queried.view.pending_verification_reason.as_deref(),
        Some("handlerValidationPanicked")
    );
    assert_eq!(ledger.lock().expect("ledger").permits().count(), 0);
    assert_eq!(runtime.active_cancel_watchers(), 0);
}
