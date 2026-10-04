//! actor 层的夹具与共用工厂。每个用例自带**独立**夹具状态，没有全局可变量。
//!
//! 确定性纪律：
//!
//! - 时钟走 `#[tokio::test(start_paused = true)]`，且**没有一处**靠时间推进制造并发窗口——
//!   窗口由闸门 channel 显式开合。需要「某条命令还没被回答」时用
//!   `oneshot::Receiver::try_recv()` 拿 `Empty`，**不用** `tokio::time::timeout`：
//!   暂停时钟在空闲时会自己把时间推过去，那样的断言会自己把自己等超时。
//! - `FakeBackend` 在 `.await` 上持有 `tokio::sync::Mutex`。生产路径禁止持锁跨 `.await`
//!   是**actor 自己**的死锁防线；夹具后端只有自己一个调用方，且除闸门外不持锁跨 await。

// 测试夹具专用豁免：分发给各用例文件的字段/常量并不都被每个用例用到。
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use super::*;
use crate::connection::types::{ClientInstanceId, EditorSessionId};
use crate::connection::{
    CommandCall, ExecutionTarget, HandleId, HandleKind, NamespaceTarget, ObjectTarget,
    OrganizationId, PrincipalId, ResourceId, StreamId,
};
use crate::registry::backend::ExecuteOnResource;
use crate::registry::backend::{
    CancelOnResource, CloseResource, CloseResourceOutcome, FinalizeHandles, HandleFinalization,
    OpenedResource, ResourceCancel,
};

pub(crate) mod cancel;
pub(crate) mod flow;
pub(crate) mod release;

/// 测试用的 runtime epoch。固定值：世代号本身不是被测对象。
pub(crate) const EPOCH: u64 = 7;
/// 宿主铸造的执行 id 前缀。
pub(crate) const EXEC_PREFIX: &str = "exec_7_";

/// [`open_request`] 里那个空闲期限。用例引用它而不是写死数字，
/// 否则「未到期限 / 已到期限」两格会随夹具改动一起悄悄失真。
pub(crate) const IDLE_DEADLINE_MS: u64 = 60_000;

/// 一个执行在 `FakeBackend` 里的剧本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Script {
    /// 立刻成功，**不**公布 cancelHandle。
    Immediate,
    /// 放行第一道闸门后公布 `cancelHandle`，放行第二道闸门后才成功。
    TwoStage,
    /// 永远不公布、也永远不结束。
    Silent,
}

impl Script {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::TwoStage => "twoStage",
            Self::Silent => "silent",
        }
    }
}

/// 闸门发送端。用例靠 `send(())` 精确控制后端的 `.await` 边界。
pub(crate) type Gate = mpsc::UnboundedSender<()>;

/// 用例收集到的后端调用痕迹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Traces {
    pub(crate) open: usize,
    pub(crate) execute: usize,
    pub(crate) cancel: usize,
    pub(crate) finalize: usize,
    pub(crate) close: usize,
    pub(crate) peak_live: usize,
    pub(crate) canceled: Vec<CancelOnResource>,
    pub(crate) finalized: Vec<FinalizeHandles>,
    pub(crate) closed_with: Vec<CloseResource>,
}

struct FakeInner {
    open_fails: bool,
    script: Script,
    execute_state: ExecutionState,
    execute_effect_outcome: EffectOutcome,
    execute_handles: Vec<SessionHandleRef>,
    driver_supports_cancel: bool,
    cancel_fails: bool,
    cancel_handle: String,
    finalize: HandleFinalization,
    close: CloseResourceOutcome,
}

/// §7.1 / §6.5 / §7.6 / §9.4 四个接缝的可控替身。
pub(crate) struct FakeBackend {
    inner: Mutex<FakeInner>,
    gate: Mutex<mpsc::UnboundedReceiver<()>>,
    gate_tx: Gate,
    open_calls: AtomicUsize,
    execute_calls: AtomicUsize,
    cancel_calls: AtomicUsize,
    finalize_calls: AtomicUsize,
    close_calls: AtomicUsize,
    live: AtomicUsize,
    peak_live: AtomicUsize,
    canceled: Mutex<Vec<CancelOnResource>>,
    finalized: Mutex<Vec<FinalizeHandles>>,
    closed_with: Mutex<Vec<CloseResource>>,
}

impl FakeBackend {
    pub(crate) async fn new(outcome: FakeOutcome) -> Arc<Self> {
        let (gate_tx, gate_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            inner: Mutex::new(FakeInner {
                open_fails: outcome.open_fails,
                script: outcome.script,
                execute_state: outcome.execute_state,
                execute_effect_outcome: outcome.execute_effect_outcome,
                execute_handles: outcome.execute_handles,
                driver_supports_cancel: outcome.driver_supports_cancel,
                cancel_fails: outcome.cancel_fails,
                cancel_handle: outcome.cancel_handle,
                finalize: outcome.finalize,
                close: outcome.close,
            }),
            gate: Mutex::new(gate_rx),
            gate_tx: gate_tx.clone(),
            open_calls: AtomicUsize::new(0),
            execute_calls: AtomicUsize::new(0),
            cancel_calls: AtomicUsize::new(0),
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
    pub(crate) fn gate(&self) -> Gate {
        self.gate_tx.clone()
    }

    /// 默认剧本：打开成功、driver 支持取消、执行立刻成功且不公布 handle。
    pub(crate) async fn defaults() -> Arc<Self> {
        Self::new(FakeOutcome::default()).await
    }

    pub(crate) fn count(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }

    pub(crate) fn open_calls(&self) -> usize {
        Self::count(&self.open_calls)
    }

    pub(crate) fn execute_calls(&self) -> usize {
        Self::count(&self.execute_calls)
    }

    pub(crate) fn cancel_calls(&self) -> usize {
        Self::count(&self.cancel_calls)
    }

    pub(crate) fn finalize_calls(&self) -> usize {
        Self::count(&self.finalize_calls)
    }

    pub(crate) fn close_calls(&self) -> usize {
        Self::count(&self.close_calls)
    }

    /// 同一时刻并发的执行数峰值：跨会话并行（CM-20）的直接证据。
    pub(crate) fn peak_live(&self) -> usize {
        Self::count(&self.peak_live)
    }

    pub(crate) async fn traces(&self) -> Traces {
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

/// 后端可配项。
pub(crate) struct FakeOutcome {
    open_fails: bool,
    script: Script,
    execute_state: ExecutionState,
    execute_effect_outcome: EffectOutcome,
    execute_handles: Vec<SessionHandleRef>,
    driver_supports_cancel: bool,
    cancel_fails: bool,
    cancel_handle: String,
    finalize: HandleFinalization,
    close: CloseResourceOutcome,
}

impl Default for FakeOutcome {
    fn default() -> Self {
        Self {
            open_fails: false,
            script: Script::Immediate,
            execute_state: ExecutionState::Succeeded,
            execute_effect_outcome: EffectOutcome::Completed,
            execute_handles: Vec::new(),
            driver_supports_cancel: true,
            cancel_fails: false,
            cancel_handle: "ch_exec_7_1".to_owned(),
            finalize: HandleFinalization {
                finalized: 0,
                remaining: 0,
                effect_outcome: EffectOutcome::RolledBack,
            },
            close: CloseResourceOutcome::Closed,
        }
    }
}

impl FakeOutcome {
    pub(crate) fn with_script(mut self, script: Script) -> Self {
        self.script = script;
        self
    }

    pub(crate) fn open_fails(mut self) -> Self {
        self.open_fails = true;
        self
    }

    pub(crate) fn driver_without_cancel(mut self) -> Self {
        self.driver_supports_cancel = false;
        self
    }

    pub(crate) fn cancel_fails(mut self) -> Self {
        self.cancel_fails = true;
        self
    }

    pub(crate) fn cancel_handle(mut self, handle: &str) -> Self {
        self.cancel_handle = handle.to_owned();
        self
    }

    pub(crate) fn finalizes(mut self, finalized: usize, remaining: usize) -> Self {
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

    pub(crate) fn finalization_unknown(mut self) -> Self {
        self.finalize = HandleFinalization {
            finalized: 0,
            remaining: 0,
            effect_outcome: EffectOutcome::Unknown,
        };
        self
    }

    pub(crate) fn close_undecidable(mut self) -> Self {
        self.close = CloseResourceOutcome::Undecidable {
            reason: "closeTimeout",
        };
        self
    }

    pub(crate) fn handles(mut self, handles: Vec<SessionHandleRef>) -> Self {
        self.execute_handles = handles;
        self
    }

    pub(crate) fn behavior(mut self, state: ExecutionState, outcome: EffectOutcome) -> Self {
        self.execute_state = state;
        self.execute_effect_outcome = outcome;
        self
    }
}

#[async_trait]
impl SessionBackend for FakeBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, ProviderError> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let inner = self.inner.lock().await;
        if inner.open_fails {
            return Err(ProviderError::SessionNotFound("openRefused".to_owned()));
        }
        Ok(OpenedResource {
            context: SessionContext::new(request.initial_target, "tester"),
            capabilities: CapabilityVersions {
                contract: "1.0.0".to_owned(),
                driver_api: "2.3.1".to_owned(),
            },
            resource_id: format!("res_{}", request.runtime_epoch),
            driver_supports_cancel: inner.driver_supports_cancel,
        })
    }

    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, ProviderError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        let live = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak_live.fetch_max(live, Ordering::SeqCst);

        let inner = self.inner.lock().await;
        let script = inner.script;
        let cancel_handle = inner.cancel_handle.clone();
        drop(inner);

        if script != Script::Immediate {
            // 第一道闸门：把执行**停在**真正干活之前。
            self.gate.lock().await.recv().await;
            if script == Script::TwoStage {
                let published = request.cancel_handle_sink.publish(cancel_handle.clone());
                if !published {
                    self.live.fetch_sub(1, Ordering::SeqCst);
                    return Err(ProviderError::ResourceLost("actorGone".to_owned()));
                }
            }
            // 第二道闸门：停在「句柄已绑定、执行未结束」。
            self.gate.lock().await.recv().await;
        }

        let inner = self.inner.lock().await;
        let stream_id = StreamId::new(format!("st_{}", request.execution_id.as_str()));
        let execution = ResourceExecution {
            execution_id: request.execution_id,
            stream_id,
            state: inner.execute_state,
            effect_outcome: inner.execute_effect_outcome,
            context_after: SessionContext::new(namespace(), "tester"),
            context_revision: 1,
            handles: inner.execute_handles.clone(),
            cancel_handle,
        };
        drop(inner);
        self.live.fetch_sub(1, Ordering::SeqCst);
        Ok(execution)
    }

    async fn cancel(&self, request: CancelOnResource) -> Result<ResourceCancel, ProviderError> {
        self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        self.canceled.lock().await.push(request);
        if self.inner.lock().await.cancel_fails {
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
        Ok(self.inner.lock().await.finalize.clone())
    }

    async fn close(&self, request: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
        self.close_calls.fetch_add(1, Ordering::SeqCst);
        self.closed_with.lock().await.push(request);
        Ok(self.inner.lock().await.close)
    }
}

// ---------------------------------------------------------------------------
// 夹具工厂
// ---------------------------------------------------------------------------

pub(crate) fn namespace() -> NamespaceTarget {
    NamespaceTarget {
        database: "app".to_owned(),
        catalog: "main".to_owned(),
        schema: "public".to_owned(),
        path: "ns/app".to_owned(),
    }
}

pub(crate) fn db_session_id() -> DbSessionId {
    DbSessionId::new("dbs_1")
}

pub(crate) fn other_db_session_id() -> DbSessionId {
    DbSessionId::new("dbs_2")
}

pub(crate) fn worker_id() -> WorkerId {
    WorkerId::new("w_1")
}

pub(crate) fn open_request(db: DbSessionId) -> OpenRequest {
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

/// 打开并已进入 `Ready` 的 actor、它的 view、以及审计出口。
pub(crate) async fn spawn_ready(
    backend: Arc<FakeBackend>,
) -> (
    SessionActor,
    SessionView,
    mpsc::UnboundedReceiver<RegistryAuditEntry>,
) {
    spawn_ready_for(backend, db_session_id()).await
}

pub(crate) async fn spawn_ready_for(
    backend: Arc<FakeBackend>,
    db: DbSessionId,
) -> (
    SessionActor,
    SessionView,
    mpsc::UnboundedReceiver<RegistryAuditEntry>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let actor = spawn_actor(
        open_request(db.clone()),
        RuntimeEpoch::new(EPOCH),
        backend,
        tx,
    );
    let view = actor
        .exec(|reply| ExecCommand::Open {
            request: open_request(db),
            reply,
        })
        .await
        .expect("open 必须成功");
    (actor, view, rx)
}

pub(crate) fn handle_of(view: &SessionView) -> SessionHandle {
    view.handle.clone()
}

pub(crate) fn execute_request(handle: &SessionHandle, expected: u64) -> ExecuteInSessionRequest {
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

/// 造一条宿主铸造的执行 id（格式 `exec_{epoch}_{seq}`）。
pub(crate) fn execution_id(seq: u64) -> ExecutionId {
    ExecutionId::new(format!("exec_{}_{}", EPOCH, seq))
}

/// 造一条句柄引用。
pub(crate) fn handle_ref(handle_id: &str, resource_id: &str) -> SessionHandleRef {
    SessionHandleRef::new(
        HandleId::new(handle_id),
        HandleKind::Transaction,
        ResourceId::new(resource_id),
        Counter(EPOCH),
    )
}

/// 旧世代的句柄（`runtime_epoch` 对不上）。
pub(crate) fn stale_handle(view: &SessionView) -> SessionHandle {
    let mut handle = view.handle.clone();
    handle.runtime_epoch = Counter(EPOCH - 1);
    handle
}

/// 收下当前已到达的审计条目。
pub(crate) fn drain_audit(
    rx: &mut mpsc::UnboundedReceiver<RegistryAuditEntry>,
) -> Vec<RegistryAuditEntry> {
    let mut entries = Vec::new();
    while let Ok(entry) = rx.try_recv() {
        entries.push(entry);
    }
    entries
}

/// 只看某会话的审计条目。
pub(crate) fn entries_of(
    rx: &mut mpsc::UnboundedReceiver<RegistryAuditEntry>,
    db: &DbSessionId,
) -> Vec<RegistryAuditEntry> {
    drain_audit(rx)
        .into_iter()
        .filter(|entry| &entry.db_session_id == db)
        .collect()
}

/// 取某类条目；多于一条即视为断言失败。
pub(crate) fn single_of_kind<'a>(
    entries: &'a [RegistryAuditEntry],
    kind: AuditKind,
) -> Option<&'a RegistryAuditEntry> {
    let mut found = entries.iter().filter(|entry| entry.kind == kind);
    let first = found.next()?;
    assert!(found.next().is_none(), "kind {kind:?} 出现了多于一条");
    Some(first)
}

/// 关闭命令。收在闭包里好让用例直接把它塞进 `exec_tx`。
pub(crate) fn close_command(
    handle: SessionHandle,
    mode: CloseMode,
    reply: Reply<SessionState>,
) -> ExecCommand {
    ExecCommand::Close {
        handle,
        mode,
        reply,
    }
}

/// 驱逐命令（§6.4 空闲）。
pub(crate) fn evict_command(at_ms: u64, reply: Reply<Option<SessionView>>) -> ExecCommand {
    ExecCommand::Evict { at_ms, reply }
}

/// 取消命令。`cancel_handle` 为 `None` 表示走 trait 那条「用登记绑定」的路径。
pub(crate) fn cancel_command(
    handle: SessionHandle,
    execution_id: ExecutionId,
    cancel_handle: Option<String>,
    reply: Reply<CancelReceipt>,
) -> ControlCommand {
    ControlCommand::Cancel {
        handle,
        execution_id,
        cancel_handle,
        reply,
    }
}

/// 让出一次调度。tokio 测试是单线程 current-thread 运行时，
/// 「让出」就是把控制权交回 actor 任务的那个动作。
pub(crate) async fn settle() {
    tokio::task::yield_now().await;
}

/// 有界等待条件成立。没有真实时间可推进，所以只能靠让出次数给上界。
/// 上界不是「兜底 sleep」，而是失败时的诊断信息：条件不成立就立刻红。
pub(crate) async fn wait_until(mut satisfied: impl FnMut() -> bool) {
    for _ in 0..512 {
        if satisfied() {
            return;
        }
        settle().await;
    }
    panic!("等待条件在 512 次让出内未成立");
}
