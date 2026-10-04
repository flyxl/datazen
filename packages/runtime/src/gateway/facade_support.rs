//! 门面测试共用的替身与夹具。
//!
//! 单独成文件：`facade_tests.rs` 与 `cancel_event_tests.rs` 都需要它们，
//! 复制两份会让「夹具本身是不是真的」的判断失去依据。
//!
//! 这里造出来的 `SessionView` 是**合成**数据，不含任何凭据或真实连接信息。

pub(crate) use std::sync::atomic::{AtomicU64, Ordering};
pub(crate) use std::sync::{Arc, Mutex};

use async_trait::async_trait;

pub(crate) use super::testing_support::{execution_id, handle, ready_view, view};
pub(crate) use super::{
    cancel_binding_reason, cancel_failed, is_requested, AlwaysAllow, AlwaysDeny,
    AuthorizationDenial, Authorizer, CancelBinding, CancelDisposition, CancelRequest,
    EventDisposition, ExecutionEvent, ExecutionEventKind, ExecutionGateway, ExecutionRequest,
    ExecutionSource, FixedClock, GatewayAction, GatewayError, IdempotencyRecord, IdempotencyScope,
    IdempotencyStore, InMemoryIdempotencyStore, RequestPrincipal, SourceKind,
    UNKNOWN_EXECUTION_REASON,
};
// `PreciseCancel` 在 `connection` 门面里是私有导入，直接回转发不出去，这里从定义处引。
pub(crate) use crate::connection::capability::PreciseCancel;
pub(crate) use crate::connection::{
    CloseMode, CommandCall, Counter, DbSessionId, ExecuteInSessionRequest, ExecutionId,
    ExecutionReceipt, ExecutionState, OrganizationId, PrincipalId, ResourceId, RuntimeError,
    SessionHandle, SessionState, SessionView, StreamId,
};
pub(crate) use crate::gateway::idempotency::IdempotencyStoreError;
pub(crate) use crate::registry::SessionPort;

// ─────────────────────────────── 端口替身 ───────────────────────────────

/// 端口替身：三个方法各自可编排返回值，并记录调用次数与实际入参。
///
/// 锁只用来读写计数与返回值，**绝不跨越 `.await` 持有**（§8 的并发纪律）。
/// 取不到锁时返回 `InvariantBroken` 而不是 panic：生产路径的失败形态就是错误。
pub(crate) struct FakePort {
    inner: Mutex<FakePortInner>,
}

pub(crate) struct FakePortInner {
    session_view: Result<SessionView, RuntimeError>,
    execute: Result<ExecutionReceipt, RuntimeError>,
    cancel: Result<ExecutionState, RuntimeError>,
    session_view_calls: u64,
    execute_calls: u64,
    cancel_calls: u64,
    executed_requests: Vec<ExecuteInSessionRequest>,
    /// 驱动往返占用的时钟刻度：在 `execute_in_session` 内部推进，
    /// 让「网关开销」和「驱动往返」在时间轴上真正分开（CM-60）。
    driver_nanos: u64,
    clock: Arc<FixedClock>,
}

impl FakePort {
    pub(crate) fn new(session_view: SessionView) -> Self {
        Self {
            inner: Mutex::new(FakePortInner {
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

    pub(crate) fn with_session_view_error(mut self, error: RuntimeError) -> Self {
        if let Ok(inner) = self.inner.get_mut() {
            inner.session_view = Err(error);
        }
        self
    }

    pub(crate) fn with_execute_error(mut self, error: RuntimeError) -> Self {
        if let Ok(inner) = self.inner.get_mut() {
            inner.execute = Err(error);
        }
        self
    }

    pub(crate) fn with_cancel(mut self, result: Result<ExecutionState, RuntimeError>) -> Self {
        if let Ok(inner) = self.inner.get_mut() {
            inner.cancel = result;
        }
        self
    }

    /// 让下一次 `execute_in_session` 在端口内部消耗 `nanos` 个时钟刻度。
    pub(crate) fn with_driver_nanos(mut self, nanos: u64) -> Self {
        if let Ok(inner) = self.inner.get_mut() {
            inner.driver_nanos = nanos;
        }
        self
    }

    /// 换用夹具的时钟：否则替身推进的是自己的钟，测出来的时间差全是假的。
    pub(crate) fn attach_clock(&self, clock: Arc<FixedClock>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.clock = clock;
        }
    }

    /// 把会话视图推到新状态：模拟「受理之后上下文/状态变了」。
    pub(crate) fn set_session_view(&self, session_view: SessionView) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.session_view = Ok(session_view);
        }
    }

    pub(crate) fn execute_calls(&self) -> u64 {
        self.read(|inner| inner.execute_calls)
    }

    pub(crate) fn cancel_calls(&self) -> u64 {
        self.read(|inner| inner.cancel_calls)
    }

    pub(crate) fn session_view_calls(&self) -> u64 {
        self.read(|inner| inner.session_view_calls)
    }

    pub(crate) fn executed_requests(&self) -> Vec<ExecuteInSessionRequest> {
        self.read(|inner| inner.executed_requests.clone())
    }

    fn read<T>(&self, pick: impl FnOnce(&FakePortInner) -> T) -> T {
        match self.inner.lock() {
            Ok(inner) => pick(&inner),
            Err(_) => panic!("FakePort 的锁被毒化：只有本模块的测试能走到这里"),
        }
    }
}

#[async_trait]
impl SessionPort for FakePort {
    async fn session_view(&self, _handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
        match self.inner.lock() {
            Ok(mut inner) => {
                inner.session_view_calls = inner.session_view_calls.saturating_add(1);
                // 字段本身就是 `Result`，替身只负责计数，不改写编排好的成败。
                inner.session_view.clone()
            }
            Err(_) => Err(RuntimeError::InvariantBroken("fakePortPoisoned")),
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
            Err(_) => Err(RuntimeError::InvariantBroken("fakePortPoisoned")),
        }
    }

    async fn cancel_execution(
        &self,
        _handle: &SessionHandle,
        execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError> {
        let _ = execution_id;
        match self.inner.lock() {
            Ok(mut inner) => {
                inner.cancel_calls = inner.cancel_calls.saturating_add(1);
                inner.cancel.clone()
            }
            Err(_) => Err(RuntimeError::InvariantBroken("fakePortPoisoned")),
        }
    }

    async fn close_session(
        &self,
        _handle: &SessionHandle,
        _mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

pub(crate) fn queued_receipt(value: &str) -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: ExecutionId::new(value),
        stream_id: StreamId::new(format!("strm_{value}")),
        state: ExecutionState::Queued,
    }
}

// ─────────────────────────────── 授权器替身 ───────────────────────────────

/// 只拒取消：证明取消入口有自己的授权检查，而不是「受理授权过就一路放行」。
pub(crate) struct DenyCancel;

impl Authorizer for DenyCancel {
    fn authorize(
        &self,
        _principal: &RequestPrincipal,
        action: GatewayAction,
        _view: &SessionView,
        _source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        match action {
            GatewayAction::Cancel => Err(AuthorizationDenial::new(action, "cancelForbidden")),
            GatewayAction::Execute => Ok(()),
        }
    }
}

/// 受理时放行、下发时拒绝 → 模拟「受理之后权限被撤销」。
pub(crate) struct FlippingAuthorizer {
    pub(crate) allow_execute: AtomicU64,
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

// ─────────────────────────────── 幂等存储替身 ───────────────────────────────

/// 读必失败的存储：验证「不可读 ≠ 没记录」（CM-54）。
///
/// 自己数写次数。`write` 一旦被调用就直接 panic——这条路径下写库本身就是缺陷，
/// 而 panic 会让红得比「返回了错误值」更醒目。
pub(crate) struct UnreadableStore {
    pub(crate) write_calls: AtomicU64,
}

impl IdempotencyStore for UnreadableStore {
    fn read(
        &self,
        _scope: &IdempotencyScope,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
        Err(IdempotencyStoreError::read("storeUnavailable"))
    }

    fn write(
        &self,
        _scope: &IdempotencyScope,
        _record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError> {
        self.write_calls.fetch_add(1, Ordering::SeqCst);
        panic!("不可读时绝不写入幂等存储");
    }
}

// ─────────────────────────────── 夹具 ───────────────────────────────

pub(crate) const SESSION: &str = "dbse_facade";
pub(crate) const IDEMPOTENCY_KEY: &str = "idem-key-facade";
pub(crate) const REVISION: u64 = 7;

pub(crate) fn db_session() -> DbSessionId {
    DbSessionId::new(SESSION)
}

pub(crate) fn session_handle() -> SessionHandle {
    handle(SESSION, 1)
}

pub(crate) fn principal() -> RequestPrincipal {
    RequestPrincipal::new(
        PrincipalId::new("principal-facade"),
        OrganizationId::new("org-facade"),
        db_session(),
    )
}

pub(crate) fn source() -> ExecutionSource {
    ExecutionSource::new(
        SourceKind::Editor,
        "edt-facade",
        Some(OrganizationId::new("org-facade")),
        Some(PrincipalId::new("principal-facade")),
    )
}

pub(crate) fn call() -> CommandCall {
    CommandCall {
        command: "query".to_owned(),
        input: serde_json::json!({ "sql": "select 1" }),
    }
}

pub(crate) fn request(expected_revision: u64) -> ExecutionRequest {
    ExecutionRequest::new(
        session_handle(),
        Counter::new(expected_revision),
        call(),
        IDEMPOTENCY_KEY,
        source(),
    )
}

pub(crate) fn ready() -> SessionView {
    ready_view(db_session(), Counter::new(1), Counter::new(REVISION))
}

pub(crate) struct Harness {
    pub(crate) gateway: ExecutionGateway,
    pub(crate) port: Arc<FakePort>,
    pub(crate) store: Arc<InMemoryIdempotencyStore>,
    pub(crate) clock: Arc<FixedClock>,
}

pub(crate) fn harness_with(
    port: Arc<FakePort>,
    authorizer: Arc<dyn Authorizer>,
    store: Arc<InMemoryIdempotencyStore>,
) -> Harness {
    let clock = FixedClock::shared();
    port.attach_clock(clock.clone());
    let gateway = ExecutionGateway::new(port.clone(), authorizer, store.clone(), clock.clone());
    Harness {
        gateway,
        port,
        store,
        clock,
    }
}

pub(crate) fn harness(port: Arc<FakePort>) -> Harness {
    harness_with(
        port,
        Arc::new(AlwaysAllow),
        InMemoryIdempotencyStore::shared(),
    )
}

pub(crate) fn ready_harness() -> Harness {
    harness(Arc::new(FakePort::new(ready())))
}

/// 受理一次，返回 `executionId`；失败就带上原因炸出来（夹具里的期望总是成功）。
pub(crate) async fn accept(h: &Harness, req: ExecutionRequest) -> ExecutionId {
    match h.gateway.accept(&principal(), req).await {
        Ok(acceptance) => acceptance.execution_id().clone(),
        Err(_) => panic!("夹具期望受理成功，实际被拒"),
    }
}

pub(crate) fn event(
    execution_id: &ExecutionId,
    sequence: u64,
    kind: ExecutionEventKind,
) -> ExecutionEvent {
    ExecutionEvent {
        execution_id: execution_id.clone(),
        db_session_id: db_session(),
        runtime_epoch: Counter::new(1),
        sequence: Counter::new(sequence),
        context_revision: Counter::new(sequence),
        kind,
        declared_source: None,
    }
}

pub(crate) async fn record_of(h: &Harness, id: &ExecutionId) -> super::ExecutionRecord {
    match h.gateway.execution(id).await {
        Some(record) => record,
        None => panic!("执行记录应当还在"),
    }
}

pub(crate) async fn cancel_with(
    h: &Harness,
    id: &ExecutionId,
    precise: PreciseCancel,
) -> Result<super::CancelOutcome, GatewayError> {
    h.gateway
        .cancel(
            &principal(),
            CancelRequest::new(CancelBinding::new(id.clone(), session_handle()), precise),
        )
        .await
}
