//! §7.6 取消 —— 走**控制旁路**，绝不排队等执行槽位。
//!
//! | 顺序 | 判据 | 处置 | 为什么不能换序 |
//! | --- | --- | --- | --- |
//! | 1 | epoch 对不上 | `Err` | 旧世代请求不得碰新会话的执行（§12.1） |
//! | 2 | 该执行正在本会话内飞行，但 driver 无独立控制路径 | `unsupported`（**正常返回**） | 「不支持」不是「失败」；把它变成 Err 会让调用方误以为要重试 |
//! | 3 | 该执行正在飞行、但取消句柄尚未公布 | `requested` + 排队 | 排队期间状态置 `cancelRequested`，句柄一到就地下发——空头支票必须被兑现 |
//! | 4 | 调用方自带 `cancelHandle` 与登记绑定不符 | `Err(CancelFailed)` | 伪造的绑定必须**一个后端调用都不产生** |
//! | 5 | 该执行已终结 | `alreadyFinished` + 原样回填终态 | 终态不是被取消写出来的 |
//! | 6 | 其余 | 后端取消 + `requested` | |
//!
//! **第 4 步只能排在公布之后，这不是笔误。** 绑定比对需要「登记绑定」这一侧的值，
//! 而它由后端在执行过程里公布；在公布之前，本次取消手里只有调用方**自称**的那一个。
//! 拿自称去比对登记值不成立（拿 A 比 A 永远相等），要判伪造就必须等真值到位，
//! 所以顺序只能是：先排队受理，等公布，再逐字比对，再下发。
//!
//! 排序真正守住的性质是：**伪造的绑定不会在后端留下任何痕迹**——
//! 第 4 步在 `state.backend.cancel(..)` 之前返回，核对失败连 `check_binding`
//! 与后端调用都不会发生。至于「先拒收这次取消、再去问绑在哪个资源上」，
//! 在控制旁路这条路径上并不存在这样的选择点。
//!
//! 早期版本的判定表把「绑定不符」排在「排队」之前：
//! 那张表描述的是一个**物理上不存在**的分支顺序，代码从一开始就不是那么走的。

use super::{check_epoch, emit, ActorState, AuditFacts};
use crate::connection::port::CancelDisposition;
use crate::connection::{
    ExecutionErrorCode, ExecutionId, ExecutionState, RuntimeError, SessionHandle,
};
use crate::registry::audit::{AuditKind, Outcome};
use crate::registry::backend::CancelOnResource;
use crate::registry::receipt::CancelReceipt;

/// 把排队中的取消**兑现**掉。
///
/// 取消先于 cancelHandle 到达时，回执已经回过 `requested`；那不是一张可以撕掉的
/// 空头支票——handle 一旦由后端公布，这里就用登记绑定把它补送下去。
/// 补送失败同样**不降级**成「已取消」：只记一条 `rejected` 审计，真实结论留给
/// 执行自己的终态去写。
///
/// 飞行中的执行始终留在 [`ActorState::in_flight`] 里（见 [`super::InFlight`]），
/// 所以这里只需要 `state`。
pub(super) async fn deliver_published(state: &mut ActorState) {
    let Some(active) = state.in_flight.as_ref() else {
        return;
    };
    let Some(bound) = active.cancel_handle.clone() else {
        return;
    };
    let request = CancelOnResource {
        resource_id: active.resource_id.clone(),
        execution_id: active.execution_id.clone(),
        cancel_handle: bound,
    };
    let outcome = state.backend.cancel(request).await;
    let (result_outcome, error_code) = match outcome {
        Ok(_) => (Outcome::Succeeded, None),
        Err(_) => (Outcome::Rejected, Some(ExecutionErrorCode::HostRejected)),
    };
    emit(
        state,
        AuditFacts {
            kind: AuditKind::CancelResolved,
            outcome: result_outcome,
            error_code,
            disposition: Some(CancelDisposition::Requested),
            execution_state: Some(ExecutionState::CancelRequested),
            handle_count: state.handles.handle_count(),
            ..AuditFacts::none()
        },
    );
}

pub(super) async fn cancel(
    state: &mut ActorState,
    handle: &SessionHandle,
    execution_id: ExecutionId,
    claimed_cancel_handle: Option<String>,
) -> Result<CancelReceipt, RuntimeError> {
    check_epoch(state, handle)?;
    let Some(physical) = state.physical.clone() else {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    };

    let in_flight_here = state
        .in_flight
        .as_ref()
        .is_some_and(|active| active.execution_id == execution_id);

    if in_flight_here {
        // §7.6：driver 没有独立取消路径时回 `unsupported`——这是**正常返回**，不是 Err。
        // 把它变成 Err 会让调用方误以为取消失败、需要重试。
        if !physical.driver_supports_cancel {
            // state 报**真实**的飞行状态（Running），不是 CancelRequested：
            // 驱动根本不支持取消，把它写成「正在取消」等于对调用方撒谎。
            let receipt = CancelReceipt::normalize(execution_id, ExecutionState::Running, false);
            emit(
                state,
                AuditFacts {
                    kind: AuditKind::CancelResolved,
                    outcome: Outcome::Rejected,
                    disposition: Some(receipt.disposition),
                    execution_state: Some(receipt.state),
                    handle_count: state.handles.handle_count(),
                    ..AuditFacts::none()
                },
            );
            return Ok(receipt);
        }

        let published = state
            .in_flight
            .as_ref()
            .and_then(|active| active.cancel_handle.clone());
        let Some(bound) = published else {
            // handle 尚未公布：这次取消仍算**已排队受理**，不能在飞行中被打成失败。
            // 记在 InFlight 上，等 `Step::Published` 兑现——回执里的 `requested`
            // 必须最终对应一次真实的后端取消，否则就是空头支票。
            if let Some(active) = state.in_flight.as_mut() {
                active.cancel_requested = true;
            }
            return Ok(CancelReceipt::normalize(
                execution_id,
                ExecutionState::CancelRequested,
                true,
            ));
        };

        // §3.2 L143：伪造的绑定必须被拒，且**不改动**绑定表、不产生任何后端调用。
        // 调用方自带 handle 时逐字比对；自带 `None` 时用 actor 自己登记的绑定，
        // 两条路径都比对**同一个** `bound`——冻结端口形状不同，不等于校验可以更松。
        if claimed_cancel_handle
            .as_deref()
            .is_some_and(|claimed| claimed != bound)
        {
            return Err(RuntimeError::CancelFailed("cancelBindingMismatch"));
        }
        state
            .handles
            .check_binding(&execution_id, &bound, &physical.resource_id)?;

        let result = state
            .backend
            .cancel(CancelOnResource {
                resource_id: physical.resource_id.clone(),
                execution_id: execution_id.clone(),
                cancel_handle: bound,
            })
            .await;
        match result {
            Ok(response) => {
                let receipt = CancelReceipt {
                    execution_id,
                    disposition: response.disposition,
                    state: response.state,
                };
                emit(
                    state,
                    AuditFacts {
                        kind: AuditKind::CancelResolved,
                        outcome: Outcome::Succeeded,
                        disposition: Some(receipt.disposition),
                        execution_state: Some(receipt.state),
                        handle_count: state.handles.handle_count(),
                        ..AuditFacts::none()
                    },
                );
                Ok(receipt)
            }
            // 取消失败**不得**退化成「已取消」——回执会说谎，
            // 调用方据此以为事务已回滚，随后在脏会话上继续写入。
            Err(_) => {
                emit(
                    state,
                    AuditFacts {
                        kind: AuditKind::CancelResolved,
                        outcome: Outcome::Rejected,
                        error_code: Some(ExecutionErrorCode::HostRejected),
                        disposition: Some(CancelDisposition::Requested),
                        handle_count: state.handles.handle_count(),
                        ..AuditFacts::none()
                    },
                );
                Err(RuntimeError::CancelFailed("cancelRequestFailed"))
            }
        }
    } else if let Some((_, terminal)) = state
        .settled
        .iter()
        .find(|(id, _)| id == &execution_id)
        .cloned()
    {
        let receipt =
            CancelReceipt::normalize(execution_id, terminal, physical.driver_supports_cancel);
        emit(
            state,
            AuditFacts {
                kind: AuditKind::CancelResolved,
                outcome: Outcome::Succeeded,
                disposition: Some(receipt.disposition),
                execution_state: Some(receipt.state),
                handle_count: state.handles.handle_count(),
                ..AuditFacts::none()
            },
        );
        Ok(receipt)
    } else {
        Err(RuntimeError::CancelFailed("unboundExecution"))
    }
}
