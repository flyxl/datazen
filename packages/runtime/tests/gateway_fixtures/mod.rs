//! 网关集成测试共用的夹具。
//!
//! 只用**公开 API** 造替身（`datazen_runtime::gateway::*` + 冻结 DTO +
//! `datazen_runtime::registry::SessionPort`）：这是集成测试与 `--lib` 单测的
//! 唯一区别，也是它存在的意义——单元测试能碰到 `pub(crate)`，集成测试碰不到，
//! 所以集成测试能独立证明「对外真的能用」。
//!
//! 刻意**不**复用 `src/gateway/testing_support.rs`：两份夹具共享代码会让
//! 「集成测试验证了真实公开 API」这件事失真。
//!
//! 这里造出来的 `SessionView` 是**合成**数据，不含任何凭据或真实连接信息。
//! 所有 ID 都带 `-contract` 后缀，便于在失败信息里一眼认出是夹具。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_platform_api::id::{ClientInstanceId, EditorSessionId};
use datazen_runtime::connection::{
    AttachmentState, CloseMode, CommandCall, ConfigRevision, ConnectionId, Counter, DbSessionId,
    ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState, ExecutionTarget,
    NamespaceTarget, OrganizationId, OwnerRef, PrincipalId, RuntimeError, SessionContext,
    SessionHandle, SessionState, SessionView, StreamId, Timestamp,
};
use datazen_runtime::gateway::idempotency::IdempotencyStoreError;
use datazen_runtime::gateway::{
    AlwaysAllow, AuthorizationDenial, Authorizer, CancelBinding, CancelRequest, ExecutionEvent,
    ExecutionEventKind, ExecutionGateway, ExecutionRequest, ExecutionSource, FixedClock,
    GatewayAction, IdempotencyRecord, IdempotencyScope, IdempotencyStore, InMemoryIdempotencyStore,
    RequestPrincipal, SourceKind,
};
use datazen_runtime::registry::SessionPort;

/// `PreciseCancel` 在 `connection` 门面里是私有导入，回转发不出去，从定义处引。
use datazen_runtime::connection::capability::PreciseCancel;

/// 夹具里的会话 id。
pub const SESSION: &str = "db_session_contract";
/// 夹具里的 `contextRevision`。
pub const REVISION: u64 = 7;
/// 夹具里的幂等键。
pub const IDEMPOTENCY_KEY: &str = "idem-contract";

fn namespace() -> NamespaceTarget {
    NamespaceTarget::unknown_placeholder()
}

fn editor_owner() -> OwnerRef {
    OwnerRef::Editor {
        organization_id: OrganizationId::new("org-contract"),
        principal_id: PrincipalId::new("principal-contract"),
        connection_id: ConnectionId::new("cnx-contract"),
        client_instance_id: ClientInstanceId::new("cli-contract"),
        editor_session_id: EditorSessionId::new("edt-contract"),
    }
}

/// 句柄。
pub fn handle(db_session_id: &str, runtime_epoch: u64) -> SessionHandle {
    SessionHandle {
        db_session_id: DbSessionId::new(db_session_id),
        runtime_epoch: Counter::new(runtime_epoch),
    }
}

/// 任意状态的会话视图。
pub fn view(
    db_session_id: &str,
    runtime_epoch: u64,
    context_revision: u64,
    state: SessionState,
) -> SessionView {
    let connection_id = ConnectionId::new("cnx-contract");
    SessionView {
        handle: handle(db_session_id, runtime_epoch),
        connection_id: connection_id.clone(),
        config_revision: ConfigRevision::new(3),
        owner: editor_owner(),
        initial_target: ExecutionTarget {
            connection_id,
            namespace: namespace(),
            object: None,
        },
        observed_context: SessionContext::new(namespace(), "dz_identity_contract"),
        context_revision: Counter::new(context_revision),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: Timestamp::new("9999-01-01T00:00:00Z"),
    }
}

/// 处于 `Ready` 的会话视图：可以受理执行。
pub fn ready_view() -> SessionView {
    view(SESSION, 1, REVISION, SessionState::Ready)
}

/// 夹具会话的句柄。
pub fn session_handle() -> SessionHandle {
    handle(SESSION, 1)
}

/// 夹具会话 id。
pub fn db_session() -> DbSessionId {
    DbSessionId::new(SESSION)
}

/// 请求主体。
pub fn principal() -> RequestPrincipal {
    RequestPrincipal::new(
        PrincipalId::new("principal-contract"),
        OrganizationId::new("org-contract"),
        db_session(),
    )
}

/// 编辑器来源。
pub fn source() -> ExecutionSource {
    ExecutionSource::new(
        SourceKind::Editor,
        "edt-contract",
        Some(OrganizationId::new("org-contract")),
        Some(PrincipalId::new("principal-contract")),
    )
}

/// 后台作业来源：不可持久化，用于验证来源校验。
pub fn background_source() -> ExecutionSource {
    ExecutionSource::new(SourceKind::Job, "job-contract", None, None)
}

/// 统一命令调用。
pub fn call() -> CommandCall {
    CommandCall {
        command: "query".to_owned(),
        input: serde_json::json!({ "sql": "select 1" }),
    }
}

/// 受理请求。
pub fn request(expected_revision: u64) -> ExecutionRequest {
    ExecutionRequest::new(
        session_handle(),
        Counter::new(expected_revision),
        call(),
        IDEMPOTENCY_KEY,
        source(),
    )
}

/// 端口回给网关的受理回执。
pub fn queued_receipt(value: &str) -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: ExecutionId::new(value),
        stream_id: StreamId::new(format!("strm_{value}")),
        state: ExecutionState::Queued,
    }
}

/// 造一条事件：归属显式带 `executionId`，其余按夹具常量填。
pub fn event(
    execution_id: &ExecutionId,
    sequence: u64,
    kind: ExecutionEventKind,
) -> ExecutionEvent {
    ExecutionEvent {
        execution_id: execution_id.clone(),
        db_session_id: db_session(),
        runtime_epoch: Counter::new(1),
        sequence: Counter::new(sequence),
        context_revision: Counter::new(REVISION),
        kind,
        declared_source: None,
    }
}

// ─────────────────────────────── 端口替身 ───────────────────────────────

/// 端口替身：四个方法各自可编排返回值，并记录调用次数与实际入参。
///
/// 锁只用来读写计数与返回值，**绝不跨越 `.await` 持有**；取不到锁时返回
/// `InvariantBroken` 而不是 panic——生产路径的失败形态就是错误。
pub struct RecordingPort {
    inner: Mutex<RecordingInner>,
}

struct RecordingInner {
    session_view: Result<SessionView, RuntimeError>,
    execute: Result<ExecutionReceipt, RuntimeError>,
    cancel: Result<ExecutionState, RuntimeError>,
    session_view_calls: u64,
    execute_calls: u64,
    cancel_calls: u64,
    executed_requests: Vec<ExecuteInSessionRequest>,
    /// 驱动往返占用的时钟刻度：在 `execute_in_session` 内部推进，
    /// 让「网关开销」与「驱动往返」在时间轴上真正分开（CM-60）。
    driver_nanos: u64,
    clock: Arc<FixedClock>,
}

impl RecordingPort {
    pub fn new(session_view: SessionView) -> Self {
        Self {
            inner: Mutex::new(RecordingInner {
                session_view: Ok(session_view),
                execute: Ok(queued_receipt("exe_from_port")),
                cancel: Ok(ExecutionState::CancelRequested),
                session_view_calls: 0,
                execute_calls: 0,
                cancel_calls: 0,
                executed_requests: Vec::new(),
                driver_nanos: 0,
                clock: FixedClock::shared(),
            }),
        }
    }

    /// 编排驱动往返耗时（刻度）。
    pub fn with_driver_nanos(self, nanos: u64) -> Self {
        self.write(|inner| inner.driver_nanos = nanos);
        self
    }

    /// 换掉会话投影：模拟受理与下发之间上下文被改动，或端口直接报错。
    pub fn set_view(&self, view: Result<SessionView, RuntimeError>) {
        self.write(|inner| inner.session_view = view);
    }

    /// 换掉取消结果：取消尚未发起时也能编排。
    pub fn set_cancel(&self, state: Result<ExecutionState, RuntimeError>) {
        self.write(|inner| inner.cancel = state);
    }

    pub fn session_view_calls(&self) -> u64 {
        self.read(|inner| inner.session_view_calls)
    }

    pub fn execute_calls(&self) -> u64 {
        self.read(|inner| inner.execute_calls)
    }

    pub fn cancel_calls(&self) -> u64 {
        self.read(|inner| inner.cancel_calls)
    }

    /// 实际下发给驱动的请求（受理时冻结的那一份）。
    pub fn executed_requests(&self) -> Vec<ExecuteInSessionRequest> {
        self.read(|inner| inner.executed_requests.clone())
    }

    /// 把端口的时钟刻度接到夹具的注入时钟上（CM-60 的时间轴必须在同一把尺子上）。
    pub fn attach_clock(&self, clock: Arc<FixedClock>) {
        self.write(|inner| inner.clock = clock);
    }

    fn write<F: FnOnce(&mut RecordingInner)>(&self, apply: F) {
        match self.inner.lock() {
            Ok(mut inner) => apply(&mut inner),
            Err(_) => panic!("夹具锁被毒化"),
        }
    }

    fn read<T, F: FnOnce(&RecordingInner) -> T>(&self, read: F) -> T {
        match self.inner.lock() {
            Ok(inner) => read(&inner),
            Err(_) => panic!("夹具锁被毒化"),
        }
    }
}

#[async_trait]
impl SessionPort for RecordingPort {
    async fn session_view(&self, _handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
        match self.inner.lock() {
            Ok(mut inner) => {
                inner.session_view_calls = inner.session_view_calls.saturating_add(1);
                inner.session_view.clone()
            }
            Err(_) => Err(RuntimeError::InvariantBroken("recordingPortPoisoned")),
        }
    }

    async fn execute_in_session(
        &self,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        match self.inner.lock() {
            Ok(mut inner) => {
                inner.execute_calls = inner.execute_calls.saturating_add(1);
                inner.executed_requests.push(request);
                // 驱动往返发生在「下发已发出」与「驱动已完成」两枚打点之间。
                inner.clock.advance(inner.driver_nanos);
                inner.execute.clone()
            }
            Err(_) => Err(RuntimeError::InvariantBroken("recordingPortPoisoned")),
        }
    }

    async fn cancel_execution(
        &self,
        _handle: &SessionHandle,
        _execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError> {
        match self.inner.lock() {
            Ok(mut inner) => {
                inner.cancel_calls = inner.cancel_calls.saturating_add(1);
                inner.cancel.clone()
            }
            Err(_) => Err(RuntimeError::InvariantBroken("recordingPortPoisoned")),
        }
    }

    async fn close_session(
        &self,
        _handle: &SessionHandle,
        _mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        match self.inner.lock() {
            Ok(mut inner) => match inner.session_view.clone() {
                Ok(mut view) => {
                    view.state = SessionState::Closed;
                    inner.session_view = Ok(view);
                    Ok(())
                }
                Err(err) => Err(err),
            },
            Err(_) => Err(RuntimeError::InvariantBroken("recordingPortPoisoned")),
        }
    }
}

// ─────────────────────────────── 幂等存储替身 ───────────────────────────────

/// 读不出来的幂等存储：§3.3 要求此时**要求核验**，而不是当作没记录重新受理。
pub struct UnreadableStore {
    pub write_calls: AtomicU64,
}

impl UnreadableStore {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self {
            write_calls: AtomicU64::new(0),
        })
    }

    pub fn write_calls(&self) -> u64 {
        self.write_calls.load(Ordering::SeqCst)
    }
}

impl IdempotencyStore for UnreadableStore {
    fn read(
        &self,
        _scope: &IdempotencyScope,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
        Err(IdempotencyStoreError::read("存储不可读"))
    }

    fn write(
        &self,
        _scope: &IdempotencyScope,
        _record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError> {
        self.write_calls.fetch_add(1, Ordering::SeqCst);
        Err(IdempotencyStoreError::write("存储不可写"))
    }
}

// ─────────────────────────────── 授权器替身 ───────────────────────────────

/// 受理时放行、下发时拒绝：证明授权在下发驱动前**再查一次**（CM-62）。
pub struct FlippingAuthorizer {
    pub allow_execute: AtomicU64,
}

impl FlippingAuthorizer {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self {
            allow_execute: AtomicU64::new(1),
        })
    }

    pub fn revoke_execute(&self) {
        self.allow_execute.store(0, Ordering::SeqCst);
    }
}

impl Authorizer for FlippingAuthorizer {
    fn authorize(
        &self,
        _principal: &RequestPrincipal,
        action: GatewayAction,
        _view: &SessionView,
        _source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        match action {
            GatewayAction::Execute if self.allow_execute.load(Ordering::SeqCst) == 0 => {
                Err(AuthorizationDenial::new(action, "permissionRevoked"))
            }
            GatewayAction::Execute | GatewayAction::Cancel => Ok(()),
        }
    }
}

// ─────────────────────────────── 夹具 ───────────────────────────────

/// 只拒取消：证明取消入口有自己的授权检查，而不是「受理授权过就一路放行」。
pub struct CancelDenyAuthorizer;

impl Authorizer for CancelDenyAuthorizer {
    fn authorize(
        &self,
        _principal: &RequestPrincipal,
        action: GatewayAction,
        _view: &SessionView,
        _source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        match action {
            GatewayAction::Execute => Ok(()),
            GatewayAction::Cancel => Err(AuthorizationDenial::new(action, "cancelNotPermitted")),
        }
    }
}

/// 读得到（无记录）但写不进去的幂等存储：验证「写失败不得留下半条记录」。
pub struct WriteFailingStore {
    pub write_calls: AtomicU64,
}

impl WriteFailingStore {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self {
            write_calls: AtomicU64::new(0),
        })
    }

    pub fn write_calls(&self) -> u64 {
        self.write_calls.load(Ordering::SeqCst)
    }
}

impl IdempotencyStore for WriteFailingStore {
    fn read(
        &self,
        _scope: &IdempotencyScope,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
        Ok(None)
    }

    fn write(
        &self,
        _scope: &IdempotencyScope,
        _record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError> {
        self.write_calls.fetch_add(1, Ordering::SeqCst);
        Err(IdempotencyStoreError::write("存储不可写"))
    }
}

pub struct Harness {
    pub gateway: ExecutionGateway,
    pub port: Arc<RecordingPort>,
    pub clock: Arc<FixedClock>,
}

/// 用自定义幂等存储搭夹具（存储留在外面，由测试自己断言写入次数）。
pub fn harness_with(
    port: Arc<RecordingPort>,
    authorizer: Arc<dyn Authorizer>,
    store: Arc<dyn IdempotencyStore>,
) -> Harness {
    let clock = FixedClock::shared();
    port.attach_clock(clock.clone());
    let gateway = ExecutionGateway::new(port.clone(), authorizer, store, clock.clone());
    Harness {
        gateway,
        port,
        clock,
    }
}

/// 就绪夹具：端口放行一切，授权恒过，时钟刻度固定。
pub fn ready_harness() -> Harness {
    let clock = FixedClock::shared();
    let port = Arc::new(RecordingPort::new(ready_view()));
    port.attach_clock(clock.clone());
    let gateway = ExecutionGateway::new(
        port.clone(),
        Arc::new(AlwaysAllow),
        InMemoryIdempotencyStore::shared(),
        clock.clone(),
    );
    Harness {
        gateway,
        port,
        clock,
    }
}

/// 就绪夹具 + 可断言写入次数的内存存储。
pub fn counted_harness() -> (Harness, Arc<InMemoryIdempotencyStore>) {
    let store = InMemoryIdempotencyStore::shared();
    let clock = FixedClock::shared();
    let port = Arc::new(RecordingPort::new(ready_view()));
    port.attach_clock(clock.clone());
    let gateway = ExecutionGateway::new(
        port.clone(),
        Arc::new(AlwaysAllow),
        store.clone(),
        clock.clone(),
    );
    (
        Harness {
            gateway,
            port,
            clock,
        },
        store,
    )
}

/// 受理一次并返回 `executionId`；失败就带原因炸出来（夹具的期望总是成功）。
pub async fn accept(h: &Harness, req: ExecutionRequest) -> ExecutionId {
    match h.gateway.accept(&principal(), req).await {
        Ok(acceptance) => acceptance.execution_id().clone(),
        Err(err) => panic!("夹具期望受理成功，实际被拒：{err:?}"),
    }
}

/// 按当前会话句柄发起一次取消。
pub fn cancel_request(execution_id: &ExecutionId, precise_cancel: PreciseCancel) -> CancelRequest {
    CancelRequest::new(
        CancelBinding::new(execution_id.clone(), session_handle()),
        precise_cancel,
    )
}

/// 事件的便捷构造：`Terminal` 终态。
pub fn terminal(state: ExecutionState) -> ExecutionEventKind {
    ExecutionEventKind::Terminal { state }
}
