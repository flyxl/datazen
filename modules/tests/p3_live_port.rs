//! P3-⑤ 的两个出口契约的**可执行形状**。
//!
//! - 任务 A：活连接出口端口。这里只用公开 API 断言「借到的是借用」「归还只有一条路」
//!   「被回收/关闭的引用立刻失效」，不碰任何内部字段。
//! - 任务 B：空闲回收接线。这里断言两个方向都有：期限开着会被回收，期限关着不会被回收。
//!
//! 断言「异常路径不留登记行」用的是四个**只读投影**而不是内部表：registry 在册 id、
//! 应用层 `sessions` 行数、目录附件、活连接台账行数。四本账一起看才能区分
//! 「摘了 registry 但没做应用层清理」这类半清理。
//!
//! | 测试 | 钉住的契约句 |
//! | --- | --- |
//! | `with_the_deadline_on_expired_sessions_are_reclaimed_and_all_four_ledgers_stay_clean` | §6.4 空闲 ⇒ 断开；应用层登记同步清理 |
//! | `with_the_deadline_off_expired_sessions_are_not_reclaimed` | `idle_deadline_ms == None` 是配置事实，不是「计数器为 0」 |
//! | `the_periodic_scanner_drives_reaping_itself_the_host_needs_no_extra_timer` | 驱动器归应用层所有 |
//! | `a_started_scanner_really_exits_after_stop_leaking_no_task` | 起得也要停得；不泄漏 task |
//! | `borrow_return_borrow_again_the_ledger_returns_to_the_same_slot_and_the_port_is_probed_twice` | 端口被真的查询（不是回放缓存） |
//! | `reaping_invalidates_outstanding_borrows_the_holder_receives_an_invalidated_reference` | 被回收 ⇒ 旧借用立刻失效 |
//! | `borrowing_without_wiring_yields_service_unavailable_and_leaves_no_registry_row` | 没接线是配置事实，不登记 |
//! | `a_host_refusal_to_lend_leaves_no_empty_registry_row` | 拒绝路径不得留下孤儿行 |
//! | `the_ledger_returns_to_zero_after_a_normal_return` | 归还恰好一次 |
//! | `dropping_the_guard_triggers_a_fallback_return` | `Drop` 就是归还 |
//! | `a_timeout_interrupting_the_holder_triggers_a_fallback_return` | 超时路径与正常路径不分叉 |
//! | `an_open_failure_leaves_no_registry_row_at_all` | 异常打开不得留下登记 |
//! | `the_cancel_and_unknown_outcome_paths_leave_no_registry_row_after_the_close` | 取消/未知效果后清理仍幂等 |
//! | `closing_a_session_invalidates_outstanding_borrows_and_leaves_no_registry_row_afterwards` | 关闭 ⇒ 在借引用作废 + 四本账干净 |
//! | `no_further_borrows_after_the_session_closes` | INV-07/INV-08：不得透明重建 |

use async_trait::async_trait;
use datazen::execution::runtime::registry::{backend::*, SessionBackend};
use datazen::execution::{
    application::{
        IdlePolicy, LiveConnection, LiveConnectionError, LiveConnectionRequest,
        LiveConnectionSource, LivePurpose, RuntimeConnectionUseCases,
    },
    runtime::connection as rt,
};
use datazen::{
    connection::domain::*,
    execution as pa,
    execution::application::idempotency::IdempotentOperation,
    execution::application::policy::*,
    execution::target::{ExecutionTarget, NamespaceTarget},
    shared::context::{OwnerRef, RequestContext},
    shared::error::PortError,
    shared::id::*,
};
use datazen::{
    execution::application::requests::*, execution::application::use_cases::ConnectionUseCases,
};
use datazen_driver_api::{
    mock_driver::{MockDriver, MockDriverOptions},
    ConnectionHandle, DatabaseDriver,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// 替身
// ---------------------------------------------------------------------------

struct Profiles {
    revision: AtomicU64,
}

#[async_trait]
impl ProfileRepository for Profiles {
    async fn list(
        &self,
        c: &RequestContext,
        _: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        Ok(vec![self.get(c, ConnectionId::new("p")).await?.unwrap()])
    }
    async fn get(
        &self,
        c: &RequestContext,
        id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let mut p = ProfileRecord::new(c.organization_id.clone(), id, "test");
        p.config_revision = ConfigRevision::new(self.revision.load(Ordering::SeqCst));
        Ok(Some(p))
    }
    async fn create(
        &self,
        _: &RequestContext,
        _: datazen::connection::dto::ProfileDraft,
        _: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn compare_and_set(
        &self,
        _: &RequestContext,
        _: ConnectionId,
        _: ConfigRevision,
        _: datazen::connection::dto::ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn disable(
        &self,
        _: &RequestContext,
        _: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn advance_credential_revision(
        &self,
        _: &RequestContext,
        _: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        unreachable!()
    }
}

struct Policy {
    allow: AtomicBool,
}

#[async_trait]
impl PolicyService for Policy {
    async fn authorize(
        &self,
        _: &RequestContext,
        _: AuthorizationSubject,
        _: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(if self.allow.load(Ordering::SeqCst) {
            AuthorizationDecision::Allow
        } else {
            AuthorizationDecision::Deny {
                reason_code: "revoked",
            }
        })
    }
    async fn verify_delegation(
        &self,
        _: &RequestContext,
        _: datazen::shared::context::DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        unreachable!()
    }
    fn isolation_key(&self, _: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("test"))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

struct Backend {
    contexts: Mutex<std::collections::HashMap<String, rt::SessionContext>>,
    sink: Mutex<Option<datazen::execution::application::RuntimeResultSink>>,
    cancelled: Mutex<Vec<ExecutionId>>,
    opened: AtomicU64,
    closed: AtomicU64,
    fail_open: AtomicBool,
    started: Notify,
    release: Notify,
}

impl Default for Backend {
    fn default() -> Self {
        Self {
            contexts: Mutex::new(std::collections::HashMap::new()),
            sink: Mutex::new(None),
            cancelled: Mutex::new(Vec::new()),
            opened: AtomicU64::new(0),
            closed: AtomicU64::new(0),
            fail_open: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        }
    }
}

#[async_trait]
impl SessionBackend for Backend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        if self.fail_open.load(Ordering::SeqCst) {
            return Err(rt::ProviderError::SessionLost("injected".into()));
        }
        let id = format!("physical-{}", self.opened.fetch_add(1, Ordering::SeqCst));
        let context = rt::SessionContext::new(r.initial_target, "observed-identity");
        lock(&self.contexts).insert(id.clone(), context.clone());
        Ok(OpenedResource {
            context,
            capabilities: datazen::execution::runtime::registry::CapabilityVersions::new(
                "test", "test",
            ),
            capability_snapshot:
                datazen::execution::runtime::registry::backend::CapabilityProvenance::new(
                    "test-driver",
                    "2.3.1",
                    7,
                ),
            resource_id: id,
            driver_supports_cancel: true,
        })
    }

    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, rt::ProviderError> {
        r.cancel_handle_sink.publish("test-cancel");
        self.started.notify_one();
        if r.command.command == "hold" {
            self.release.notified().await;
        }
        let context = lock(&self.contexts)
            .get(&r.resource_id)
            .cloned()
            .ok_or_else(|| rt::ProviderError::SessionLost("unknown resource".into()))?;
        let cancelled = lock(&self.cancelled).contains(&r.execution_id);
        Ok(ResourceExecution {
            execution_id: r.execution_id.clone(),
            stream_id: StreamId::new(format!("actual-{}", r.execution_id)),
            state: if cancelled {
                rt::ExecutionState::Cancelled
            } else {
                rt::ExecutionState::Succeeded
            },
            effect_outcome: if cancelled || r.command.command == "unknown" {
                rt::EffectOutcome::Unknown
            } else {
                rt::EffectOutcome::Completed
            },
            context_after: context,
            context_revision: r.expected_context_revision,
            handles: Vec::new(),
            cancel_handle: "test-cancel".into(),
        })
    }

    async fn cancel(&self, r: CancelOnResource) -> Result<ResourceCancel, rt::ProviderError> {
        lock(&self.cancelled).push(r.execution_id);
        self.release.notify_one();
        Ok(ResourceCancel {
            disposition: rt::port::CancelDisposition::Requested,
            state: rt::ExecutionState::CancelRequested,
        })
    }

    async fn finalize_handles(
        &self,
        r: FinalizeHandles,
    ) -> Result<HandleFinalization, rt::ProviderError> {
        Ok(HandleFinalization {
            finalized: r.handles.len(),
            remaining: 0,
            effect_outcome: rt::EffectOutcome::RolledBack,
        })
    }

    async fn close(&self, r: CloseResource) -> Result<CloseResourceOutcome, rt::ProviderError> {
        self.closed.fetch_add(1, Ordering::SeqCst);
        lock(&self.contexts).remove(&r.resource_id);
        Ok(CloseResourceOutcome::Closed)
    }
}

/// 宿主侧的出向端口实现：把已存在的那份物理连接交出来，绝不新建。
struct HostLive {
    refuse: AtomicBool,
    requests: Mutex<Vec<rt::SessionHandle>>,
}

impl Default for HostLive {
    fn default() -> Self {
        Self {
            refuse: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LiveConnectionSource for HostLive {
    async fn open_live(
        &self,
        request: LiveConnectionRequest,
    ) -> Result<LiveConnection, LiveConnectionError> {
        lock(&self.requests).push(request.handle.clone());
        if self.refuse.load(Ordering::SeqCst) {
            return Err(LiveConnectionError::Refused("host declined".into()));
        }
        let id = request.handle.db_session_id.as_str().to_string();
        let driver: Arc<dyn DatabaseDriver> =
            MockDriver::new("sqlite", MockDriverOptions::default());
        Ok(LiveConnection::new(
            driver,
            ConnectionHandle {
                id: id.clone(),
                pool_id: format!("pool-{id}"),
            },
        ))
    }
}

// ---------------------------------------------------------------------------
// 装配
// ---------------------------------------------------------------------------

fn ctx(user: &str, client: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org"),
        PrincipalId::new(user),
        None,
        ClientInstanceId::new(client),
        RequestId::new("request"),
        None,
    )
}

fn target(database: &str) -> ExecutionTarget {
    ExecutionTarget::new(ConnectionId::new("p"), NamespaceTarget::database(database))
}

/// 装配一个接好线、已带宿主的 runtime 门面。
fn setup() -> (Arc<RuntimeConnectionUseCases>, Arc<Backend>, Arc<HostLive>) {
    let backend = Arc::new(Backend::default());
    let host = Arc::new(HostLive::default());
    let service = Arc::new(RuntimeConnectionUseCases::new(
        Arc::new(Profiles {
            revision: AtomicU64::new(1),
        }),
        Arc::new(Policy {
            allow: AtomicBool::new(true),
        }),
        backend.clone(),
    ));
    *lock(&backend.sink) = Some(service.result_sink());
    service.with_live_connections(host.clone());
    (service, backend, host)
}

async fn open(
    service: &RuntimeConnectionUseCases,
    c: &RequestContext,
    editor: &str,
) -> pa::OpenSessionReceipt {
    let token = service
        .issue_submission_token(c, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();
    service
        .open_session(
            c,
            OpenSessionRequest::new(
                target("a"),
                OwnerRef::Editor {
                    client_instance_id: c.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new(editor),
                },
                token.idempotency_key,
            ),
        )
        .await
        .unwrap()
}

async fn execute(
    service: &RuntimeConnectionUseCases,
    c: &RequestContext,
    h: &pa::SessionHandle,
    command: &str,
) -> datazen::execution::ExecutionReceipt {
    let token = service
        .issue_submission_token(c, IdempotentOperation::ExecuteInSession, Some(h))
        .await
        .unwrap();
    service
        .execute_in_session(
            c,
            ExecuteInSessionRequest::new(
                h.clone(),
                CommandCall::new(command, serde_json::json!({})),
                token.idempotency_key,
            ),
        )
        .await
        .unwrap()
}

async fn terminal(
    service: &RuntimeConnectionUseCases,
    c: &RequestContext,
    id: &ExecutionId,
) -> datazen::execution::ExecutionView {
    for _ in 0..1000 {
        let view = service.get_execution(c, id.clone()).await.unwrap();
        if view.state.is_terminal() {
            return view;
        }
        tokio::task::yield_now().await;
    }
    panic!("execution did not settle")
}

/// 短期限：把 §9.2 的 30 分钟按比例缩到 40 ms，语义不变但测试能在毫秒级跑完。
const SHORT: Duration = Duration::from_millis(40);
/// 扫一次就足够跨过 `SHORT` 的等待，60 ms 在 CI 上留足余量。
const PAST_DEADLINE: Duration = Duration::from_millis(160);

async fn close(service: &RuntimeConnectionUseCases, c: &RequestContext, h: &pa::SessionHandle) {
    service
        .close_session(
            c,
            CloseSessionRequest::new(h.clone(), CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// 任务 B：空闲回收的两个方向
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_deadline_on_expired_sessions_are_reclaimed_and_all_four_ledgers_stay_clean() {
    let (service, _, _) = setup();
    service.with_idle_policy(IdlePolicy::with_deadline(SHORT, Duration::from_secs(180)));
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let id = opened.session.handle.db_session_id.clone();
    assert!(service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 1);
    assert!(service.is_attached(&opened.session.handle));

    tokio::time::sleep(PAST_DEADLINE).await;
    let reclaimed = service.reclaim_idle().await;

    assert_eq!(reclaimed.reclaimed, vec![id.clone()]);
    assert!(reclaimed.interrupted_borrows.is_empty());
    // 四本账：registry、应用层 sessions、目录附件、活连接台账。
    assert!(!service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 0);
    assert!(!service.is_attached(&opened.session.handle));
    assert_eq!(service.live_connection_rows(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_deadline_off_expired_sessions_are_not_reclaimed() {
    let (service, _, _) = setup();
    service.with_idle_policy(IdlePolicy::disabled());
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let id = opened.session.handle.db_session_id.clone();

    tokio::time::sleep(PAST_DEADLINE).await;
    let reclaimed = service.reclaim_idle().await;

    assert!(reclaimed.reclaimed.is_empty());
    assert!(service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 1);
    assert!(service.is_attached(&opened.session.handle));
    // 显式清理后仍然是干净的：关着的策略不是「不清理」。
    close(&service, &c, &opened.session.handle).await;
    assert!(!service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_periodic_scanner_drives_reaping_itself_the_host_needs_no_extra_timer() {
    let (service, _, _) = setup();
    service.with_idle_policy(IdlePolicy::with_deadline(SHORT, Duration::from_millis(30)));
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let id = opened.session.handle.db_session_id.clone();

    let reclaimer = service.start_idle_reclaimer();
    for _ in 0..200 {
        if !service.is_registered(&id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    reclaimer.stop();

    assert!(!service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 0);
    assert!(!service.is_attached(&opened.session.handle));
}

/// 扫描器**能被停**，且停之后 tokio 真的回收了那个任务（不泄漏）。
///
/// 这一条是「接线」的另一半：只断言 `start_idle_reclaimer()` 被调用过，那么把
/// `stop()` 改成空实现的实现照样全绿——于是每次进程退出都留一个活着的扫描任务。
/// 判据因此是行为：`is_finished()` 从 false 翻到 true。
///
/// 与上一条的分工：上一条证明「扫描器真的会回收」，这一条证明「扫完能停下」。
/// 合起来才覆盖驱动器完整生命周期，缺一条都只测了一半。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_started_scanner_really_exits_after_stop_leaking_no_task() {
    let (service, _, _) = setup();
    // 期限开着、扫描间隔 5 ms：这一条不关心回收谁，只关心起停本身。
    service.with_idle_policy(IdlePolicy::with_deadline(SHORT, Duration::from_millis(5)));
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let handle = opened.session.handle.clone();

    let reclaimer = service.start_idle_reclaimer();
    assert!(
        !reclaimer.is_finished(),
        "刚起时任务必须还在跑；立即退出说明测的是另一件事"
    );

    reclaimer.stop();

    // 有界轮询而不是 sleep 一个大数：CI 慢机器不误判，快机器不白等。
    for _ in 0..400 {
        if reclaimer.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        reclaimer.is_finished(),
        "stop() 之后扫描任务必须退出——不退出就是每次进程退出都留一个活任务"
    );

    // 幂等：再停一次不能让已退出的任务复活。
    reclaimer.stop();
    assert!(reclaimer.is_finished());

    // 停的是驱动器，不是会话：会话仍可按需显式关闭，且关闭不留登记行。
    close(&service, &c, &handle).await;
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.tracked_session_rows(), 0);
}

/// 同一会话先借后还再借：台账计数必须回到同一格，且端口**真的被查了两次**。
///
/// 「第二轮回放缓存」是这条路最像成功的失败形状：计数对、内容也对，但端口压根
/// 没被问。因此 `host.requests` 的条数必须等于轮数，而不是 ≤ 轮数。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn borrow_return_borrow_again_the_ledger_returns_to_the_same_slot_and_the_port_is_probed_twice() {
    let (service, _, host) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let handle = opened.session.handle.clone();

    for _ in 1..=2u32 {
        let lease = service
            .borrow_live_connection(&c, &handle, LivePurpose::CommandExecution)
            .await
            .expect("每一轮都必须借得到：接线不能只对第一次生效");
        let lent = lease.connection().expect("新借出的必须可用");
        assert_eq!(
            lent.handle().id,
            handle.db_session_id.as_str(),
            "每一轮拿到的仍必须是那一个会话的资源"
        );
        assert_eq!(service.live_connection_borrows(), 1);
        assert_eq!(service.live_connection_rows(), 1);
        let released = lease.release();
        assert!(!released.revoked);
        assert_eq!(service.live_connection_rows(), 0);
        assert_eq!(service.live_connection_borrows(), 0);
    }

    assert_eq!(
        lock(&host.requests).len(),
        2,
        "两轮借出必须各自真的过一次端口；只有一次就说明第二轮回放了缓存"
    );
    close(&service, &c, &handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reaping_invalidates_outstanding_borrows_the_holder_receives_an_invalidated_reference() {
    let (service, _, _) = setup();
    service.with_idle_policy(IdlePolicy::with_deadline(SHORT, Duration::from_secs(180)));
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let id = opened.session.handle.db_session_id.clone();
    let lease = service
        .borrow_live_connection(&c, &opened.session.handle, LivePurpose::CommandExecution)
        .await
        .unwrap();
    assert_eq!(service.live_connection_borrows(), 1);
    assert_eq!(service.live_connection_rows(), 1);

    tokio::time::sleep(PAST_DEADLINE).await;
    let reclaimed = service.reclaim_idle().await;

    assert_eq!(reclaimed.reclaimed, vec![id.clone()]);
    assert_eq!(reclaimed.interrupted_borrows, vec![id.clone()]);
    // 宿主手上那份引用此刻已是死的：不能再当活连接用。
    assert!(matches!(
        lease.connection(),
        Err(LiveConnectionError::Revoked)
    ));
    let released = lease.release();
    assert!(released.revoked);
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
}

// ---------------------------------------------------------------------------
// 任务 A：借出与归还的契约
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn borrowing_without_wiring_yields_service_unavailable_and_leaves_no_registry_row() {
    let backend = Arc::new(Backend::default());
    let service = RuntimeConnectionUseCases::new(
        Arc::new(Profiles {
            revision: AtomicU64::new(1),
        }),
        Arc::new(Policy {
            allow: AtomicBool::new(true),
        }),
        backend.clone(),
    );
    *lock(&backend.sink) = Some(service.result_sink());
    assert!(!service.has_live_connections());
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;

    let err = service
        .borrow_live_connection(&c, &opened.session.handle, LivePurpose::Inspection)
        .await
        .expect_err("unwired port must refuse");

    assert_eq!(
        err.code,
        datazen::shared::error::ApiErrorCode::ServiceUnavailable
    );
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
    // 拒绝不得连累会话本身。
    assert!(service.is_registered(&opened.session.handle.db_session_id));
    close(&service, &c, &opened.session.handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_refusal_to_lend_leaves_no_empty_registry_row() {
    let (service, _, host) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    host.refuse.store(true, Ordering::SeqCst);

    let err = service
        .borrow_live_connection(&c, &opened.session.handle, LivePurpose::CommandExecution)
        .await
        .expect_err("host refusal must surface");

    assert_eq!(
        err.code,
        datazen::shared::error::ApiErrorCode::ServiceUnavailable
    );
    assert_eq!(lock(&host.requests).len(), 1);
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
    assert_eq!(service.live_connection_rows(), 0);
    close(&service, &c, &opened.session.handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_ledger_returns_to_zero_after_a_normal_return() {
    let (service, _, host) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let lease = service
        .borrow_live_connection(&c, &opened.session.handle, LivePurpose::CommandExecution)
        .await
        .unwrap();
    let connection = lease.connection().expect("fresh borrow must be usable");
    assert_eq!(
        connection.handle().id,
        opened.session.handle.db_session_id.as_str()
    );

    let released = lease.release();
    assert!(!released.revoked);
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
    assert_eq!(lock(&host.requests).len(), 1);
    close(&service, &c, &opened.session.handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_guard_triggers_a_fallback_return() {
    let (service, _, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    {
        let _lease = service
            .borrow_live_connection(&c, &opened.session.handle, LivePurpose::Inspection)
            .await
            .unwrap();
        assert_eq!(service.live_connection_rows(), 1);
    }
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
    close(&service, &c, &opened.session.handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_timeout_interrupting_the_holder_triggers_a_fallback_return() {
    let (service, _, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let handle = opened.session.handle.clone();

    // 借到之后卡在一个永不完成的 future 上，被 timeout 打断。
    let outcome = tokio::time::timeout(Duration::from_millis(80), async {
        let lease = service
            .borrow_live_connection(&c, &handle, LivePurpose::LongLivedHostWork)
            .await
            .unwrap();
        std::future::pending::<()>().await;
        drop(lease);
    })
    .await;
    assert!(outcome.is_err(), "the holder must have been cut off");
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
    close(&service, &c, &handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_failure_leaves_no_registry_row_at_all() {
    let (service, backend, _) = setup();
    backend.fail_open.store(true, Ordering::SeqCst);
    let c = ctx("u", "c");
    let token = service
        .issue_submission_token(&c, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();

    let err = service
        .open_session(
            &c,
            OpenSessionRequest::new(
                target("a"),
                OwnerRef::Editor {
                    client_instance_id: c.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new("e"),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect_err("injected open failure must surface");
    assert_eq!(
        err.code,
        // `SessionNotFound` 而不是 `SessionLost`：后者意味着「曾经活着、后来没了」，
        // 而这一栏压根没立起来。宿主据此区分「重试打开」和「先查会话再决定」。
        datazen::shared::error::ApiErrorCode::SessionNotFound
    );

    assert!(service.registered_session_ids().is_empty());
    assert_eq!(service.tracked_session_rows(), 0);
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cancel_and_unknown_outcome_paths_leave_no_registry_row_after_the_close() {
    let (service, backend, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let id = opened.session.handle.db_session_id.clone();

    // 路径一：执行中取消。
    let held = execute(&service, &c, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    service
        .cancel_execution(&c, held.execution_id.clone())
        .await
        .unwrap();
    let settled = terminal(&service, &c, &held.execution_id).await;
    assert!(settled.state.is_terminal());

    // 路径二：效果未知（无法判定提交与否）。
    let unknown = execute(&service, &c, &opened.session.handle, "unknown").await;
    let settled = terminal(&service, &c, &unknown.execution_id).await;
    assert!(settled.state.is_terminal());

    // 两条异常路径之后都还能正常关闭，且关闭不留登记行。
    let lease = service
        .borrow_live_connection(&c, &opened.session.handle, LivePurpose::CommandExecution)
        .await
        .unwrap();
    close(&service, &c, &opened.session.handle).await;

    assert!(matches!(
        lease.connection(),
        Err(LiveConnectionError::Revoked)
    ));
    let released = lease.release();
    assert!(released.revoked);
    assert!(!service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 0);
    assert!(!service.is_attached(&opened.session.handle));
    assert_eq!(service.live_connection_rows(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_a_session_invalidates_outstanding_borrows_and_leaves_no_registry_row_afterwards() {
    let (service, backend, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let id = opened.session.handle.db_session_id.clone();
    let lease = service
        .borrow_live_connection(&c, &opened.session.handle, LivePurpose::CommandExecution)
        .await
        .unwrap();

    close(&service, &c, &opened.session.handle).await;

    assert!(matches!(
        lease.connection(),
        Err(LiveConnectionError::Revoked)
    ));
    assert!(lease.release().revoked);
    assert_eq!(backend.closed.load(Ordering::SeqCst), 1);
    assert!(!service.is_registered(&id));
    assert_eq!(service.tracked_session_rows(), 0);
    assert!(!service.is_attached(&opened.session.handle));
    assert_eq!(service.live_connection_rows(), 0);
    assert_eq!(service.live_connection_borrows(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_further_borrows_after_the_session_closes() {
    let (service, _, host) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "e").await;
    let handle = opened.session.handle.clone();
    close(&service, &c, &handle).await;

    let err = service
        .borrow_live_connection(&c, &handle, LivePurpose::Inspection)
        .await
        .expect_err("closed session must not lend");

    assert_eq!(err.code, datazen::shared::error::ApiErrorCode::SessionLost);
    // 端口压根没被问：请求在 runtime 自己的登记上就失败了。
    assert!(lock(&host.requests).is_empty());
    assert_eq!(service.live_connection_rows(), 0);
}
