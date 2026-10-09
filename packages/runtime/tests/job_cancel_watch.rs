//! 阶段内取消（CANCEL_WATCH）集成测试。
//!
//! 覆盖 §2.3 的取消语义在**阶段执行期间**是否成立：
//! - C1 阶段执行中收到取消 → 阶段观察到并终止，`dispatch` 收敛到 `Cancelled`，
//!   效果按既有提交边界判定（有边界 `PartiallyApplied` / 无边界 `RolledBack`）。
//! - C2 看守者不泄漏：阶段之间与 `dispatch` 收尾后 `active_cancel_watchers() == 0`，
//!   且仓储读次数不再增长。
//! - C3 轮询失败按 `CANCEL_POLL_FAILED` 的刻意决策处理：告警后停止轮询、不打断阶段。
//! - C4 取消不制造提交边界：取消前已确认的边界保留，取消本身一条都不新增。
//! - C5 排队取消与阶段间取消语义不回归。
//! - C6 Job 已终结时看守者自己退出轮询（`terminal => return` 早退分支，见 `terminal_exit.rs`）。
//!
//! 所有用例会真的**阻塞在阶段里**：阶段持续轮询内核那一个 `CancelToken`，直到翻转才返回，
//! 因此不可能靠"取消恰好赶在开跑前"这种时序侥幸通过。

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobDefinition, JobProgress, JobState,
};
use datazen_platform_api::id::{
    ClientInstanceId, Counter, ExecutionId, IdempotencyKey, JobId, PrincipalId, RequestId, StageId,
    Timestamp, WorkerId,
};
use datazen_platform_api::ports::job::JobRepository;

use datazen_runtime::budget::BudgetLedger;
use datazen_runtime::job::{
    CancelToken, EndpointRef, EndpointRole, FrozenPlan, HandlerRegistry, InMemoryJobRepository,
    JobError, JobHandler, JobRuntime, RecoveryVerdict, SharedClock, StageOutcome, StageSpec,
    StageTerminal, CANCEL_POLL_INTERVAL,
};
use support::{config, conn, org};

// 轮询间隔本身的可核查性独立成文件，同上。
#[path = "job_cancel_watch/poll_interval.rs"]
mod poll_interval;

// 阶段内 panic 的展开路径独立成文件，同上。
#[path = "job_cancel_watch/in_stage_panic.rs"]
mod in_stage_panic;

// 阶段执行期的轮询故障观测独立成文件，守住单文件 800 行规模。
// 集成测试的 crate 根就在 `tests/` 下，模块解析不走「同名子目录」惯例，用 `#[path]` 指过去。
#[path = "job_cancel_watch/poll_fault.rs"]
mod poll_fault;

// 终态早退分支（Job 已终结时看守者自己退出）的观测独立成文件，同上。
#[path = "job_cancel_watch/terminal_exit.rs"]
mod terminal_exit;

/// 阶段内等待内核令牌的耐心上限。CI 抖动也远达不到这个量级；到达上限就返回
/// "没等到"，于是测试会以断言失败（而不是挂死）暴露看守器没工作。
const PATIENCE: Duration = Duration::from_secs(5);

/// `n` 个轮询周期。
///
/// `Duration * u32` 不是 const（stable 上 `Mul` 的 impl 没标 const），所以在 const
/// 上下文里只能自己按 secs / nanos 展开——这也是让"窗口"保持真源可推导的前提。
const fn periods(n: u64) -> Duration {
    let total_nanos = CANCEL_POLL_INTERVAL.subsec_nanos() as u64 * n;
    Duration::new(
        CANCEL_POLL_INTERVAL.as_secs() * n + total_nanos / 1_000_000_000,
        (total_nanos % 1_000_000_000) as u32,
    )
}

/// 观察窗口：跨过它之后"读次数不再增长"才有意义。
///
/// **从 [`CANCEL_POLL_INTERVAL`] 推导，不写死毫秒数。** 这一组里十处断言
/// （C2/C3/C6）全是同一个句式："跨过窗口，读次数必须增长 / 必须不增长"。
/// 窗口一旦比一个轮询周期还短，这个句式就恒成立——看门狗是死是活都测得出来是绿的：
/// 窗口 400ms、间隔被调到 5s 时，窗口里连一次轮询都落不下，"不增长"自动为真，
/// 于是一个**根本没在跑的看门狗**和一个正常工作的看门狗给出同一个结论。
/// 写成 8 个周期之后，间隔无论往哪边调，"至少跨过两个周期"都由真源保证。
const QUIET_WINDOW: Duration = periods(8);

// ---------------------------------------------------------------- 共用夹具

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

fn payload(plan_id: &str) -> serde_json::Value {
    serde_json::json!({
        "consumedPlanId": plan_id,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "selectionRevision": 1,
    })
}

fn definition(job_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        kind: "schemaDiffApply".into(),
        owner: datazen_platform_api::OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "migration".into(),
        },
        payload: payload(job_id),
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

fn endpoints() -> Vec<EndpointRef> {
    vec![EndpointRef {
        connection_id: conn(),
        service_key: "svc".into(),
        objects: vec!["users".into()],
        role: EndpointRole::SourceReader,
    }]
}

/// 阶段脚本。`ProbeHandler` 按 stage_id 取对应脚本。
#[derive(Clone, Copy)]
enum Plan {
    /// 在阶段内等取消，观察到就返回 `Cancelled`，不带任何边界。
    WaitCancel,
    /// 先确认一条提交边界，再在阶段内等取消，返回 `Cancelled` + 那一条边界。
    BoundaryThenWaitCancel,
    /// 不取消：睡一段远超轮询间隔的时间后返回 `Succeeded`。
    Idle,
    /// 跑完后（返回前）请求取消，用来命中"阶段之间"那条边界检查。
    CancelAfterSelf,
    /// 打开仓储读故障跑一段时间，再关掉，返回 `Succeeded`。
    PollFault,
    /// **阻塞在阶段里**，直到测试侧放行；放行后直接返回 `Succeeded`。
    ///
    /// 阶段本身不碰仓储故障开关：故障（`fail_get`）与终态（`force_terminal_for_test`）
    /// 都由测试在**自己选定的时刻**注入，否则「注入之前看护者到底有没有在轮询」这件事
    /// 无法观测——故障窗口随阶段开跑一起打开的话，entry 时刻的读次数已经处在故障里，
    /// "读次数不再增长"这条断言可以空转通过（它只证明计数被冻结，不证明看护者曾在轮询）。
    ///
    /// 与 `PollFault` 的区别只有时序控制：阶段必须停在窗口里不动，测试才能在
    /// 「阶段仍在执行」的窗口里采样读次数与告警条数——阶段一返回，看护者就被
    /// abort + await 掉，采样到的就不是同一件事了。
    Held,
    /// 进入阶段之后立刻 **panic 展开**，用来命中 `run_stage_watched` 的展开路径：
    /// `run_stage` 的 `.await` 处抛出，`watch.join()` 那行永远跑不到，
    /// 看守者只剩 `CancelWatch` 的 `Drop` 兜底（见 `in_stage_panic.rs`）。
    ///
    /// 入口的 `Entered` 消息照发，所以测试能确认"阶段真的开跑了"，
    /// 从而排除"panic 发生在阶段开跑之前"这种时序侥幸。
    PanicInStage,
}

/// handler → 测试的消息通道，避免任何"通知早于等待"的丢信号问题。
#[derive(Debug)]
enum Msg {
    Entered {
        stage: String,
        /// 进入阶段时内核令牌的状态。阶段内取消场景必须是 `false`，
        /// 否则就成了"开跑前就已取消"的旧路径，测不到阶段内取消。
        cancel_at_entry: bool,
        /// 进入阶段时在飞的看守者数：上一个阶段的看守者必须已经回收。
        watchers_at_entry: usize,
        get_calls_at_entry: usize,
    },
    Finished {
        stage: String,
        /// 阶段是否真的观察到内核令牌被翻转。
        observed_cancel: bool,
    },
}

struct ProbeHandler {
    repo: Arc<InMemoryJobRepository>,
    ctx: RequestContext,
    job_id: JobId,
    /// 回指 runtime，用来在阶段入口读取在飞的看守者数。
    runtime_slot: Arc<Mutex<Option<Arc<JobRuntime>>>>,
    /// `PollFaultHeld` 的放行信号：测试侧决定故障窗口持续多久。
    release: Arc<tokio::sync::Notify>,
    tx: tokio::sync::mpsc::UnboundedSender<Msg>,
    plans: Vec<(&'static str, Plan)>,
}

impl ProbeHandler {
    fn plan_of(&self, stage_id: &StageId) -> Plan {
        self.plans
            .iter()
            .find(|(id, _)| *id == stage_id.as_str())
            .map(|(_, plan)| *plan)
            .unwrap_or_else(|| panic!("未知阶段 {stage_id:?}"))
    }

    fn boundary(&self, stage_id: &StageId, batch: &str) -> CommitBoundary {
        CommitBoundary {
            stage_id: stage_id.clone(),
            stable_target_fingerprint: "sha256:abc".into(),
            committed_at: Timestamp::new("2026-01-01T00:00:01Z"),
            operation_id: None,
            batch_id: Some(batch.into()),
            payload_digest: Some("sha256:payload".into()),
            evidence: vec!["target-batch-record".into()],
            verified_at: None,
        }
    }

    fn outcome(
        &self,
        spec: &StageSpec,
        terminal: StageTerminal,
        boundaries: Vec<CommitBoundary>,
    ) -> StageOutcome {
        let succeeded = terminal == StageTerminal::Succeeded;
        StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: JobProgress {
                read: Counter::new(1),
                converted: Counter::new(1),
                attempted: Counter::new(1),
                committed: Counter::new(if succeeded { 1 } else { 0 }),
                unknown: Counter::new(0),
            },
            commit_boundaries: boundaries,
            execution_ids: vec![ExecutionId::new("exec-1")],
            artifact_ids: vec![],
            effect_outcome: match terminal {
                StageTerminal::Succeeded => EffectOutcome::Completed,
                StageTerminal::Failed => EffectOutcome::RolledBack,
                StageTerminal::Cancelled => EffectOutcome::NotStarted,
                StageTerminal::Unknown => EffectOutcome::Unknown,
            },
            error_code: None,
        }
    }
}

/// 阶段内的长活：在阶段执行期间反复看内核那一个令牌，直到被翻转或耐心耗尽。
/// 返回 true 表示阶段确实看到了阶段内取消。
async fn wait_for_kernel_cancel(cancel: &CancelToken) -> bool {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        if cancel.is_cancelled() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[async_trait::async_trait]
impl JobHandler for ProbeHandler {
    fn kind(&self) -> &str {
        "schemaDiffApply"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, _plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        Ok(self
            .plans
            .iter()
            .map(|(id, _)| StageSpec {
                stage_id: StageId::new(*id),
                kind: "apply".into(),
                depends_on: vec![],
            })
            .collect())
    }
    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        let plan = self.plan_of(&spec.stage_id);
        let watchers_at_entry = self
            .runtime_slot
            .lock()
            .expect("slot")
            .as_ref()
            .map(|rt| rt.active_cancel_watchers())
            .unwrap_or_default();
        let _ = self.tx.send(Msg::Entered {
            stage: spec.stage_id.as_str().to_string(),
            cancel_at_entry: cancel.is_cancelled(),
            watchers_at_entry,
            get_calls_at_entry: self.repo.get_calls(),
        });

        let mut boundaries = Vec::new();
        let observed_cancel = match plan {
            Plan::WaitCancel => wait_for_kernel_cancel(cancel).await,
            Plan::BoundaryThenWaitCancel => {
                // 取消之前就已经确认的边界：必须原样保留。
                boundaries.push(self.boundary(&spec.stage_id, "batch-before-cancel"));
                wait_for_kernel_cancel(cancel).await
            }
            Plan::Idle => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                cancel.is_cancelled()
            }
            Plan::CancelAfterSelf => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                // 返回之前落取消意图：下一个阶段必须由 `dispatch` 的阶段边界检查接住。
                self.repo
                    .request_cancel(&self.ctx, &self.job_id)
                    .expect("阶段内落取消意图");
                cancel.is_cancelled()
            }
            Plan::PollFault => {
                self.repo.fail_get(true);
                tokio::time::sleep(Duration::from_millis(250)).await;
                // 关掉故障后再返回，否则 `dispatch` 自己的读也会失败。
                self.repo.fail_get(false);
                cancel.is_cancelled()
            }
            Plan::Held => {
                // 阻塞在这里等测试放行：整个观察窗口里阶段都还在跑。
                // 故障/终态由测试自己注入，注入时刻才是被观测的那件事。
                let release = self.release.clone();
                release.notified().await;
                cancel.is_cancelled()
            }
            Plan::PanicInStage => {
                // 这里 panic 的消息是固定字面量，不含任何连接串或凭据。
                panic!("阶段内 panic：用于观测取消看守者在展开路径上的存活");
            }
        };
        let _ = self.tx.send(Msg::Finished {
            stage: spec.stage_id.as_str().to_string(),
            observed_cancel,
        });

        let terminal = match (plan, observed_cancel) {
            (_, true) => StageTerminal::Cancelled,
            _ => StageTerminal::Succeeded,
        };
        Ok(self.outcome(spec, terminal, boundaries))
    }
    fn verify_recovery(&self, _checkpoint: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::ResumeAfterVerify { resume_through: 1 }
    }
}

/// 装好一个 job + runtime，返回 (runtime, 发消息的接收端, 仓储)。
struct Rig {
    runtime: Arc<JobRuntime>,
    rx: tokio::sync::mpsc::UnboundedReceiver<Msg>,
    repo: Arc<InMemoryJobRepository>,
    job_id: JobId,
    /// 放行 `PollFaultHeld` 的阶段。
    release: Arc<tokio::sync::Notify>,
}

async fn rig(job_id: &str, plans: Vec<(&'static str, Plan)>) -> Rig {
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
    repo.accept(&ctx(), definition(job_id), &IdempotencyKey::new(job_id))
        .await
        .expect("accept");

    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    ledger.lock().expect("lock").ensure_service(&conn());

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let slot: Arc<Mutex<Option<Arc<JobRuntime>>>> = Arc::new(Mutex::new(None));
    let release = Arc::new(tokio::sync::Notify::new());
    let handler = Arc::new(ProbeHandler {
        repo: repo.clone(),
        ctx: ctx(),
        job_id: JobId::new(job_id),
        runtime_slot: slot.clone(),
        release: release.clone(),
        tx,
        plans,
    });
    let mut handlers = HandlerRegistry::new();
    handlers.register(handler);
    let runtime = Arc::new(JobRuntime::new(
        repo.clone(),
        Arc::new(handlers),
        ledger,
        clock,
        0,
    ));
    *slot.lock().expect("slot") = Some(runtime.clone());
    Rig {
        runtime,
        rx,
        repo,
        job_id: JobId::new(job_id),
        release,
    }
}

impl Rig {
    fn start(
        &self,
    ) -> tokio::task::JoinHandle<
        Result<datazen_runtime::job::JobResult, datazen_platform_api::error::PortError>,
    > {
        let runtime = self.runtime.clone();
        let job_id = self.job_id.clone();
        tokio::spawn(async move {
            runtime
                .run(&ctx(), &job_id, &WorkerId::new("w1"), &endpoints())
                .await
        })
    }

    async fn next(&mut self) -> Msg {
        tokio::time::timeout(PATIENCE, self.rx.recv())
            .await
            .expect("等待 handler 消息超时")
            .expect("handler 未上报")
    }
}

// ---------------------------------------------------------------- C1：无既有边界

/// C1/C4：阶段执行期间请求取消 → 阶段观察到令牌被翻转并终止；没有任何既有提交边界时
/// 收敛为 `Cancelled` + `RolledBack`，且一条提交边界都没有被记下。
#[tokio::test]
async fn mid_stage_cancel_without_prior_boundary_preserves_handler_effect() {
    let mut rig = rig("job-cw-1", vec![("s1", Plan::WaitCancel)]).await;
    let run = rig.start();

    // 先确认阶段真的开跑了，且此刻还没被取消 —— 这样后面发生的只能是阶段内取消。
    match rig.next().await {
        Msg::Entered {
            stage,
            cancel_at_entry,
            watchers_at_entry,
            ..
        } => {
            assert_eq!(stage, "s1");
            assert!(!cancel_at_entry, "进入阶段时不得已被取消，否则测的是旧路径");
            assert_eq!(watchers_at_entry, 1, "阶段执行期间必须正好一个看守者在飞");
        }
        other => panic!("期望进入阶段事件，得到 {other:?}"),
    }
    rig.repo
        .request_cancel(&ctx(), &rig.job_id)
        .expect("阶段内落取消意图");

    match rig.next().await {
        Msg::Finished {
            stage,
            observed_cancel,
        } => {
            assert_eq!(stage, "s1");
            assert!(observed_cancel, "阶段必须真的观察到内核令牌被翻转");
        }
        other => panic!("期望阶段结束事件，得到 {other:?}"),
    }

    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
    assert!(
        rig.repo.committed_boundaries(&rig.job_id).is_empty(),
        "取消不得制造提交边界"
    );
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        0,
        "不得留下游离看守者"
    );
}

// ---------------------------------------------------------------- C1 + C4：已有边界

/// C1/C4：取消发生在一条已确认的提交边界之后 → `Cancelled` + `PartiallyApplied`，
/// 已确认的那一条边界原样保留，取消**没有**新增任何边界。
#[tokio::test]
async fn mid_stage_cancel_keeps_prior_boundary_and_adds_none() {
    let mut rig = rig("job-cw-2", vec![("s1", Plan::BoundaryThenWaitCancel)]).await;
    let run = rig.start();
    assert!(matches!(rig.next().await, Msg::Entered { .. }));
    rig.repo
        .request_cancel(&ctx(), &rig.job_id)
        .expect("阶段内落取消意图");
    assert!(matches!(
        rig.next().await,
        Msg::Finished {
            observed_cancel: true,
            ..
        }
    ));

    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::PartiallyApplied);
    let persisted = rig.repo.committed_boundaries(&rig.job_id);
    assert_eq!(
        persisted.len(),
        1,
        "取消前确认的边界保留、取消不新增边界: {persisted:?}"
    );
    assert_eq!(
        persisted[0].batch_id.as_deref(),
        Some("batch-before-cancel")
    );
}

// ---------------------------------------------------------------- C2：不泄漏

/// C2：上一个阶段的看守者必须在下一个阶段开跑之前就被回收，`dispatch` 收尾后也没有残留，
/// 且仓储读次数不再增长（说明没有游离任务还在轮询）。
#[tokio::test]
async fn cancel_watcher_is_reaped_between_stages_and_after_dispatch() {
    let mut rig = rig("job-cw-3", vec![("s1", Plan::Idle), ("s2", Plan::Idle)]).await;
    let run = rig.start();

    match rig.next().await {
        Msg::Entered {
            stage,
            watchers_at_entry,
            ..
        } => {
            assert_eq!(stage, "s1");
            assert_eq!(watchers_at_entry, 1);
        }
        other => panic!("期望进入 s1，得到 {other:?}"),
    }
    match rig.next().await {
        Msg::Finished { stage, .. } => assert_eq!(stage, "s1"),
        other => panic!("期望 s1 结束，得到 {other:?}"),
    }
    match rig.next().await {
        Msg::Entered {
            stage,
            watchers_at_entry,
            ..
        } => {
            assert_eq!(stage, "s2", "s1 成功后必须继续派发 s2");
            assert_eq!(
                watchers_at_entry, 1,
                "s2 开跑时只允许 s2 自己的看守者在飞，s1 的必须已经 abort + await 掉"
            );
        }
        other => panic!("期望进入 s2，得到 {other:?}"),
    }
    assert!(matches!(
        rig.next().await,
        Msg::Finished {
            stage: _,
            observed_cancel: false,
        }
    ));

    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        0,
        "dispatch 返回后无游离看守者"
    );

    let before = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    assert_eq!(
        rig.repo.get_calls(),
        before,
        "dispatch 返回后不得还有任务继续读仓储"
    );
}

// ---------------------------------------------------------------- C3：轮询失败

/// C3：`CANCEL_POLL_FAILED` 的刻意决策——轮询读失败时告警并停止轮询，**不**把阶段变成失败。
/// 用例同时证明故障期间看护者确实在轮询、故障停止后读次数不再增长。
#[tokio::test]
async fn cancel_poll_failure_stops_polling_without_failing_the_stage() {
    let mut rig = rig("job-cw-4", vec![("s1", Plan::PollFault)]).await;
    let run = rig.start();

    let entered = match rig.next().await {
        Msg::Entered {
            get_calls_at_entry, ..
        } => get_calls_at_entry,
        other => panic!("期望进入阶段事件，得到 {other:?}"),
    };
    match rig.next().await {
        Msg::Finished {
            observed_cancel, ..
        } => assert!(
            !observed_cancel,
            "读失败不得被当成取消：失败只能让我们错过取消，不能凭空造出取消"
        ),
        other => panic!("期望阶段结束事件，得到 {other:?}"),
    }

    let result = run.await.expect("join").expect("run");
    assert_eq!(
        result.state,
        JobState::Succeeded,
        "轮询失败按决策 (a) 处理：让阶段跑完，而不是 fail-closed"
    );
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert!(
        rig.repo.get_calls() > entered,
        "故障期间看护者必须真的在轮询，否则这个用例没测到任何东西"
    );
    assert_eq!(rig.runtime.active_cancel_watchers(), 0);

    let before = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    assert_eq!(
        rig.repo.get_calls(),
        before,
        "轮询失败后必须停止轮询，而不是静默地一直重试"
    );
}

// ---------------------------------------------------------------- C5：不回归

/// 没有取消时看护者绝不能自己翻转令牌：一个健康的 Job 必须正常跑完。
#[tokio::test]
async fn no_cancel_never_flips_the_kernel_token() {
    let mut rig = rig("job-cw-5", vec![("s1", Plan::Idle)]).await;
    let run = rig.start();
    assert!(matches!(rig.next().await, Msg::Entered { .. }));
    assert!(
        matches!(
            rig.next().await,
            Msg::Finished {
                observed_cancel: false,
                ..
            }
        ),
        "没有取消意图时令牌不得被翻转"
    );
    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
}

/// C5：阶段之间落下的取消仍然由 `dispatch` 的阶段边界检查接住——s2 开跑时令牌已是取消态，
/// 阶段级语义不回归。
#[tokio::test]
async fn between_stage_cancel_is_still_delivered_at_the_stage_boundary() {
    let mut rig = rig(
        "job-cw-6",
        vec![("s1", Plan::CancelAfterSelf), ("s2", Plan::WaitCancel)],
    )
    .await;
    let run = rig.start();

    match rig.next().await {
        Msg::Entered {
            cancel_at_entry,
            stage,
            ..
        } => {
            assert_eq!(stage, "s1");
            assert!(!cancel_at_entry);
        }
        other => panic!("期望进入 s1，得到 {other:?}"),
    }
    match rig.next().await {
        Msg::Finished { stage, .. } => {
            assert_eq!(stage, "s1");
        }
        other => panic!("期望 s1 结束，得到 {other:?}"),
    }
    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
    assert_eq!(rig.runtime.active_cancel_watchers(), 0);
    assert!(
        rig.rx.try_recv().is_err(),
        "cancel prevents the next stage from starting"
    );
    let record = rig
        .repo
        .get(&ctx(), rig.job_id.clone())
        .await
        .expect("query stage projection");
    assert_eq!(record.view.stage.as_deref(), Some("s1"));
    assert_eq!(record.stages.len(), 1);
    assert_eq!(record.stages[0].stage_id, StageId::new("s1"));
    assert!(record.stages[0].started_at.is_some());
    assert!(
        !record
            .stages
            .iter()
            .any(|stage| stage.stage_id == StageId::new("s2") && stage.started_at.is_some()),
        "cancelled next stage must not acquire a started record"
    );
}

/// C5：排队取消（开跑前就已请求）仍是一条"意图"，直接落到 `NotStarted`，不派发任何阶段。
#[tokio::test]
async fn queued_cancel_still_finalizes_not_started() {
    let mut rig = rig("job-cw-7", vec![("s1", Plan::WaitCancel)]).await;
    rig.repo
        .request_cancel(&ctx(), &rig.job_id)
        .expect("落取消意图");
    let run = rig.start();
    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
    assert!(rig.rx.try_recv().is_err(), "排队取消不得派发任何阶段");
    assert_eq!(rig.runtime.active_cancel_watchers(), 0);
}
