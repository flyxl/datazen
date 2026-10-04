//! §6.3 会话 actor：单会话串行、跨会话并行的执行仲裁器。
//!
//! ## 为什么是邮箱而不是锁
//!
//! 「同一会话内并发度为 1、不同会话之间真正并发」（CM-20）有两条实现路径：
//!
//! | 路径 | 形状 | 代价 |
//! | --- | --- | --- |
//! | 全局 `Mutex` 串行 | 一把锁包住整个 registry 的执行 | 不同会话也互相排队，CM-20 直接不成立 |
//! | 每会话 actor + 邮箱 | 每个会话一条 FIFO 队列 | 需要处理飞行中取消、关闭排队这些边界 |
//!
//! 这里选后者。**`Mutex` 绝不跨 `.await`**：actor 状态由循环本身以 `&mut` 独占，
//! 跨 `.await` 存活的只有 [`InFlight`] 里那些**按值**带走的字段（后端 `Arc`、
//! `JoinHandle`、cancelHandle 公布通道、调用方的 reply 通道）。
//! 不存在「拿着锁在 await 里排队」这种形状。
//!
//! ## 两条通道，不是一条
//!
//! - `exec_rx`：**执行队列**。`Open` / `Execute` / `Close` / `Evict` / `RegisterHandles` 按 FIFO。
//!   飞行中进来的这些请求**排队**，不并发——这是 CM-20 的「同会话并发度 1」在代码里的落点。
//! - `ctrl_rx`：**控制旁路**（§6.3）。`Cancel` / `InvalidateWorker` 在执行**飞行中**必须能被服务，
//!   否则「执行中取消」这条主路径会被自己的队列堵死，CM-22 无从谈起。
//!
//! 只有 `View` 被放行在飞行中读取：它是纯投影，不碰物理资源。
//!
//! ## 释放顺序只有这一份实现
//!
//! §9.4 的「归池前检查」是一条**有序**序列，把它拆成两处实现就等于允许乱序：
//!
//! ```text
//! 1. finalize_handles —— 在**原资源**上回滚/终结已登记句柄（按 resource_id 分组）
//! 2. drain_handles    —— 从宿主账上注销（此后 registered_handles 必然为 0）
//! 3. close            —— 关闭物理资源
//! 4. 清空绑定与句柄   —— actor 终止后不得重建任何句柄（§6.5）
//! ```
//!
//! 第 1 步排在关闭之前，是因为在新资源上关掉「旧句柄」等于把一个仍然可提交的
//! 句柄连同它的资源一起丢掉——调用方随后 commit 会拿到一个静默失败的假成功（CM-73）。
//!
//! ## 「不得返回成功」的具体落点
//!
//! `Open` 在已有物理资源时回 `InvariantBroken`；`View` / `Execute` / `RegisterHandles`
//! 在 actor 尚未 open 时回 `SessionClosed`；飞行中再次收到 `Execute` 会被排队而不是并发。
//! 这些错误**不是**「暂时不可用」，它们表示「请求的前提不成立」，
//! 因此调用方不得把它们当成需要重试的信号。

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinError, JoinHandle};

use crate::connection::error::ProviderError;
use crate::connection::port::CancelDisposition;
use crate::connection::RuntimeError;
use crate::connection::{
    AttachmentState, CloseMode, ConfigRevision, ConnectionId, Counter, DbSessionId, EffectOutcome,
    ExecuteInSessionRequest, ExecutionErrorCode, ExecutionId, ExecutionReceipt, ExecutionState,
    ExecutionTarget, OwnerRef, SessionContext, SessionHandle, SessionHandleRef, SessionState,
    SessionView, Timestamp, WorkerId,
};

use crate::registry::audit::{AuditKind, CapabilityVersions, Outcome, RegistryAuditEntry};
use crate::registry::backend::{OpenResource, ResourceExecution, SessionBackend};
use crate::registry::epoch::RuntimeEpoch;
use crate::registry::handles::{ExecutionBinding, HandleRegistry};
use crate::registry::receipt::CancelReceipt;

// 取消、执行、释放各自成文件：三者都是**独立判定 + 独立审计**的长流程，
// 混进 actor 主体会让主循环的信噪比塌掉。
mod cancel;
mod context;
mod exec;
mod release;

use cancel::cancel;
use release::ReleaseReason;

#[cfg(test)]
mod tests;

/// 单请求回执通道。actor 退出时发送端被丢弃，调用方拿到错误而不是永远等待。
pub type Reply<T> = oneshot::Sender<Result<T, RuntimeError>>;

/// 审计外发口。actor **不持有**审计台账的锁，只是往这条通道里推条目；registry 侧排队收。
/// 锁因此永远不会被一个 `.await` 持有。
pub type AuditOutbox = mpsc::UnboundedSender<RegistryAuditEntry>;

/// 打开一个会话所需的全部输入。
///
/// `db_session_id` 由**调用方**给出（§ID 术语纪律：directory 发号，registry 不生成），
/// actor 不校验它的来源，只负责把它与 `runtime_epoch` 一起焊进 [`SessionHandle`]。
#[derive(Debug, Clone)]
pub struct OpenRequest {
    pub db_session_id: DbSessionId,
    pub worker_id: WorkerId,
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub owner: OwnerRef,
    pub initial_target: ExecutionTarget,
    pub expires_at: Timestamp,
    /// 空闲驱逐期限（调用方注入的绝对毫秒）。`None` 表示不参与空闲驱逐。
    pub idle_deadline_ms: Option<u64>,
}

/// 执行队列（FIFO）。
pub enum ExecCommand {
    Open {
        request: OpenRequest,
        reply: Reply<SessionView>,
    },
    View {
        handle: SessionHandle,
        reply: Reply<SessionView>,
    },
    Execute {
        request: ExecuteInSessionRequest,
        reply: Reply<ExecutionReceipt>,
    },
    RegisterHandles {
        handle: SessionHandle,
        handles: Vec<SessionHandleRef>,
        reply: Reply<usize>,
    },
    SetIdleDeadline {
        handle: SessionHandle,
        deadline_ms: Option<u64>,
        reply: Reply<Option<u64>>,
    },
    Close {
        handle: SessionHandle,
        mode: CloseMode,
        reply: Reply<SessionState>,
    },
    /// §7.4-6 第 4 步：举发放闸门，停止旧会话发放新执行（资源与句柄原封不动）。
    HoldForReplacement {
        handle: SessionHandle,
        reply: Reply<()>,
    },
    /// §7.4-6 第 10 项：提交前失败，放下闸门，旧会话**原样**恢复发放。
    ResumeAfterReplacement {
        handle: SessionHandle,
        reply: Reply<()>,
    },
    /// 空闲驱逐。`Ok(None)` 表示**未到期限**，这是一次正常的空操作，不是失败。
    Evict {
        at_ms: u64,
        reply: Reply<Option<SessionView>>,
    },
}

/// 控制旁路（§6.3）。飞行中必须可服务。
pub enum ControlCommand {
    Cancel {
        handle: SessionHandle,
        execution_id: ExecutionId,
        /// `Some` = 调用方**自带**的 cancelHandle，走 §3.2 L143 的伪造校验。
        /// `None` = 调用方只给了 executionId（冻结的 `SessionPort` 形状就是如此），
        /// actor 必须用自己在飞行开始时登记的绑定，**不得**因此跳过校验。
        cancel_handle: Option<String>,
        reply: Reply<CancelReceipt>,
    },
    /// §12 / CM-58：worker 租约失效。`Ok(false)` 表示本会话不属于该 worker。
    InvalidateWorker {
        worker_id: WorkerId,
        reply: Reply<bool>,
    },
}

/// actor 对外的句柄。克隆廉价，可放进任何结构；发送者一掉，actor 收尾。
#[derive(Clone)]
pub struct SessionActor {
    db_session_id: DbSessionId,
    exec_tx: mpsc::UnboundedSender<ExecCommand>,
    ctrl_tx: mpsc::UnboundedSender<ControlCommand>,
}

impl SessionActor {
    pub fn db_session_id(&self) -> &DbSessionId {
        &self.db_session_id
    }

    /// actor 是否已退出。true 之后任何请求都拿不到成功。
    pub fn is_closed(&self) -> bool {
        self.exec_tx.is_closed()
    }

    pub async fn exec<T>(
        &self,
        build: impl FnOnce(Reply<T>) -> ExecCommand,
    ) -> Result<T, RuntimeError> {
        let (tx, rx) = oneshot::channel();
        if self.exec_tx.send(build(tx)).is_err() {
            return Err(RuntimeError::UnknownSession(self.db_session_id.to_string()));
        }
        rx.await.unwrap_or(Err(RuntimeError::SessionClosed(
            self.db_session_id.to_string(),
        )))
    }

    pub async fn control<T>(
        &self,
        build: impl FnOnce(Reply<T>) -> ControlCommand,
    ) -> Result<T, RuntimeError> {
        let (tx, rx) = oneshot::channel();
        if self.ctrl_tx.send(build(tx)).is_err() {
            return Err(RuntimeError::UnknownSession(self.db_session_id.to_string()));
        }
        rx.await.unwrap_or(Err(RuntimeError::SessionClosed(
            self.db_session_id.to_string(),
        )))
    }
}

/// 已打开的物理资源。
#[derive(Debug, Clone)]
struct Physical {
    resource_id: String,
    driver_supports_cancel: bool,
    capabilities: CapabilityVersions,
}

/// 飞行中的执行。跨 `.await` 存活的**全部**数据都在这里，按值携带。
///
/// 这里**只有状态，没有 await 点**：两个 await 点（cancelHandle 公布通道、执行任务句柄）
/// 挂在 [`ActorState::bind_rx`] / [`ActorState::join`] 上，由主循环取到局部变量去
/// `select!`。原因很硬：主循环必须一边等执行、一边接控制旁路（§7.6），而取消要读的
/// 正是这份飞行状态；只要把 `InFlight` 取进局部变量，控制旁路就会落在一个
/// `in_flight == None` 的窗口里，把「飞行中」误判成「未绑定」，每一次飞行中取消
/// 都回 `unboundExecution`。执行本体必须**始终留在 `state` 里**。
struct InFlight {
    execution_id: ExecutionId,
    resource_id: String,
    cancel_handle: Option<String>,
    /// 取消在 cancelHandle 公布**之前**就到了（CM-22 的排队面）。
    /// 后端一公布 handle 就必须把这次取消补送下去，否则回执里的
    /// `requested` 是一句没有人兑现的承诺。
    cancel_requested: bool,
    reply: Reply<ExecutionReceipt>,
}

struct ActorState {
    db_session_id: DbSessionId,
    runtime_epoch: RuntimeEpoch,
    worker_id: WorkerId,
    backend: Arc<dyn SessionBackend>,
    audit: AuditOutbox,
    context_revision: u64,
    idle_deadline_ms: Option<u64>,
    physical: Option<Physical>,
    handles: HandleRegistry,
    /// §7.4-6 的发放闸门。`Some(举起前的状态)` = 闸门举着，替换期间旧会话
    /// 停止发放新执行；`None` = 没在替换里。存**原状态**而不是布尔，是为了
    /// 提交前失败时能原样恢复，而不是写死一个状态凭空抬回去。
    replacement_hold: Option<SessionState>,
    /// 飞行中的执行。其余时候为 `None`。
    in_flight: Option<InFlight>,
    /// 飞行中的执行要等的两个 await 点，由主循环 `take()` 到局部变量去 `select!`，
    /// 处理完再放回。它们和 [`Self::in_flight`] 是同一次执行的**同一份数据**，
    /// 谁也不能先于谁落地：接收端被丢弃而 `in_flight` 还在，等于执行凭空消失。
    bind_rx: Option<mpsc::UnboundedReceiver<String>>,
    join: Option<JoinHandle<Result<ResourceExecution, ProviderError>>>,
    /// 飞行中收到的、必须排到执行之后的执行队列请求（CM-20 的排队面）。
    deferred: VecDeque<ExecCommand>,
    /// 最近终态，用于 §7.6 的 `alreadyFinished` 判定。上限 [`exec::SETTLED_CAP`]。
    settled: Vec<(ExecutionId, ExecutionState)>,
    execution_seq: u64,
    view: SessionView,
}

/// 启动一个会话 actor。
pub fn spawn_actor(
    request: OpenRequest,
    runtime_epoch: RuntimeEpoch,
    backend: Arc<dyn SessionBackend>,
    audit: AuditOutbox,
) -> SessionActor {
    let db_session_id = request.db_session_id.clone();
    let (exec_tx, exec_rx) = mpsc::unbounded_channel();
    let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel();

    let placeholder = SessionView {
        handle: SessionHandle {
            db_session_id: db_session_id.clone(),
            runtime_epoch: Counter(runtime_epoch.get()),
        },
        connection_id: request.connection_id.clone(),
        config_revision: request.config_revision.clone(),
        owner: request.owner.clone(),
        initial_target: request.initial_target.clone(),
        observed_context: SessionContext::new(
            request.initial_target.namespace.clone(),
            String::new(),
        ),
        context_revision: Counter(0),
        state: SessionState::New,
        attachment_state: AttachmentState::Detached,
        active_execution_id: None,
        expires_at: request.expires_at.clone(),
    };

    let state = ActorState {
        db_session_id: db_session_id.clone(),
        runtime_epoch,
        worker_id: request.worker_id.clone(),
        backend,
        audit,
        context_revision: 0,
        idle_deadline_ms: request.idle_deadline_ms,
        physical: None,
        handles: HandleRegistry::new(),
        replacement_hold: None,
        in_flight: None,
        bind_rx: None,
        join: None,
        deferred: VecDeque::new(),
        settled: Vec::new(),
        execution_seq: 0,
        view: placeholder,
    };

    tokio::spawn(run_actor(state, request, exec_rx, ctrl_rx));

    SessionActor {
        db_session_id,
        exec_tx,
        ctrl_tx,
    }
}

/// 循环里的一次推进。
enum Step {
    /// cancelHandle 被后端公布。
    Published(Option<String>),
    /// 控制旁路或执行队列已处理，无状态变化。
    Pending,
    /// 执行结束。
    Joined(Result<Result<ResourceExecution, ProviderError>, JoinError>),
}

async fn run_actor(
    mut state: ActorState,
    open: OpenRequest,
    mut exec_rx: mpsc::UnboundedReceiver<ExecCommand>,
    mut ctrl_rx: mpsc::UnboundedReceiver<ControlCommand>,
) {
    // Open 走队列而不是直连：它必须和后续请求**排在同一条 FIFO 上**，
    // 否则一次并发的 Execute 可能排在 Open 前面执行。
    let _ = open;
    let mut exec_open = true;

    while exec_open || state.in_flight.is_some() {
        if state.in_flight.is_none() {
            tokio::select! {
                biased;
                Some(msg) = ctrl_rx.recv() => handle_control(&mut state, msg).await,
                msg = exec_rx.recv() => match msg {
                    Some(msg) => handle_exec(&mut state, msg).await,
                    None => exec_open = false,
                },
            }
            continue;
        }

        // 只把两个 await 点取到局部变量：`state` 因此在整个 `select!` 里可借给
        // `handle_control`，控制旁路才看得见飞行中的执行（见 [`InFlight`] 的说明）。
        let mut bind_rx = state.bind_rx.take();
        let mut join = state.join.take();

        let step = tokio::select! {
            biased;
            published = await_bind(&mut bind_rx) => Step::Published(published),
            msg = ctrl_rx.recv() => {
                if let Some(msg) = msg {
                    handle_control(&mut state, msg).await;
                }
                // 控制通道全关不代表可以退出：飞行中的执行还要收尾。
                Step::Pending
            },
            msg = exec_rx.recv() => {
                if let Some(msg) = msg {
                    handle_exec_while_flying(&mut state, msg).await;
                } else {
                    exec_open = false;
                }
                Step::Pending
            },
            outcome = await_join(&mut join) => Step::Joined(outcome),
        };

        match step {
            Step::Published(Some(cancel_handle)) => {
                // 先取出来：`state.in_flight.as_mut()` 会把整个 `state` 借走。
                let context_revision = state.context_revision;
                if let Some(active) = state.in_flight.as_mut() {
                    let binding = ExecutionBinding {
                        execution_id: active.execution_id.clone(),
                        cancel_handle: cancel_handle.clone(),
                        resource_id: active.resource_id.clone(),
                        context_revision,
                    };
                    state.handles.bind_execution(binding);
                    active.cancel_handle = Some(cancel_handle);
                    let queued = active.cancel_requested;
                    if queued {
                        // 提前到的取消必须补发，这里借的仍然是同一份飞行状态。
                        cancel::deliver_published(&mut state).await;
                    }
                }
                state.bind_rx = bind_rx;
                state.join = join;
            }
            Step::Published(None) => {
                // 发送端全关 = 后端不会再公布任何 handle。把这个**已关闭**的接收端
                // 放回去，会让 `biased` 的第一分支每轮都立刻就绪，饿死 join 分支——
                // 那是一个吃掉一个核的忙循环，执行永远收不了尾。丢掉它，等 join。
                drop(bind_rx);
                state.bind_rx = None;
                state.join = join;
            }
            Step::Pending => {
                state.bind_rx = bind_rx;
                state.join = join;
            }
            Step::Joined(outcome) => {
                // 两个 await 点都已兑现，不再放回。
                drop(bind_rx);
                drop(join);
                state.bind_rx = None;
                state.join = None;
                if let Some(active) = state.in_flight.take() {
                    exec::apply_completion(&mut state, active, outcome).await;
                }
                exec::drain_deferred(&mut state).await;
            }
        }
    }

    finish(&mut state).await;
}

/// 等后端公布 cancelHandle。接收端已经被丢弃时永不就绪。
async fn await_bind(bind_rx: &mut Option<mpsc::UnboundedReceiver<String>>) -> Option<String> {
    match bind_rx.as_mut() {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

/// 等飞行中的执行任务收尾。句柄已经被兑现时永不就绪。
async fn await_join(
    join: &mut Option<JoinHandle<Result<ResourceExecution, ProviderError>>>,
) -> Result<Result<ResourceExecution, ProviderError>, JoinError> {
    match join.as_mut() {
        Some(handle) => handle.await,
        None => std::future::pending().await,
    }
}

async fn handle_exec(state: &mut ActorState, msg: ExecCommand) {
    // §7.4-6 发放闸门：唯一的判定与拒绝执行点，放在所有命令的共同入口，
    // 这样「哪条发放路径忘了查闸门」在结构上就不可能发生。
    let msg = match context::gate(state, msg) {
        Ok(msg) => msg,
        Err(()) => return,
    };
    match msg {
        ExecCommand::Open { request, reply } => {
            let _ = reply.send(open(state, request).await);
        }
        ExecCommand::View { handle, reply } => {
            let _ = reply.send(read_view(state, &handle));
        }
        ExecCommand::Execute { request, reply } => exec::start_execution(state, request, reply),
        ExecCommand::RegisterHandles {
            handle,
            handles,
            reply,
        } => {
            let _ = reply.send(register_handles(state, &handle, handles));
        }
        ExecCommand::SetIdleDeadline {
            handle,
            deadline_ms,
            reply,
        } => {
            let _ = reply.send(set_idle_deadline(state, &handle, deadline_ms));
        }
        ExecCommand::Close {
            handle,
            mode,
            reply,
        } => {
            let _ = reply.send(close(state, &handle, mode).await);
        }
        ExecCommand::Evict { at_ms, reply } => {
            let _ = reply.send(evict_idle(state, at_ms).await);
        }
        ExecCommand::HoldForReplacement { handle, reply } => {
            let _ = reply.send(context::hold(state, &handle));
        }
        ExecCommand::ResumeAfterReplacement { handle, reply } => {
            let _ = reply.send(context::resume(state, &handle));
        }
    }
}

async fn handle_exec_while_flying(state: &mut ActorState, msg: ExecCommand) {
    // 只有只读投影可以插空回答；其余一律排队，否则同会话就会出现两条并发执行。
    if let ExecCommand::View { handle, reply } = msg {
        let _ = reply.send(read_view(state, &handle));
        return;
    }
    state.deferred.push_back(msg);
}

/// actor 退出时的最后收尾：物理资源不能因为「所有句柄都被丢掉」而泄漏。
async fn finish(state: &mut ActorState) {
    if state.physical.is_none() {
        return;
    }
    state.view.state = SessionState::Closing;
    let _ = release::release(state, CloseMode::RollbackAndClose, ReleaseReason::ActorGone).await;
}

// ---------------------------------------------------------------------------
// §7.1 打开
// ---------------------------------------------------------------------------

async fn open(state: &mut ActorState, request: OpenRequest) -> Result<SessionView, RuntimeError> {
    if state.physical.is_some() {
        return Err(RuntimeError::InvariantBroken("sessionAlreadyOpen"));
    }
    state.view.state = SessionState::Opening;

    let opened = state
        .backend
        .open(OpenResource {
            connection_id: request.connection_id.clone(),
            config_revision: request.config_revision.clone(),
            owner: request.owner.clone(),
            initial_target: request.initial_target.namespace.clone(),
            runtime_epoch: state.runtime_epoch.get(),
        })
        .await;

    let opened = match opened {
        Ok(opened) => opened,
        Err(_cause) => {
            state.view.state = SessionState::New;
            emit(
                state,
                AuditFacts {
                    kind: AuditKind::SessionRegistrationFailed,
                    outcome: Outcome::Rejected,
                    error_code: Some(ExecutionErrorCode::HostRejected),
                    ..AuditFacts::none()
                },
            );
            // §4.1：登记失败不得返回成功，也不得留下半开的记录。
            return Err(RuntimeError::UnknownSession(
                request.db_session_id.to_string(),
            ));
        }
    };

    state.context_revision = 1;
    state.view.observed_context = opened.context.clone();
    state.view.context_revision = Counter(state.context_revision);
    state.view.state = SessionState::Ready;
    state.physical = Some(Physical {
        resource_id: opened.resource_id.clone(),
        driver_supports_cancel: opened.driver_supports_cancel,
        capabilities: opened.capabilities.clone(),
    });

    emit(
        state,
        AuditFacts {
            kind: AuditKind::SessionRegistered,
            outcome: Outcome::Succeeded,
            ..AuditFacts::none()
        },
    );
    Ok(state.view.clone())
}

// ---------------------------------------------------------------------------
// §4.4 只读投影
// ---------------------------------------------------------------------------

fn read_view(state: &ActorState, handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
    check_epoch(state, handle)?;
    // 丢失的会话仍然可读：它只剩一具墓碑，但墓碑上的事务状态必须是 `Unknown`
    // 而不是 `Active`（CM-73）——上层要能看到「这条链的状态已经不可知」。
    // 墓碑仍然不能当活会话用：任何写操作都会在 `physical.is_none()` 上被拒。
    if state.physical.is_none() && state.view.state != SessionState::Lost {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    }
    Ok(state.view.clone())
}

fn check_epoch(state: &ActorState, handle: &SessionHandle) -> Result<(), RuntimeError> {
    if state.runtime_epoch.matches(handle) {
        Ok(())
    } else {
        Err(RuntimeError::UnknownSession(
            handle.db_session_id.to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// §7.6 取消（控制旁路）
// ---------------------------------------------------------------------------

async fn handle_control(state: &mut ActorState, msg: ControlCommand) {
    match msg {
        ControlCommand::Cancel {
            handle,
            execution_id,
            cancel_handle,
            reply,
        } => {
            let _ = reply.send(cancel(state, &handle, execution_id, cancel_handle).await);
        }
        ControlCommand::InvalidateWorker { worker_id, reply } => {
            let _ = reply.send(invalidate_worker(state, &worker_id).await);
        }
    }
}

/// §12 / CM-58：worker 租约失效，该会话转 `SessionLost`。
async fn invalidate_worker(
    state: &mut ActorState,
    worker_id: &WorkerId,
) -> Result<bool, RuntimeError> {
    if state.worker_id != *worker_id || state.physical.is_none() {
        return Ok(false);
    }
    // 释放失败本身就是「转 Lost」的证据，不能因为失败就反过来报「未失效」。
    //
    // R-02：`SessionInvalidated` 这条审计**只由 `release::release` 发**（走
    // `release_kind(InvalidateWorker)`）。这里再补一条，会产出两条字段逐字相同的条目
    // ——只有自增 `id` 不同——那不是「两条证据」，是同一条事实被数了两遍：
    // 按条数统计作废次数的调用方会把 1 次作废读成 2 次。
    //
    // 能走到这里的路径 `release` 必定发审计，所以删掉这里不会留下空洞：
    // `physical.is_none()` 的早返回（不发审计）已被上面的守卫排除，而
    // `RollbackAndClose` 不可能产生 `CloseRejected`（只有 `RequireNoTransaction` 才会），
    // 于是每条可达路径要么走成功分支、要么走 `undecidable` 分支，两者都发。
    let _ = release::release(
        state,
        CloseMode::RollbackAndClose,
        ReleaseReason::InvalidateWorker,
    )
    .await;
    Ok(true)
}

// ---------------------------------------------------------------------------
// §6.5 / §9.4 登记与释放
// ---------------------------------------------------------------------------

fn register_handles(
    state: &mut ActorState,
    handle: &SessionHandle,
    handles: Vec<SessionHandleRef>,
) -> Result<usize, RuntimeError> {
    check_epoch(state, handle)?;
    if state.physical.is_none() {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    }
    state.handles.register(handles)
}

fn set_idle_deadline(
    state: &mut ActorState,
    handle: &SessionHandle,
    deadline_ms: Option<u64>,
) -> Result<Option<u64>, RuntimeError> {
    check_epoch(state, handle)?;
    if state.physical.is_none() {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    }
    state.idle_deadline_ms = deadline_ms;
    Ok(state.idle_deadline_ms)
}

async fn close(
    state: &mut ActorState,
    handle: &SessionHandle,
    mode: CloseMode,
) -> Result<SessionState, RuntimeError> {
    check_epoch(state, handle)?;
    release::release(state, mode, ReleaseReason::Close).await
}

async fn evict_idle(
    state: &mut ActorState,
    at_ms: u64,
) -> Result<Option<SessionView>, RuntimeError> {
    if state.physical.is_none() {
        return Ok(None);
    }
    let Some(deadline) = state.idle_deadline_ms else {
        return Ok(None);
    };
    if at_ms < deadline {
        return Ok(None);
    }
    release::release(state, CloseMode::RollbackAndClose, ReleaseReason::Evict).await?;
    Ok(Some(state.view.clone()))
}

/// 审计事实。字段与 [`RegistryAuditEntry`] 一一对应，只是省掉了 actor 必填的那几项。
struct AuditFacts {
    kind: AuditKind,
    outcome: Outcome,
    error_code: Option<ExecutionErrorCode>,
    disposition: Option<CancelDisposition>,
    execution_state: Option<ExecutionState>,
    effect_outcome: Option<EffectOutcome>,
    handle_count: usize,
}

impl AuditFacts {
    fn none() -> Self {
        Self {
            kind: AuditKind::SessionRegistered,
            outcome: Outcome::Succeeded,
            error_code: None,
            disposition: None,
            execution_state: None,
            effect_outcome: None,
            handle_count: 0,
        }
    }
}

fn emit(state: &ActorState, facts: AuditFacts) {
    // 审计条目存**线上字面量**（CM-72）：这里做的是纯字面量投影，
    // 没有任何一处自己拼字符串。拼出来的错字不会被类型系统挡住。
    let entry = RegistryAuditEntry {
        kind: facts.kind,
        db_session_id: state.db_session_id.clone(),
        runtime_epoch: state.runtime_epoch.get(),
        outcome: facts.outcome,
        error_code: facts.error_code.map(ExecutionErrorCode::as_str),
        disposition: facts.disposition.map(CancelDisposition::as_str),
        execution_state: facts
            .execution_state
            .map(crate::registry::audit::state_literal),
        effect_outcome: facts.effect_outcome.map(EffectOutcome::as_str),
        capability_versions: state
            .physical
            .as_ref()
            .map(|physical| physical.capabilities.clone()),
        handle_count: facts.handle_count,
    };
    let _ = state.audit.send(entry);
}
