//! P3 接缝覆盖率补齐（验证用新增文件，**不属于目标提交 `8e0e874f`**）。
//!
//! 这里只补 `8e0e874f` 自带单测**没有覆盖**的分支，逐条对应 `port.rs` /
//! `error.rs` 里的契约句与 `connection-management.md` 的条款：
//!
//! | 用例 | 补的分支 | 依据 |
//! |---|---|---|
//! | `terminal_sessions_are_still_readable_including_lost` | `session_view` 在 `Lost` 上仍返回 `Ok` | `port.rs:44-45` |
//! | `close_is_idempotent_on_a_lost_session` | `close_session` 在 `Lost` 上幂等 | `port.rs:87-88`、§6.1 |
//! | `close_rejects_on_every_blocking_transaction_state` | `Aborted` 事务 | `port.rs:95-97`（自带单测只测 `Active`/`Unknown`）|
//! | `budget_and_quarantine_rejections_cross_the_trait_object` | `BudgetExhausted` / `SessionQuarantined` | `port.rs:58` |
//! | `cancel_failure_never_degrades_into_a_cancel_outcome` | `CancelFailed` 的对外呈现 | `port.rs:78`、`error.rs:244` |
//! | `invariant_broken_has_no_request_side_code` | `InvariantBroken` | `error.rs:244` |
//! | `counter_boundaries_do_not_alias_on_the_seam` | `Counter::ZERO` / `u64::MAX` epoch | `§6.3` 逐字段匹配 |
//! | `context_revision_mismatch_carries_server_actual_at_boundaries` | revision 载荷极值 | `port.rs:57` |
//! | `reason_literals_are_pinned` | 9 个 reason 字面量 | `error.rs:209` |
//!
//! 这些用例断言的是**被测提交的契约文本**（`port.rs` 的文档句）与
//! `RuntimeError` 自身行为，不是这份 fake 的行为；因此同一份文件可以原样
//! 拿去跑 Wave 1 的真实实现，作为接缝一致性判据。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_runtime::connection::error::ApiErrorCode;
use datazen_runtime::connection::types::{ClientInstanceId, EditorSessionId};
use datazen_runtime::connection::{
    AttachmentState, CloseMode, CommandCall, ConfigRevision, ConnectionId, Counter, DbSessionId,
    ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState, ExecutionTarget,
    NamespaceTarget, OrganizationId, OwnerRef, PrincipalId, RuntimeError, SessionContext,
    SessionHandle, SessionState, SessionView, StreamId, Timestamp, TransactionState,
};
use datazen_runtime::registry::SessionPort;

/// 脚本化 fake：`fail_with` 指定执行路径要报的错误，`transaction` 指定事务态，
/// `state` 指定会话态。默认全成功。
struct ScriptedPort {
    view: SessionView,
    fail_execute: Option<RuntimeError>,
    fail_cancel: Option<RuntimeError>,
    close_calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl ScriptedPort {
    fn new(view: SessionView) -> Self {
        Self {
            view,
            fail_execute: None,
            fail_cancel: None,
            close_calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    fn failing_execute(mut self, err: RuntimeError) -> Self {
        self.fail_execute = Some(err);
        self
    }

    fn failing_cancel(mut self, err: RuntimeError) -> Self {
        self.fail_cancel = Some(err);
        self
    }

    /// §6.3 末段：`dbSessionId` 与 `runtimeEpoch` 必须逐字段同时相等。
    fn locate(&self, handle: &SessionHandle) -> Result<(), RuntimeError> {
        if self.view.handle != *handle {
            return Err(RuntimeError::UnknownSession(
                handle.db_session_id.as_str().to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl SessionPort for ScriptedPort {
    async fn session_view(&self, handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
        self.locate(handle)?;
        // 终态不短路：`Closed` / `Lost` 是可读终态（`port.rs:44-45`）。
        Ok(self.view.clone())
    }

    async fn execute_in_session(
        &self,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        self.locate(&request.handle)?;
        if let Some(err) = &self.fail_execute {
            return Err(err.clone());
        }
        if request.expected_context_revision != self.view.context_revision {
            return Err(RuntimeError::ContextRevisionMismatch {
                expected: request.expected_context_revision.get(),
                actual: self.view.context_revision.get(),
            });
        }
        Ok(ExecutionReceipt {
            execution_id: ExecutionId::new("exec_seam"),
            stream_id: StreamId::new("stream_seam"),
            state: ExecutionState::Queued,
        })
    }

    async fn cancel_execution(
        &self,
        handle: &SessionHandle,
        _execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError> {
        self.locate(handle)?;
        if let Some(err) = &self.fail_cancel {
            return Err(err.clone());
        }
        Ok(ExecutionState::CancelRequested)
    }

    async fn close_session(
        &self,
        handle: &SessionHandle,
        mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        self.close_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.locate(handle)?;
        // 幂等分支先于事务分支（`port.rs:87-90`）：§6.1 的「Lost → Closed 幂等」
        // 意味着清理已完成的会话重复 close 一律 `Ok(())`，不再重新判定事务。
        if matches!(self.view.state, SessionState::Closed | SessionState::Lost) {
            return Ok(());
        }
        if mode == CloseMode::RequireNoTransaction
            && self.view.observed_context.transaction_state != TransactionState::None
        {
            return Err(RuntimeError::CloseRejected("activeTransaction"));
        }
        Ok(())
    }
}

fn handle_of(db_session_id: &str, epoch: u64) -> SessionHandle {
    SessionHandle {
        db_session_id: DbSessionId::new(db_session_id),
        runtime_epoch: Counter::new(epoch),
    }
}

fn view_with(
    state: SessionState,
    transaction: TransactionState,
    epoch: u64,
    revision: u64,
) -> SessionView {
    let namespace = NamespaceTarget::unknown_placeholder();
    SessionView {
        handle: handle_of("db_session_seam", epoch),
        connection_id: ConnectionId::new("conn_1"),
        config_revision: ConfigRevision::new(7),
        owner: OwnerRef::Editor {
            organization_id: OrganizationId::new("org_1"),
            principal_id: PrincipalId::new("pr_1"),
            connection_id: ConnectionId::new("conn_1"),
            client_instance_id: ClientInstanceId::new("cli-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        },
        initial_target: ExecutionTarget {
            connection_id: ConnectionId::new("conn_1"),
            namespace: namespace.clone(),
            object: None,
        },
        observed_context: SessionContext {
            transaction_state: transaction,
            ..SessionContext::new(namespace, "app_user")
        },
        context_revision: Counter::new(revision),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: Timestamp::new("2026-01-01T00:30:00Z"),
    }
}

fn request_of(handle: &SessionHandle, expected: u64) -> ExecuteInSessionRequest {
    ExecuteInSessionRequest {
        handle: handle.clone(),
        expected_context_revision: Counter::new(expected),
        call: CommandCall {
            command: "query".to_owned(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
        },
        idempotency_key: "idem-seam".to_owned(),
    }
}

fn port_of(view: SessionView) -> Arc<dyn SessionPort> {
    Arc::new(ScriptedPort::new(view))
}

/// `port.rs:44-45`：终态会话**仍然返回 `Ok`**。自带单测只覆盖了 `Closed`；
/// `Lost` 同样是可读终态，漏掉它等于没验证「区分重建 vs 重试」这句话。
#[tokio::test]
async fn terminal_sessions_are_still_readable_including_lost() {
    for state in [SessionState::Lost, SessionState::Closed] {
        let port = port_of(view_with(state, TransactionState::None, 3, 12));
        let view = port
            .session_view(&handle_of("db_session_seam", 3))
            .await
            .unwrap_or_else(|e| panic!("{state:?} 是可读终态，不得变成 Err：{e:?}"));
        assert_eq!(view.state, state, "读回的状态必须与登记态一致");
        assert_eq!(view.handle, handle_of("db_session_seam", 3));
    }
}

/// `port.rs:87-88` + §6.1「Lost → Closed 幂等」：`Lost` 会话上的重复 close
/// 必须返回 `Ok(())`，而不是 `CloseRejected` 或 `UnknownSession`。
#[tokio::test]
async fn close_is_idempotent_on_a_lost_session() {
    let concrete = ScriptedPort::new(view_with(
        SessionState::Lost,
        TransactionState::Unknown,
        3,
        12,
    ));
    let port: Arc<dyn SessionPort> = Arc::new(concrete);
    let handle = handle_of("db_session_seam", 3);

    for _ in 0..3 {
        assert_eq!(
            port.close_session(&handle, CloseMode::RequireNoTransaction)
                .await,
            Ok(()),
            "Lost 会话的重复 close 必须幂等返回 Ok(())"
        );
    }
}

/// `port.rs:95-97` 列的阻塞事务态是 `Active` / `Aborted` / `Unknown` 三个；
/// 自带单测只跑了前者和第三者，`Aborted` 是漏网的那一个。
#[tokio::test]
async fn close_rejects_on_every_blocking_transaction_state() {
    for transaction in [
        TransactionState::Active,
        TransactionState::Aborted,
        TransactionState::Unknown,
    ] {
        let concrete = ScriptedPort::new(view_with(SessionState::Ready, transaction, 3, 12));
        let close_calls = concrete.close_calls.clone();
        let port: Arc<dyn SessionPort> = Arc::new(concrete);
        let handle = handle_of("db_session_seam", 3);

        assert_eq!(
            port.close_session(&handle, CloseMode::RequireNoTransaction)
                .await,
            Err(RuntimeError::CloseRejected("activeTransaction")),
            "{transaction:?} 属于阻塞事务态，必须把处置权交还调用方"
        );
        assert_eq!(
            port.session_view(&handle).await.unwrap().state,
            SessionState::Ready,
            "{transaction:?} 下被拒的 close 不得关闭会话"
        );
        assert_eq!(
            close_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "被拒的 close 只尝试一次，事务判定必须在方法内完成"
        );
    }
}

/// `port.rs:58`：预算耗尽与隔离是 `execute_in_session` 的两条失败条件，
/// 但仓库里**没有任何代码构造过它们**。这里让它们穿过 trait object，
/// 并钉住对外路由（`§13` 的 `ResourceBusy` / `SessionLost`）。
#[tokio::test]
async fn budget_and_quarantine_rejections_cross_the_trait_object() {
    let handle = handle_of("db_session_seam", 3);
    for (err, code) in [
        (
            RuntimeError::BudgetExhausted("orgQuota"),
            ApiErrorCode::ResourceBusy,
        ),
        (
            RuntimeError::SessionQuarantined("inUse"),
            ApiErrorCode::SessionLost,
        ),
    ] {
        let port: Arc<dyn SessionPort> = Arc::new(
            ScriptedPort::new(view_with(
                SessionState::Ready,
                TransactionState::None,
                3,
                12,
            ))
            .failing_execute(err.clone()),
        );
        assert_eq!(
            port.execute_in_session(request_of(&handle, 12)).await,
            Err(err.clone())
        );
        assert_eq!(err.api_code(), Some(code), "{err:?} 的对外路由被改了");
    }
}

/// `port.rs:78`：取消失败**不得**降级成「已取消」。`CancelFailed::api_code()`
/// 刻意返回 `None`（`error.rs:244`）——外层因此拿不到任何请求侧码，
/// 也就无法把一次取消失败伪装成一次合法的取消结果。
#[tokio::test]
async fn cancel_failure_never_degrades_into_a_cancel_outcome() {
    let handle = handle_of("db_session_seam", 3);
    let err = RuntimeError::CancelFailed("driverUnreachable");
    let port: Arc<dyn SessionPort> = Arc::new(
        ScriptedPort::new(view_with(
            SessionState::Executing,
            TransactionState::None,
            3,
            12,
        ))
        .failing_cancel(err.clone()),
    );

    let got = port
        .cancel_execution(&handle, &ExecutionId::new("exec_missing"))
        .await;
    assert_eq!(got, Err(err.clone()));
    // 关键：返回值既不是 `Ok(CancelRequested)`，也不是 `Ok(Cancelled)`。
    assert!(
        !matches!(got, Ok(state) if matches!(state, ExecutionState::CancelRequested | ExecutionState::Cancelled)),
        "取消失败不得返回任何取消态"
    );
    assert_eq!(err.api_code(), None);
    assert_eq!(err.reason(), "cancelFailed");
}

/// `error.rs:244`：`InvariantBroken` 不映射任何请求侧码——它不是业务事实，
/// 编一个 `ApiErrorCode` 等于把一次 bug 伪装成一次合法拒绝。仓库里目前
/// 没有任何生产代码构造它，这里锁定「它出现时也必须如此」。
#[tokio::test]
async fn invariant_broken_has_no_request_side_code() {
    let err = RuntimeError::InvariantBroken("activeExecutionMismatch");
    assert_eq!(err.api_code(), None);
    assert_eq!(err.reason(), "invariantBroken");
    let rejected: Result<u64, RuntimeError> = err.clone().rejected("executeInSession");
    assert_eq!(rejected, Err(err), "rejected() 必须原样交还错误");
}

/// §6.3 末段的逐字段匹配在计数器极值上同样成立：`Counter::new(0)` 与
/// `Counter::new(u64::MAX)` 都是合法句柄，别让边界值退化成「总是匹配」。
#[tokio::test]
async fn counter_boundaries_do_not_alias_on_the_seam() {
    for (registered, probed, must_match) in [
        (0u64, 0u64, true),
        (0, 1, false),
        (u64::MAX, u64::MAX, true),
        (u64::MAX, u64::MAX - 1, false),
        (u64::MAX - 1, u64::MAX, false),
        (1, u64::MAX, false),
    ] {
        let port = port_of(view_with(
            SessionState::Ready,
            TransactionState::None,
            registered,
            12,
        ));
        let probe = handle_of("db_session_seam", probed);
        let got = port.session_view(&probe).await;
        assert_eq!(
            got.is_ok(),
            must_match,
            "登记 epoch={registered} 与探测 epoch={probed} 的命中结果不符 §6.3"
        );
        if let Err(e) = got {
            assert_eq!(
                e,
                RuntimeError::UnknownSession("db_session_seam".to_owned())
            );
        }
    }

    // 同一 epoch、不同 dbSessionId 同样不得命中（逐字段，不是只比其中一个）。
    let port = port_of(view_with(
        SessionState::Ready,
        TransactionState::None,
        7,
        12,
    ));
    assert_eq!(
        port.session_view(&handle_of("db_session_other", 7)).await,
        Err(RuntimeError::UnknownSession("db_session_other".to_owned()))
    );
}

/// `port.rs:57`：`ContextRevisionMismatch` 必须携带服务端实际值，载荷是
/// 裸 `u64`，极值不得被截断或回绕。
#[tokio::test]
async fn context_revision_mismatch_carries_server_actual_at_boundaries() {
    for (expected, actual) in [(0u64, u64::MAX), (u64::MAX, 0), (1, u64::MAX)] {
        let port = port_of(view_with(
            SessionState::Ready,
            TransactionState::None,
            3,
            actual,
        ));
        assert_eq!(
            port.execute_in_session(request_of(&handle_of("db_session_seam", 3), expected))
                .await,
            Err(RuntimeError::ContextRevisionMismatch { expected, actual }),
            "载荷必须逐字带回调用方的 expected 与服务端的 actual"
        );
    }
}

/// `error.rs:209`：`reason()` 是 tracing 事件字段的取值来源。自带单测只断言
/// 非空且互不重复——重命名不会被发现。这里把 9 个字面量钉死。
#[test]
fn reason_literals_are_pinned() {
    let pairs = [
        (RuntimeError::UnknownSession("s".into()), "unknownSession"),
        (RuntimeError::SessionClosed("s".into()), "sessionClosed"),
        (RuntimeError::SessionLost("s".into()), "sessionLost"),
        (
            RuntimeError::ContextRevisionMismatch {
                expected: 1,
                actual: 2,
            },
            "contextRevisionMismatch",
        ),
        (RuntimeError::BudgetExhausted("b".into()), "budgetExhausted"),
        (
            RuntimeError::SessionQuarantined("q".into()),
            "sessionQuarantined",
        ),
        (RuntimeError::CancelFailed("c".into()), "cancelFailed"),
        (RuntimeError::CloseRejected("c".into()), "closeRejected"),
        (RuntimeError::InvariantBroken("i".into()), "invariantBroken"),
    ];
    for (err, literal) in &pairs {
        assert_eq!(err.reason(), *literal, "{err:?} 的 reason 字面量被改了");
    }
    // `api_code()` 刻意把 `UnknownSession` 与 `SessionClosed` 折叠成同一个码，
    // 但 `reason()` 是内部分诊词表，必须保留这个区分——否则日志里「会话没找到」
    // 与「会话已关闭」将无法分辨。注意 `SessionLost` 两边字面量碰巧同名
    // （均为 "sessionLost"），属命名巧合，不构成同一命名空间。
    for (err, collapsed) in [
        (
            RuntimeError::UnknownSession("s".into()),
            ApiErrorCode::SessionNotFound,
        ),
        (
            RuntimeError::SessionClosed("s".into()),
            ApiErrorCode::SessionNotFound,
        ),
    ] {
        assert_eq!(err.api_code(), Some(collapsed));
        assert_ne!(
            err.reason(),
            collapsed.as_str(),
            "{err:?} 的 reason 不得与被折叠掉的请求侧码同形"
        );
    }
}

/// `rejected()` 对**全部 9 个变体**都必须原样交还错误（含载荷），
/// 且 `T` 不限于 `()`——`session_view` 这类读路径的返回类型是 `SessionView`。
#[tokio::test]
async fn rejected_preserves_every_variant_verbatim() {
    let errs = [
        RuntimeError::UnknownSession("db_session_1".into()),
        RuntimeError::SessionClosed("db_session_1".into()),
        RuntimeError::SessionLost("db_session_1".into()),
        RuntimeError::ContextRevisionMismatch {
            expected: u64::MAX,
            actual: 0,
        },
        RuntimeError::BudgetExhausted("orgQuota".into()),
        RuntimeError::SessionQuarantined("inUse".into()),
        RuntimeError::CancelFailed("driverUnreachable".into()),
        RuntimeError::CloseRejected("activeTransaction".into()),
        RuntimeError::InvariantBroken("activeExecutionMismatch".into()),
    ];
    for err in errs {
        let with_view: Result<SessionView, RuntimeError> = err.clone().rejected("session_view");
        assert_eq!(with_view, Err(err.clone()), "rejected() 不得改写错误或载荷");
        let with_unit: Result<(), RuntimeError> = err.clone().rejected("closeSession");
        assert_eq!(with_unit, Err(err.clone()));
    }
}
