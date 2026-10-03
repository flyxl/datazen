//! `SessionPort` —— registry 接缝的契约本体（shared-boundaries-and-ports.md §4 的端口约定）。
//!
//! **为什么是 trait 而不是具体类型**：P3 Wave 2 的 `p3-registry` 负责实现它，
//! `p3-gateway` 负责消费它。两条轨道并行开发时唯一的共同前提就是这一个 trait 的签名，
//! 所以本文件里**不允许**出现任何实现体、任何结构体状态、任何 `tokio` 运行时对象。
//!
//! **四条纪律**（缺一条就是接缝失效，不要为了图省事放宽）：
//!
//! 1. **签名里只有冻结类型**：`crate::connection::*` 的 DTO 与 `datazen_platform_api` 的 ID
//!    newtype。禁止裸 `String` 当 id（§4），禁止引用资源层租约/预算协调器的**实现**。
//! 2. **只报事实，不做判定**：本端口返回 `RuntimeError`，由更外层决定它是
//!    `ApiError`/`ExecutionErrorCode` 还是根本不该上线路（见 `RuntimeError::api_code`）。
//! 3. **不泄露内部结构**：不返回 actor 邮箱、不返回登记表的锁、不返回资源句柄。
//! 4. **失败不留半状态**：任何 `Err` 都意味着这次调用对 registry 的可见状态**没有**改变，
//!    或者改变已被本调用自己撤销。调用方据此可以直接重试而不必先查询。
//!
//! 拒绝语义的权威来源是 connection-management.md §6.1（状态机）、§6.3（锁与队列）、
//! §7.2/§7.5/§7.6（三个方法的处理顺序）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::connection::{
    CloseMode, ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState,
    RuntimeError, SessionHandle, SessionView,
};

/// 会话登记表的会话面 —— Wave 2 实现它，gateway 消费它。
///
/// 对象安全的硬性要求：`Arc<dyn SessionPort>` 是唯一允许的持有形态
/// （`SessionPort: Send + Sync + 'static` + `#[async_trait]` 共同保证）。
/// 因此**禁止**给本 trait 加泛型方法、返回 `Self` 的方法或 `Sized` 约束的方法。
#[async_trait]
pub trait SessionPort: Send + Sync + 'static {
    /// 读取会话当前投影（§6.1 的状态、§6.4 的 attachment 与到期点、§6.3 的 `contextRevision`）。
    ///
    /// 这是唯一的读路径：调用方判断「能不能执行」必须以本方法的返回为准，
    /// 而不是从自己缓存的 `SessionView` 推。§12.1 要求旧 session 的事件不得更新新会话，
    /// 因此返回的 `handle` 必须与传入的句柄**逐字段相等**，实现方不得改写它。
    ///
    /// 失败条件：`RuntimeError::UnknownSession`（句柄不在表中，或 `runtime_epoch`
    /// 与当前登记不符——陈旧 epoch 一律按未知句柄处理，不能泄露「资源被替换过」）。
    /// 终态会话**仍然返回 `Ok`**：`Closed`/`Lost` 是可读的终态，调用方要靠它才能
    /// 区分「重建会话」和「重试同一会话」，把它们变成 `Err` 会逼调用方盲猜。
    async fn session_view(&self, handle: &SessionHandle) -> Result<SessionView, RuntimeError>;

    /// 在既有会话内提交一次执行（§7.2 全部 11 步）。
    ///
    /// 返回的是**接受回执**，不是执行结果：`Ok(receipt)` 只表示请求已受理、
    /// `executionId` 已生成（§7.2 第 3 步），`SQL` 是否成功要等终态事件或 `getExecution`。
    ///
    /// `request.expected_context_revision` 是乐观并发闸门（§6.3 末段）：实现方必须在
    /// **真正执行前**再比一次，不能只在入队时比一次。
    ///
    /// 失败条件：`UnknownSession`；`SessionLost` / `SessionClosed`（§7.2 第 5 步的
    /// 「明确失败」）；`ContextRevisionMismatch`（携带服务端实际值，供调用方重读后由用户重发，
    /// 见 §13 `ContextConflict` 的调用者动作）；`BudgetExhausted`；`SessionQuarantined`。
    async fn execute_in_session(
        &self,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError>;

    /// 请求取消一次执行（§7.6），返回**请求落地后的执行状态快照**。
    ///
    /// 返回 `Ok(ExecutionState::CancelRequested)` 只表示「取消已被登记」，
    /// 不表示执行已取消——终态只能由终态事件或 `getExecution` 给出（§7.6 末段）。
    ///
    /// **与冻结 DTO 的已知偏差（Wave 1 必须补齐）**：§7.6 规定对外返回
    /// `CancelReceipt { executionId, disposition, state }`，其中 `disposition` 能表达
    /// `requested` / `unsupported` / `alreadyFinished` 三种不同处置。当前冻结类型里
    /// 没有对应 DTO，本端口因此退回返回 `ExecutionState`：无法区分「driver 不支持取消」
    /// 与「已经是终态」。Wave 1 落地 `CancelReceipt` 后应把本方法返回值换成它，
    /// **不要**在 Wave 2 里就地私造一个结构体绕过去。
    ///
    /// 失败条件：`UnknownSession`；`CancelFailed`（取消绑定与
    /// `executionId`/`runtimeEpoch`/`resourceBindingId` 对不上，或 driver 独立控制路径不可达）。
    /// 取消失败**不得**降级成「已取消」。
    async fn cancel_execution(
        &self,
        handle: &SessionHandle,
        execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError>;

    /// 关闭会话（§7.5 全部 7 步）。
    ///
    /// **必须幂等**：对已经 `Closed`（或已 `Lost` 后完成清理）的会话重复调用返回 `Ok(())`，
    /// 不返回错误也不重复执行回滚/关闭（§6.1「Lost → Closed 幂等」、§7.5 第 1 步
    /// 「幂等查 tombstone」）。调用方在超时重试后无法区分这两种情况，
    /// 非幂等的 close 会让每一次网络重试都可能变成一次重复回滚。
    ///
    /// `mode` 的语义由调用方承担：`RequireNoTransaction` 表示「有活跃/中止/未知事务就拒绝，
    /// 不要替我决定」；`RollbackAndClose` 表示「先取消运行中执行、等停止、回滚、再关闭」。
    ///
    /// 失败条件：`UnknownSession`；`CloseRejected`（`RequireNoTransaction` 撞上
    /// `Active`/`Aborted`/`Unknown` 事务，对应 §13 `TransactionResolutionRequired`，
    /// **此时会话没有被关闭**，调用方可把它交给用户选择处理方式）。
    async fn close_session(
        &self,
        handle: &SessionHandle,
        mode: CloseMode,
    ) -> Result<(), RuntimeError>;
}

/// 接缝形状的**编译期**护栏：把 `Arc<dyn SessionPort>` 写进签名，
/// 一次编译就证明这个 trait 仍然对象安全、仍然满足 `Send + Sync + 'static`。
///
/// 这不是「顺手加的类型检查」而是接缝契约本身：Wave 2 的 registry 与 gateway
/// 都只会以 trait 对象形态持有本端口，如果对象安全在某次改动里破了，
/// 应当在这里、在 Wave 2 接入之前失败，而不是让它作为潜伏问题漂过两个轨道。
///
/// `dead_code` 是有意 `allow` 的：护栏不需要被调用，它的价值全在编译通过。
#[allow(dead_code)]
fn assert_object_safe(port: Arc<dyn SessionPort>) -> Arc<dyn SessionPort> {
    port
}

#[cfg(test)]
mod tests {
    use tokio::sync::Mutex;

    use super::*;
    use crate::connection::types::{ClientInstanceId, EditorSessionId};
    use crate::connection::{
        AttachmentState, CommandCall, ConfigRevision, ConnectionId, Counter, DbSessionId,
        ExecutionTarget, NamespaceTarget, OrganizationId, OwnerRef, PrincipalId, SessionContext,
        SessionState, StreamId, Timestamp, TransactionState,
    };

    /// 最小 fake：只实现本 trait 的四个方法，不建连、不碰 driver、不排 actor 队列。
    ///
    /// 它只复现接缝层自己承诺的四条规则——句柄逐字段匹配（含 `runtime_epoch`）、
    /// `contextRevision` 乐观闸门、终态明确失败、关闭幂等。
    /// 资源层、预算、actor 串行化都属于 Wave 2 的实现，不在这里提前代写一遍：
    /// 代写的实现会成为第二份事实来源，而它必然与 Wave 2 漂移。
    struct FakeSessionPort {
        inner: Mutex<FakeState>,
    }

    struct FakeState {
        view: SessionView,
        executions: Vec<(ExecutionId, ExecutionState)>,
        next_execution: u64,
    }

    impl FakeSessionPort {
        fn new(view: SessionView) -> Self {
            Self {
                inner: Mutex::new(FakeState {
                    view,
                    executions: Vec::new(),
                    next_execution: 1,
                }),
            }
        }

        /// §6.1：`Lost`/`Closed` 是两种判定依据不同的终态，必须分别报出，
        /// 不能塌成一个「会话不可用」。塌了调用方就分不清「重建即可」和「先核验资源」。
        fn terminal_error(view: &SessionView) -> Option<RuntimeError> {
            let id = view.handle.db_session_id.as_str().to_owned();
            match view.state {
                SessionState::Lost => Some(RuntimeError::SessionLost(id)),
                SessionState::Closed => Some(RuntimeError::SessionClosed(id)),
                _ => None,
            }
        }
    }

    /// §6.3 末段：客户端 `dbSessionId` **与** `runtimeEpoch` 必须精确匹配。
    /// 陈旧 epoch 按未知句柄处理——不泄露「资源被换过一次」，也不给旧句柄留后门。
    fn locate(view: &SessionView, handle: &SessionHandle) -> Result<(), RuntimeError> {
        if view.handle != *handle {
            return RuntimeError::UnknownSession(handle.db_session_id.as_str().to_owned())
                .rejected("locate");
        }
        Ok(())
    }

    fn handle_of(db_session_id: &str, epoch: u64) -> SessionHandle {
        SessionHandle {
            db_session_id: DbSessionId::new(db_session_id),
            runtime_epoch: Counter::new(epoch),
        }
    }

    /// 造一个就绪会话。参数化 `transaction` 让测试能分别表达「无事务」与「事务状态未知」。
    fn ready_view(transaction: TransactionState) -> SessionView {
        let namespace = NamespaceTarget::unknown_placeholder();
        SessionView {
            handle: handle_of("db_session_1", 3),
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
            context_revision: Counter::new(12),
            state: SessionState::Ready,
            attachment_state: AttachmentState::Attached,
            active_execution_id: None,
            expires_at: Timestamp::new("2026-01-01T00:30:00Z"),
        }
    }

    fn request(expected_context_revision: u64) -> ExecuteInSessionRequest {
        ExecuteInSessionRequest {
            handle: handle_of("db_session_1", 3),
            expected_context_revision: Counter::new(expected_context_revision),
            call: CommandCall {
                command: "query".to_owned(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
            },
            idempotency_key: "idem-1".to_owned(),
        }
    }

    #[async_trait]
    impl SessionPort for FakeSessionPort {
        async fn session_view(&self, handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
            let state = self.inner.lock().await;
            locate(&state.view, handle)?;
            Ok(state.view.clone())
        }

        async fn execute_in_session(
            &self,
            request: ExecuteInSessionRequest,
        ) -> Result<ExecutionReceipt, RuntimeError> {
            let mut state = self.inner.lock().await;
            locate(&state.view, &request.handle)?;
            if let Some(terminal) = Self::terminal_error(&state.view) {
                return terminal.rejected("executeInSession");
            }
            // §7.2 第 4 步：真正执行前再比一次 revision，只在入队时比一次是不够的。
            if request.expected_context_revision != state.view.context_revision {
                return RuntimeError::ContextRevisionMismatch {
                    expected: request.expected_context_revision.get(),
                    actual: state.view.context_revision.get(),
                }
                .rejected("executeInSession");
            }
            let execution_id = ExecutionId::new(format!("exec_{}", state.next_execution));
            state.next_execution += 1;
            // 接受即登记（§7.2 第 3 步）：交回回执时 active 标记已经落位。
            state.view.state = SessionState::Executing;
            state.view.active_execution_id = Some(execution_id.clone());
            state
                .executions
                .push((execution_id.clone(), ExecutionState::Queued));
            Ok(ExecutionReceipt {
                execution_id,
                stream_id: StreamId::new("stream_1"),
                state: ExecutionState::Queued,
            })
        }

        async fn cancel_execution(
            &self,
            handle: &SessionHandle,
            execution_id: &ExecutionId,
        ) -> Result<ExecutionState, RuntimeError> {
            let mut state = self.inner.lock().await;
            locate(&state.view, handle)?;
            let slot = state
                .executions
                .iter_mut()
                .find(|(id, _)| id == execution_id)
                .ok_or(RuntimeError::CancelFailed("unboundExecution"))?;
            // §7.6：终态优先。已终结的执行再取消不得被改写成 CancelRequested。
            let is_terminal = matches!(
                slot.1,
                ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
            );
            if !is_terminal {
                slot.1 = ExecutionState::CancelRequested;
            }
            Ok(slot.1)
        }

        async fn close_session(
            &self,
            handle: &SessionHandle,
            mode: CloseMode,
        ) -> Result<(), RuntimeError> {
            let mut state = self.inner.lock().await;
            locate(&state.view, handle)?;
            // §7.5 第 1 步：关闭态不再接受新请求，但 close 本身幂等。
            if state.view.state == SessionState::Closed {
                return Ok(());
            }
            // §7.5 第 2 步：`Unknown` 事务同样挡住 requireNoTransaction。
            // 把 Unknown 当成「无事务」放行，等于拿一句「已回滚」的谎话换一次成功返回。
            if mode == CloseMode::RequireNoTransaction
                && state.view.observed_context.transaction_state != TransactionState::None
            {
                return RuntimeError::CloseRejected("activeTransaction").rejected("closeSession");
            }
            state.view.state = SessionState::Closed;
            Ok(())
        }
    }

    /// A3 接缝的**非平凡**验收：既证明 `Arc<dyn SessionPort>` 这条唯一持有形态成立、
    /// 四个方法都能穿过 trait 对象真的跑起来，也证明每条失败路径符合 §6/§7 的承诺。
    #[tokio::test]
    async fn session_port_dispatches_through_the_trait_object_and_rejects_by_contract() {
        // 1) 对象安全 + 构造：下游两条轨道只能以 trait 对象形态持有 registry。
        let fake = Arc::new(FakeSessionPort::new(ready_view(TransactionState::None)));
        let port: Arc<dyn SessionPort> = fake;
        let handle = handle_of("db_session_1", 3);

        // 2) 读路径：句柄原样回投影，调用方据此判断能不能执行。
        let view = port
            .session_view(&handle)
            .await
            .expect("已登记句柄必须可读");
        assert_eq!(view.handle, handle);
        assert_eq!(view.context_revision.get(), 12);
        assert_eq!(view.state, SessionState::Ready);
        assert_eq!(view.active_execution_id, None, "空闲会话不得有活动执行");

        // 3) 陈旧 epoch 一律按未知句柄处理（§6.3 末段）。
        assert_eq!(
            port.session_view(&handle_of("db_session_1", 2)).await,
            Err(RuntimeError::UnknownSession("db_session_1".to_owned())),
            "陈旧 runtimeEpoch 不得命中当前登记"
        );

        // 4) 乐观闸门：revision 不符必须被拒，且**不留半状态**——
        //    这是接缝纪律第 4 条，也是「失败后可安全重试」的前提。
        assert_eq!(
            port.execute_in_session(request(11)).await,
            Err(RuntimeError::ContextRevisionMismatch {
                expected: 11,
                actual: 12,
            })
        );
        let after_reject = port.session_view(&handle).await.expect("拒绝后仍可读");
        assert_eq!(
            after_reject.active_execution_id, None,
            "被拒的执行不得留下 active 标记"
        );
        assert_eq!(
            after_reject.state,
            SessionState::Ready,
            "被拒的执行不得把会话推进到 Executing"
        );

        // 5) revision 对齐才受理；回执只表示「已接受」（§7.2 第 3 步）。
        let receipt = port
            .execute_in_session(request(12))
            .await
            .expect("revision 对齐应受理");
        assert_eq!(receipt.state, ExecutionState::Queued);
        let executing = port.session_view(&handle).await.expect("受理后仍可读");
        assert_eq!(executing.state, SessionState::Executing);
        assert_eq!(
            executing.active_execution_id,
            Some(receipt.execution_id.clone()),
            "交回回执时 active 执行必须已经登记"
        );

        // 6) 取消只登记意图，终态仍由终态事件给出（§7.6 末段）。
        assert_eq!(
            port.cancel_execution(&handle, &receipt.execution_id).await,
            Ok(ExecutionState::CancelRequested)
        );
        assert_eq!(
            port.cancel_execution(&handle, &receipt.execution_id).await,
            Ok(ExecutionState::CancelRequested),
            "重复取消必须幂等，不得改写已有状态"
        );

        // 7) 取消绑定对不上就是取消失败，绝不能降级成「已取消」。
        assert_eq!(
            port.cancel_execution(&handle, &ExecutionId::new("exec_missing"))
                .await,
            Err(RuntimeError::CancelFailed("unboundExecution"))
        );

        // 8) 关闭幂等（§6.1 / §7.5 第 1 步）：重复调用不重复执行回滚。
        port.close_session(&handle, CloseMode::RollbackAndClose)
            .await
            .expect("回滚关闭应成功");
        port.close_session(&handle, CloseMode::RequireNoTransaction)
            .await
            .expect("终态会话上的重复 close 必须幂等返回 Ok");
        assert_eq!(
            port.session_view(&handle).await.expect("终态仍可读").state,
            SessionState::Closed
        );
    }

    /// §7.5 第 2 步：`requireNoTransaction` 撞上活跃/未知事务必须拒绝且**不关闭**，
    /// 把处置权交还调用方；`rollbackAndClose` 才是「我决定，帮我回滚」。
    #[tokio::test]
    async fn close_hands_the_transaction_decision_back_to_the_caller() {
        for transaction in [TransactionState::Active, TransactionState::Unknown] {
            let port: Arc<dyn SessionPort> =
                Arc::new(FakeSessionPort::new(ready_view(transaction)));
            let handle = handle_of("db_session_1", 3);

            assert_eq!(
                port.close_session(&handle, CloseMode::RequireNoTransaction)
                    .await,
                Err(RuntimeError::CloseRejected("activeTransaction")),
                "{transaction:?} 下必须拒绝，而不是替调用者决定"
            );
            assert_eq!(
                port.session_view(&handle)
                    .await
                    .expect("拒绝后仍可读")
                    .state,
                SessionState::Ready,
                "{transaction:?} 下被拒的 close 不得关闭会话"
            );

            port.close_session(&handle, CloseMode::RollbackAndClose)
                .await
                .expect("显式回滚关闭应成功");
            assert_eq!(
                port.session_view(&handle).await.expect("终态仍可读").state,
                SessionState::Closed
            );
        }
    }

    /// §7.2 第 5 步：`Lost` 与 `Closed` 都必须**明确失败**，且是两种不同的失败——
    /// 调用方要靠这个区别决定「先核验资源再重建」还是「直接重建」。
    #[tokio::test]
    async fn terminal_sessions_fail_execution_with_distinct_reasons() {
        for (state, expected) in [
            (
                SessionState::Lost,
                RuntimeError::SessionLost("db_session_1".to_owned()),
            ),
            (
                SessionState::Closed,
                RuntimeError::SessionClosed("db_session_1".to_owned()),
            ),
        ] {
            let mut view = ready_view(TransactionState::None);
            view.state = state;
            let port: Arc<dyn SessionPort> = Arc::new(FakeSessionPort::new(view));
            assert_eq!(
                port.execute_in_session(request(12)).await,
                Err(expected),
                "{state:?} 必须明确失败，且原因可与另一种终态区分"
            );
        }
    }
}
