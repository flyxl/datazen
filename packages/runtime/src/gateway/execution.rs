//! 统一执行网关的实现（`ExecutionGateway` 的固有方法）。
//!
//! 与类型定义分文件的原因：`mod.rs` 同时承担「模块树 + 重导出 + 类型声明」，
//! 再塞进一个五百行的固有 impl 块就把文件顶过 `gateway_contract` 的 800 行硬上限。
//! 本文件**不引入任何新语义**：内容逐行搬自 `mod.rs`，只多一个 `use super::*`。
//!
//! 可见性无需改动：子模块天然可见父模块的私有项（Rust 隐私是「定义处及其后代可见」），
//! 所以 `ExecutionGateway` 的私有字段 `port` / `authorizer` / `ledger` / `clock` /
//! `tokens` / `state` 在这里可直接访问，`pub(super)` 一个都不用加。

use super::*;

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
        let receipt = self
            .port
            .execute_bound(execution_id.clone(), port_request)
            .await?;

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
