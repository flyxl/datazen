//! 八个操作的实现（fake-runtime-fixtures.md §3.1、§3.2、§4.1、§5.1）。
//! 第九个操作 `closeResource` 住在同目录的 `close.rs`（§3.1 逐操作校验不变）。
//!
//! 每个操作都遵守两条硬规则：
//!
//! 1. **§3.1**：`ResourceHandle` 只由提供方签发，且**每个操作**都要校验
//!    `resourceId` + `runtimeEpoch` + owner —— 入口统一走
//!    `FakeResourceProvider::resolve`（见 `handles` 子模块）。
//! 2. **§3.2**：状态迁移必须诚实。`resetResource` 返回 `Clean` **不等于**事务已终结；
//!    提交不可判定时 `effectOutcome` 必须是 `Unknown`，**永远不能**是 `Completed`。
//!
//! 故障注入由 `FakeScript` 驱动，且**只改返回值**：状态迁移与 journal 写入照常发生，
//! 所以注入故障后的台账与基线台账走完全相同的 §5.3 断言路径 —— 这正是
//! `fake_resource::tests::the_change_point_rules_stay_expressible_under_injected_faults`
//! 要证明的事情（R4：故障归 `fake_resource` 所有的依据）。
//!
//! 全程**没有任何 sleep**：超时窗口挂在假单调时钟上；顺序只由 journal 的 `seq` 判定（§5 L269）。
//!
//! `ProviderError::UnsupportedPlan` 是**无载荷**变体（§3.1 的错误面刻意收敛），
//! 所以「脚本把别的操作的故障排到了本操作上」这类内部错误只能报 `UnsupportedPlan`，
//! 排错要打印脚本本身（`FaultKind::catalog_id()` + `ResourceOp::as_str()`），
//! **不要**把故障细节拼进生产错误消息。

use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{
    AcquireResourceRequest, CancelDisposition, CancelReceipt, ChangeContextOutcome,
    ChangeContextRequest, CompletionStatus, ConnectionCostPolicy, DescribeResourceRequest,
    ExecuteOnResourceRequest, ExecutionCompletion, ObserveSessionRequest, OpenSessionReceipt,
    PermitReason, RequestCancelRequest, ResetDiscardReason, ResetOutcome, ResetResourceRequest,
    ResourceDescriptor, ResourceHandle, ResourceHealth, ReusePolicy, SessionContinuity,
    SessionObservation, TransactionObservation,
};
use crate::connection::session::{
    AttachmentState, HandleKind, SessionContext, SessionHandle, SessionState, SessionView,
    TransactionState,
};
use crate::connection::testing::journal::ResourceEvent;
use crate::connection::types::{
    ClientInstanceId, Counter, LeaseId, OwnerRef, ResourceId, StreamId, Timestamp,
};

use super::handles::AcquiredResource;
use super::script::{execution_error_code, FaultKind, ResourceOp};
use super::state::{Accounting, FakeResource, FakeResourceState};
use super::{FakeResourceProvider, PROVIDER_ID};

impl FakeResourceProvider {
    // ---- 1. describeResource ----

    /// §3.1 L414：`describeResource` 返回七项齐全的资源描述符。
    /// fake 的 `sessionContinuity` 固定 `fixed`（一条物理连接 ≡ 一个会话）。
    pub fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ProviderError> {
        // 这个 fake 提供方只服务构造时指定的那一个 `connectionId`（§3.1）。
        if request.target.connection_id.as_str() != self.target.connection_id.as_str() {
            return Err(ProviderError::TargetUnsupported(format!(
                "假提供方只服务 {}，收到 {}",
                self.target.connection_id.as_str(),
                request.target.connection_id.as_str()
            )));
        }
        if let Some((_, kind)) = self.script.take(ResourceOp::Describe) {
            return match kind {
                // F1：派发前拒绝。
                FaultKind::UnsupportedPlan => Err(ProviderError::UnsupportedPlan),
                // 别把别的操作的故障排到 describe 上。
                _ => Err(ProviderError::UnsupportedPlan),
            };
        }
        Ok(self.descriptor())
    }

    /// 假资源的统一描述符。`describeResource` 与 `acquireResource` 用的是同一份，
    /// 对外暴露以便 `FakeHarness` 在组装 `AcquireResourceRequest` 时复用同一口径。
    pub fn descriptor(&self) -> ResourceDescriptor {
        ResourceDescriptor {
            provider_id: PROVIDER_ID.to_owned(),
            resource_key: format!("{}:{}", PROVIDER_ID, self.target.connection_id.as_str()),
            session_continuity: SessionContinuity::Fixed,
            reuse_policy: ReusePolicy::ResetBeforeReturn,
            initialization_requirements: vec!["capabilityProbe".to_owned()],
            connection_cost_policy: ConnectionCostPolicy::PerPhysicalConnection,
            namespace_shape: crate::connection::types::schema_required_shape(),
            capabilities: self.capabilities(),
        }
    }

    // ---- 2. acquireResource ----

    /// §3.1 L130：`acquireResource` 申请资源，**先记账**再建资源。
    ///
    /// §5.3 规则 1：每次 `resourceId` 创建 → permit 余额 +1 且 `live_resources` 同步 +1。
    pub fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
    ) -> Result<OpenSessionReceipt, ProviderError> {
        self.create_resource(request).map(|(_, _, receipt)| receipt)
    }

    /// 夹具侧的便捷包装：除了 `OpenSessionReceipt`，还要能继续操作的 `ResourceHandle`
    /// 与 `PermitSet`。`ResourceHandle` 只是只读校验凭证；真正的会话句柄必须经
    /// `register_handle` 登记（§5.3 规则 5），否则就是孤立句柄（反例，§9.1）。
    pub fn acquire(
        &self,
        request: &AcquireResourceRequest,
    ) -> Result<AcquiredResource, ProviderError> {
        let (resource_id, handle, receipt) = self.create_resource(request)?;
        let resource = self.resolve(&resource_id, &handle, true)?;
        Ok(AcquiredResource {
            resource_id,
            handle,
            permits: resource.permit_set(),
            attachment_token: receipt.attachment_token,
            session: receipt.session,
        })
    }

    fn create_resource(
        &self,
        request: &AcquireResourceRequest,
    ) -> Result<(ResourceId, ResourceHandle, OpenSessionReceipt), ProviderError> {
        // F12 要等资源真的建出来才能造句柄，所以先把注入记下来，建完再落。
        let mut orphan_reason: Option<&'static str> = None;
        if let Some((_, kind)) = self.script.take(ResourceOp::Acquire) {
            match kind {
                // F2：预算占满。窗口挂在假单调时钟上，**不 sleep**。
                FaultKind::BudgetBusy { busy_for, reason } => {
                    let until_nanos = self.now_nanos().saturating_add(busy_for.as_nanos() as u64);
                    let mut busy = self
                        .budget_busy
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    busy.until_nanos = until_nanos;
                    busy.reason = if reason.is_empty() {
                        "budget-busy"
                    } else {
                        reason
                    };
                }
                // F3：连接 + 初始化失败。
                FaultKind::ConnectAndInit { code } => {
                    return Err(ProviderError::ProtocolError(code.to_owned()))
                }
                // F12：句柄登记反例 —— 见下方 `orphan_reason` 的落点。
                FaultKind::HandleNotReturned { reason } => orphan_reason = Some(reason),
                _ => return Err(ProviderError::UnsupportedPlan),
            }
        }
        {
            let busy = self
                .budget_busy
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.now_nanos() < busy.until_nanos {
                let reason = if busy.reason.is_empty() {
                    "budget-busy"
                } else {
                    busy.reason
                };
                return Err(ProviderError::ResourceBusy(reason));
            }
        }

        let resource_id = self.ids.next_resource_id();
        let epoch = self
            .ids
            .next_runtime_epoch(&request.db_session_id, &request.owner);
        let permit_id = self.next_permit_id();
        let context = SessionContext::new(
            self.target.namespace.clone(),
            self.execution_identity.clone(),
        );
        let resource = FakeResource::new(
            resource_id.clone(),
            self.descriptor().resource_key,
            request.pool_key.clone(),
            request.owner.clone(),
            self.target.clone(),
            self.capabilities(),
            Accounting::new(request.budget_class, permit_id.clone()),
            request.db_session_id.clone(),
            epoch.counter,
            context.clone(),
        );
        let handle = ResourceHandle::issue(&resource_id, &epoch.counter, &request.owner);

        self.lock()
            .insert(resource_id.as_str().to_owned(), resource);

        // 台账顺序：permit +1 → Created → OpeningReady。
        self.journal
            .record_permit(&permit_id, 1, PermitReason::Acquire, request.budget_class);
        for event in [ResourceEvent::Created, ResourceEvent::OpeningReady] {
            self.journal.record_resource_event(
                &resource_id,
                event,
                &request.owner,
                request.pool_key.clone(),
                request.budget_class,
            );
        }
        self.journal.register_active_session(&request.db_session_id);

        // F12：句柄造出来了，runtime **拒绝**把它交给宿主 —— `acquire` 照常成功
        // （句柄登记不是 acquire 的失败条件），但假侧只落一条 `orphaned`，
        // 宿主登记册里永远不会有它。§4.2 F12 ⇒ I7 在关闭回收之前必然不成立。
        if let Some(reason) = orphan_reason {
            let execution_id = self.ids.next_execution_id(&request.db_session_id);
            let handle_id =
                crate::connection::types::HandleId::new(format!("hdl_{}", execution_id.as_str()));
            self.orphan_handle(&resource_id, HandleKind::Transaction, handle_id, reason)?;
        }

        let session = self.session_view(&resource_id, epoch.counter, context);
        Ok((
            resource_id,
            handle,
            OpenSessionReceipt {
                session,
                attachment_token: self.ids.attachment_token(),
            },
        ))
    }

    // ---- 3. executeOnResource ----

    /// §5.1 L416：九项齐全，且**如实**报告本次执行的结论。
    pub fn execute_on_resource(
        &self,
        request: &ExecuteOnResourceRequest,
    ) -> Result<ExecutionCompletion, ProviderError> {
        // F12 必须**在 `resolve` 之前**取脚本并改写句柄：跨 epoch 复用要死在
        // `verify` 的 epoch 门闸上。取晚了 / 改错了字段，失败会落到 resourceId
        // 或 owner 那一关，测到的就不再是 epoch 门闸。
        let fault = self.script.take(ResourceOp::Execute).map(|(_, kind)| kind);
        // 只改 `runtimeEpoch`，`resourceId` 与 `ownerToken` 原样带着 ——
        // `verify` 的顺序是 resourceId → epoch → owner，所以必然停在 epoch 这一关。
        let presented = if matches!(fault.as_ref(), Some(FaultKind::CrossEpochHandleReuse)) {
            ResourceHandle {
                resource_id: request.handle.resource_id.clone(),
                runtime_epoch: Counter(request.handle.runtime_epoch.0.saturating_sub(1)),
                owner_token: request.handle.owner_token.clone(),
            }
        } else {
            request.handle.clone()
        };
        let resource = self.resolve(&presented.resource_id, &presented, false)?;

        // F1：派发前拒绝 —— **不**写 execution 终态，因为根本没有派发过（§4.2 F1）。
        if let Some(FaultKind::UnsupportedPlan) = fault {
            return Err(ProviderError::UnsupportedPlan);
        }

        let started = self.journal.record_execution_started(
            &resource.resource_id,
            &request.execution_id,
            request.command_id.as_str(),
        );
        {
            let key = request.handle.resource_id.as_str().to_owned();
            let mut resources = self.lock();
            if let Some(slot) = resources.get_mut(&key) {
                slot.state = FakeResourceState::Executing;
                slot.execution_seq = slot.next_execution_seq();
            }
        }

        // F4 / F5：派发**之后**失败 → 必须有终态记录
        // （§5.3「每次执行终态 protocolDrained 已记录或显式置 false」）。
        //
        // `errorCode` 与 `effectOutcome` 是两个独立命名空间（§4.2 F4），所以这里按**故障发生
        // 在哪一步**分别取值，而不是共用一个判定：
        //  - `StatementDispatch`：语句还没送出去，作用域**未开始** → `notStarted`；
        //  - `ResultTransport`：语句已在服务端执行、结果读不回来 → `unknown`；
        //  - 若 `errorCode` 本身属于不可判定类（超时 / 协议错误 / 取消 / 连接丢失），
        //    一律压成 `unknown`，禁止回填 `completed` / `rolledBack`（CM-44、CM-47，§13 L774）。
        let (completion_status, error_code, effect_outcome) = match &fault {
            Some(FaultKind::StatementDispatch { code }) => {
                let code = execution_error_code(code);
                let outcome = if EffectOutcome::is_undecidable(Some(code)) {
                    EffectOutcome::Unknown
                } else {
                    EffectOutcome::NotStarted
                };
                (CompletionStatus::Error, Some(code), outcome)
            }
            Some(FaultKind::ResultTransport { code }) => {
                let code = execution_error_code(code);
                (CompletionStatus::Error, Some(code), EffectOutcome::Unknown)
            }
            Some(_) => return Err(ProviderError::UnsupportedPlan),
            None => (CompletionStatus::Ok, None, EffectOutcome::Completed),
        };
        self.journal
            .record_execution_terminal(started, effect_outcome, error_code, true, None);
        {
            let key = request.handle.resource_id.as_str().to_owned();
            let mut resources = self.lock();
            if let Some(slot) = resources.get_mut(&key) {
                slot.state = FakeResourceState::Ready;
                slot.last_error_code = error_code;
            }
        }

        // 事务态是 `Unknown` 时，观测结论也必须是 `unknown`，不能报「无事务」。
        let transaction_observation = if resource.transaction_state == TransactionState::Unknown {
            TransactionObservation::Unknown
        } else {
            TransactionObservation::None
        };
        Ok(ExecutionCompletion {
            completion_status,
            error_code,
            effect_outcome,
            context_before: Some(resource.context.clone()),
            context_after: Some(resource.context.clone()),
            transaction_observation,
            resource_health: resource.health,
            ..ExecutionCompletion::ok()
        })
    }

    // ---- 4. observeSession ----

    /// §3.2：观测可以是 `unknown`，但**不得**回填 `initialTarget`（§5.1 L417）。
    pub fn observe_session(
        &self,
        request: &ObserveSessionRequest,
    ) -> Result<SessionObservation, ProviderError> {
        if let Some((_, kind)) = self.script.take(ResourceOp::Observe) {
            return match kind {
                // F6：CM-14 / CM-18 的可注入 `unknown`。
                FaultKind::ObserveUnknown => Ok(SessionObservation::all_unknown()),
                _ => Err(ProviderError::UnsupportedPlan),
            };
        }
        let resource = self.resolve(&request.handle.resource_id, &request.handle, false)?;
        Ok(SessionObservation {
            context: resource.context.clone(),
            transaction_state: resource.transaction_state,
            handle_count: resource.registered_handles(),
        })
    }

    // ---- 5. changeContext ----

    /// §3.2 L138：`changeContext` 有三种结果 —— `Confirmed` / `RequiresReplacement` / `Unsupported`。
    pub fn change_context(
        &self,
        request: &ChangeContextRequest,
    ) -> Result<ChangeContextOutcome, ProviderError> {
        let resource = self.resolve(&request.handle.resource_id, &request.handle, false)?;
        if let Some((_, kind)) = self.script.take(ResourceOp::ChangeContext) {
            match kind {
                // F7：需要换宿主 —— 这是 `Ok` 值，不是错误。
                FaultKind::RequiresReplacement { reason } => {
                    return Ok(ChangeContextOutcome::RequiresReplacement { reason })
                }
                // F7：乐观并发冲突。
                FaultKind::ContextConflict => {
                    return Err(ProviderError::ContextConflict(context_conflict(
                        request.expected_context_revision,
                        resource.context_revision,
                    )))
                }
                _ => return Err(ProviderError::UnsupportedPlan),
            }
        }
        if request.expected_context_revision != resource.context_revision {
            return Err(ProviderError::ContextConflict(context_conflict(
                request.expected_context_revision,
                resource.context_revision,
            )));
        }
        let after = SessionContext::new(
            request.target.namespace.clone(),
            self.execution_identity.clone(),
        );
        let key = request.handle.resource_id.as_str().to_owned();
        let context_revision = {
            let mut resources = self.lock();
            let slot = resources.get_mut(&key).ok_or_else(|| {
                ProviderError::SessionLost(format!("资源 {key} 在切换上下文时消失"))
            })?;
            slot.context = after.clone();
            slot.context_revision.saturating_increment()
        };
        Ok(ChangeContextOutcome::Confirmed {
            context: after,
            context_revision,
        })
    }

    // ---- 7. requestCancel ----

    /// §3.2 L143：只有命中未完成执行才返回 `Requested`。
    pub fn request_cancel(
        &self,
        request: &RequestCancelRequest,
    ) -> Result<CancelReceipt, ProviderError> {
        // F9：假提供方不支持精确取消 —— 返回 `Unsupported`，**不是**错误。
        if let Some((_, FaultKind::CancelRejected { code })) =
            self.script.take(ResourceOp::RequestCancel)
        {
            let code = execution_error_code(code);
            let key = request.handle.resource_id.as_str().to_owned();
            let mut resources = self.lock();
            if let Some(slot) = resources.get_mut(&key) {
                slot.last_error_code = Some(code);
            }
            return Ok(CancelReceipt {
                execution_id: request.execution_id.clone(),
                disposition: CancelDisposition::Unsupported,
            });
        }
        let _ = self.resolve(&request.handle.resource_id, &request.handle, false)?;
        let disposition = if self
            .journal
            .live_executions()
            .contains(&request.execution_id)
        {
            CancelDisposition::Requested
        } else {
            CancelDisposition::AlreadyFinished
        };
        Ok(CancelReceipt {
            execution_id: request.execution_id.clone(),
            disposition,
        })
    }

    // ---- 8. resetResource ----

    /// §3.2 L146：`resetResource` 返回 `Clean` **不等于**事务已终结 ——
    /// `FakeResource::reset_for_reuse` 一个字都不碰 `transaction_state`。
    ///
    /// §4.1 的 F10 三个变体都落在这里。贯穿性硬规则：**注入只改返回值，
    /// 不碰资源状态机** —— 否则测到的就不是「驱动说 Clean、宿主说不能归池」，
    /// 而是我们自己把状态改脏了。`CleanButPreconditionUnmet` 尤其如此：
    /// 资源照样走一遍真实的 `reset_for_reuse()`，只是把结论按脚本报成 `Clean`。
    pub fn reset_resource(
        &self,
        request: &ResetResourceRequest,
    ) -> Result<ResetOutcome, ProviderError> {
        let injected = self.script.take(ResourceOp::Reset).map(|(_, kind)| kind);
        let resource = self.resolve(&request.handle.resource_id, &request.handle, false)?;
        match injected {
            // F10：reset 超时。基线在 reset 上**从不**报错，所以这里的 `Err`
            // 必然来自脚本 —— 这本身就是注入生效的可观察证据。
            Some(FaultKind::ResetTimeout) => {
                return Err(ProviderError::CleanupFailed(format!(
                    "reset 未在期限内返回（{}）",
                    request.handle.resource_id.as_str()
                )))
            }
            // F10：驱动自报 `Discard` 并给出用例指定的原因。基线只可能产出
            // `SessionStillExecuting` / `HealthDegraded`，脚本能给
            // `TemporaryObjectsPresent` 这种基线**产不出来**的原因。
            Some(FaultKind::ResetDiscard { reason }) => {
                return Ok(ResetOutcome::Discard { reason })
            }
            _ => {}
        }
        let degraded = matches!(resource.health, ResourceHealth::Degraded);
        let key = request.handle.resource_id.as_str().to_owned();
        let live = {
            let mut resources = self.lock();
            let slot = resources
                .get_mut(&key)
                .ok_or_else(|| ProviderError::SessionLost(format!("资源 {key} 不存在")))?;
            slot.reset_for_reuse();
            slot.has_open_transaction()
        };
        Ok(
            if matches!(injected, Some(FaultKind::CleanButPreconditionUnmet)) {
                // F10 反例：驱动报了 `Clean`，但 `live`/`degraded` 说明宿主前置并不满足。
                // 宿主必须自己去查 `can_return_to_pool()`，不能信驱动（§4.2 F10）。
                ResetOutcome::Clean
            } else if live {
                // 会话还在事务里 → 归池必须 `Discard`，绝不能报 `Clean`（§3.2 L146）。
                ResetOutcome::Discard {
                    reason: ResetDiscardReason::SessionStillExecuting,
                }
            } else if degraded {
                ResetOutcome::Discard {
                    reason: ResetDiscardReason::HealthDegraded,
                }
            } else {
                ResetOutcome::Clean
            },
        )
    }

    // ---- 9. closeResource ----
    // ---- 夹具辅助 ----

    /// §9.3：给资源钉一个 lease（I3）。驱逐路径要据此归还。
    pub fn pin_lease(&self, resource_id: &ResourceId) -> LeaseId {
        let lease_id = self.ids.next_lease_id(resource_id);
        self.leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(resource_id.as_str().to_owned())
            .or_default()
            .push(lease_id.clone());
        self.journal.register_lease(&lease_id, resource_id);
        lease_id
    }

    /// 记录一条流事件序号（§4.3 I8：序号必须连续）。
    pub fn record_stream_event(&self, stream_id: &StreamId, seq: u64) {
        self.journal.record_stream_event(stream_id, seq);
    }

    fn session_view(
        &self,
        resource_id: &ResourceId,
        runtime_epoch: Counter,
        context: SessionContext,
    ) -> SessionView {
        let key = resource_id.as_str().to_owned();
        let resources = self.lock();
        let (db_session_id, owner) = match resources.get(&key) {
            Some(resource) => (
                resource.db_session_id.clone(),
                resource.owner.clone().unwrap_or_else(owner_fallback),
            ),
            // 理论上不可达：资源刚由 `create_resource` 插入。
            None => return self.fallback_session_view(runtime_epoch, context),
        };
        drop(resources);
        SessionView {
            handle: SessionHandle {
                db_session_id,
                runtime_epoch,
            },
            connection_id: self.target.connection_id.clone(),
            config_revision: self.config_revision,
            owner,
            initial_target: self.target.clone(),
            observed_context: context,
            context_revision: Counter::ZERO,
            state: SessionState::Ready,
            attachment_state: AttachmentState::Attached,
            active_execution_id: None,
            // `expires_at` 取假 UTC 时钟，不读墙钟，也不含任何凭据（§13）。
            expires_at: Timestamp::new(self.clock.utc().as_rfc3339()),
        }
    }

    fn fallback_session_view(
        &self,
        runtime_epoch: Counter,
        context: SessionContext,
    ) -> SessionView {
        SessionView {
            handle: SessionHandle {
                db_session_id: self.ids.next_db_session_id(),
                runtime_epoch,
            },
            connection_id: self.target.connection_id.clone(),
            config_revision: self.config_revision,
            owner: owner_fallback(),
            initial_target: self.target.clone(),
            observed_context: context,
            context_revision: Counter::ZERO,
            state: SessionState::Ready,
            attachment_state: AttachmentState::Attached,
            active_execution_id: None,
            expires_at: Timestamp::new(self.clock.utc().as_rfc3339()),
        }
    }
}

fn context_conflict(expected: Counter, actual: Counter) -> String {
    format!(
        "expected_context_revision={}，当前 contextRevision={}",
        expected.get(),
        actual.get()
    )
}

/// 资源表里理论上永远有 owner（`FakeResource::new` 必填）。这个 fallback 只让
/// `session_view` 在异常形状下也能返回，不会给 journal 写任何东西。
fn owner_fallback() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-unknown"),
    }
}
