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
//!
//! ## 与 runtime 侧 C1 的分工（两半缺一不可）
//!
//! 这一组证明的是**后半段**：`CancelToken` 上的那一位，在阶段阻塞运行途中被内核
//! 翻成 true 之后，data-transfer 的管道真的会**停下来**，而且停在第一次 commit 之前。
//! 它证明不了前半段——「这一位究竟有没有被翻」。
//!
//! 前半段在 `packages/runtime/tests/job_cancel_watch.rs` 的
//! `mid_stage_cancel_without_prior_boundary_rolls_back`（runtime 侧 C1），那里读得到
//! 内存仓储的 `get_calls()`，能够断言轮询次数真的在涨。
//!
//! 为什么这里读不到：`get_calls()` / `fail_get()` 是
//! `#[cfg(any(test, feature = "test-harness"))]` 缝，而 `datazen-data-transfer` 的
//! `[features]` 只有 `webdriver = []`——**没有 `test-harness`**（runtime 是靠一条
//! 自引用的 dev-dependency 才拿到的：`datazen-runtime = { path = ".", features =
//! ["test-harness"] }`）。给它补一个 feature 意味着动本 crate 的 feature 面，
//! 远超本组测试的职责，所以此处只能用 `WATCHER_SLACK` 那样的一段有界等待，
//! 让断言落在**终态**上。
//!
//! 结论是一个**成对**的结论，拆开任何一半都会静默塌陷：
//! - 删掉本文件：`flag()` 哪天退回入口快照，本组照常绿——因为测试侧自己翻令牌，
//!   内核有没有真的把取消送到，测不到。
//! - 删掉 runtime 侧 C1：内核哪天不再翻转那一位，本组照常绿——因为等待到期后
//!   状态/边界断言失败这句话永远轮不到说，用例靠"闸门松开后仍提交"间接报错，
//!   而那是一个**依赖时序**的间接证据。
//!
//! 所以：**这两半要一起改、一起留。** 要动其中一半，先确认另一半还在。

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
    JobRuntime, SharedClock, StageSpec, StageTerminal, CANCEL_POLL_INTERVAL,
};

use super::real_sqlite::*;
use super::*;

/// 阶段真正阻塞在第一批写入里的上限。
const GATE_WAIT: Duration = Duration::from_secs(10);

/// `n` 个内核轮询周期。
///
/// `Duration * u32` 不是 const（stable 上 `Mul` 的 impl 没标 const），所以在 const
/// 上下文里只能自己按 secs / nanos 展开。这条约束是从编译器的实测错误里来的，
/// 不是风格偏好。
const fn periods(n: u64) -> Duration {
    let total_nanos = CANCEL_POLL_INTERVAL.subsec_nanos() as u64 * n;
    Duration::new(
        CANCEL_POLL_INTERVAL.as_secs() * n + total_nanos / 1_000_000_000,
        (total_nanos % 1_000_000_000) as u32,
    )
}

/// `request_cancel` 之后留给阶段看守者轮询的余量。
///
/// 内存仓储的 `get_calls()` 是 `#[cfg(any(test, feature = "test-harness"))]` 缝，
/// 本 crate 没有开 `test-harness` feature，因此这里读不到轮询次数，只能给一段
/// 有界等待。等待本身不参与断言：若取消没被送到，松开闸门后三批都会提交，
/// 下面的状态/边界断言会**明确失败**并指出「取消没被送达」，而不是静默变绿。
///
/// **余量按周期数写，不按毫秒数写。** 它过去写死 1200ms 并在注释里手工标注
/// 「内核 50ms，余量约 24 个周期」——两处各记一个数字，谁也管不着谁：
/// 内核把间隔调成 5s 时，余量自动缩到不足一个周期，本用例会以
/// 「取消没被送达」报错，**报的是一个假的结论**（真正变的是内核，不是迁移管道）。
/// 改成 24 个周期之后，两者只有真源一个，间隔往哪边调都不会把
/// 「余量不够」误报成「管道没看见取消令牌」。
const WATCHER_SLACK: Duration = periods(24);

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
pub(super) fn freeze_body() -> TransferFreezeBody {
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

struct RealRun {
    outcome: KernelRun,
    source_path: std::path::PathBuf,
    target_path: std::path::PathBuf,
    /// 字段名带下划线：它不参与断言，只负责把临时目录的寿命延到断言之后。
    #[allow(dead_code)]
    dir: Arc<tempfile::TempDir>,
}

struct KernelRun {
    state: JobState,
    effect: EffectOutcome,
    boundaries: usize,
    /// 失败原因随断言一起打印：真引擎报的错（SQL 被拒、事务语义不符）正是
    /// 这个夹具要抓的东西，吞掉它等于把唯一的信号扔了。
    error: Option<String>,
}

/// 把真 handler 塞进真内核跑一遍；`cancel_at_gate` 决定是否在闸门处请内核取消。
///
/// 返回值里带着临时目录，是因为回读断言必须发生在文件还在的时候——
/// 目录一掉，`count_rows` 会当场新建一个空库然后数出 0 行，断言就空转了。
async fn run_through_kernel(fixture: RealSqlite, cancel_at_gate: bool) -> RealRun {
    let job_id = JobId::new("job-dt-kernel-cancel-sqlite");
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
    repo.accept(
        &ctx(),
        job_definition(&job_id),
        &IdempotencyKey::new("job-dt-kernel-cancel-sqlite"),
    )
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

    if cancel_at_gate {
        let gate = fixture
            .gate
            .as_ref()
            .expect("the cancel run must have been built with an armed gate");
        await_gated_sqlite_write(gate).await;
        repo.request_cancel(&ctx(), &job_id)
            .expect("request_cancel");
        // 内核需要时间发现请求并翻转令牌；窗口长度按 `CANCEL_POLL_INTERVAL`
        // 推出来，不是写死的毫秒数。
        tokio::time::sleep(WATCHER_SLACK).await;
        gate.opened.notify_one();
    }

    let result = running.await.expect("join").expect("run");
    RealRun {
        outcome: KernelRun {
            state: result.state,
            effect: result.effect_outcome,
            boundaries: repo.committed_boundaries(&job_id).len(),
            error: result.error,
        },
        source_path: fixture.source_path.clone(),
        target_path: fixture.target_path.clone(),
        dir: fixture.dir.clone(),
    }
}

/// 对照组：不取消的时候，真引擎**真的**把 6 行全搬过去。
///
/// 没有这一条，下一条里"目标 0 行"就可能是"管道压根没写出过东西"，
/// 断言就会空转。它把"能搬"和"搬完被回滚"这两件事分开钉住。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sqlite_engine_round_trips_every_row_when_nothing_cancels_it() {
    let fixture = real_sqlite(false).await;
    assert_eq!(
        count_rows(&fixture.source_path).await,
        6,
        "the source fixture must really hold six rows before the transfer starts"
    );

    let run = run_through_kernel(fixture, false).await;

    assert_eq!(
        run.outcome.state,
        JobState::Succeeded,
        "data stage failed on a real engine: {:?}",
        run.outcome.error
    );
    assert_eq!(run.outcome.effect, EffectOutcome::Completed);
    assert_eq!(
        run.outcome.boundaries, 3,
        "batch size 2 over six rows is three batches"
    );
    assert_eq!(
        count_rows(&run.target_path).await,
        6,
        "a real engine must really accept the generated INSERTs, or the cancel case below proves nothing"
    );
}

/// 真引擎上的内核取消：第一批写入前取消 ⇒ 目标文件最终 0 行。
///
/// 这是 ① 的验收用例。杀它的变异是"把闸门去掉"——取消信号在第一批落库之后才到，
/// 三批全部 commit，目标 6 行，断言必须红。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_sqlite_engine_ends_up_empty_after_a_kernel_cancel() {
    let fixture = real_sqlite(true).await;
    assert_eq!(
        count_rows(&fixture.source_path).await,
        6,
        "the source fixture must really hold six rows before the transfer starts"
    );

    let run = run_through_kernel(fixture, true).await;

    assert_eq!(
        run.outcome.state,
        JobState::Cancelled,
        "a cancel must outrank any stage failure: {:?}",
        run.outcome.error
    );
    assert_eq!(run.outcome.effect, EffectOutcome::RolledBack);
    assert_eq!(
        run.outcome.boundaries, 0,
        "a rolled-back batch confirms no boundary"
    );
    assert_eq!(
        count_rows(&run.target_path).await,
        0,
        "the rows the gate let through were rolled back by a real SQLite ROLLBACK, not by a fake dropping a Vec"
    );
}
