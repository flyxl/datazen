//! 释放顺序 —— 唯一的归池前检查执行者。
//!
//! 四步顺序不可换、不可并行、不可提前：
//!
//! ```text
//! 1. 在**各自登记时的资源上**终结句柄（回滚/提交）
//! 2. 宿主侧归池前检查（终态 / 无活跃消费者与取消句柄 / protocolDrained / 已登记句柄为空）
//! 3. 物理关闭资源
//! 4. 作废一切绑定，actor 终止后不得重建
//! ```
//!
//! **为什么 1 必须在 3 之前**：句柄登记时记的是哪一个资源，提交就必须落在那一个资源上。
//! 先关资源再回滚，回滚请求会落到新连上的资源上，等于把旧句柄提交到新会话。
//!
//! **为什么「driver 说 Clean」不能跳过第 2 步**：driver 对已经交出去的句柄没有可见性，
//! 它的 Clean 不构成事务终结的证据。宿主自己的登记表是唯一可信来源。

use super::{emit, ActorState, AuditFacts};
use crate::connection::error::ProviderError;
use crate::connection::{
    CloseMode, EffectOutcome, ExecutionErrorCode, RuntimeError, SessionHandleRef, SessionState,
    TransactionState,
};
use crate::registry::audit::{AuditKind, Outcome};
use crate::registry::backend::{
    close_request_for, ready_to_return_to_pool, CloseResourceOutcome, FinalizeHandles,
    HandleDisposition, HandleFinalization,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReleaseReason {
    Close,
    Evict,
    InvalidateWorker,
    ActorGone,
}

/// 唯一释放例程。**顺序**是本函数的全部价值所在，见模块文档。
pub(super) async fn release(
    state: &mut ActorState,
    mode: CloseMode,
    reason: ReleaseReason,
) -> Result<SessionState, RuntimeError> {
    let Some(physical) = state.physical.clone() else {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    };

    // `RequireNoTransaction` 下有活事务就**直接拒绝关闭**，不做任何半途动作。
    // `Unknown` 不算「无事务」——只查句柄账本会被「执行从未交还事务状态」骗过：
    // 一次 `begin` 但从未登记句柄的会话会在 RequireNoTransaction 下被静默放行。
    if mode == CloseMode::RequireNoTransaction
        && (!state.handles.is_empty()
            || state.view.observed_context.transaction_state != TransactionState::None)
    {
        return Err(RuntimeError::CloseRejected("transactionInProgress"));
    }

    state.view.state = SessionState::Closing;

    // 第 1 步：在**各自登记时的资源**上终结句柄。资源被替换过的会话，
    // 旧句柄只认旧资源——按 resource_id 分组正是为了防止跨资源终结。
    //
    // `registered_before` 是**宿主自己账上**的登记数：driver 对已交出的句柄
    // 没有可见性，它的 Clean 不构成事务终结的证据，所以这个数只能由宿主自己留底。
    //
    // 取样走 `handles()`（只读）而不是 `drain_handles()`：**注销必须发生在确认之后**，
    // 而不是发生在请求之前。先清账会让第 2 步的 (b) 变成恒真式——宿主「检查已登记
    // 句柄为空」，检查的却是自己刚清空的那本账，driver 报 Clean 而宿主仍有活句柄时
    // 判据要求的那个失败就永远不会发生。
    let registered_before = state.handles.handle_count();
    let pending: Vec<SessionHandleRef> = state.handles.handles().to_vec();
    let disposition = match mode {
        CloseMode::RequireNoTransaction => HandleDisposition::Commit,
        CloseMode::RollbackAndClose => HandleDisposition::Rollback,
    };
    let mut undecidable: Option<&'static str> = None;
    let mut finalized_total = 0usize;
    for (resource_id, batch) in group_by_resource(&pending) {
        match state
            .backend
            .finalize_handles(FinalizeHandles {
                resource_id,
                handles: batch.clone(),
                disposition,
            })
            .await
        {
            Ok(result) => {
                finalized_total += result.finalized;
                let cause = classify(result.clone());
                // 只有后端**逐字确认了这一批**（确认数等于这批的登记数，且没有剩余）
                // 才从宿主账上注销。少报、多报、`remaining > 0` 一律留着——留着才有
                // 第 2 步 (b) 那个「宿主自己数一遍」的判据可执行。
                if cause.is_none() && result.finalized == batch.len() {
                    state.handles.retire(&batch);
                }
                undecidable = undecidable.or(cause);
            }
            Err(_) => undecidable = undecidable.or(Some("handleFinalizeFailed")),
        }
    }

    // 第 2 步：宿主侧归池前检查。两道都过才算数：
    //   a) 后端确认终结掉的句柄数 == 宿主登记的句柄数（没有句柄从账上漏掉）；
    //   b) 释放后宿主账上确实为空。
    // 少了 (a)，driver 少报的场景就会静默通过；少了 (b)，两道判据就都只读 driver 的
    // 数字——明令「driver 对已交出的句柄没有可见性，其 Clean 不构成事务终结的
    // 证据」，宿主账必须**自己**数一遍。(a) 与 (b) 并非冗余：driver 一批多报、另一批
    // 少报而总数正好对平时，(a) 过了，只有 (b) 会失败（见 `registry_release.rs` 的
    // `driver两批确认数正好对平但宿主账未清空时仍判不可知`）。
    if finalized_total != registered_before
        || !ready_to_return_to_pool(state.handles.handle_count())
    {
        undecidable = undecidable.or(Some("registeredHandlesRemained"));
    }

    // 第 3 步：无论能不能确认终结，物理资源都必须**真正关掉**（「任一失败都关闭」）。
    //
    // 这里传的是**释放后**的宿主账：宿主自数那一步之前它恒为 0，因为取样用的是 `drain_handles`；
    // 现在它如实反映「还有句柄没被确认终结」，`CloseResource.registered_handles`
    // 因此成为这条判据在后端请求形状上的可观测面。
    //
    // `close_request_for` 的第二项（「先去补发一次终结」）在这里被显式丢弃：第 1 步已经
    // 对**每个**登记句柄按批次发过一次终结请求，账上剩下的正是那批**没被确认**的句柄，
    // 再发一遍只是把同一个不可判定的命运重掷一次骰子。对「确认不了」的处置是
    // **关闭 + 如实记 `Undecided`**，不是重试。丢弃的理由写在丢弃点上。
    let (close_request, _retry_finalize) = close_request_for(
        physical.resource_id.clone(),
        state.handles.handle_count(),
        Vec::new(),
    );
    let closed = match state.backend.close(close_request).await {
        Ok(CloseResourceOutcome::Closed) => true,
        Ok(CloseResourceOutcome::Undecidable { .. }) | Err(_) => false,
    };
    if !closed {
        undecidable = undecidable.or(Some("closeOutcomeUndecidable"));
    }

    // 第 4 步：actor 终止后不得重建任何句柄或绑定。
    state.handles.invalidate_bindings();
    state.physical = None;
    state.deferred.clear();
    // 墓碑写成 `Lost` 的条件是**任何一步**不可判定，不是「关资源的回报不可判定」。
    // 宿主句柄计数对不上时事务链的命运已经不可知，驱动报干净、资源也关掉了，
    // 但墓碑仍不能报 `Closed`——那等于替一条不可知的链背书「已干净收尾」，
    // 与下面 `Undecided` 的审计和 `Unknown` 的事务状态自相矛盾。
    state.view.state = if undecidable.is_some() {
        SessionState::Lost
    } else {
        SessionState::Closed
    };
    state.view.active_execution_id = None;

    // 事务状态：无法确认终结时**必须停止报 Active**。
    // 继续报 Active 会让上层在一条可能已经回滚的链上接着提交。
    if let Some(cause) = undecidable {
        state.view.observed_context.transaction_state = TransactionState::Unknown;
        emit(
            state,
            AuditFacts {
                kind: release_kind(reason),
                outcome: Outcome::Undecided,
                error_code: Some(ExecutionErrorCode::ResourceLost),
                effect_outcome: Some(EffectOutcome::Unknown),
                handle_count: 0,
                ..AuditFacts::none()
            },
        );
        return Err(RuntimeError::SessionLost(cause.to_owned()));
    }

    emit(
        state,
        AuditFacts {
            kind: release_kind(reason),
            outcome: Outcome::Succeeded,
            handle_count: 0,
            ..AuditFacts::none()
        },
    );
    Ok(state.view.state)
}

/// 句柄终结结果 → 无法确认时的原因码。`None` 表示这一步是确定的。
pub(super) fn classify(result: HandleFinalization) -> Option<&'static str> {
    if result.remaining > 0 {
        Some("handlesStillOpen")
    } else if matches!(result.effect_outcome, EffectOutcome::Unknown) {
        Some("handleFinalizeUnknown")
    } else {
        None
    }
}

/// 按 `resource_id` 分组，**保持首次出现顺序**。
pub(super) fn group_by_resource(
    handles: &[SessionHandleRef],
) -> Vec<(String, Vec<SessionHandleRef>)> {
    let mut groups: Vec<(String, Vec<SessionHandleRef>)> = Vec::new();
    for handle in handles {
        let resource_id = handle.resource_id.as_str().to_owned();
        match groups.iter_mut().find(|(id, _)| id == &resource_id) {
            Some((_, batch)) => batch.push(handle.clone()),
            None => groups.push((resource_id, vec![handle.clone()])),
        }
    }
    groups
}

pub(super) fn release_kind(reason: ReleaseReason) -> AuditKind {
    match reason {
        ReleaseReason::Close => AuditKind::SessionClosed,
        ReleaseReason::Evict => AuditKind::SessionEvicted,
        ReleaseReason::InvalidateWorker | ReleaseReason::ActorGone => AuditKind::SessionInvalidated,
    }
}

pub(super) fn provider_to_runtime(cause: ProviderError) -> RuntimeError {
    match cause {
        ProviderError::SessionLost(_) | ProviderError::ResourceLost(_) => {
            RuntimeError::SessionLost("resourceLost".to_owned())
        }
        ProviderError::RuntimeEpochMismatch(_) => {
            RuntimeError::SessionLost("runtimeEpochMismatch".to_owned())
        }
        ProviderError::OutcomeUnknown(_) | ProviderError::RollbackFailed(_) => {
            RuntimeError::InvariantBroken("outcomeUnknown")
        }
        _ => RuntimeError::CloseRejected("backendRejected"),
    }
}
