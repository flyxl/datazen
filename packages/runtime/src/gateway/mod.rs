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
//! 3. [`Self::owned_view`]（**无锁 await**）：重读 `session_view` → **归属授权** → 终态拒绝
//!    （`Closed`/`Closing`/`Lost`——端口对终态会话**返回 `Ok`**，不判就会把已关闭会话的
//!    执行判成「已受理」）。三者次序不可调换，理由见 [`owner_binding`] 模块头（CM-05）。
//! 4. 临界区内：幂等查重 →（`Hit` 直接回放，不再走乐观闸门）→ 乐观并发闸门 →
//!    分配 `executionId` → 登记幂等记录 → 入队。
//!
//! 查重**必须**排在乐观闸门前面：重发的语义是「上次那次」，而上次那次受理时的
//! `context_revision` 往往已经过期了。先校验 revision 会让重发永远失败，
//! 调用方就会换一个 key 重试——那正是 CM-54 要禁止的重复写入。
//!
//! # 下发顺序（§3.4 / CM-62）
//!
//! 1. 找执行记录，找不到即拒绝（绝不新建一次「碰巧的」执行）；
//! 2. [`Self::owned_view`]：重读 `session_view` → **重校验权限**（受理时授权过一次，
//!    不代表下发时仍然有权）→ 终态拒绝；
//! 3. 重比 `expected_context_revision`（§3.1 要求「真正执行前再比一次」），
//!    不一致返回 [`RuntimeError::ContextRevisionMismatch`] 并携带服务端实际值；
//! 4. 记 CM-60 第一段起点 → 下发驱动 → 记第二段起点 → 登记回执/事件状态 → 落样本。
//!
//! 步骤 4 的两个记号点包住的是**驱动往返**，因此「假 SQL 执行」「预算/角色排队」
//! 「网络传输」天然落在区间外或区间内由驱动侧决定，网关不再额外计时（§3.5）。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::connection::capability::PreciseCancel;
use crate::connection::{
    Counter, ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState, ResourceId,
    RuntimeError, SessionHandle, SessionView,
};
use crate::gateway::owner_binding::resolve_owned_session;
use crate::gateway::request::RedactedExecuteRequest;
use crate::registry::SessionPort;
use GatewayAction::{Cancel, Execute};

pub mod cancel;
pub mod events;
pub mod idempotency;
pub mod owner_binding;
pub mod provenance;
pub mod request;
pub mod retention;
pub mod timing;
pub mod token;

/// `ExecutionGateway` 的固有方法（约五百行）。类型留在本文件，实现进子文件，
/// 否则本文件会被顶过 `gateway_contract` 的 800 行硬上限。纯搬迁，无新语义。
mod execution;

#[cfg(test)]
pub(crate) mod testing_support;

// 门面测试按 800 行上限拆成三块：共用替身 + 受理下发 + 取消事件。
// 事件水位的单元测试同样单列一块，否则 `events.rs` 会顶破上限。
#[cfg(test)]
mod cancel_event_tests;
#[cfg(test)]
mod event_store_tests;
#[cfg(test)]
pub(crate) mod facade_support;
#[cfg(test)]
mod facade_tests;
// CM-70：令牌层与保留期各自单列一块，否则两个源文件都会顶破 800 行上限。
#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod token_tests;

pub use cancel::binding as cancel_binding_reason;
pub use cancel::{
    cancel_failed, disposition_from_port_state, is_requested, state_literal, unsupported_outcome,
    verify_binding, CancelBinding, CancelOutcome, CancelRequest, UNKNOWN_EXECUTION_REASON,
};
// D-03：取消处置三态只有一处定义——冻结的 `connection::port::CancelDisposition`。
// 网关此前自带一份同名副本并从这里转出，那份副本已被删除；这里转出的是**同一个类型**，
// 因此 `gateway::CancelDisposition` 这条路径仍然可用，且不再存在两个可各自漂移的定义处。
pub use crate::connection::port::CancelDisposition;
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
pub use retention::{
    Grant, GrantRegistry, RetentionSweep, RetiredGrant, SweepReport, RETENTION_AFTER_EXPIRY_NANOS,
};
pub use timing::{FixedClock, MonotonicClock, OverheadProbe, OverheadSamples};
pub use token::{
    token_digest, KeyVersion, SubmissionOperation, SubmissionTokenGuard, TokenAdmission,
    TokenIssueError, TokenKeyring, TokenPayload, TokenRejection, DEFAULT_TTL_NANOS,
};

/// 网关内部记账的一条执行。
///
/// **每个执行自带事件水位线与计时探针**，不是全局一份：水位线绑定在
/// `(dbSessionId, runtimeEpoch)` 上，两个会话并发交错的事件不能共用一个水位线。
#[derive(Clone)]
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
    /// 受理时冻结的语义指纹。下发时它就是围栏键（CM-70）：请求一旦真的
    /// 交给了驱动，这个指纹上的任何自动重试都必须先核验。它和 `port_request`
    /// 一样受理定型，下发只读。
    fingerprint: RequestFingerprint,
    probe: OverheadProbe,
    events: EventStore,
    dispatched: bool,
    last_state: ExecutionState,
}
// `Debug` 手写不派生：`idempotency_key` 与内嵌的 `port_request` 里都是签名令牌。

/// `Debug` **手写、不派生**：装了令牌层之后 `idempotency_key` 就是那把签名提交令牌，
/// 而 [`ExecutionGateway::execution`] 是公开 API，返回值直接 `{:?}` 就把一份可重放的
/// 凭据抄进日志/审计。`port_request` 里还嵌着同一个令牌，所以那一层走
/// [`RedactedExecuteRequest`]，否则派生 `Debug` 会从字段深处把令牌带出来。
/// 要拿键就调 [`Self::idempotency_key`]，那是有意的出口。
impl std::fmt::Debug for ExecutionRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionRecord")
            .field("execution_id", &self.execution_id)
            .field("handle", &self.handle)
            .field("expected_context_revision", &self.expected_context_revision)
            .field("resource_binding_id", &self.resource_binding_id)
            .field("source", &self.source)
            .field("idempotency_key", &"<redacted>")
            .field("port_request", &RedactedExecuteRequest(&self.port_request))
            .field("fingerprint", &self.fingerprint)
            .field("probe", &self.probe)
            .field("events", &self.events)
            .field("dispatched", &self.dispatched)
            .field("last_state", &self.last_state)
            .finish()
    }
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

/// 一次**结局未知**的写入：账本读不出来，于是谁也不知道那次写到底落没落。
///
/// 键里**刻意没有** `idempotencyKey` 与令牌指纹——留一个空子就等于让围栏失效，
/// 因为「换个新键自动重试未知写入」正是围栏要挡的那件事（CM-70）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UnknownOutcome {
    db_session_id: String,
    fingerprint: RequestFingerprint,
}

impl UnknownOutcome {
    fn of(handle: &SessionHandle, fingerprint: &RequestFingerprint) -> Self {
        Self {
            db_session_id: handle.db_session_id.as_str().to_owned(),
            fingerprint: fingerprint.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct GatewayState {
    records: HashMap<ExecutionId, ExecutionRecord>,
    samples: OverheadSamples,
    next_sequence: u64,
    /// CM-70：结局未知的写入。命中它的新提交一律要求核验，不受理。
    unverified: HashSet<UnknownOutcome>,
}

/// 围栏只有两个动作，都以 `(dbSessionId, 语义指纹)` 为键。
///
/// 键里**不能**有令牌或幂等键：围栏要挡的正是「换个新键自动重试」，
/// 键绑在令牌上等于没围（CM-70）。
impl GatewayState {
    fn is_unverified(&self, handle: &SessionHandle, fingerprint: &RequestFingerprint) -> bool {
        self.unverified
            .contains(&UnknownOutcome::of(handle, fingerprint))
    }

    /// 记一次「结局未知」。CM-70 有**两个**抬围栏的点，理由不同但键相同：
    /// 账本读不出来（受理时）与请求已经交给驱动（下发时，见
    /// [`ExecutionGateway::dispatch`]）。两者都满足「这条语义写入可能已经
    /// 生效，只是没人知道」。
    fn raise_unknown_outcome(&mut self, handle: &SessionHandle, fingerprint: &RequestFingerprint) {
        self.unverified
            .insert(UnknownOutcome::of(handle, fingerprint));
    }
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
    /// 令牌闸门（CM-70）。`None` 表示调用方没装签名令牌层——此时受理退回
    /// 「键即不透明字符串」的旧行为，由 [`Self::with_submission_tokens`] 显式装配。
    tokens: Option<Arc<SubmissionTokenGuard>>,
    state: Mutex<GatewayState>,
}

impl std::fmt::Debug for ExecutionGateway {
    /// 端口 / 授权器 / 时钟都是 `dyn` 端口，没有 `Debug` 契约；
    /// 这里只打印**结构**，不打印依赖——打印依赖等于把端口内部的会话句柄写进日志。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionGateway").finish_non_exhaustive()
    }
}
