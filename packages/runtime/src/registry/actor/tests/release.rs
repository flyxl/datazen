//! actor 的释放用例：§9.4 归池前的释放顺序、CM-73 / CM-74 空闲驱逐、§4.1 关闭。
//!
//! 本文件的模块名与 `super::super::release` 同名，所以这里**不用** `use super::super::*`
//! ——那个 glob 会把 `actor::release` 这个模块名拖进来并把本文件自己顶掉。
//! 一律走全路径 `super::super::release::…`。

use super::super::{AuditKind, CloseMode, RuntimeError, SessionState};
use super::*;
use crate::connection::TransactionState;
use crate::registry::backend::HandleDisposition;

/// T10 / §9.4：驱动回报「干净收工」，但宿主自己手上还留着句柄。
///
/// 这是本 CM 的全部要害——只信驱动回报就等于把一个还能提交的事务句柄
/// 交给上一任使用者。所以宿主侧的计数必须独立复核，对不上就判不可知。
/// 注意 `close_calls() == 1`：判定不可知**不**意味着跳过第 3 步，物理资源
/// 照样要关掉，只是回执改成一笔「结果不可知」。
#[tokio::test(start_paused = true)]
async fn 驱动报干净但宿主仍有句柄时判定不可知() {
    let backend = FakeBackend::new(FakeOutcome::default().finalizes(0, 0)).await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    actor
        .exec(|reply| ExecCommand::RegisterHandles {
            handle: handle.clone(),
            handles: vec![handle_ref("h_1", "res_7")],
            reply,
        })
        .await
        .expect("句柄登记必须成功");

    let outcome = actor
        .exec(|reply| close_command(handle.clone(), CloseMode::RollbackAndClose, reply))
        .await;
    assert_eq!(
        outcome,
        Err(RuntimeError::SessionLost(
            "registeredHandlesRemained".to_owned()
        ))
    );
    assert_eq!(
        backend.close_calls(),
        1,
        "判不可知也得把物理资源关掉，否则是泄漏"
    );

    // 墓碑上事务状态必须是 `Unknown`：既不能继续报 `Active`，也不能报 `None`。
    let tombstone = actor
        .exec(|reply| ExecCommand::View {
            handle: handle.clone(),
            reply,
        })
        .await
        .expect("墓碑必须仍可读（CM-73 要能观测到）");
    assert_eq!(tombstone.state, SessionState::Lost);
    assert_eq!(
        tombstone.observed_context.transaction_state,
        TransactionState::Unknown
    );

    // 墓碑不可写。
    let execute = actor
        .exec(|reply| ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply,
        })
        .await;
    assert_eq!(
        execute,
        Err(RuntimeError::SessionClosed(db_session_id().to_string()))
    );

    let entries = drain_audit(&mut audit);
    let released = single_of_kind(&entries, AuditKind::SessionClosed).expect("必须有释放审计");
    assert_eq!(released.outcome, Outcome::Undecided);
    assert_eq!(released.error_code, Some("resourceLost"));
    assert_eq!(released.effect_outcome, Some("unknown"));
}

/// T11 / CM-72：关闭结果不可知时，同样如实回一笔 `SessionLost`。
///
/// 与 T10 分开是因为证据来源不同：这里不可知的是**驱动关资源的回报**，
/// 而 T10 不可知的是**宿主的句柄计数**。两条路径都必须落到同一份结论上。
#[tokio::test(start_paused = true)]
async fn 关闭回报不可知时如实回报会话丢失() {
    let backend = FakeBackend::new(FakeOutcome::default().close_undecidable()).await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let outcome = actor
        .exec(|reply| close_command(handle.clone(), CloseMode::RollbackAndClose, reply))
        .await;
    assert_eq!(
        outcome,
        Err(RuntimeError::SessionLost(
            "closeOutcomeUndecidable".to_owned()
        ))
    );

    let tombstone = actor
        .exec(|reply| ExecCommand::View { handle, reply })
        .await
        .expect("墓碑必须仍可读");
    assert_eq!(tombstone.state, SessionState::Lost);
    assert_eq!(
        tombstone.observed_context.transaction_state,
        TransactionState::Unknown
    );

    let entries = drain_audit(&mut audit);
    let released = single_of_kind(&entries, AuditKind::SessionClosed).expect("必须有释放审计");
    assert_eq!(released.outcome, Outcome::Undecided);
}

/// T12 / §9.4 的分组前提：句柄必须按 `resourceId` 分组下发。
///
/// 纯函数一格。把三个句柄（两个资源）打乱顺序喂进去，断言分组数、组内顺序
/// （按首次出现）与批次纯净性——每批只带自己的句柄，不许捎带别的资源。
/// 这条错了，上面的用例在多资源下就会漏发句柄，而且照样是绿的。
#[test]
fn 句柄按资源分组且批次纯净() {
    let handles = vec![
        handle_ref("h_1", "res_2"),
        handle_ref("h_2", "res_1"),
        handle_ref("h_3", "res_2"),
    ];
    let groups = super::super::release::group_by_resource(&handles);

    assert_eq!(groups.len(), 2, "两个资源必须分两组");
    assert_eq!(groups[0].0, "res_2", "分组顺序按首次出现");
    assert_eq!(groups[1].0, "res_1");
    assert_eq!(
        groups[0]
            .1
            .iter()
            .map(|h| h.handle_id.as_str())
            .collect::<Vec<_>>(),
        vec!["h_1", "h_3"],
        "组内相对顺序必须保持"
    );
    assert_eq!(groups[1].1.len(), 1);
    for (resource_id, batch) in &groups {
        for handle in batch {
            assert_eq!(
                handle.resource_id.as_str(),
                resource_id,
                "批次里混进了别的资源的句柄"
            );
        }
    }
    assert!(super::super::release::group_by_resource(&[]).is_empty());
}

/// T12 的第二段：正常关闭时，两批句柄都被下发、账目对平、资源被关。
#[tokio::test(start_paused = true)]
async fn 关闭时逐资源下发句柄且计数对平() {
    // 驱动报的终结数必须与**宿主自己的登记数**对上（§9.4 归池前检查第 2 步）。
    // 这里登记 2 个、驱动也报 2 个，是「一致且干净」的正例。
    let backend = FakeBackend::new(FakeOutcome::default().finalizes(2, 0)).await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    actor
        .exec(|reply| ExecCommand::RegisterHandles {
            handle: handle.clone(),
            handles: vec![handle_ref("h_1", "res_7"), handle_ref("h_2", "res_7")],
            reply,
        })
        .await
        .expect("句柄登记必须成功");

    let outcome = actor
        .exec(|reply| close_command(handle.clone(), CloseMode::RollbackAndClose, reply))
        .await
        .expect("关闭必须成功");
    assert_eq!(outcome, SessionState::Closed);

    let traces = backend.traces().await;
    assert_eq!(traces.finalized.len(), 1, "同一资源一批");
    assert_eq!(traces.finalized[0].resource_id, "res_7");
    assert_eq!(traces.finalized[0].handles.len(), 2);
    assert_eq!(traces.finalized[0].disposition, HandleDisposition::Rollback);
    assert_eq!(
        traces.closed_with[0].registered_handles, 0,
        "回池检查必须发生在下发之后、关资源之前"
    );

    let entries = drain_audit(&mut audit);
    let closed = single_of_kind(&entries, AuditKind::SessionClosed).expect("必须有关闭审计");
    assert_eq!(closed.outcome, Outcome::Succeeded);
}

/// T13 / CM-73：空闲驱逐的顺序——先在**原资源**上把事务映射回滚掉，再关资源。
///
/// 断言分三层：未到期限时是一次**正常的空操作**（`Ok(None)`，不发任何请求，
/// 不是错误）；到期限时才动手；动手时下发的是回滚而不是提交，且资源 id 是
/// 当前那个 `res_7` 而不是任何新资源。
#[tokio::test(start_paused = true)]
async fn 空闲驱逐先在原资源回滚再关资源() {
    let backend = FakeBackend::new(FakeOutcome::default().finalizes(1, 0)).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    actor
        .exec(|reply| ExecCommand::RegisterHandles {
            handle: handle.clone(),
            handles: vec![handle_ref("h_1", "res_7")],
            reply,
        })
        .await
        .expect("句柄登记必须成功");

    let not_due = actor
        .exec(|reply| evict_command(IDLE_DEADLINE_MS - 1, reply))
        .await
        .expect("未到期限是正常空操作");
    assert_eq!(not_due, None);
    assert_eq!(backend.finalize_calls(), 0, "未到期限不许碰句柄");
    assert_eq!(backend.close_calls(), 0, "未到期限不许关资源");

    let evicted = actor
        .exec(|reply| evict_command(IDLE_DEADLINE_MS, reply))
        .await
        .expect("驱逐必须成功");
    let evicted = evicted.expect("到期限必须真的驱逐");
    assert_eq!(evicted.state, SessionState::Closed);

    let traces = backend.traces().await;
    assert_eq!(traces.finalized.len(), 1);
    assert_eq!(
        traces.finalized[0].disposition,
        HandleDisposition::Rollback,
        "驱逐不得替用户提交事务"
    );
    assert_eq!(
        traces.finalized[0].resource_id, "res_7",
        "回滚必须落在原资源上"
    );
    assert_eq!(traces.closed_with[0].registered_handles, 0);
}

/// T13 的第二段 / CM-74：事务在飞时 `RequireNoTransaction` 必须明确拒绝。
///
/// 关键是**什么都没做**：既不下发句柄、也不关资源——拒绝必须是零副作用的。
#[tokio::test(start_paused = true)]
async fn 有在飞事务时要求无事务关闭被明确拒绝且零副作用() {
    let backend = FakeBackend::new(FakeOutcome::default().finalizes(1, 0)).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    actor
        .exec(|reply| ExecCommand::RegisterHandles {
            handle: handle.clone(),
            handles: vec![handle_ref("h_1", "res_7")],
            reply,
        })
        .await
        .expect("句柄登记必须成功");

    let refused = actor
        .exec(|reply| close_command(handle.clone(), CloseMode::RequireNoTransaction, reply))
        .await;
    assert_eq!(
        refused,
        Err(RuntimeError::CloseRejected("transactionInProgress"))
    );
    assert_eq!(backend.finalize_calls(), 0, "拒绝不许下发回滚");
    assert_eq!(backend.close_calls(), 0, "拒绝不许关资源");

    // 会话必须仍然是活的——一次拒绝不该把会话带走。
    let still = actor
        .exec(|reply| ExecCommand::View { handle, reply })
        .await
        .expect("会话必须仍可读");
    assert_eq!(still.state, SessionState::Ready);
}

/// T17 / §4.1：执行在飞时来的关闭请求**排到执行之后**。
///
/// 关资源是要拿回句柄的，所以在飞行执行还没落地时动手，顺序就错了。
/// 证据是飞行途中 `close_calls() == 0` 且回执还没到；等执行落地后它才被回答。
#[tokio::test(start_paused = true)]
async fn 飞行执行期间到来的关闭被推迟到执行之后() {
    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::Silent)).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let (tx, execute_reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;

    let (close_tx, mut close_reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(close_command(
            handle.clone(),
            CloseMode::RollbackAndClose,
            close_tx,
        ))
        .expect("执行队列必须还开着");
    for _ in 0..8 {
        settle().await;
    }

    assert_eq!(backend.close_calls(), 0, "飞行执行期间不许关资源");
    assert_eq!(
        close_reply.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty),
        "关闭回执不该在执行落地前到"
    );

    for _ in 0..4 {
        backend.gate().send(()).expect("闸门接收端必须还在");
    }
    execute_reply
        .await
        .expect("回执通道必须送达")
        .expect("执行必须成功");
    let closed = close_reply
        .await
        .expect("回执通道必须送达")
        .expect("关闭必须成功");
    assert_eq!(closed, SessionState::Closed);
    assert_eq!(backend.close_calls(), 1);
}
