//! 统一执行网关（`ExecutionGateway`）。
//!
//! 单进程执行闸门：把「受理一次执行」和「执行一次执行」分成两个动作，
//! 中间夹一道乐观并发闸门和一次权限重校验。
//!
//! # 为什么要拆成受理与下发两段
//!
//! §7.2 第 3 步要求「返回回执**早于** SQL 跑完」。所以 [`ExecutionGateway::accept`]
//! 返回的 [`GatewayAcceptance`] 是**受理回执**，不是结果：它的 `state` 恒为
//! [`ExecutionState::Queued`]。把受理回执当成执行结果，UI 就会在第一行数据回来之前
//! 显示「执行成功」。两者在类型上是同一个冻结 DTO（§4 不允许我另造一个），
//! 所以区别只能由 [`GatewayAcceptance::is_queued_not_finished`] 显式断言。
//!
//! # 受理顺序（§3.1）
//!
//! 1. 同步参数校验（命令非空、幂等键可用、来源可持久化）；
//! 2. 算幂等作用域与请求指纹；
//! 3. `session_view`（**无锁 await**）；
//! 4. 终态会话拒绝（`Closed`/`Lost`/`Closing`）——端口对终态会话**返回 `Ok`**，
//!    不在这里判就会把已关闭会话的执行判成「已受理」；
//! 5. 入口授权；
//! 6. 临界区内：幂等查重 →（`Hit` 直接回放，不再走乐观闸门）→ 乐观并发闸门 →
//!    分配 `executionId` → 登记幂等记录 → 入队。
//!
//! 查重**必须**排在乐观闸门前面：重发的语义是「上次那次」，而上次那次受理时的
//! `context_revision` 往往已经过期了。先校验 revision 会让重发永远失败，
//! 调用方就会换一个 key 重试——那正是 CM-54 要禁止的重复写入。
//!
//! # 下发顺序（§3.4 / CM-62）
//!
//! 1. 找执行记录，找不到即拒绝（绝不新建一次「碰巧的」执行）；
//! 2. 重读 `session_view` + 终态会话拒绝；
//! 3. **重校验权限**——受理时授权过一次，不代表下发时仍然有权；
//! 4. 重比 `expected_context_revision`（§3.1 要求「真正执行前再比一次」），
//!    不一致返回 [`RuntimeError::ContextRevisionMismatch`] 并携带服务端实际值；
//! 5. 记 CM-60 第一段起点 → 下发驱动 → 记第二段起点；
//! 6. 登记回执/事件状态 → 记第二段终点 → 落样本。
//!
//! 步骤 5 的两个记号点包住的是**驱动往返**，因此「假 SQL 执行」「预算/角色排队」
//! 「网络传输」天然落在区间外或区间内由驱动侧决定，网关不再额外计时（§3.5）。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::connection::capability::PreciseCancel;
use crate::connection::{
    Counter, ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState, ResourceId,
    RuntimeError, SessionHandle, SessionState, SessionView,
};
use crate::registry::SessionPort;

pub mod cancel;
pub mod events;
pub mod idempotency;
pub mod provenance;
pub mod request;
pub mod timing;

#[cfg(test)]
pub(crate) mod testing_support;

// 门面测试按 800 行上限拆成三块：共用替身 + 受理下发 + 取消事件。
#[cfg(test)]
mod cancel_event_tests;
#[cfg(test)]
pub(crate) mod facade_support;
#[cfg(test)]
mod facade_tests;

pub use cancel::binding as cancel_binding_reason;
pub use cancel::{
    cancel_failed, disposition_from_port_state, state_literal, unsupported_outcome, verify_binding,
    CancelBinding, CancelDisposition, CancelOutcome, CancelRequest, UNKNOWN_EXECUTION_REASON,
};
pub use events::{EventDisposition, EventStore, ExecutionEvent, ExecutionEventKind};
pub use idempotency::{
    IdempotencyLedger, IdempotencyLookup, IdempotencyRecord, IdempotencyScope, IdempotencyStore,
    InMemoryIdempotencyStore, RequestFingerprint, StoreFailureKind,
};
pub use provenance::{
    AlwaysAllow, AlwaysDeny, AuthorizationDenial, Authorizer, ExecutionSource, GatewayAction,
    RequestPrincipal, SourceKind,
};
pub use request::{AcceptanceDisposition, ExecutionRequest, GatewayAcceptance, GatewayError};
pub use timing::{FixedClock, MonotonicClock, OverheadProbe, OverheadSamples};

/// 网关内部记账的一条执行。
///
/// **每个执行自带事件水位线与计时探针**，不是全局一份：水位线绑定在
/// `(dbSessionId, runtimeEpoch)` 上，两个会话并发交错的事件不能共用一个水位线。
#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    execution_id: ExecutionId,
    handle: SessionHandle,
    expected_context_revision: Counter,
    resource_binding_id: Option<ResourceId>,
    source: ExecutionSource,
    idempotency_key: String,
    /// 受理时构造一次并冻结。下发只改句柄，不改 `call` / `idempotencyKey`：
    /// 受理与下发之间命令被改写，等于下发的东西和回执对不上。
    port_request: ExecuteInSessionRequest,
    probe: OverheadProbe,
    events: EventStore,
    dispatched: bool,
    last_state: ExecutionState,
}

impl ExecutionRecord {
    pub fn execution_id(&self) -> &ExecutionId {
        &self.execution_id
    }

    pub fn handle(&self) -> &SessionHandle {
        &self.handle
    }

    pub fn expected_context_revision(&self) -> Counter {
        self.expected_context_revision
    }

    pub fn resource_binding_id(&self) -> Option<&ResourceId> {
        self.resource_binding_id.as_ref()
    }

    /// 来源**只**来自受理请求，之后任何事件都不能改写它（CM-61）。
    pub fn source(&self) -> &ExecutionSource {
        &self.source
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    /// 发给端口的请求体快照。
    pub fn port_request(&self) -> &ExecuteInSessionRequest {
        &self.port_request
    }

    pub fn is_dispatched(&self) -> bool {
        self.dispatched
    }

    pub fn last_state(&self) -> ExecutionState {
        self.last_state
    }

    pub fn observed_context_revision(&self) -> Counter {
        self.events.context_revision()
    }

    pub fn observed_row_count(&self) -> u64 {
        self.events.row_count()
    }

    pub fn observed_sequence(&self) -> Option<Counter> {
        self.events.last_sequence()
    }

    /// 是否需要读快照或重新订阅（CM-55 序列空洞）。
    pub fn needs_recovery(&self) -> bool {
        self.events.needs_recovery()
    }

    pub fn resubscribe_from(&self) -> Option<Counter> {
        self.events.resubscribe_from()
    }

    /// 被事件丢弃的「自带来源」帧数量。恒应大于 0 表示有人试图改写来源（CM-61）。
    pub fn declared_source_events_ignored(&self) -> u64 {
        self.events.declared_source_events_ignored()
    }
}

#[derive(Debug, Default)]
struct GatewayState {
    records: HashMap<ExecutionId, ExecutionRecord>,
    samples: OverheadSamples,
    next_sequence: u64,
}

/// 统一执行网关。
///
/// 线程安全：内部 `tokio::sync::Mutex`，**任何时候都不跨 `await` 持锁**——
/// 跨 `await` 持锁会把端口 IO 的耗时算进所有其他请求的等待里。
pub struct ExecutionGateway {
    port: Arc<dyn SessionPort>,
    authorizer: Arc<dyn Authorizer>,
    ledger: IdempotencyLedger,
    clock: Arc<dyn MonotonicClock>,
    state: Mutex<GatewayState>,
}

impl ExecutionGateway {
    pub fn new(
        port: Arc<dyn SessionPort>,
        authorizer: Arc<dyn Authorizer>,
        store: Arc<dyn IdempotencyStore>,
        clock: Arc<dyn MonotonicClock>,
    ) -> Self {
        Self {
            port,
            authorizer,
            ledger: IdempotencyLedger::new(store),
            clock,
            state: Mutex::new(GatewayState::default()),
        }
    }

    /// 受理一次执行。返回的是**受理回执**（`Queued`），不是执行结果。
    pub async fn accept(
        &self,
        principal: &RequestPrincipal,
        request: ExecutionRequest,
    ) -> Result<GatewayAcceptance, GatewayError> {
        request.validate()?;

        let scope = request.scope();
        let fingerprint = request.fingerprint();
        let handle = request.handle.clone();

        let view = self.port.session_view(&handle).await?;
        reject_terminal_session(&view)?;
        self.authorizer
            .authorize(principal, GatewayAction::Execute, &view, &request.source)?;

        let mut state = self.state.lock().await;
        match self.ledger.lookup(&scope, &fingerprint) {
            IdempotencyLookup::Hit(existing) => Ok(GatewayAcceptance::replayed(
                &existing.execution_id,
                &request.source,
                existing.first_accepted_at_nanos,
            )),
            IdempotencyLookup::Conflict { existing, incoming } => {
                Err(GatewayError::IdempotencyConflict {
                    key: scope.key().to_owned(),
                    existing: existing.execution_id,
                    incoming: incoming.as_str().to_owned(),
                })
            }
            IdempotencyLookup::Unreadable { message } => {
                // 绝不降级成 Miss 后另写一条——那会真的把同一个语义请求再跑一遍。
                Err(GatewayError::IdempotencyVerificationRequired { message })
            }
            IdempotencyLookup::Miss => {
                request.check_context_revision(view.context_revision)?;

                let execution_id = ExecutionId::new(format!(
                    "exe_{}_{}",
                    handle.db_session_id.as_str(),
                    state.next_sequence
                ));
                let accepted_at = self.clock.now_nanos();
                self.ledger
                    .reserve(
                        &scope,
                        IdempotencyRecord {
                            execution_id: execution_id.clone(),
                            fingerprint,
                            first_accepted_at_nanos: accepted_at,
                        },
                    )
                    .map_err(|err| GatewayError::IdempotencyPersistFailed {
                        message: err.message,
                    })?;

                state.next_sequence = state.next_sequence.saturating_add(1);
                let mut events = EventStore::new();
                events.bind(handle.db_session_id.clone(), handle.runtime_epoch);
                let record = ExecutionRecord {
                    execution_id: execution_id.clone(),
                    handle,
                    expected_context_revision: request.expected_context_revision,
                    resource_binding_id: request.resource_binding_id.clone(),
                    source: request.source.clone(),
                    idempotency_key: scope.key().to_owned(),
                    port_request: request.to_port_request(),
                    probe: OverheadProbe::accepted(self.clock.as_ref()),
                    events,
                    dispatched: false,
                    last_state: ExecutionState::Queued,
                };
                state.records.insert(execution_id.clone(), record);
                Ok(GatewayAcceptance::accepted(
                    &execution_id,
                    &request.source,
                    accepted_at,
                ))
            }
        }
    }

    /// 下发给驱动。受理与下发之间的乐观并发闸门与权限在这里各再查一次。
    pub async fn dispatch(
        &self,
        principal: &RequestPrincipal,
        execution_id: &ExecutionId,
    ) -> Result<ExecutionReceipt, GatewayError> {
        let (handle, expected_revision, source, port_request) = {
            let mut state = self.state.lock().await;
            let record =
                state
                    .records
                    .get_mut(execution_id)
                    .ok_or(GatewayError::InvalidRequest {
                        reason: "unknownExecution",
                    })?;
            if record.dispatched {
                return Err(GatewayError::InvalidRequest {
                    reason: "alreadyDispatched",
                });
            }
            record.dispatched = true;
            (
                record.handle.clone(),
                record.expected_context_revision,
                record.source.clone(),
                record.port_request.clone(),
            )
        };

        let view = self.port.session_view(&handle).await?;
        // 闸门在**碰驱动之前**重过一遍：会话还在、权限还在、修订号没动。
        // 任一道没过，请求就还没出网关——此时必须把「已下发」占位还回去，
        // 否则一条从没下发过的执行会自称已下发，调用方再也重试不了。
        let gate = reject_terminal_session(&view).and_then(|()| {
            self.authorizer
                .authorize(principal, GatewayAction::Execute, &view, &source)
                .map_err(GatewayError::from)
        });
        let gate = gate.and_then(|()| {
            if expected_revision == view.context_revision {
                Ok(())
            } else {
                Err(RuntimeError::ContextRevisionMismatch {
                    expected: expected_revision.get(),
                    actual: view.context_revision.get(),
                }
                .into())
            }
        });
        if let Err(error) = gate {
            self.release_dispatch_reservation(execution_id).await;
            return Err(error);
        }

        // 段①终点：紧贴端口调用**之前**打点。把打点放在 await 之后，
        // 网关自己等锁的时间就会算进「网关开销」，口径立刻不成立。
        {
            let mut state = self.state.lock().await;
            if let Some(record) = state.records.get_mut(execution_id) {
                record.probe.mark_dispatch_issued(self.clock.as_ref());
            }
        }

        let receipt = self.port.execute_in_session(port_request).await?;

        // 段②起点：驱动刚返回，登记还没开始。
        let mut state = self.state.lock().await;
        if let Some(record) = state.records.get_mut(execution_id) {
            record.probe.mark_driver_completed(self.clock.as_ref());
            record.last_state = receipt.state;
        }
        // 段②终点：回执 / 事件状态已登记。
        let registration_completed_at = self.clock.now_nanos();
        if let Some(record) = state.records.get_mut(execution_id) {
            if let Some(sample) = record.probe.sample(registration_completed_at) {
                state.samples.push(sample);
            }
        }
        Ok(receipt)
    }

    /// 收回「已下发」占位：只在**尚未**调用驱动时使用。
    ///
    /// 占位本身是为了挡住并发重复下发；闸门没过说明请求根本没出网关，
    /// 这时留着占位就是谎报状态。
    async fn release_dispatch_reservation(&self, execution_id: &ExecutionId) {
        let mut state = self.state.lock().await;
        if let Some(record) = state.records.get_mut(execution_id) {
            record.dispatched = false;
        }
    }

    /// 取消一次执行。三态 `disposition` 见 [`CancelDisposition`]。
    pub async fn cancel(
        &self,
        principal: &RequestPrincipal,
        request: CancelRequest,
    ) -> Result<CancelOutcome, GatewayError> {
        let execution_id = request.binding.execution_id.clone();

        let (handle, resource_binding_id, source) = {
            let state = self.state.lock().await;
            let record = state
                .records
                .get(&execution_id)
                .ok_or_else(|| GatewayError::Runtime(cancel_failed(UNKNOWN_EXECUTION_REASON)))?;
            (
                record.handle.clone(),
                record.resource_binding_id.clone(),
                record.source.clone(),
            )
        };

        // 绑定不一致必须在**触达端口之前**拒掉：发出去的取消如果落到别的会话或
        // 别的资源绑定上，那是把 A 的执行停了，而调用方以为停的是 B。
        verify_binding(&request, &handle, resource_binding_id.as_ref())
            .map_err(|reason| GatewayError::Runtime(cancel_failed(reason)))?;

        let view = self.port.session_view(&handle).await?;
        self.authorizer
            .authorize(principal, GatewayAction::Cancel, &view, &source)?;

        if request.precise_cancel == PreciseCancel::Unsupported {
            // 驱动不支持精确取消：绝不下发一次「取消整个会话」来假装精确取消成功。
            let mut state = self.state.lock().await;
            if let Some(record) = state.records.get_mut(&execution_id) {
                let outcome =
                    unsupported_outcome(&execution_id, record.last_state, self.clock.now_nanos());
                return Ok(outcome);
            }
            return Ok(unsupported_outcome(
                &execution_id,
                ExecutionState::Queued,
                self.clock.now_nanos(),
            ));
        }

        let observed_at = self.clock.now_nanos();
        // Err 一律原样上抛：取消失败降级成「已取消」会让 UI 显示一个假的终态。
        let state_after = self.port.cancel_execution(&handle, &execution_id).await?;

        {
            let mut inner = self.state.lock().await;
            if let Some(record) = inner.records.get_mut(&execution_id) {
                record.last_state = state_after;
            }
        }
        Ok(CancelOutcome::new(
            execution_id,
            disposition_from_port_state(state_after),
            state_after,
            observed_at,
        ))
    }

    /// 交付一条事件。
    ///
    /// 归属**只**看事件自带的 `execution_id`：同一会话并发多条执行时，
    /// 靠 `(dbSessionId, runtimeEpoch)` 反查是概率性的，会把事件投给另一条执行。
    /// 网关里没有这个 `executionId` 时返回 [`EventDisposition::Unbound`]，
    /// 调用方应重新建立订阅关系。会话 / epoch 是否匹配交给该执行的水位线判断。
    pub async fn apply_event(&self, event: ExecutionEvent) -> EventDisposition {
        let mut state = self.state.lock().await;
        let Some(record) = state.records.get_mut(&event.execution_id) else {
            return EventDisposition::Unbound;
        };
        let disposition = record.events.apply(&event);
        if matches!(
            disposition,
            EventDisposition::Applied | EventDisposition::Gap { .. }
        ) {
            record.last_state = record.events.execution_state();
        }
        disposition
    }

    /// 用快照对齐水位线（CM-55 空洞恢复）。
    pub async fn recover_from_snapshot(
        &self,
        execution_id: &ExecutionId,
        view: &SessionView,
    ) -> bool {
        let mut state = self.state.lock().await;
        match state.records.get_mut(execution_id) {
            Some(record) => {
                record.events.recover_from_snapshot(view);
                record.last_state = record.events.execution_state();
                true
            }
            None => false,
        }
    }

    pub async fn execution(&self, execution_id: &ExecutionId) -> Option<ExecutionRecord> {
        let state = self.state.lock().await;
        state.records.get(execution_id).cloned()
    }

    pub async fn execution_count(&self) -> usize {
        let state = self.state.lock().await;
        state.records.len()
    }

    /// CM-60 样本仓库快照。
    pub async fn overhead_samples(&self) -> OverheadSamples {
        let state = self.state.lock().await;
        state.samples.clone()
    }

    /// 门禁开销 p95（纳秒）。**逐请求求和之后**再取分位数，不是两个分位数相加。
    pub async fn overhead_p95(&self) -> Option<u64> {
        let state = self.state.lock().await;
        state.samples.overhead_p95()
    }
}

impl std::fmt::Debug for ExecutionGateway {
    /// 端口 / 授权器 / 时钟都是 `dyn` 端口，没有 `Debug` 契约；
    /// 这里只打印**结构**，不打印依赖——打印依赖等于把端口内部的会话句柄写进日志。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionGateway").finish_non_exhaustive()
    }
}

/// 终态会话不是「已受理」。端口对 `Closed`/`Lost` 会话**返回 `Ok`**，
/// 漏掉这一步等于把一个已经关闭的会话的执行判成功。
fn reject_terminal_session(view: &SessionView) -> Result<(), GatewayError> {
    match view.state {
        SessionState::Closed | SessionState::Closing => {
            Err(RuntimeError::SessionClosed(view.handle.db_session_id.as_str().to_owned()).into())
        }
        SessionState::Lost => {
            Err(RuntimeError::SessionLost(view.handle.db_session_id.as_str().to_owned()).into())
        }
        _ => Ok(()),
    }
}
