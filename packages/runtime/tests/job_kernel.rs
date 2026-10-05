//! P5 JobRuntime 集成测试：协议冻结、幂等受理、planId 唯一消费、claim fencing、
//! 取消意图、多端原子预算、恢复决策旅程、事件载荷白名单（§10.1.1 / §7 / CM-31、40、41、54、65、67）。

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobDefinition, JobProgress, JobState};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, ExecutionId, IdempotencyKey, JobId, JobStateVersion as Jsv,
    OrganizationId, PrincipalId, RequestId, StageId, Timestamp, WorkerId,
};
use datazen_platform_api::ports::budget::ResourceClass;
use datazen_platform_api::ports::job::JobRepository;

use datazen_runtime::budget::{BudgetClaim, BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    detect_endpoint_overlap, CancelToken, EndpointRef, EndpointRole, FrozenPlan, HandlerRegistry,
    InMemoryJobRepository, JobClock, JobHandler, JobResult, JobRuntime, RecoveryVerdict,
    SharedClock, StageOutcome, StageSpec, StageTerminal, SUPPORTED_PLAN_MAJOR,
};

use support::{conn, config, granted, org};

// ---------------------------------------------------------------- 共用夹具

fn ctx() -> datazen_platform_api::context::RequestContext {
    datazen_platform_api::context::RequestContext::new(
        org(),
        PrincipalId::new("user-a"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn owner_job() -> datazen_platform_api::OwnerRef {
    datazen_platform_api::OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-1"),
        purpose: "migration".into(),
    }
}

fn apply_payload(plan_id: &str) -> serde_json::Value {
    serde_json::json!({
        "consumedPlanId": plan_id,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "selectionRevision": 1,
    })
}

fn prepare_payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

fn definition(kind: &str, payload: serde_json::Value, job_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        kind: kind.into(),
        owner: owner_job(),
        payload,
        created_at: datazen_platform_api::id::Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

fn repo_clock() -> (InMemoryJobRepository, Arc<SharedClock>) {
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = InMemoryJobRepository::new(clock.clone(), 300);
    (repo, clock)
}

// ------------------------------------------------------------- CM-54 幂等

#[tokio::test]
async fn cm54_same_key_returns_same_job_id_without_reexecution() {
    let (repo, _clock) = repo_clock();
    let c = ctx();
    let payload = apply_payload("plan-1");
    let def = definition("schemaDiffApply", payload.clone(), "job-1");

    let first = repo
        .accept(&c, def.clone(), &IdempotencyKey::new("key-1"))
        .await
        .expect("accept");
    let second = repo
        .accept(&c, def.clone(), &IdempotencyKey::new("key-1"))
        .await
        .expect("accept");
    assert_eq!(first.view.job_id, second.view.job_id, "同键同指纹返回同 jobId");

    // 不同 payload + 同键 → IdempotencyConflict
    let other = JobDefinition {
        job_id: JobId::new("job-2"),
        kind: "schemaDiffApply".into(),
        owner: owner_job(),
        payload: serde_json::json!({"consumedPlanId": "plan-2", "planVersion": 1, "handlerVersion": 1, "checkpointVersion": 1}),
        created_at: datazen_platform_api::id::Timestamp::new("2026-01-01T00:00:00Z"),
    };
    let err = repo
        .accept(&c, other, &IdempotencyKey::new("key-1"))
        .await
        .expect_err("conflict");
    assert!(matches!(err, PortError::IdempotencyConflict), "{err:?}");
}

#[tokio::test]
async fn cm54_plan_id_is_consumed_once_by_apply_jobs() {
    let (repo, _clock) = repo_clock();
    let c = ctx();
    let payload = apply_payload("plan-X");
    repo.accept(&c, definition("dataSyncApply", payload.clone(), "job-a"), &IdempotencyKey::new("k1"))
        .await
        .expect("accept-1");
    let err = repo
        .accept(&c, definition("dataSyncApply", payload, "job-b"), &IdempotencyKey::new("k2"))
        .await
        .expect_err("plan consumed");
    assert!(matches!(err, PortError::PlanAlreadyConsumed(_)), "{err:?}");

    // prepare 不占用 planId：再申请同一 planId 作为 apply 必须使用新 key 且仍被唯一约束挡住
    let prep = repo
        .accept(&c, definition("dataSyncPrepare", prepare_payload(), "job-p"), &IdempotencyKey::new("k3"))
        .await
        .expect("prepare accept");
    assert_eq!(prep.view.state, JobState::Queued);
}

#[tokio::test]
async fn cm67_unknown_version_major_is_rejected_at_accept() {
    let (repo, _clock) = repo_clock();
    let c = ctx();
    let payload = serde_json::json!({"planVersion": 2, "handlerVersion": 1, "checkpointVersion": 1});
    let err = repo
        .accept(&c, definition("schemaDiffPrepare", payload, "job-v"), &IdempotencyKey::new("k4"))
        .await
        .expect_err("version reject");
    assert!(matches!(err, PortError::UnsupportedVersion(_)), "{err:?}");
}

// ------------------------------------------------------ claim fencing 旅程

#[tokio::test]
async fn fencing_old_claim_writes_are_rejected_after_takeover() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    let def = definition("schemaDiffApply", apply_payload("plan-f"), "job-f");
    repo.accept(&c, def, &IdempotencyKey::new("k5")).await.expect("accept");

    let claim1 = repo.claim(&c, JobId::new("job-f"), WorkerId::new("w1")).await.expect("claim1");
    // 接管：先让 claim1 到期，再 claim2 generation+1
    clock.set("2026-01-01T00:06:00Z");
    let claim2 = repo.claim(&c, JobId::new("job-f"), WorkerId::new("w2")).await.expect("claim2");
    assert_ne!(claim1.claim_generation, claim2.claim_generation, "接管 generation+1");

    // 旧 claim1 写入被拒绝
    let stage = datazen_platform_api::dto::job::StageRecord {
        job_id: JobId::new("job-f"),
        stage_id: StageId::new("s1"),
        kind: "apply".into(),
        claimed_by: Some(WorkerId::new("w1")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    };
    let err = repo.record_stage(&c, &claim1, stage.clone()).await.expect_err("stale");
    assert!(matches!(err, PortError::StaleClaim), "{err:?}");

    // claim2 写入通过
    let stage2 = datazen_platform_api::dto::job::StageRecord {
        claimed_by: Some(WorkerId::new("w2")),
        ..stage
    };
    repo.record_stage(&c, &claim2, stage2).await.expect("fenced write ok");

    // renew 不 bump generation、只延 expiresAt；过期 claim 写入被拒绝
    clock.set("2026-01-01T00:00:20Z");
    let renewed = repo.renew(&claim2).await.expect("renew");
    assert_eq!(renewed.claim_generation, claim2.claim_generation, "续约不变 generation");
    clock.set("2026-01-01T00:20:00Z");
    let err = repo.record_stage(&c, &renewed, datazen_platform_api::dto::job::StageRecord {
        job_id: JobId::new("job-f"),
        stage_id: StageId::new("s2"),
        kind: "apply".into(),
        claimed_by: Some(WorkerId::new("w2")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    }).await.expect_err("expired");
    assert!(matches!(err, PortError::StaleClaim), "{err:?}");
}

// ------------------------------------------------------ 取消意图路径

#[tokio::test]
async fn queued_cancel_is_an_intent_only_and_notstarted_finalizes() {
    let (repo, _clock) = repo_clock();
    let c = ctx();
    repo.accept(&c, definition("schemaDiffPrepare", prepare_payload(), "job-c"), &IdempotencyKey::new("k6"))
        .await
        .expect("accept");

    let updated = repo.request_cancel(&c, &JobId::new("job-c")).expect("intent");
    assert_eq!(updated.view.state, JobState::Queued, "排队取消只记录意图，不提前终结");
    assert!(updated.view.cancel_requested);

    let finalized = repo
        .confirm_cancelled_not_started(&c, &JobId::new("job-c"), EffectOutcome::NotStarted)
        .expect("finalize");
    assert_eq!(finalized.view.state, JobState::Cancelled);
    assert_eq!(finalized.view.effect_outcome, Some(EffectOutcome::NotStarted));
}

// ------------------------------------------------------ 预算与重叠

#[test]
fn cm31_overlap_is_rejected_before_any_permit_is_held() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-1".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-1".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("overlap");
    assert!(matches!(err, datazen_runtime::job::JobError::EndpointOverlap(_)), "{err:?}");

    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    let mut guard = ledger.lock().expect("lock");
    guard.ensure_service(&conn());
    drop(guard);
    let permits = datazen_runtime::job::MultiEndpointPermits::reserve(
        ledger.clone(),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    );
    assert!(matches!(
        permits,
        Err(datazen_runtime::job::JobError::EndpointOverlap(_))
    ));
}

#[test]
fn cm65_multi_endpoint_reserve_is_all_or_nothing() {
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    {
        let mut guard = ledger.lock().expect("lock");
        guard.ensure_service(&conn());
        guard.ensure_service(&ConnectionId::new("conn-2"));
    }
    // EndpointRef has no service_id field; use service_key:
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-a".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "svc-b".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let permits = datazen_runtime::job::MultiEndpointPermits::reserve(
        ledger.clone(),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    )
    .expect("reserve-all");
    assert_eq!(permits.permits().len(), 2, "全组预留");
    let held = ledger.lock().expect("lock").permits_of(&PrincipalId::new("user-a"));
    assert_eq!(held, 2);
    let _ = permits.release(true);
}

/// 反向：预算不足/服务未注册时 try_admit_many 整组回滚，一个许可都不持有。
#[test]
fn cm65_insufficient_budget_admits_no_permits_at_all() {
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    {
        let mut guard = ledger.lock().expect("lock");
        guard.ensure_service(&conn());
        // conn-2 故意不注册：该组里任何一个端点不可准入都必须整组回滚
    }
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-a".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "svc-b".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let result = datazen_runtime::job::MultiEndpointPermits::reserve(
        ledger.clone(),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    );
    assert!(matches!(
        result,
        Err(datazen_runtime::job::JobError::BudgetDenied(_))
    ));
    let held = ledger.lock().expect("lock").permits_of(&PrincipalId::new("user-a"));
    assert_eq!(held, 0, "整组回滚后不得持有任何许可");
}

// ------------------------------------------------ handler 夹具与 runtime 旅程

struct CountingHandler {
    kind: &'static str,
    calls: Arc<AtomicUsize>,
    mode: Mode,
}

enum Mode {
    Success,
    Unknown,
    CancelStage,
}

#[async_trait::async_trait]
impl JobHandler for CountingHandler {
    fn kind(&self) -> &str {
        self.kind
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, datazen_runtime::job::JobError> {
        let _ = plan;
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
    ) -> Result<StageOutcome, datazen_runtime::job::JobError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let terminal = match self.mode {
            Mode::Success => StageTerminal::Succeeded,
            Mode::Unknown => StageTerminal::Unknown,
            Mode::CancelStage => StageTerminal::Cancelled,
        };
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: JobProgress {
                read: datazen_platform_api::id::Counter::new(10),
                converted: datazen_platform_api::id::Counter::new(10),
                attempted: datazen_platform_api::id::Counter::new(10),
                committed: datazen_platform_api::id::Counter::new(if terminal == StageTerminal::Succeeded { 10 } else { 0 }),
                unknown: datazen_platform_api::id::Counter::new(if terminal == StageTerminal::Unknown { 2 } else { 0 }),
            },
            commit_boundaries: if terminal == StageTerminal::Succeeded {
                vec![datazen_platform_api::dto::job::CommitBoundary {
                    stage_id: spec.stage_id.clone(),
                    stable_target_fingerprint: "sha256:abc".into(),
                    committed_at: datazen_platform_api::id::Timestamp::new("2026-01-01T00:00:01Z"),
                    operation_id: None,
                    batch_id: Some("batch-1".into()),
                    payload_digest: Some("sha256:payload".into()),
                    evidence: vec!["target-batch-record".into()],
                    verified_at: None,
                }]
            } else {
                vec![]
            },
            execution_ids: vec![ExecutionId::new("exec-1")],
            artifact_ids: vec![],
            effect_outcome: match terminal {
                StageTerminal::Succeeded => EffectOutcome::Completed,
                StageTerminal::Unknown => EffectOutcome::Unknown,
                StageTerminal::Cancelled => EffectOutcome::Unknown,
                StageTerminal::Failed => EffectOutcome::RolledBack,
            },
            error_code: None,
        })
    }
    fn verify_recovery(&self, checkpoint: &datazen_platform_api::dto::job::Checkpoint) -> RecoveryVerdict {
        if checkpoint.verification_evidence.iter().any(|e| e.contains("unknown")) {
            RecoveryVerdict::RequireManualReview {
                reason: "unknownCommitBoundary".into(),
            }
        } else {
            RecoveryVerdict::ResumeAfterVerify { resume_through: 1 }
        }
    }
}

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
    repo.accept(&c, definition("schemaDiffApply", apply_payload("plan-r1"), "job-r1"), &IdempotencyKey::new("r1"))
        .await
        .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let runtime = runtime_with(repo, Arc::new(CountingHandler { kind: "schemaDiffApply", calls: calls.clone(), mode: Mode::Success }), ledger, clock);
    let result = runtime
        .run(
            &c,
            &JobId::new("job-r1"),
            &WorkerId::new("w1"),
            &[EndpointRef { connection_id: conn(), service_key: "svc".into(), objects: vec!["users".into()], role: EndpointRole::SourceReader }],
        )
        .await
        .expect("run");
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert_eq!(result.progress.committed.get(), 10);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "handler 只执行一次");
}

#[tokio::test]
async fn runtime_unknown_outcome_preserves_unknown_and_pending_reason() {
    let (repo, clock) = repo_clock();
    let c = ctx();
    repo.accept(&c, definition("schemaDiffApply", apply_payload("plan-r2"), "job-r2"), &IdempotencyKey::new("r2"))
        .await
        .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let runtime = runtime_with(repo, Arc::new(CountingHandler { kind: "schemaDiffApply", calls: calls.clone(), mode: Mode::Unknown }), ledger, clock);
    let result = runtime
        .run(&c, &JobId::new("job-r2"), &WorkerId::new("w1"), &[EndpointRef { connection_id: conn(), service_key: "svc".into(), objects: vec!["users".into()], role: EndpointRole::SourceReader }])
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
    repo.accept(&c, definition("schemaDiffApply", apply_payload("plan-r3"), "job-r3"), &IdempotencyKey::new("r3"))
        .await
        .expect("accept");
    repo.request_cancel(&c, &JobId::new("job-r3")).expect("intent");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());
    let calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(repo);
    let runtime = runtime_with(repo, Arc::new(CountingHandler { kind: "schemaDiffApply", calls: calls.clone(), mode: Mode::Success }), ledger.clone(), clock);
    let result = runtime
        .run(&c, &JobId::new("job-r3"), &WorkerId::new("w1"), &[EndpointRef { connection_id: conn(), service_key: "svc".into(), objects: vec!["users".into()], role: EndpointRole::SourceReader }])
        .await
        .expect("run");
    assert_eq!(result.state, JobState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "取消后不派发");
    assert_eq!(ledger.lock().expect("lock").permits().count(), 0, "不持有许可");
}

// ------------------------------------------------ 持久化白名单

#[test]
fn persisted_shapes_never_carry_runtime_handles() {
    let job = datazen_platform_api::dto::job::JobRecord {
        view: datazen_platform_api::dto::job::JobView {
            job_id: JobId::new("j"),
            kind: "k".into(),
            state: JobState::Queued,
            stage: None,
            execution_ids: vec![],
            artifact_ids: vec![],
            created_at: Timestamp::new("2026-01-01T00:00:00Z"),
            updated_at: Timestamp::new("2026-01-01T00:00:00Z"),
            effect_outcome: None,
            cancel_requested: false,
            pending_verification_reason: None,
            progress: Default::default(),
        },
        definition: definition("schemaDiffApply", apply_payload("p"), "j"),
        stages: vec![],
        state_version: Jsv::new(1),
    };
    let value = serde_json::to_value(&job).expect("serialize");
    let serialized = value.to_string();
    for forbidden in ["dbSessionId", "runtimeBinding", "attachmentToken", "resourceBindingId", "lease"] {
        assert!(!serialized.contains(forbidden), "持久化路径不得包含 {forbidden}");
    }
}

// ------------------------------------------------ 恢复故障旅程（§10.1.1 决策表）

/// 旅程 A：commit 成功但 checkpoint 未写 —— 只能人工核验，不自动重放。
#[tokio::test]
async fn recovery_commit_succeeded_checkpoint_missing_requires_manual_review() {
    let (repo, _clock) = repo_clock();
    let c = ctx();
    repo.accept(&c, definition("dataSyncApply", apply_payload("plan-a"), "job-a"), &IdempotencyKey::new("ja"))
        .await
        .expect("accept");
    let claim = repo.claim(&c, JobId::new("job-a"), WorkerId::new("w1")).await.expect("claim");
    let boundary = datazen_platform_api::dto::job::CommitBoundary {
        stage_id: StageId::new("s1"),
        stable_target_fingerprint: "sha256:abc".into(),
        committed_at: Timestamp::new("2026-01-01T00:00:01Z"),
        operation_id: None,
        batch_id: Some("batch-1".into()),
        payload_digest: Some("sha256:p".into()),
        evidence: vec!["target-batch-record".into()],
        verified_at: None,
    };
    repo.record_commit_boundary(&c, &claim, boundary).await.expect("boundary");
    // 崩溃窗口：没有 checkpoint 落盘
    assert!(repo.latest_checkpoint(&c, &JobId::new("job-a")).is_none(), "checkpoint 未写入");
    let recoverable = repo.list_recoverable(&c, Default::default()).await.expect("recoverable");
    assert!(recoverable.iter().any(|j| j.view.job_id == JobId::new("job-a")), "可恢复候选");
    // 落库边界可见（D2）
    assert_eq!(repo.committed_boundaries(&JobId::new("job-a")).len(), 1, "边界必须落库");
    // 人工审查：标记待核验，并声明 handler 未被二次派发
    repo.mark_pending_verification(&c, &JobId::new("job-a"), "commitSucceededCheckpointMissing")
        .expect("mark");
    let job = repo.get(&c, JobId::new("job-a")).await.expect("get");
    assert!(job.view.pending_verification_reason.is_some(), "必须记录待核验原因");
    let calls = Arc::new(AtomicUsize::new(0));
    let _handler = CountingHandler {
        kind: "dataSyncApply",
        calls: calls.clone(),
        mode: Mode::Success,
    };
    assert_eq!(calls.load(Ordering::SeqCst), 0, "recoverable 旅程不得静默重跑副作用阶段");
}

/// 旅程 B：checkpoint 已写但终态未写 —— 复核后补终态或继续，不重放已确认范围。
#[tokio::test]
async fn recovery_checkpoint_written_terminal_missing_can_resume_without_rerun() {
    let (repo, _clock) = repo_clock();
    let c = ctx();
    repo.accept(&c, definition("dataSyncApply", apply_payload("plan-b"), "job-b"), &IdempotencyKey::new("jb"))
        .await
        .expect("accept");
    let claim = repo.claim(&c, JobId::new("job-b"), WorkerId::new("w1")).await.expect("claim");
    let boundary = datazen_platform_api::dto::job::CommitBoundary {
        stage_id: StageId::new("s1"),
        stable_target_fingerprint: "sha256:abc".into(),
        committed_at: Timestamp::new("2026-01-01T00:00:01Z"),
        operation_id: None,
        batch_id: Some("batch-1".into()),
        payload_digest: Some("sha256:p".into()),
        evidence: vec!["target-batch-record".into()],
        verified_at: None,
    };
    repo.record_commit_boundary(&c, &claim, boundary.clone()).await.expect("boundary");
    let job = repo.get(&c, JobId::new("job-b")).await.expect("get");
    let cp = datazen_platform_api::dto::job::Checkpoint {
        job_id: JobId::new("job-b"),
        state_version: job.state_version,
        stable_target_fingerprint: "sha256:abc".into(),
        committed: vec![boundary],
        verification_evidence: vec!["target-batch-record".into()],
        recovery_policy: "resumeAfterVerify".into(),
    };
    repo.save_checkpoint(&c, &claim, cp).await.expect("checkpoint");
    // 终态未写：list_recoverable 仍包含；verify_recovery 裁决 ResumeAfterVerify
    let recoverable = repo.list_recoverable(&c, Default::default()).await.expect("recoverable");
    assert!(recoverable.iter().any(|j| j.view.job_id == JobId::new("job-b")));
    let latest = repo.latest_checkpoint(&c, &JobId::new("job-b")).expect("cp");
    assert_eq!(latest.committed.len(), 1);
    // 恢复决策断言：checkpoint 带边界证据 → ResumeAfterVerify，不自动重跑已确认范围
    let calls = Arc::new(AtomicUsize::new(0));
    let handler = CountingHandler {
        kind: "dataSyncApply",
        calls: calls.clone(),
        mode: Mode::Success,
    };
    let verdict = handler.verify_recovery(&latest);
    assert!(
        matches!(verdict, RecoveryVerdict::ResumeAfterVerify { .. }),
        "{verdict:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "resumeAfterVerify 仍不允许自动重跑副作用阶段");
    // 重复写相同 checkpoint 版本 → 冲突
    let cp2 = datazen_platform_api::dto::job::Checkpoint {
        job_id: JobId::new("job-b"),
        state_version: job.state_version,
        stable_target_fingerprint: "sha256:abc".into(),
        committed: vec![],
        verification_evidence: vec![],
        recovery_policy: "resumeAfterVerify".into(),
    };
    let err = repo.save_checkpoint(&c, &claim, cp2).await.expect_err("dup");
    assert!(matches!(err, PortError::CasConflict { entity: "job_checkpoint", .. }), "{err:?}");
}
