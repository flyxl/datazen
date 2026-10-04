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
            tokens: None,
            state: Mutex::new(GatewayState::default()),
        }
    }

    /// 「这条句柄在本主体名下是否存在且可用」的**唯一**判定入口（CM-04 / CM-05 / CM-06）。
    /// 受理、下发、取消三个调用点都必须走它。`expected_revision` 只在下发闸门给值（CAS 期望值）。
    async fn owned_view(
        &self,
        principal: &RequestPrincipal,
        action: GatewayAction,
        handle: &SessionHandle,
        source: &ExecutionSource,
        expected_revision: Option<Counter>,
    ) -> Result<SessionView, GatewayError> {
        resolve_owned_session(
            &*self.port,
            &*self.authorizer,
            principal,
            action,
            handle,
            source,
            expected_revision,
        )
        .await
    }

    /// 装配签名令牌层（CM-70）。受理路径的**第 0 步**变为令牌闸门：
    /// 验签 → 墓碑 → 过期 → owner 代次，任一不过即拒绝，**不分配 `executionId`、
    /// 不写账本、不下发**。
    pub fn with_submission_tokens(
        port: Arc<dyn SessionPort>,
        authorizer: Arc<dyn Authorizer>,
        store: Arc<dyn IdempotencyStore>,
        clock: Arc<dyn MonotonicClock>,
        tokens: Arc<SubmissionTokenGuard>,
    ) -> Self {
        Self {
            port,
            authorizer,
            ledger: IdempotencyLedger::new(store),
            clock,
            tokens: Some(tokens),
            state: Mutex::new(GatewayState::default()),
        }
    }

    /// 已装配的令牌闸门。`None` 即未装配。
    pub fn submission_tokens(&self) -> Option<&Arc<SubmissionTokenGuard>> {
        self.tokens.as_ref()
    }

    /// 保留期清扫（CM-70 §3.5）：授予与账本记录**一并**删。
    ///
    /// 两边必须同时删，只删一边会各自留个洞：只删授予，重放仍命中账本返回原回执
    /// （还算安全，但掩盖了「记录已过期」这件事）；只删账本，令牌没过期时会
    /// **真的再执行一遍**——CM-70 禁止的就是这个。
    ///
    /// 顺序固定为先 `GrantRegistry::sweep`（那里已经判过 `expires_at` 与
    /// `retained_until`）再按返回的作用域删账本，所以「未过期不得删」这道闸
    /// 只存在一处，不会在两个地方各判一次而判出不同结果。
    pub fn sweep_idempotency_retention(&self, now_nanos: u64) -> RetentionSweep {
        let Some(guard) = &self.tokens else {
            return RetentionSweep::default();
        };
        let report = guard.sweep(now_nanos);
        let mut outcome = RetentionSweep {
            retired: report.deleted.len(),
            refused: report.refused.clone(),
            ledger_deleted: 0,
            tombstones_pruned: report.tombstones_pruned,
        };
        for retired in &report.deleted {
            match self.ledger.forget(&retired.scope) {
                Ok(true) => outcome.ledger_deleted += 1,
                Ok(false) => {}
                Err(err) => tracing::warn!(
                    digest = %retired.digest,
                    error = %err,
                    "保留期清扫：账本记录删除失败，授予已退役，重放仍被墓碑拒绝"
                ),
            }
        }
        outcome
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

        // ── CM-70 第 0 步：令牌闸门 ────────────────────────────────────────
        // 位置是**语义要求**，不是代码风格：闸门必须在账本查重之前。
        // 若排在之后，一条「已被保留期清扫删除账本记录、但自身尚未过期」的令牌
        // 会一路走到第 4 步查重得到 `Miss`，然后被真的再执行一遍——
        // 正是 CM-70 断言禁止的行为。闸门在前，请求在第 4 步之前就死了。
        let admission = match &self.tokens {
            None => None,
            Some(guard) => Some(
                guard
                    .admit(
                        &request.idempotency_key,
                        self.clock.now_nanos(),
                        Some(handle.runtime_epoch),
                    )
                    .map_err(|rejection| match rejection {
                        // owner 重启后的旧令牌：与 `session_view` 发现会话已丢失
                        // 是**同一种可观测结局**（`SessionLost`），但触发路径不同。
                        TokenRejection::OwnerEpochMismatch => {
                            GatewayError::Runtime(RuntimeError::SessionLost(format!(
                                "submission token owner epoch mismatch on {}",
                                handle.db_session_id.as_str()
                            )))
                        }
                        other => GatewayError::SubmissionTokenRejected {
                            reason: other.reason(),
                        },
                    })?,
            ),
        };

        let view = self
            .owned_view(principal, Execute, &handle, &request.source, None)
            .await?;

        let mut state = self.state.lock().await;
        match self.ledger.lookup(&scope, &fingerprint) {
            IdempotencyLookup::Hit(existing) => Ok(GatewayAcceptance::replayed(
                &existing.execution_id,
                &request.source,
                existing.first_accepted_at_nanos,
            )),
            IdempotencyLookup::Conflict { existing, incoming } => {
                // 刻意不回显键：它恒等于调用方刚递上来的那把令牌（作用域相同
                // 才可能冲突），回显是零信息 + 一份凭据泄漏。理由见
                // `GatewayError::IdempotencyConflict`。
                Err(GatewayError::IdempotencyConflict {
                    existing: existing.execution_id,
                    incoming: incoming.as_str().to_owned(),
                })
            }
            IdempotencyLookup::Unreadable { message } => {
                // 绝不降级成 Miss 后另写一条——那会真的把同一个语义请求再跑一遍。
                // 这次读不出来本身就是「结局未知」，把它记进围栏：下次换个新键
                // 回来（哪怕账本这会儿读得动了）也一样要核验，不许自动重试。
                state.raise_unknown_outcome(&handle, &fingerprint);
                Err(GatewayError::IdempotencyVerificationRequired { message })
            }
            IdempotencyLookup::Miss => {
                // 围栏：同一个会话、同一个语义写入已经有一次结局未知的尝试。
                // 走到这里说明账本读得动、也确实没有这条记录——但「读得动」不等于
                //「上一次没写进去」，所以仍不接受，等显式核验。
                if state.is_unverified(&handle, &fingerprint) {
                    return Err(GatewayError::IdempotencyVerificationRequired {
                        message: format!(
                            "同一写入存在结局未知的尝试（指纹 {}），自动重试已被拒绝；                             核验后请显式调用 resolve_unknown_outcome",
                            fingerprint.as_str()
                        ),
                    });
                }
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
                            // 账本留一份，执行记录再留一份给围栏用；两处不可分叉。
                            fingerprint: fingerprint.clone(),
                            first_accepted_at_nanos: accepted_at,
                        },
                    )
                    .map_err(|err| GatewayError::IdempotencyPersistFailed {
                        message: err.message,
                    })?;

                // 账本写成功才算受理。授予与账本记录**同时**登记，两者都由同一张
                // 令牌的到期时刻推导，所以清扫要么两个都删、要么两个都不删。
                if let (Some(guard), Some(admission)) = (&self.tokens, &admission) {
                    guard.register(admission, &execution_id, &scope);
                }

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
                    fingerprint,
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
        let (handle, expected_revision, source, port_request, fingerprint) = {
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
                record.fingerprint.clone(),
            )
        };

        // 闸门在**碰驱动之前**重过一遍：会话还在、权限还在、修订号没动。
        // 任一道没过，请求就还没出网关——此时必须把「已下发」占位还回去，
        // 否则一条从没下发过的执行会自称已下发，调用方再也重试不了。
        let gate = self
            .owned_view(
                principal,
                Execute,
                &handle,
                &source,
                Some(expected_revision),
            )
            .await;
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

        // CM-70：围栏在**把请求交给驱动的那一刻**抬起，刻意不看返回。
        //
        // 触发条件是「已下发」而非「驱动报错了」：SQL 可能已落库而结果没回来。判据的
        // 「响应丢失」有两种丢法——驱动报错（落没落库没人知道）与驱动成功但回执在路上
        // 丢了（落库了，只是提交者不知道）；后者网关这侧不可观测，前者只是它的可观测
        // 替身，把抬围栏绑在错误分支上等于给第二条丢法留了自动重试的口子。
        //
        // 对驱动语义上判定「本次没生效」的那类失败，网关同样抬栏：这是符合判据的保守
        // 实现——判据是一条**禁令**（不得拿新键自动重试一条**结局未知**的写入），
        // 并不要求放行什么；网关看不到驱动内部的落库顺序，替它断言「这次确定没生效」
        // 只会造出一个无从核实的乐观前提。
        // 抬栏的代价是「同一条语义写入不能自动重跑第二遍」，这正是指纹制重的本意：要
        // 再跑一次走 `resolve_unknown_outcome` 显式放行（`tests/cm70/retries.rs` 有
        // 专条用例），受理时那处抬栏（账本读不出来）共用 `raise_unknown_outcome`。
        {
            let mut state = self.state.lock().await;
            state.raise_unknown_outcome(&handle, &fingerprint);
        }

        // 闸门没过时上面的 `release_dispatch_reservation` 已经把请求挡在驱动之外，
        // 所以这里抬围栏不误伤：请求一旦走到这行，就真的出网关了。
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

        self.owned_view(principal, Cancel, &handle, &source, None)
            .await?;

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

    /// 核验过那次结局未知的写入之后，解除围栏（CM-70）。
    ///
    /// 这是围栏**唯一**的出口。调用方必须先确认那次写到底落没落（查库、
    /// 看业务唯一键），然后才轮到改本地状态；让受理路径自己「过一会儿就忘」，
    /// 等于把重复写入又放回来。返回是否真的解除了——对不存在的围栏调用不算数，
    /// 调用方据此知道核验是否针对了正确的写入。
    pub async fn resolve_unknown_outcome(
        &self,
        db_session_id: &str,
        fingerprint: &RequestFingerprint,
    ) -> bool {
        let mut state = self.state.lock().await;
        state.unverified.remove(&UnknownOutcome {
            db_session_id: db_session_id.to_owned(),
            fingerprint: fingerprint.clone(),
        })
    }

    /// 是否仍有结局未知的写入。围栏状态必须可观测，否则调用方无法报告
    /// 「这次请求被挡是因为上一次结局未知」，只能看到一个笼统的核验要求。
    pub async fn has_unverified_outcome(
        &self,
        db_session_id: &str,
        fingerprint: &RequestFingerprint,
    ) -> bool {
        let state = self.state.lock().await;
        state.unverified.contains(&UnknownOutcome {
            db_session_id: db_session_id.to_owned(),
            fingerprint: fingerprint.clone(),
        })
    }

    /// 结局未知的写入条数。
    pub async fn unverified_outcome_count(&self) -> usize {
        let state = self.state.lock().await;
        state.unverified.len()
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
