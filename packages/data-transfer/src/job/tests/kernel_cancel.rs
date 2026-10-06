//! CANCEL_WATCH 的 data-transfer 侧集成（复审 D1）。
//!
//! 这一组测试回答的是「内核那一位取消令牌，data-transfer 到底看不看得见」：
//!
//! 1. `kernel_cancel_stops_the_data_stage_before_the_first_commit`：把**真的
//!    `DataTransferHandler`** 通过 `JobRuntime` 派发出去，阶段在第一批写入里真正
//!    阻塞，运行中 `request_cancel`，断言管道在第一次 commit 之前停住、
//!    整轮没有产出任何提交边界（§7 / C4）。
// 2. `pipeline_reads_the_same_bit_the_kernel_flips`：不经过 runtime，直接从测试侧
//!    翻转 `CancelToken`，证明 `CancelToken::flag()` 交给管道的就是内核那一位，
//!    而不是阶段入口处的一份快照。
//!
//! 两条都必须在「`flag()` 退回入口快照」时变红——那是本组测试的存在理由。

use std::time::Duration;

use datazen_platform_api::context::{OwnerRef, RequestContext};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobDefinition, JobState};
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, IdempotencyKey, JobId, OrganizationId, PrincipalId,
    RequestId, Timestamp, WorkerId,
};
use datazen_platform_api::ports::budget::ServiceQuota;
use datazen_platform_api::ports::job::JobRepository;

use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    CancelToken, EndpointRef, EndpointRole, HandlerRegistry, InMemoryJobRepository, JobHandler as _,
    JobRuntime, SharedClock, StageSpec, StageTerminal,
};

use super::*;

/// 阶段真正阻塞在第一批写入里的上限。
const GATE_WAIT: Duration = Duration::from_secs(10);
/// `request_cancel` 之后留给阶段看守者轮询的余量。
///
/// 内存仓储的 `get_calls()` 是 `#[cfg(any(test, feature = "test-harness"))]` 缝，
/// 本 crate 没有开 `test-harness` feature，因此这里读不到轮询次数，只能给一段
/// 有界等待（内核 `CANCEL_POLL_INTERVAL` 为 50ms，余量约 24 个周期）。等待本身
/// 不参与断言：若取消没被送到，松开闸门后三批都会提交，下面的状态/边界断言
/// 会**明确失败**并指出「取消没被送达」，而不是静默变绿。
const WATCHER_SLACK: Duration = Duration::from_millis(1200);

fn org() -> OrganizationId {
    OrganizationId::new("org-acme")
}

fn conn() -> ConnectionId {
    ConnectionId::new("conn-pg-main")
}

fn ctx() -> RequestContext {
    RequestContext::new(
        org(),
        PrincipalId::new("user-a"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn endpoints() -> Vec<EndpointRef> {
    vec![EndpointRef {
        connection_id: conn(),
        service_key: "svc".into(),
        objects: vec!["t".into()],
        role: EndpointRole::SourceReader,
    }]
}

fn budget() -> BudgetConfig {
    let config = BudgetConfig::new(ServiceQuota::new(8, [1, 1, 1, 0]).expect("service quota"));
    config.validate().expect("budget config");
    config
}

fn job_definition(job_id: &JobId) -> JobDefinition {
    JobDefinition {
        job_id: job_id.clone(),
        kind: "dataTransferApply".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "migration".into(),
        },
        payload: serde_json::json!({
            "consumedPlanId": "plan-1",
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "selectionRevision": 1,
        }),
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

/// 冻结体走 `forbidAutoResume`：`resumeAfterVerify` 要求完整稳定 key 与快照证据，
/// 与本组用例要验证的取消路径无关。
fn freeze_body() -> TransferFreezeBody {
    TransferFreezeBody {
        job: job_for_pipeline(2),
        mapping_fingerprint: "fp-cm".into(),
        recovery_policy: "forbidAutoResume".into(),
        has_structure_ir: false,
        stable_key_columns: vec![],
        snapshot_proven: false,
    }
}

fn data_stage_spec() -> StageSpec {
    StageSpec {
        stage_id: datazen_platform_api::id::StageId::new("data"),
        kind: "data".into(),
        depends_on: vec![],
    }
}

fn six_rows() -> Rows {
    (1..=6)
        .map(|id| {
            vec![
                Some(Value::Integer(id)),
                Some(Value::String(format!("v{id}"))),
            ]
        })
        .collect()
}

/// 源端 6 行、目标端在**第一批**写入处阻塞的可取消夹具。
struct BlockedFirstBatch {
    target: Arc<FakeDb>,
    gate: Arc<SlowGate>,
    handler: DataTransferHandler,
}

fn blocked_first_batch() -> BlockedFirstBatch {
    let schema = schema_with_snapshot(&["id", "name"]);
    let source = Arc::new(FakeDb {
        rows: six_rows(),
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: None,
    });
    let gate = Arc::new(SlowGate {
        entered: tokio::sync::Notify::new(),
        opened: tokio::sync::Notify::new(),
        first: AtomicBool::new(true),
    });
    let target = Arc::new(FakeDb {
        rows: vec![],
        schema: schema.clone(),
        state: Mutex::new(State::default()),
        commit_error_after_effect: false,
        slow: Some(gate.clone()),
    });
    let handler = DataTransferHandler::apply(
        freeze_body(),
        vec![inspected_for(&["id", "name"])],
        HashMap::from([("t".to_string(), schema.clone())]),
        HashMap::from([("t".to_string(), schema)]),
        TransferEndpoints::Database {
            source_driver: source,
            source_handle: src_handle(),
            target_driver: target.clone(),
            target_handle: tgt_handle(),
            source_type: "postgresql".into(),
            target_type: "postgresql".into(),
        },
        None,
    );
    BlockedFirstBatch {
        target,
        gate,
        handler,
    }
}

/// 阻塞点必须在写事务的 execute 里，不是在「进阶段」那一刻。
async fn await_blocked_write(gate: &SlowGate) {
    assert!(
        tokio::time::timeout(GATE_WAIT, gate.entered.notified())
            .await
            .is_ok(),
        "the data stage never reached the gated write"
    );
}

/// 运行中取消 ⇒ Cancelled / RolledBack / 零提交边界。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kernel_cancel_stops_the_data_stage_before_the_first_commit() {
    let fixture = blocked_first_batch();
    let job_id = JobId::new("job-dt-kernel-cancel");
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
    repo.accept(&ctx(), job_definition(&job_id), &IdempotencyKey::new("job-dt-kernel-cancel"))
        .await
        .expect("accept");
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(budget())));
    ledger.lock().expect("ledger lock").ensure_service(&conn());
    let mut handlers = HandlerRegistry::new();
    handlers.register(Arc::new(fixture.handler));
    let runtime = JobRuntime::new(repo.clone(), Arc::new(handlers), ledger, clock, 0);

    let run_job_id = job_id.clone();
    let running = tokio::spawn(async move {
        runtime
            .run(&ctx(), &run_job_id, &WorkerId::new("w1"), &endpoints())
            .await
    });

    // 阶段真正阻塞在第一批写入里——此时 job 处于 running。
    await_blocked_write(&fixture.gate).await;
    repo.request_cancel(&ctx(), &job_id).expect("request_cancel");
    tokio::time::sleep(WATCHER_SLACK).await;
    fixture.gate.opened.notify_one();
    let result = running.await.expect("join").expect("run");

    assert_eq!(
        result.state,
        JobState::Cancelled,
        "a mid-stage cancel observed by the pipeline must end the job cancelled"
    );
    assert_eq!(
        result.effect_outcome,
        EffectOutcome::RolledBack,
        "no commit boundary was produced, so the whole run rolled back (§7)"
    );
    assert!(
        repo.committed_boundaries(&job_id).is_empty(),
        "cancellation must not create a commit boundary (C4)"
    );
    let target = fixture.target.state.lock().expect("target state");
    assert!(
        target.committed.is_empty(),
        "the in-flight batch was rolled back, so nothing stays committed"
    );
    assert_eq!(
        target.write_sqls.len(),
        1,
        "no batch may be written after the cancel was delivered"
    );
}

/// 管道读的就是内核那一位：测试侧翻转令牌，阶段必须同样在第一次 commit 之前停住。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_reads_the_same_bit_the_kernel_flips() {
    let fixture = blocked_first_batch();
    let token = CancelToken::new();
    let flag = token.flag();
    assert!(!flag.load(Ordering::SeqCst), "the token starts unset");

    let spec = data_stage_spec();
    let handler = Arc::new(fixture.handler);
    let staged_token = token.clone();
    let staged = tokio::spawn(async move { handler.run_stage(&spec, &staged_token).await });
    await_blocked_write(&fixture.gate).await;
    token.cancel();
    assert!(
        flag.load(Ordering::SeqCst),
        "flag() must alias the kernel's own bit"
    );
    fixture.gate.opened.notify_one();
    let outcome = staged.await.expect("join").expect("run_stage");

    assert_eq!(outcome.terminal, StageTerminal::Cancelled);
    assert_eq!(
        outcome.effect_outcome,
        EffectOutcome::PartiallyApplied,
        "a cancelled stage reports the cancelled effect at stage level"
    );
    assert!(
        outcome.commit_boundaries.is_empty(),
        "the rolled-back batch produced no confirmed boundary"
    );
    let target = fixture.target.state.lock().expect("target state");
    assert!(target.committed.is_empty());
    assert_eq!(target.write_sqls.len(), 1);
}