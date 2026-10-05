//! registry 集成测试的公共夹具。
//!
//! # 为什么另建一套后端替身
//!
//! `src/registry/actor/tests.rs` 里那份 `FakeBackend` 是 `#[cfg(test)]` 模块私有的，
//! **编译单元内**才存在。集成测试是另一个 crate，拿不到它；把它 `pub(crate)` 放开
//! 也只会让生产 crate 背上一个「为测试存在的后端」。因此这里另写一份，
//! 只依赖 `datazen_runtime` 的**公开**面——这本身就是一道额外的关卡：
//! 夹具能编译，就说明 `registry::backend` 的接缝形状对 crate 外的人真的可用。
//!
//! # 确定性纪律（与 crate 内夹具同规）
//!
//! - 时钟走 `#[tokio::test(start_paused = true)]`，且**没有一处**靠时间推进制造并发窗口。
//!   窗口由闸门 channel 显式开合：需要「命令还没被回答」时用
//!   `oneshot::Receiver::try_recv()` 拿 `Empty`，**不用** `tokio::time::timeout`——
//!   暂停时钟在空闲时会自己把时间推过去，那样的断言会自己把自己等超时。
//! - `ScriptedBackend` 在 `.await` 上持有 `tokio::sync::Mutex`。生产路径禁止持锁跨 `.await`
//!   是**actor 自己**的死锁防线；夹具后端只有自己一个调用方，且除闸门外不持锁跨 await。
//!
//! # 本文件不读 `.env` / `.env.test`
//!
//! 全部目标都是内存里的合成替身，不产生任何真实外部连接。

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, Mutex};

use datazen_platform_api::id::RuntimeEpoch as PlatformRuntimeEpoch;

use datazen_runtime::connection::types::{ClientInstanceId, EditorSessionId};
use datazen_runtime::connection::ProviderError;
use datazen_runtime::connection::{
    CloseMode, CommandCall, ConfigRevision, ConnectionId, Counter, DbSessionId, EffectOutcome,
    ExecuteInSessionRequest, ExecutionId, ExecutionState, ExecutionTarget, HandleId, HandleKind,
    NamespaceTarget, ObjectTarget, OrganizationId, OwnerRef, PrincipalId, ResourceId,
    SessionContext, SessionHandle, SessionHandleRef, SessionView, StreamId, Timestamp, WorkerId,
};
use datazen_runtime::directory::SessionHandle as DirectoryHandle;
use datazen_runtime::registry::actor::OpenRequest;
use datazen_runtime::registry::backend::{
    CancelOnResource, CloseResource, CloseResourceOutcome, ExecuteOnResource, FinalizeHandles,
    HandleFinalization, OpenResource, OpenedResource, ResourceCancel, ResourceExecution,
    SessionBackend,
};
use datazen_runtime::registry::{CancelDisposition, CapabilityVersions, SessionRegistry};

/// 测试用的 runtime epoch。固定值：世代号本身不是被测对象。
pub const EPOCH: u64 = 7;
/// 宿主铸造的执行 id 前缀。
pub const EXEC_PREFIX: &str = "exec_7_";
/// [`open_request`] 里那个空闲期限。用例引用它而不是写死数字，
/// 否则「未到期限 / 已到期限」两格会随夹具改动一起悄悄失真。
pub const IDLE_DEADLINE_MS: u64 = 60_000;
/// 会话数上限：留一个空位好测「额度用尽」。
pub const SESSION_LIMIT: usize = 4;

/// 闸门发送端。用例靠 `send(())` 精确控制后端的 `.await` 边界。
pub type Gate = mpsc::UnboundedSender<()>;

/// 一个执行在 [`ScriptedBackend`] 里的剧本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    /// 立刻成功，**不**公布 cancelHandle。
    Immediate,
    /// 放行第一道闸门后公布 `cancelHandle`，放行第二道闸门后才成功。
    TwoStage,
    /// 永远不公布、也永远不结束。
    Silent,
}

impl Script {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::TwoStage => "twoStage",
            Self::Silent => "silent",
        }
    }

    /// 真正干活之前要吃几道闸门。
    pub const fn gates(self) -> usize {
        match self {
            Self::Immediate => 0,
            Self::TwoStage => 2,
            Self::Silent => 2,
        }
    }
}

/// 用例收集到的后端调用痕迹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traces {
    pub open: usize,
    pub execute: usize,
    pub cancel: usize,
    pub finalize: usize,
    pub close: usize,
    /// 同一时刻并发的执行数峰值：跨会话并行（CM-20）的直接证据。
    pub peak_live: usize,
    pub canceled: Vec<CancelOnResource>,
    pub finalized: Vec<FinalizeHandles>,
    pub closed_with: Vec<CloseResource>,
}

struct BackendState {
    open_fails: bool,
    script: Script,
    execute_state: ExecutionState,
    execute_effect_outcome: EffectOutcome,
    execute_handles: Vec<SessionHandleRef>,
    execute_context_revision: u64,
    execute_fails: bool,
    driver_supports_cancel: bool,
    cancel_fails: bool,
    cancel_handle: String,
    finalize: HandleFinalization,
    /// 逐次回报的终结结果（见 [`BackendPlan::finalize_sequence`]）。
    finalize_sequence: Vec<HandleFinalization>,
    finalize_index: usize,
    close: CloseResourceOutcome,
}

/// §7.1 / §6.5 / §7.6 / §9.4 四个接缝的可控替身。
pub struct ScriptedBackend {
    state: Mutex<BackendState>,
    gate: Mutex<mpsc::UnboundedReceiver<()>>,
    gate_tx: Gate,
    open_calls: AtomicUsize,
    execute_calls: AtomicUsize,
    cancel_calls: AtomicUsize,
    published: AtomicUsize,
    finalize_calls: AtomicUsize,
    close_calls: AtomicUsize,
    live: AtomicUsize,
    peak_live: AtomicUsize,
    canceled: Mutex<Vec<CancelOnResource>>,
    finalized: Mutex<Vec<FinalizeHandles>>,
    closed_with: Mutex<Vec<CloseResource>>,
}

impl ScriptedBackend {
    pub async fn new(plan: BackendPlan) -> Arc<Self> {
        let (gate_tx, gate_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            state: Mutex::new(BackendState {
                open_fails: plan.open_fails,
                script: plan.script,
                execute_state: plan.execute_state,
                execute_effect_outcome: plan.execute_effect_outcome,
                execute_handles: plan.execute_handles,
                execute_context_revision: plan.execute_context_revision,
                execute_fails: plan.execute_fails,
                driver_supports_cancel: plan.driver_supports_cancel,
                cancel_fails: plan.cancel_fails,
                cancel_handle: plan.cancel_handle,
                finalize: plan.finalize,
                finalize_sequence: plan.finalize_sequence,
                finalize_index: 0,
                close: plan.close,
            }),
            gate: Mutex::new(gate_rx),
            gate_tx,
            open_calls: AtomicUsize::new(0),
            execute_calls: AtomicUsize::new(0),
            cancel_calls: AtomicUsize::new(0),
            published: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            close_calls: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            peak_live: AtomicUsize::new(0),
            canceled: Mutex::new(Vec::new()),
            finalized: Mutex::new(Vec::new()),
            closed_with: Mutex::new(Vec::new()),
        })
    }

    /// 闸门发送端：用例靠它精确开合后端的 `.await` 边界。
    pub fn gate(&self) -> Gate {
        self.gate_tx.clone()
    }

    /// 默认剧本：打开成功、driver 支持取消、执行立刻成功且不公布 handle。
    pub async fn defaults() -> Arc<Self> {
        Self::new(BackendPlan::default()).await
    }

    pub fn open_calls(&self) -> usize {
        self.open_calls.load(Ordering::SeqCst)
    }

    pub fn execute_calls(&self) -> usize {
        self.execute_calls.load(Ordering::SeqCst)
    }

    pub fn cancel_calls(&self) -> usize {
        self.cancel_calls.load(Ordering::SeqCst)
    }

    /// cancelHandle 已经**公布**给 actor 的次数。
    ///
    /// 「调用方声称的绑定」要跟「actor 自己登记的绑定」逐字比对，
    /// 所以测试必须知道后端到底公布没公布：靠 `execute_calls()` 猜是不行的，
    /// 那只是「已经进了 execute」，与句柄是否下发是两件事。
    pub fn published_calls(&self) -> usize {
        self.published.load(Ordering::SeqCst)
    }

    pub fn finalize_calls(&self) -> usize {
        self.finalize_calls.load(Ordering::SeqCst)
    }

    pub fn close_calls(&self) -> usize {
        self.close_calls.load(Ordering::SeqCst)
    }

    pub fn peak_live(&self) -> usize {
        self.peak_live.load(Ordering::SeqCst)
    }

    pub async fn traces(&self) -> Traces {
        Traces {
            open: self.open_calls(),
            execute: self.execute_calls(),
            cancel: self.cancel_calls(),
            finalize: self.finalize_calls(),
            close: self.close_calls(),
            peak_live: self.peak_live(),
            canceled: self.canceled.lock().await.clone(),
            finalized: self.finalized.lock().await.clone(),
            closed_with: self.closed_with.lock().await.clone(),
        }
    }
}

/// 后端可配项。默认全「干净」，用例只改自己那一格。
#[derive(Debug, Clone)]
pub struct BackendPlan {
    open_fails: bool,
    script: Script,
    execute_state: ExecutionState,
    execute_effect_outcome: EffectOutcome,
    execute_handles: Vec<SessionHandleRef>,
    execute_context_revision: u64,
    execute_fails: bool,
    driver_supports_cancel: bool,
    cancel_fails: bool,
    cancel_handle: String,
    finalize: HandleFinalization,
    /// 逐次回报：跨资源分批终结时每批的确认数必须不同，而 §9.4 比的是
    /// **累计**确认数与宿主登记数，一份固定回报表达不了这种形状。
    finalize_sequence: Vec<HandleFinalization>,
    close: CloseResourceOutcome,
}

impl Default for BackendPlan {
    fn default() -> Self {
        Self {
            open_fails: false,
            script: Script::Immediate,
            execute_state: ExecutionState::Succeeded,
            execute_effect_outcome: EffectOutcome::Completed,
            execute_handles: Vec::new(),
            // 打开成功后 actor 的世代是 1；这里必须给**不同**的值，
            // 否则「执行推进上下文世代」这条不变量在夹具层面就被抹平了：
            // 前后都是 1，任何 `assert_eq!(after, before + 1)` 都会蒙对。
            execute_context_revision: 2,
            execute_fails: false,
            driver_supports_cancel: true,
            cancel_fails: false,
            cancel_handle: "ch_exec_7_1".to_owned(),
            finalize: HandleFinalization {
                finalized: 0,
                remaining: 0,
                effect_outcome: EffectOutcome::RolledBack,
            },
            finalize_sequence: Vec::new(),
            close: CloseResourceOutcome::Closed,
        }
    }
}

impl BackendPlan {
    pub fn with_script(mut self, script: Script) -> Self {
        self.script = script;
        self
    }

    pub fn open_fails(mut self) -> Self {
        self.open_fails = true;
        self
    }

    pub fn execute_fails(mut self) -> Self {
        self.execute_fails = true;
        self
    }

    pub fn driver_without_cancel(mut self) -> Self {
        self.driver_supports_cancel = false;
        self
    }

    pub fn cancel_fails(mut self) -> Self {
        self.cancel_fails = true;
        self
    }

    pub fn cancel_handle(mut self, handle: &str) -> Self {
        self.cancel_handle = handle.to_owned();
        self
    }

    pub fn finalizes(mut self, finalized: usize, remaining: usize) -> Self {
        self.finalize = HandleFinalization {
            finalized,
            remaining,
            effect_outcome: if remaining == 0 {
                EffectOutcome::RolledBack
            } else {
                EffectOutcome::Unknown
            },
        };
        self
    }

    /// 逐次回报终结结果：`[(finalized, remaining), ..]`，第 n 次调用取第 n 项，
    /// 超出后重复最后一项。
    pub fn finalize_sequence(mut self, steps: &[(usize, usize)]) -> Self {
        self.finalize_sequence = steps
            .iter()
            .map(|(finalized, remaining)| HandleFinalization {
                finalized: *finalized,
                remaining: *remaining,
                effect_outcome: if *remaining == 0 {
                    EffectOutcome::RolledBack
                } else {
                    EffectOutcome::Unknown
                },
            })
            .collect();
        self
    }

    pub fn close_undecidable(mut self) -> Self {
        self.close = CloseResourceOutcome::Undecidable {
            reason: "closeTimeout",
        };
        self
    }

    /// 执行完成后 actor 会把会话世代**置成**这个值（后端是权威）。
    pub fn context_revision(mut self, revision: u64) -> Self {
        self.execute_context_revision = revision;
        self
    }

    pub fn handles(mut self, handles: Vec<SessionHandleRef>) -> Self {
        self.execute_handles = handles;
        self
    }

    pub fn behavior(mut self, state: ExecutionState, outcome: EffectOutcome) -> Self {
        self.execute_state = state;
        self.execute_effect_outcome = outcome;
        self
    }
}

#[async_trait]
impl SessionBackend for ScriptedBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, ProviderError> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let state = self.state.lock().await;
        if state.open_fails {
            return Err(ProviderError::SessionNotFound("openRefused".to_owned()));
        }
        Ok(OpenedResource {
            context: SessionContext::new(request.initial_target, "tester"),
            capabilities: CapabilityVersions {
                contract: "1.0.0".to_owned(),
                driver_api: "2.3.1".to_owned(),
            },
            resource_id: format!("res_{}", request.runtime_epoch),
            driver_supports_cancel: state.driver_supports_cancel,
        })
    }

    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, ProviderError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        let live = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak_live.fetch_max(live, Ordering::SeqCst);

        let state = self.state.lock().await;
        let script = state.script;
        let cancel_handle = state.cancel_handle.clone();
        drop(state);

        if script != Script::Immediate {
            // 第一道闸门：把执行**停在**真正干活之前。
            self.gate.lock().await.recv().await;
            if script == Script::TwoStage {
                let published = request.cancel_handle_sink.publish(cancel_handle.clone());
                self.published.fetch_add(1, Ordering::SeqCst);
                if !published {
                    self.live.fetch_sub(1, Ordering::SeqCst);
                    return Err(ProviderError::ResourceLost("actorGone".to_owned()));
                }
            }
            // 第二道闸门：停在「句柄已绑定、执行未结束」。
            self.gate.lock().await.recv().await;
        }

        let state = self.state.lock().await;
        if state.execute_fails {
            self.live.fetch_sub(1, Ordering::SeqCst);
            return Err(ProviderError::HostRejected("executeRejected".to_owned()));
        }
        let stream_id = StreamId::new(format!("st_{}", request.execution_id.as_str()));
        let execution = ResourceExecution {
            execution_id: request.execution_id,
            stream_id,
            state: state.execute_state,
            effect_outcome: state.execute_effect_outcome,
            context_after: SessionContext::new(namespace(), "tester"),
            context_revision: state.execute_context_revision,
            handles: state.execute_handles.clone(),
            cancel_handle,
        };
        drop(state);
        self.live.fetch_sub(1, Ordering::SeqCst);
        Ok(execution)
    }

    async fn cancel(&self, request: CancelOnResource) -> Result<ResourceCancel, ProviderError> {
        self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        self.canceled.lock().await.push(request);
        if self.state.lock().await.cancel_fails {
            return Err(ProviderError::HostRejected("cancelRejected".to_owned()));
        }
        Ok(ResourceCancel {
            disposition: CancelDisposition::Requested,
            state: ExecutionState::CancelRequested,
        })
    }

    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, ProviderError> {
        self.finalize_calls.fetch_add(1, Ordering::SeqCst);
        self.finalized.lock().await.push(request);
        let mut state = self.state.lock().await;
        if state.finalize_sequence.is_empty() {
            return Ok(state.finalize.clone());
        }
        let index = state.finalize_index.min(state.finalize_sequence.len() - 1);
        state.finalize_index += 1;
        Ok(state.finalize_sequence[index].clone())
    }

    async fn close(&self, request: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
        self.close_calls.fetch_add(1, Ordering::SeqCst);
        self.closed_with.lock().await.push(request);
        Ok(self.state.lock().await.close)
    }
}

// ---------------------------------------------------------------------------
// 夹具工厂
// ---------------------------------------------------------------------------

pub fn namespace() -> NamespaceTarget {
    NamespaceTarget {
        database: "app".to_owned(),
        catalog: "main".to_owned(),
        schema: "public".to_owned(),
        path: "ns/app".to_owned(),
    }
}

pub fn db_session_id() -> DbSessionId {
    DbSessionId::new("dbs_1")
}

pub fn other_db_session_id() -> DbSessionId {
    DbSessionId::new("dbs_2")
}

pub fn worker_id() -> WorkerId {
    WorkerId::new("w_1")
}

pub fn open_request(db: DbSessionId) -> OpenRequest {
    OpenRequest {
        db_session_id: db,
        worker_id: worker_id(),
        connection_id: ConnectionId::new("conn_1"),
        config_revision: ConfigRevision::new(1),
        owner: OwnerRef::Editor {
            organization_id: OrganizationId::new("org_1"),
            principal_id: PrincipalId::new("pr_1"),
            connection_id: ConnectionId::new("conn_1"),
            client_instance_id: ClientInstanceId::new("cli-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        },
        initial_target: ExecutionTarget {
            connection_id: ConnectionId::new("conn_1"),
            namespace: namespace(),
            object: Some(ObjectTarget {
                kind: "table".to_owned(),
                name: "orders".to_owned(),
                signature: String::new(),
            }),
        },
        expires_at: Timestamp::new("2026-01-01T00:30:00Z"),
        idle_deadline_ms: Some(IDLE_DEADLINE_MS),
    }
}

/// 造一条宿主铸造的执行 id（格式 `exec_{epoch}_{seq}`）。
pub fn execution_id(seq: u64) -> ExecutionId {
    ExecutionId::new(format!("exec_{}_{}", EPOCH, seq))
}

/// 造一条句柄引用。
pub fn handle_ref(handle_id: &str, resource_id: &str) -> SessionHandleRef {
    SessionHandleRef::new(
        HandleId::new(handle_id),
        HandleKind::Transaction,
        ResourceId::new(resource_id),
        Counter(EPOCH),
    )
}

pub fn handle_of(view: &SessionView) -> SessionHandle {
    view.handle.clone()
}

/// 目录侧的 runtime epoch 字符串。与 `ContextReplacer::directory_handle` 的派生式
/// **逐字一致**：重放幂等靠的就是这个句柄派生的 operation key，两端一旦漂移，幂等短路
/// 会安静地不命中。
///
/// 全仓一共两处 `rte-` 格式化：本函数与生产侧 `registry/context.rs` 里那个同名的
/// `epoch_string`（两端必须逐字一致，不能只靠测试替身）。测试侧曾经还各写一份
/// `format!("rte-{:08}", …)` 和几个写死的 `"rte-0000000N"` 字面量，那些已收拢到这里。
pub fn epoch_string(epoch: u64) -> String {
    format!("rte-{:08}", epoch)
}

/// 登记表**第一个**会话拿 `Counter(1)`，`epoch_string(1)` 落在 `rte-00000001`。
pub const FIRST_SESSION_EPOCH: u64 = 1;
/// 登记表**第二个**会话（替换路径上的候选）拿 `Counter(2)`。
pub const SECOND_SESSION_EPOCH: u64 = 2;

/// 由 runtime 句柄推目录句柄。测试侧一律走这里，不要就地拼字符串。
pub fn directory_handle_of(handle: &SessionHandle) -> DirectoryHandle {
    DirectoryHandle::new(
        handle.db_session_id.clone(),
        PlatformRuntimeEpoch::new(epoch_string(handle.runtime_epoch.get())),
    )
}

/// 旧世代的句柄（`runtime_epoch` 对不上）。
pub fn stale_handle(view: &SessionView) -> SessionHandle {
    let mut handle = view.handle.clone();
    handle.runtime_epoch = Counter(EPOCH - 1);
    handle
}

pub fn execute_request(handle: &SessionHandle, expected: u64) -> ExecuteInSessionRequest {
    ExecuteInSessionRequest {
        handle: handle.clone(),
        expected_context_revision: Counter(expected),
        call: CommandCall {
            command: "query".to_owned(),
            input: serde_json::json!({ "sql": "select 1" }),
        },
        idempotency_key: "idem_1".to_owned(),
    }
}

/// 打开一个已登记、已进入 `Ready` 的会话，返回它的投影。
pub async fn register_ready(registry: &SessionRegistry, db: DbSessionId) -> SessionView {
    registry
        .register_session(open_request(db))
        .await
        .expect("登记必须成功")
}

/// 只吃一个调度让步：足以让已就绪的任务跑完一个 `.await` 往返，
/// 不足以跨过需要显式闸门的窗口。
pub async fn settle() {
    tokio::task::yield_now().await;
}

/// 把替身提升成登记表持有的那个接口对象。
///
/// 登记表存的是 `Arc<dyn SessionBackend>`；`Arc::clone(&x)` 的返回类型是 `Self`，
/// 不是强制转换点，所以 `SessionRegistry::new(Arc::clone(&替身), ..)` 编译不过。
pub fn as_backend(backend: &Arc<ScriptedBackend>) -> Arc<dyn SessionBackend> {
    Arc::clone(backend) as Arc<dyn SessionBackend>
}

/// 非阻塞地问一次「好了没」。
///
/// 并发窗口的就绪判据不能用 `tokio::time::timeout`：暂停时钟下运行时会自动推进时间，
/// 「等 10ms 以为在超时」和「真的超时」在这里是同一件事，测出来的永远是前者。
/// `oneshot::try_recv` 把这件事变成无歧义的三态：`Some`（好了）、`Empty`（还在飞）、
/// `Closed`（任务没了）。
pub fn ask<T>(rx: &mut oneshot::Receiver<T>) -> Option<T> {
    rx.try_recv().ok()
}

/// 有界轮询地问「好了没」，每轮只让出一次调度。
///
/// `tokio::spawn` 出来的任务不会立刻跑：不给它一次 `yield_now` 就问，
/// 永远问不到东西，于是「控制面被堵死」和「控制面还没被调度」长得一模一样。
/// 同样不用定时器：这里数的是调度轮次，不是墙上的毫秒。
pub async fn ask_until<T>(rx: &mut oneshot::Receiver<T>) -> Option<T> {
    for _ in 0..512 {
        if let Ok(value) = rx.try_recv() {
            return Some(value);
        }
        settle().await;
    }
    rx.try_recv().ok()
}

/// 有界等待条件成立。**只作失败诊断用**：条件不成立时循环耗尽即返回 `false`，
/// 调用方随后拿 `false` 去 `assert!` 出真正的失败信息，而不是在这里 panic。
pub async fn wait_until<F: Fn() -> bool>(condition: F) -> bool {
    for _ in 0..512 {
        if condition() {
            return true;
        }
        settle().await;
    }
    condition()
}

/// 用例默认的 `CloseMode`：先回滚再关。用例关心的是**释放顺序**，
/// 不是 `RequireNoTransaction` 那条拒绝分支（后者在 `registry_release.rs` 里单独测）。
pub fn close_any() -> CloseMode {
    CloseMode::RollbackAndClose
}
