//! §6.3 执行的准入、飞行与收尾。
//!
//! 三件事按顺序发生，缺一不可：
//!
//! | 步骤 | 函数 | 判据 |
//! | --- | --- | --- |
//! | 准入 | [`admit_execution`] | epoch、并发度、物理资源、上下文修订，全部**同步**判定 |
//! | 起飞 | [`start_execution`] | 登记 `in_flight` 与两个 await 点，再把执行体 spawn 出去 |
//! | 收尾 | [`apply_completion`] | 先登记句柄，**再**把终态交给调用方（§6.5） |
//!
//! 执行本体永远**不**被搬出 [`ActorState`]：飞行期间控制旁路要读的正是这份状态，
//! 一旦主循环把它 `take()` 到局部变量，取消就会落进一个 `in_flight == None` 的窗口里
//! （见 [`super::InFlight`]）。
//!
//! 排队的执行在 [`drain_deferred`] 里出队，条件是「当前没有飞行中的执行」——这就是
//! CM-20「同会话并发度 1」的全部实现。

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinError;

use super::release::provider_to_runtime;
use crate::connection::error::ProviderError;
use crate::connection::{
    Counter, EffectOutcome, ExecuteInSessionRequest, ExecutionErrorCode, ExecutionId,
    ExecutionReceipt, ExecutionState, RuntimeError, SessionState,
};
use crate::registry::audit::{AuditKind, Outcome};
use crate::registry::backend::{CancelHandleSink, ExecuteOnResource, ResourceExecution};

use super::{check_epoch, emit, handle_exec, ActorState, AuditFacts, InFlight, Reply};

/// 保留多少条终态供 `alreadyFinished` 判定。再多只会让这张表变成内存负担：
/// §7.6 要判定的是**近期刚结束**的执行，不是历史全量。
pub(super) const SETTLED_CAP: usize = 16;

pub(super) fn start_execution(
    state: &mut ActorState,
    request: ExecuteInSessionRequest,
    reply: Reply<ExecutionReceipt>,
) {
    if let Err(failure) = admit_execution(state, &request) {
        let _ = reply.send(Err(failure));
        return;
    }

    let Some(physical) = state.physical.clone() else {
        let _ = reply.send(Err(RuntimeError::SessionClosed(
            state.db_session_id.to_string(),
        )));
        return;
    };

    state.execution_seq += 1;
    let execution_id = ExecutionId::new(format!(
        "exec_{}_{}",
        state.runtime_epoch.get(),
        state.execution_seq
    ));

    let (bind_tx, bind_rx) = mpsc::unbounded_channel();
    let sink = CancelHandleSink::new(bind_tx);
    let backend = Arc::clone(&state.backend);
    let resource_id = physical.resource_id.clone();
    let context_revision = state.context_revision;
    let call = request.call;

    let spawn_execution_id = execution_id.clone();
    let join = tokio::spawn(async move {
        backend
            .execute(ExecuteOnResource {
                execution_id: spawn_execution_id,
                resource_id: resource_id.clone(),
                command: call,
                expected_context_revision: context_revision,
                cancel_handle_sink: sink,
            })
            .await
    });

    state.view.state = SessionState::Executing;
    state.view.active_execution_id = Some(execution_id.clone());
    state.in_flight = Some(InFlight {
        execution_id,
        resource_id: physical.resource_id,
        cancel_handle: None,
        cancel_requested: false,
        reply,
    });
    // 两个 await 点连同飞行状态一起留在 `state` 上，由主循环取到局部变量去 `select!`。
    state.bind_rx = Some(bind_rx);
    state.join = Some(join);
}

/// 执行的前置校验。全部是同步判断，因此不需要跨 `.await` 的状态。
fn admit_execution(
    state: &ActorState,
    request: &ExecuteInSessionRequest,
) -> Result<(), RuntimeError> {
    check_epoch(state, &request.handle)?;
    if state.in_flight.is_some() {
        return Err(RuntimeError::InvariantBroken("executionAlreadyInFlight"));
    }
    if state.physical.is_none() {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    }
    // CM-20 的「切库竞态」：请求带着旧修订打过来，必须被拒，而不是在新目标上执行。
    if request.expected_context_revision.get() != state.context_revision {
        return Err(RuntimeError::ContextRevisionMismatch {
            expected: request.expected_context_revision.get(),
            actual: state.context_revision,
        });
    }
    Ok(())
}

pub(super) async fn apply_completion(
    state: &mut ActorState,
    active: InFlight,
    outcome: Result<Result<ResourceExecution, ProviderError>, JoinError>,
) {
    state.view.active_execution_id = None;
    if state.physical.is_some() {
        state.view.state = SessionState::Ready;
    }

    let execution = match outcome {
        // JoinError 是任务 panic/abort：绝不 unwrap，也不假装执行失败——
        // 「结果未知」必须以 OutcomeUnknown 的形状出去。
        Err(_join) => Err(RuntimeError::InvariantBroken("executionTaskLost")),
        Ok(Err(cause)) => Err(provider_to_runtime(cause)),
        Ok(Ok(execution)) => Ok(execution),
    };

    let execution = match execution {
        Ok(execution) => execution,
        Err(failure) => {
            state.handles.release_execution(&active.execution_id);
            remember(state, active.execution_id.clone(), ExecutionState::Failed);
            emit(
                state,
                AuditFacts {
                    kind: AuditKind::ExecutionCompleted,
                    outcome: Outcome::Undecided,
                    error_code: Some(ExecutionErrorCode::HostRejected),
                    execution_state: Some(ExecutionState::Failed),
                    effect_outcome: Some(EffectOutcome::Unknown),
                    handle_count: state.handles.handle_count(),
                    ..AuditFacts::none()
                },
            );
            let _ = active.reply.send(Err(failure));
            return;
        }
    };

    // §6.5：句柄必须**先**登记，再把终态交给调用方。
    let register_error = state.handles.register(execution.handles.clone()).err();
    state.context_revision = execution.context_revision;
    state.view.context_revision = Counter(execution.context_revision);
    state.view.observed_context = execution.context_after.clone();
    state.handles.release_execution(&active.execution_id);
    remember(state, active.execution_id.clone(), execution.state);

    emit(
        state,
        AuditFacts {
            kind: AuditKind::ExecutionCompleted,
            outcome: match execution.effect_outcome {
                EffectOutcome::Unknown => Outcome::Undecided,
                EffectOutcome::NotStarted => Outcome::Rejected,
                _ => Outcome::Succeeded,
            },
            execution_state: Some(execution.state),
            effect_outcome: Some(execution.effect_outcome),
            handle_count: state.handles.handle_count(),
            ..AuditFacts::none()
        },
    );

    let _ = active.reply.send(match register_error {
        Some(failure) => Err(failure),
        None => Ok(ExecutionReceipt {
            execution_id: execution.execution_id,
            stream_id: execution.stream_id,
            state: execution.state,
        }),
    });
}

/// 把飞行期间排队的执行按 FIFO 放出来，只在「没有飞行中的执行」时才放行一条。
pub(super) async fn drain_deferred(state: &mut ActorState) {
    while state.in_flight.is_none() {
        let Some(msg) = state.deferred.pop_front() else {
            return;
        };
        handle_exec(state, msg).await;
    }
}

fn remember(state: &mut ActorState, execution_id: ExecutionId, terminal: ExecutionState) {
    state.settled.push((execution_id, terminal));
    if state.settled.len() > SETTLED_CAP {
        let excess = state.settled.len() - SETTLED_CAP;
        state.settled.drain(..excess);
    }
}
