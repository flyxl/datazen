//! 基准的执行侧：夹具、并发驱动、事件投影、样本切分。
//!
//! ## 口径（全部来自 `fake-runtime-fixtures.md` §11）
//!
//! - **§11.1**：release 构建、单进程、**无数据库网络**、固定 fake 命令 10 毫秒（虚拟时间驱动）、
//!   并发 8、预热 1000、每轮 10000 个已获准且未排队的请求、5 轮、逐轮 p95 ≤ 10 毫秒。
//! - **§11.2**：附加耗时 = 段①（网关完成鉴权/参数校验 → 派发 driver）+ 段②（driver completion →
//!   回执/事件状态登记完成）两段**单调时间之和**；fake SQL 的 10 毫秒由 `tokio::time::pause()` 的
//!   自动推进消费，**真实等待为零**，两段窗口用的是 `std::time::Instant`，不受虚拟时间影响。
//! - **§11.3**：逐请求先求和再取分位数（nearest-rank，第 `ceil(0.95*N)` 项，1-based）；
//!   排队与被拒的请求**不进 p95 样本**；失败样本不删除、失败数单列。
//! - **§11.4**：压力半另跑，不与延迟半共用入口。
//! - **§11.6**：基准与功能测试**分开二进制入口**，CI 才能区分超时原因。
//!
//! ## 必须原样披露的两处偏离
//!
//! 1. **单线程运行时**。§11.2 要求 `tokio::time::pause()`，而 `pause()` 在多线程运行时上会 panic，
//!    因此这里用 `new_current_thread`。8 路并发共享一个线程：测到的 p95 是**单核下界**，
//!    不含跨核竞争与跨核缓存争用。
//! 2. **夹具是手抄的**。`tests/gateway_fixtures/` 只对集成测试可见，二进制入口够不到它；
//!    这里照着它逐字段复刻，抄错字段会让基准测一个不存在的会话形状。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use datazen_platform_api::id::{ClientInstanceId, EditorSessionId};
use datazen_runtime::connection::{
    AttachmentState, CommandCall, ConfigRevision, ConnectionId, Counter, DbSessionId,
    ExecutionId, NamespaceTarget, OrganizationId, OwnerRef, PrincipalId, SessionContext,
    SessionHandle, SessionState, SessionView, Timestamp,
};
use datazen_runtime::gateway::{
    AlwaysAllow, ExecutionEvent, ExecutionGateway, ExecutionRequest, ExecutionSource, GatewayError,
    InMemoryIdempotencyStore, RequestPrincipal, SourceKind,
};

use crate::clock::InstantClock;
use crate::driver::{event_kind, event_sequence, FakeDriverPort, ProjectionReport};
use crate::outcome::{
    BenchError, BenchRun, ExecutionJournal, PermitReconciliation, Percentiles, RawSample,
    RoundOutcome, SampleOutcome,
};
use crate::plan::{BenchPlan, SPEC_MEMORY_BYTES, SPEC_VCPUS};

/// 夹具会话 id。全基准只用一个逻辑会话（§11.1 只要求单进程固定 fake command）。
const SESSION_ID: &str = "db_session_cm60_bench";
/// 夹具 `runtimeEpoch`。
const RUNTIME_EPOCH: u64 = 1;
/// 夹具 `contextRevision`。
const REVISION: u64 = 7;
/// 幂等键前缀；每个请求追加自己的序号，保证键**逐请求唯一**。
const IDEMPOTENCY_PREFIX: &str = "idem-cm60-bench";
/// 执行 id 前缀，避免与任何真实资源同名。
const EXEC_PREFIX: &str = "cm60-bench";
/// 压力半承担 permit 收支对账的测试二进制（§11.5 + §11.4）。
const PERMIT_CARRIER: &str = "cargo test -p datazen-runtime --test cm60_pressure_drain";

// ─────────────────────────────── 夹具 ───────────────────────────────

fn namespace() -> NamespaceTarget {
    NamespaceTarget::unknown_placeholder()
}

fn owner() -> OwnerRef {
    OwnerRef::Editor {
        organization_id: OrganizationId::new("org-cm60-bench"),
        principal_id: PrincipalId::new("principal-cm60-bench"),
        connection_id: ConnectionId::new("cnx-cm60-bench"),
        client_instance_id: ClientInstanceId::new("cli-cm60-bench"),
        editor_session_id: EditorSessionId::new("edt-cm60-bench"),
    }
}

fn handle() -> SessionHandle {
    SessionHandle {
        db_session_id: DbSessionId::new(SESSION_ID),
        runtime_epoch: Counter::new(RUNTIME_EPOCH),
    }
}

/// 处于 `Ready` 的会话视图：可受理执行。
fn ready_view() -> SessionView {
    let connection_id = ConnectionId::new("cnx-cm60-bench");
    SessionView {
        handle: handle(),
        connection_id: connection_id.clone(),
        config_revision: ConfigRevision::new(3),
        owner: owner(),
        initial_target: datazen_runtime::connection::ExecutionTarget {
            connection_id,
            namespace: namespace(),
            object: None,
        },
        observed_context: SessionContext::new(namespace(), "dz_identity_cm60_bench"),
        context_revision: Counter::new(REVISION),
        state: SessionState::Ready,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: Timestamp::new("9999-01-01T00:00:00Z"),
    }
}

fn principal() -> RequestPrincipal {
    RequestPrincipal::new(
        PrincipalId::new("principal-cm60-bench"),
        OrganizationId::new("org-cm60-bench"),
        DbSessionId::new(SESSION_ID),
    )
}

fn source() -> ExecutionSource {
    ExecutionSource::new(
        SourceKind::Editor,
        "edt-cm60-bench",
        Some(OrganizationId::new("org-cm60-bench")),
        Some(PrincipalId::new("principal-cm60-bench")),
    )
}

fn call() -> CommandCall {
    CommandCall {
        command: "query".to_owned(),
        input: serde_json::json!({ "sql": "select 1" }),
    }
}

/// 第 `index` 个请求。幂等键带序号：**键重复就会退化成幂等重发**，
/// 重发不产生新执行，两次请求会落在同一个 executionId 上，事件流随即被判重复。
fn request(index: usize) -> ExecutionRequest {
    ExecutionRequest::new(
        handle(),
        Counter::new(REVISION),
        call(),
        format!("{IDEMPOTENCY_PREFIX}-{index:08}"),
        source(),
    )
}

/// 该执行的事件帧。
fn event(execution_id: &ExecutionId, sequence: u64) -> ExecutionEvent {
    ExecutionEvent {
        execution_id: execution_id.clone(),
        db_session_id: DbSessionId::new(SESSION_ID),
        runtime_epoch: Counter::new(RUNTIME_EPOCH),
        sequence: Counter::new(sequence),
        context_revision: Counter::new(REVISION),
        kind: event_kind(sequence),
        declared_source: None,
    }
}

// ─────────────────────────────── 驱动 ───────────────────────────────

/// 一个 worker 的累计账。
#[derive(Debug, Default)]
struct WorkerAcc {
    outcome: SampleOutcome,
    rejections: BTreeMap<String, usize>,
    projection: ProjectionReport,
    events_projected: u64,
}

impl WorkerAcc {
    fn merge(&mut self, other: WorkerAcc) {
        self.outcome.merge(&other.outcome);
        for (reason, count) in other.rejections {
            *self.rejections.entry(reason).or_insert(0) += count;
        }
        self.projection.merge(&other.projection);
        self.events_projected = self.events_projected.saturating_add(other.events_projected);
    }
}

/// `GatewayError` 的分类标签。被拒样本按标签分桶，不混成一个数字。
fn rejection_label(error: &GatewayError) -> String {
    match error {
        GatewayError::Runtime(err) => format!("runtime:{}", err.reason()),
        GatewayError::PermissionDenied { action, .. } => format!("permission:{action:?}"),
        GatewayError::IdempotencyVerificationRequired { .. } => "idempotency:unreadable".to_owned(),
        GatewayError::IdempotencyConflict { .. } => "idempotency:conflict".to_owned(),
        GatewayError::IdempotencyPersistFailed { .. } => "idempotency:persist".to_owned(),
        GatewayError::InvalidRequest { reason } => format!("invalid:{reason}"),
    }
}

/// 单个请求的完整生命周期：受理 → 派发 → 事件投影。
async fn one(
    gateway: &ExecutionGateway,
    port: &FakeDriverPort,
    who: &RequestPrincipal,
    index: usize,
    acc: &mut WorkerAcc,
) {
    acc.outcome.requested += 1;
    let acceptance = match gateway.accept(who, request(index)).await {
        Ok(acceptance) => acceptance,
        Err(error) => {
            // 被拒：不进 p95 样本，原因单列（§11.3）。
            acc.outcome.rejected += 1;
            *acc.rejections.entry(rejection_label(&error)).or_insert(0) += 1;
            return;
        }
    };
    if acceptance.is_replay() {
        // 幂等重发不产生新执行，因此**不派发**：否则同一 executionId 会收到两遍事件。
        acc.outcome.replays += 1;
        return;
    }
    let execution_id: ExecutionId = acceptance.execution_id().clone();
    match gateway.dispatch(who, &execution_id).await {
        Ok(_receipt) => acc.outcome.completed += 1,
        Err(error) => {
            // 派发失败：失败数单列，且它**不占用**样本（样本只来自成功的两段计时）。
            acc.outcome.dispatch_failed += 1;
            *acc.rejections.entry(rejection_label(&error)).or_insert(0) += 1;
            return;
        }
    }
    project(gateway, port, &execution_id, acc).await;
}

/// 把一次执行的事件帧按序投给网关，统计重复 / 丢失（CM-60 性能门槛）。
async fn project(
    gateway: &ExecutionGateway,
    port: &FakeDriverPort,
    execution_id: &ExecutionId,
    acc: &mut WorkerAcc,
) {
    for sequence in event_sequence() {
        let disposition = gateway.apply_event(event(execution_id, sequence)).await;
        match disposition {
            datazen_runtime::gateway::EventDisposition::Applied => {
                acc.events_projected += 1;
                // 终态到达，执行从「在途」里退出（§11.5 的泄漏对账）。
                if sequence == u64::from(crate::driver::EVENTS_PER_EXECUTION) {
                    port.mark_terminal();
                }
            }
            datazen_runtime::gateway::EventDisposition::IgnoredDuplicate { .. } => {
                acc.projection.duplicates += 1;
            }
            datazen_runtime::gateway::EventDisposition::IgnoredOutOfOrder { .. } => {
                acc.projection.out_of_order += 1;
            }
            datazen_runtime::gateway::EventDisposition::IgnoredDuplicateChunk { .. } => {
                acc.projection.duplicate_chunks += 1;
            }
            datazen_runtime::gateway::EventDisposition::Gap { .. } => {
                acc.projection.lost += 1;
            }
            datazen_runtime::gateway::EventDisposition::Unbound => {
                acc.projection.unbound += 1;
            }
            datazen_runtime::gateway::EventDisposition::IgnoredStaleSession { .. }
            | datazen_runtime::gateway::EventDisposition::IgnoredStaleEpoch { .. }
            | datazen_runtime::gateway::EventDisposition::IgnoredForeignEpoch { .. } => {
                acc.projection.foreign += 1;
            }
        }
    }
}

/// 跑一轮（预热轮也走这里，只是样本不进分位数）。
///
/// 并发度是「同时在飞的请求数上限」：用一张全局工位表分发序号，
/// 总数因此**精确等于** `total`，不依赖 `total / concurrency` 的整除。
async fn drive(
    gateway: Arc<ExecutionGateway>,
    port: Arc<FakeDriverPort>,
    total: usize,
    concurrency: usize,
) -> Result<(WorkerAcc, u64), BenchError> {
    let started = Instant::now();
    let next = Arc::new(AtomicUsize::new(0));
    let who = Arc::new(principal());
    let mut tasks = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let gateway = Arc::clone(&gateway);
        let port = Arc::clone(&port);
        let next = Arc::clone(&next);
        let who = Arc::clone(&who);
        tasks.push(tokio::spawn(async move {
            let mut acc = WorkerAcc::default();
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= total {
                    break;
                }
                one(&gateway, &port, &who, index, &mut acc).await;
            }
            acc
        }));
    }
    let mut merged = WorkerAcc::default();
    for task in tasks {
        let acc = task
            .await
            .map_err(|error| BenchError::RuntimeSetup(format!("worker panicked: {error}")))?;
        merged.merge(acc);
    }
    Ok((
        merged,
        u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
    ))
}

/// 取出本轮新增的两段计时。网关的样本仓库是**累积且没有清空接口**的，
/// 所以逐轮分位数靠「轮前长度 + 轮后切片」切出来，而不是靠 reset。
async fn slice_samples(gateway: &ExecutionGateway, before: usize) -> Vec<RawSample> {
    let all = gateway.overhead_samples().await;
    all.as_slice()
        .iter()
        .skip(before)
        .enumerate()
        .map(|(ordinal, sample)| RawSample {
            round: 0,
            ordinal: ordinal + 1,
            gateway_nanos: sample.gateway_nanos,
            registration_nanos: sample.registration_nanos,
        })
        .collect()
}

/// 把一轮的 worker 账组装成 [`RoundOutcome`]。
async fn assemble(
    gateway: &ExecutionGateway,
    acc: WorkerAcc,
    round: usize,
    concurrency: usize,
    before: usize,
    wall_time_nanos: u64,
) -> RoundOutcome {
    let samples = slice_samples(gateway, before).await;
    let totals: Vec<u64> = samples
        .iter()
        .map(|sample| {
            sample
                .gateway_nanos
                .saturating_add(sample.registration_nanos)
        })
        .collect();
    RoundOutcome {
        round,
        concurrency,
        samples: samples
            .into_iter()
            .map(|mut sample| {
                sample.round = round;
                sample
            })
            .collect(),
        outcome: acc.outcome,
        rejections: acc.rejections,
        // 网关没有排队受理态（`AcceptanceDisposition` 只有 Accepted / Replayed），
        // 也没有预算台账，所以排队数**结构上恒为 0**。这不是「没测到」，是「不存在」，
        // 但等待分位数仍按 §11.3 输出为 `None`，让读产物的人看见这一栏存在且为空。
        queued_wait: Percentiles::of(&[]),
        percentiles: Percentiles::of(&totals),
        wall_time_nanos,
        event_projection: acc.projection,
    }
}

/// 校验计划可运行。不校验「符合判据」——那由 [`BenchRun::comparable_to_criterion`] 负责。
fn check_plan(plan: BenchPlan) -> Result<(), BenchError> {
    if plan.rounds == 0 || plan.per_round == 0 || plan.concurrency == 0 {
        return Err(BenchError::InvalidPlan(format!(
            "零样本轮次无法判定：rounds = {}，per_round = {}，concurrency = {}",
            plan.rounds, plan.per_round, plan.concurrency
        )));
    }
    Ok(())
}

/// 产物里的口径披露。
fn notes(
    plan: BenchPlan,
    conforms: bool,
    measured_vcpus: u32,
    measured_memory_bytes: u64,
) -> Vec<String> {
    let mut notes = vec![
        format!(
            "虚拟时间：tokio::time::pause() + 自动推进消费 fake 命令的 {} 毫秒；两段窗口用 \
             std::time::Instant 采样，不受虚拟时间影响；真实等待为零。",
            plan.fake_command.as_millis()
        ),
        "单线程运行时：pause() 在多线程运行时上会 panic，故 8 路并发共享一个线程。\
         测得的 p95 是单核下界，不含跨核竞争与跨核缓存争用。"
            .to_owned(),
        "禁止任何出站 socket（§11.1）：tokio 运行时只 enable_time()，未装 IO 驱动；\
         fake 端口不碰文件、不碰网络。"
            .to_owned(),
        "夹具为手抄：tests/gateway_fixtures 只对集成测试可见，二进制入口够不到，\
         故在此照其逐字段复刻。"
            .to_owned(),
        "分位数实现只有一份：datazen_runtime::latency::nearest_rank_percentile，\
         nearest-rank、第 ceil(0.95*N) 项、1-based、不插值。"
            .to_owned(),
        "排队数结构上恒为 0：网关的受理处置只有 Accepted/Replayed，且未接入预算台账，\
         不存在队列等待路径；因此 §11.3 要求的等待分位数按空样本输出为 None。"
            .to_owned(),
        "permit 收支对账在压力半（§11.4 要求压力与延迟分开跑）：\
         tests/cm60_pressure_drain.rs 的 ResourceJournal 在每一个 connect/close/permit 变化点断言。"
            .to_owned(),
    ];
    notes.push(format!(
        "判据环境锚点为 {SPEC_VCPUS} vCPU / {SPEC_MEMORY_BYTES} 字节（可复现性锚点，非容量要求）；\
         本次运行实际环境为 {measured_vcpus} vCPU / {measured_memory_bytes} 字节，见 environment 段。"
    ));
    if measured_vcpus != SPEC_VCPUS || measured_memory_bytes != SPEC_MEMORY_BYTES {
        notes.push(format!(
            "本次运行未在判据指定的 {SPEC_VCPUS} vCPU / {SPEC_MEMORY_BYTES} 字节环境复测，\
             因此不得作「按判据达标」的结论（实测机更快的部分只会让 p95 偏乐观，\
             偏保守方向并不对称）。"
        ));
    }
    notes.push(if conforms {
        "本次运行逐字等于 §11.1 的规格计划。".to_owned()
    } else {
        format!(
            "本次运行**不是** §11.1 的规格计划（预热 {} / 每轮 {} / {} 轮 / 并发 {} / fake {} 毫秒），\
             不作「按判据达标」的结论。",
            plan.warmup,
            plan.per_round,
            plan.rounds,
            plan.concurrency,
            plan.fake_command.as_millis()
        )
    });
    notes
}

/// 汇总 journal。覆盖**含预热**的全部请求：泄漏不因发生在预热阶段而消失。
fn journal(
    port: &FakeDriverPort,
    driver: &crate::driver::DriverJournal,
    warmup: &RoundOutcome,
    rounds: &[RoundOutcome],
    execution_count: usize,
) -> ExecutionJournal {
    let mut admitted = 0usize;
    let mut dispatched = 0usize;
    let mut completed = 0usize;
    let mut projection = ProjectionReport::default();
    for round in std::iter::once(warmup).chain(rounds.iter()) {
        // 受理 = 进入 dispatch 的 + 派发失败的 + 幂等重发的；被拒的从未获准。
        admitted = admitted.saturating_add(
            round
                .outcome
                .completed
                .saturating_add(round.outcome.dispatch_failed)
                .saturating_add(round.outcome.replays),
        );
        dispatched = dispatched.saturating_add(
            round
                .outcome
                .completed
                .saturating_add(round.outcome.dispatch_failed),
        );
        completed = completed.saturating_add(round.outcome.completed);
        projection.merge(&round.event_projection);
    }
    projection.outstanding_at_end = projection.outstanding_at_end.max(port.outstanding());
    let balanced =
        port.outstanding() == 0 && projection.is_clean() && driver.dropped_round_trips() == 0;
    ExecutionJournal {
        admitted,
        dispatched,
        completed,
        outstanding_at_end: port.outstanding(),
        session_view_calls: driver.session_view_calls(),
        execute_calls: driver.execute_calls(),
        cancel_calls: driver.cancel_calls(),
        close_calls: driver.close_calls(),
        events_emitted: driver.events_emitted(),
        events_projected: projection.applied,
        execution_records_retained: execution_count,
        projection,
        driver_round_trip: driver.round_trip_nanos(),
        driver_round_trip_median_nanos: driver.round_trip_median_nanos(),
        dropped_round_trips: driver.dropped_round_trips(),
        permit_reconciliation: PermitReconciliation {
            gateway_ledger_balanced: balanced,
            budget_permit_ledger_carrier: PERMIT_CARRIER.to_owned(),
            budget_permit_ledger_command: PERMIT_CARRIER.to_owned(),
        },
    }
}

/// 跑一次基准（环境留空：给自测用）。
///
/// 正式入口请用 [`run_bench_on`]，把实测 CPU/内存写进产物的 `environment` 段——
/// §11.6 要求照着产物就能重跑，环境留空就等于还得回头猜。
pub fn run_bench(plan: BenchPlan) -> Result<BenchRun, BenchError> {
    run_bench_on(plan, 0, 0)
}

/// 跑一次基准并记下实测环境。
///
/// 构造 current-thread + 启用时间的运行时并 `pause()`：§11.2 要求 fake 命令的 10 毫秒
/// 由虚拟时间消费，`pause()` 在多线程运行时上会 panic，所以这里不能开多线程（已披露）。
/// 副作用也要说清楚：8 个并发任务因此共享一个线程，实测 p95 是**单核下界**，
/// 不含跨核竞争。
pub fn run_bench_on(
    plan: BenchPlan,
    measured_vcpus: u32,
    measured_memory_bytes: u64,
) -> Result<BenchRun, BenchError> {
    check_plan(plan)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|error| BenchError::RuntimeSetup(error.to_string()))?;
    runtime.block_on(async {
        tokio::time::pause();
        drive_all(plan, measured_vcpus, measured_memory_bytes).await
    })
}

async fn drive_all(
    plan: BenchPlan,
    measured_vcpus: u32,
    measured_memory_bytes: u64,
) -> Result<BenchRun, BenchError> {
    let started = Instant::now();
    let (port, driver_journal) = FakeDriverPort::new(ready_view(), plan.fake_command, EXEC_PREFIX);
    let gateway = Arc::new(ExecutionGateway::new(
        Arc::clone(&port) as Arc<dyn datazen_runtime::registry::SessionPort>,
        AlwaysAllow::shared(),
        InMemoryIdempotencyStore::shared(),
        Arc::new(InstantClock::new()),
    ));

    // 预热：样本照采，但**不进任何分位数**，也不进 raw 产物。
    let warmup_before = gateway.overhead_samples().await.len();
    let (warmup_acc, warmup_wall) = drive(
        Arc::clone(&gateway),
        Arc::clone(&port),
        plan.warmup,
        plan.concurrency,
    )
    .await?;
    let warmup = assemble(
        &gateway,
        warmup_acc,
        0,
        plan.concurrency,
        warmup_before,
        warmup_wall,
    )
    .await;

    let mut rounds = Vec::with_capacity(plan.rounds);
    let mut raw = Vec::with_capacity(plan.per_round.saturating_mul(plan.rounds));
    for round in 1..=plan.rounds {
        let before = gateway.overhead_samples().await.len();
        let (acc, wall) = drive(
            Arc::clone(&gateway),
            Arc::clone(&port),
            plan.per_round,
            plan.concurrency,
        )
        .await?;
        let outcome = assemble(&gateway, acc, round, plan.concurrency, before, wall).await;
        raw.extend_from_slice(&outcome.samples);
        rounds.push(outcome);
    }

    // journal 覆盖**含预热**的全部请求：泄漏不因发生在预热阶段而消失。
    let journal = journal(
        &port,
        &driver_journal,
        &warmup,
        &rounds,
        gateway.execution_count().await,
    );
    let run = BenchRun {
        plan,
        conforms_to_spec: true,
        measured_vcpus,
        measured_memory_bytes,
        warmup,
        rounds,
        raw,
        journal,
        wall_time_nanos: u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
        notes: Vec::new(),
    };
    let conforms = run.plan.is_spec_plan();
    Ok(BenchRun {
        conforms_to_spec: conforms,
        notes: notes(run.plan, conforms, measured_vcpus, measured_memory_bytes),
        ..run
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::DEFAULT_PLAN;

    fn tiny() -> BenchPlan {
        BenchPlan {
            warmup: 4,
            per_round: 8,
            rounds: 2,
            concurrency: 2,
            fake_command: std::time::Duration::from_millis(10),
        }
    }

    #[test]
    fn a_plan_with_no_samples_is_refused() {
        let error = run_bench(BenchPlan {
            per_round: 0,
            ..tiny()
        });
        assert!(error.is_err(), "零样本轮次不能被判定为通过");
    }

    #[tokio::test(start_paused = true)]
    async fn every_request_produces_exactly_one_sample() {
        tokio::time::pause();
        let plan = tiny();
        let run = drive_all(plan, 8, 17_179_869_184).await.unwrap_or_else(|error| panic!("bench failed: {error}"));
        let round = &run.rounds[0];
        assert_eq!(round.outcome.requested, plan.per_round);
        assert_eq!(round.n(), plan.per_round, "完成的请求必须各有一次两段计时");
        assert_eq!(round.failures(), 0);
        assert_eq!(round.outcome.queued, 0, "网关没有排队受理态");
    }

    #[tokio::test(start_paused = true)]
    async fn event_stream_is_lossless_across_the_round() {
        tokio::time::pause();
        let run = drive_all(tiny(), 8, 17_179_869_184).await.unwrap();
        assert!(run.journal.projection.is_clean(), "事件重复/丢失必须为 0");
        assert_eq!(run.journal.outstanding_at_end, 0, "在途执行必须归零");
    }

    #[tokio::test(start_paused = true)]
    async fn warmup_samples_stay_out_of_the_measured_rounds() {
        tokio::time::pause();
        let plan = tiny();
        let run = drive_all(plan, 8, 17_179_869_184).await.unwrap();
        assert_eq!(run.warmup.outcome.requested, plan.warmup);
        assert_eq!(run.raw.len(), plan.per_round * plan.rounds);
    }

    #[tokio::test(start_paused = true)]
    async fn idempotency_keys_are_unique_per_request() {
        tokio::time::pause();
        let run = drive_all(tiny(), 8, 17_179_869_184).await.unwrap();
        let total_replays: usize = std::iter::once(&run.warmup)
            .chain(run.rounds.iter())
            .map(|round| round.outcome.replays)
            .sum();
        assert_eq!(total_replays, 0, "键重复会让两次请求落在同一个 executionId 上");
    }

    #[test]
    fn the_default_plan_is_the_criterion_plan() {
        assert!(DEFAULT_PLAN.is_spec_plan());
        assert!(run_bench(DEFAULT_PLAN).is_ok());
    }
}
