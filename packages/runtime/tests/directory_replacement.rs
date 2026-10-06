//! 替换提交的原子性（CM-68 / A3.5）。
//!
//! 替换最容易出的事故是「旧的和新的同时跑起来」。本文件把每一条防线都钉死：
//!
//! - `Prepared` 候选从出生起就**不可路由**，但它的 `dbSessionId` 已被占住；
//! - `Committed` 之后旧条目改关闭路由、新条目放行，**恰好一边**可路由；
//! - `RolledBack` 之后候选销毁、旧条目原样恢复；
//! - 没 `Prepared` 就 `Committed`：直接报错，状态一个字都不改；
//! - 结果未知：**两边都不可路由**，按 operation key 查清之后只落定一边；
//! - 同一个替换操作键**只接受一次** `Prepared`，重复投递被拒且不改动第一次的状态；
//! - 旧会话的挂载令牌在新会话上一律不作数；
//! - 按 key 查状态给候选补的那次放行**只在候选还卡在屏障里时才动手**：已放行的不动它，
//!   已关闭的更不许复活它。
//!
//! 断言全部非空：把「候选先放行」「提交时旧条目先留着」「未知结果按『多半成功』处理」
//! 这些改法，都会让下面至少一条断言变红。末一条尤其反直觉——补放行在**多数**时候确实该
//! 无条件执行，只有末两条把它钉死：把补放行换回无条件重写，本文件另外十条全绿，只有
//! 「查状态不得复活已关闭的候选」与「查状态不得撤掉活着的候选的期限」两条变红（实测）。

mod common;

use std::time::Duration;

use common::{client, cycling_directory, editor_draft, principal};
use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ClientInstanceId, ConnectionId, DbSessionId, EditorSessionId, OrganizationId,
    PrincipalId, RuntimeEpoch, Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::{
    CloseDisposition, ReplacementCommit, ReplacementOperation, SessionDirectory, SessionOwner,
};
use datazen_runtime::directory::{
    AttachmentRejection, CommitStatus, InMemorySessionDirectory, MonoInstant,
    ReplacementOperationKey, SessionHandle, UnknownCommitOutcome,
};

const EPOCH: &str = "rte-replacement-0001";
const NEXT_EPOCH: &str = "rte-replacement-0002";

/// 候选会话的 owner。替换里新条目不经过 `open_session`（还没人持有它的令牌），
/// 所以这里直接构造，字段和 `open_session` 产出的完全一致。
fn candidate_owner(db_session_id: &str, runtime_epoch: &str, registered_at: &str) -> SessionOwner {
    SessionOwner {
        db_session_id: DbSessionId::new(db_session_id),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("user-1"),
        connection_id: ConnectionId::new("conn-1"),
        owner: OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("client-1-editor"),
        },
        worker_id: WorkerId::new("worker-1"),
        runtime_epoch: RuntimeEpoch::new(runtime_epoch),
        resource_epoch: 0,
        last_business_activity: Timestamp::new(registered_at),
    }
}

struct Opened {
    handle: SessionHandle,
    token: AttachmentToken,
}

async fn open(dir: &InMemorySessionDirectory) -> Opened {
    let (handle, token) = dir
        .open_session(
            editor_draft("org-1", "user-1", "conn-1", "client-1"),
            RuntimeEpoch::new(EPOCH),
        )
        .expect("会话应登记成功");
    Opened { handle, token }
}

fn commit_for(
    old: &SessionHandle,
    new: &SessionOwner,
    operation: ReplacementOperation,
) -> ReplacementCommit {
    ReplacementCommit {
        old: old.clone(),
        new_owner: new.clone(),
        operation,
    }
}

/// 候选令牌在替换前后都不存在；这里只造一个「像模像样的」外来令牌，
/// 用来证明旧会话的令牌带不过去。
fn foreign_token() -> AttachmentToken {
    AttachmentToken::new("atk_from_the_previous_session_0001")
}

async fn attach_ok(
    dir: &InMemorySessionDirectory,
    handle: &SessionHandle,
    token: AttachmentToken,
) -> AttachmentOutcomeAssert {
    match dir.attach_from_client(handle, principal("user-1"), client("client-1"), token) {
        Ok(_) => AttachmentOutcomeAssert::Ok,
        Err(rejection) => AttachmentOutcomeAssert::Rejected(rejection),
    }
}

enum AttachmentOutcomeAssert {
    Ok,
    Rejected(AttachmentRejection),
}

#[tokio::test]
async fn prepared_candidate_is_invisible_and_reserves_its_id() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0001", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");

    // 候选不可路由：四条路径全都要拒。
    assert!(
        !dir.is_routable(&candidate_handle),
        "prepared 候选绝不能可路由，否则新旧会同时跑"
    );
    assert!(
        dir.resolve(&candidate_handle).is_err(),
        "prepared 候选不能被解析出 owner"
    );
    assert!(
        dir.lookup(candidate_handle.db_session_id.clone())
            .await
            .expect("查询可路由条目不该报错")
            .is_none(),
        "prepared 候选对 lookup 也必须隐身"
    );
    assert!(
        matches!(
            attach_ok(&dir, &candidate_handle, foreign_token()).await,
            AttachmentOutcomeAssert::Rejected(AttachmentRejection::NotRoutable(_))
        ),
        "prepared 候选不能被挂载：挂载令牌由提交成功后才签发"
    );

    // 但候选 ID 已被占住：并发 register 抢不走。
    assert!(
        matches!(
            dir.register(candidate.clone()).await,
            Err(PortError::CasConflict { .. })
        ),
        "prepared 候选的 ID 必须被占住，否则并发 register 会抢走同一个候选"
    );

    // 旧会话此刻仍然原样可路由——prepare 不是切换。
    assert!(
        dir.is_routable(&old.handle),
        "prepare 阶段旧会话必须照常可路由"
    );
    assert!(dir
        .commit_status(&ReplacementOperationKey::for_handle(&old.handle))
        .is_pending());
}

/// 同一个替换操作键**只允许被 `Prepared` 一次**。
///
/// 风险在于操作键由旧句柄唯一决定（`rep_{db_session_id}@{runtime_epoch}`），
/// 于是「重投一次 prepare」看起来总是无害的——反正最终要提交的也是同一个旧会话。
/// 实际上它会静默覆盖操作记录，把**第一个**候选永久孤立在屏障态里：
/// 那个候选再也不会被提交、也不会被回滚，却一直占着 `dbSessionId` 不放。
/// 所以重复投递必须在入口就被拒，且**不得**改动第一次 prepare 的任何状态。
#[tokio::test]
async fn a_second_prepared_for_the_same_operation_key_is_rejected() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let key = ReplacementOperationKey::for_handle(&old.handle);
    let first = candidate_owner("dbs_candidate_0001", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let first_handle = first.to_handle();
    // 第二个候选刻意用**不同的 dbSessionId**：「同一个候选重投」被候选 ID 占位挡住，
    // 真正没被挡住的正是「换了个候选重投」这条路。
    let second = candidate_owner("dbs_candidate_0002", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let second_handle = second.to_handle();

    dir.commit_replacement(commit_for(
        &old.handle,
        &first,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("第一次 prepare 应成功");
    let entries_after_first = dir.len();

    let err = dir
        .commit_replacement(commit_for(
            &old.handle,
            &second,
            ReplacementOperation::Prepared,
        ))
        .await
        .expect_err("同一操作键的第二次 prepare 必须被拒");
    assert!(
        matches!(&err, PortError::CasConflict { entity, .. } if &**entity == "replacement"),
        "重复 prepare 应当报替换语义上的 CAS 冲突，实际是：{err}"
    );

    // 第二次 prepare 什么也不许留下：不建第二个条目，也不动第一个。
    assert_eq!(
        dir.len(),
        entries_after_first,
        "被拒的重复 prepare 不得新增任何条目"
    );
    assert!(
        !dir.contains(&second_handle.db_session_id),
        "第二个候选根本不该进入目录，它的 ID 也不该被占住"
    );
    assert!(
        dir.commit_status(&key).is_pending(),
        "第一次 prepare 的屏障状态必须原封不动"
    );

    // 屏障照常结算：被提交的是**第一个**候选，第二个永远不会有可路由的一天。
    dir.commit_replacement(commit_for(
        &old.handle,
        &first,
        ReplacementOperation::Committed,
    ))
    .await
    .expect("commit 应成功");
    assert!(
        dir.is_routable(&first_handle) && !dir.is_routable(&second_handle),
        "结算的必须是第一次 prepare 登记的候选"
    );
    assert!(!dir.is_routable(&old.handle), "提交之后旧条目必须关闭路由");
}

#[tokio::test]
async fn committed_replacement_closes_the_old_entry_atomically() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0002", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    let outcome = dir
        .commit_replacement(commit_for(
            &old.handle,
            &candidate,
            ReplacementOperation::Committed,
        ))
        .await
        .expect("commit 应成功");
    assert_eq!(outcome.operation, ReplacementOperation::Committed);
    assert_eq!(outcome.replaced, old.handle);

    assert!(
        dir.is_routable(&candidate_handle) && !dir.is_routable(&old.handle),
        "提交之后必须恰好一边可路由：新条目放行，旧条目关闭路由"
    );
    assert!(
        dir.lookup(old.handle.db_session_id.clone())
            .await
            .expect("查询可路由条目不该报错")
            .is_none(),
        "旧条目关闭路由后不能继续被查到"
    );
    assert!(
        dir.resolve(&old.handle).is_err()
            && dir
                .touch(&old.handle, Timestamp::new("2026-01-01T00:01:00.000Z"))
                .await
                .is_err(),
        "旧句柄的所有入口都要显式失败"
    );
    assert!(
        dir.commit_status(&ReplacementOperationKey::for_handle(&old.handle))
            .is_committed(),
        "状态查询必须给出已提交的确定答案"
    );
    assert_eq!(
        dir.lookup(candidate_handle.db_session_id.clone()).await,
        Ok(Some(candidate.clone())),
        "新条目应带着新 owner 与新 epoch 放行"
    );
}

#[tokio::test]
async fn rolled_back_replacement_restores_the_old_entry() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0003", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::RolledBack,
    ))
    .await
    .expect("rollback 应成功");

    assert!(
        dir.is_routable(&old.handle) && !dir.is_routable(&candidate_handle),
        "回滚之后必须只剩旧会话可路由"
    );
    assert!(
        dir.lookup(candidate_handle.db_session_id.clone())
            .await
            .expect("查询可路由条目不该报错")
            .is_none(),
        "回滚必须销毁候选条目，而不是留一条不可路由的僵尸"
    );
    assert_eq!(
        dir.commit_status(&ReplacementOperationKey::for_handle(&old.handle)),
        CommitStatus::RolledBack {
            restored: old.handle.clone()
        },
        "回滚的确定答案必须指向旧句柄"
    );
    assert!(
        matches!(
            attach_ok(&dir, &old.handle, old.token.clone()).await,
            AttachmentOutcomeAssert::Ok
        ),
        "回滚后旧会话的挂载仍然成立（期限与挂载状态都没被动过）"
    );
}

#[tokio::test]
async fn commit_without_prepared_fails_and_changes_nothing() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0004", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();
    let before = dir.revision();

    let err = dir
        .commit_replacement(commit_for(
            &old.handle,
            &candidate,
            ReplacementOperation::Committed,
        ))
        .await
        .expect_err("没 prepare 就 commit 必须失败");
    assert!(
        matches!(err, PortError::CasConflict { .. }),
        "跳过 prepare 的提交必须报冲突，而不是默默切换"
    );
    assert_eq!(dir.revision(), before, "失败的提交不许留下任何状态变更");
    assert!(
        dir.is_routable(&old.handle) && !dir.is_routable(&candidate_handle),
        "失败的提交之后旧会话原样可路由，候选依旧不存在"
    );

    let err = dir
        .commit_replacement(commit_for(
            &old.handle,
            &candidate,
            ReplacementOperation::RolledBack,
        ))
        .await
        .expect_err("没 prepare 就 rollback 也必须失败");
    assert!(matches!(err, PortError::CasConflict { .. }));
    assert_eq!(dir.revision(), before, "rollback 失败同样不留下状态变更");
}

#[tokio::test]
async fn unknown_commit_outcome_holds_both_sides_until_status_is_queried() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0005", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();
    let key = ReplacementOperationKey::for_handle(&old.handle);

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    // 服务端其实已提交，只是确认没送到调用方。
    dir.lose_next_commit_reply(Some(UnknownCommitOutcome::Committed));

    let err = dir
        .commit_replacement(commit_for(
            &old.handle,
            &candidate,
            ReplacementOperation::Committed,
        ))
        .await
        .expect_err("结果未知时不得回成功");
    assert!(
        matches!(err, PortError::ProviderTimeout(_)),
        "结果未知必须是超时语义，而不是假成功或假失败"
    );

    // 屏障期间两边都不可路由——绝不允许新旧同时执行。
    assert!(
        !dir.is_routable(&old.handle),
        "结果未知时旧会话必须停在屏障上，不能继续执行"
    );
    assert!(
        !dir.is_routable(&candidate_handle),
        "结果未知时新会话也不可路由：新旧不得同时执行"
    );
    assert!(
        dir.touch(&old.handle, Timestamp::new("2026-01-01T00:02:00.000Z"))
            .await
            .is_err(),
        "屏障期间旧句柄不得刷新任何业务时间"
    );
    // 屏障期间业务侧彻底停摆：执行入口也一起拒，避免「查清之前先跑一条」。
    assert!(
        dir.observe(&old.handle).is_err() && dir.observe(&candidate_handle).is_err(),
        "屏障期间新旧两边都读不到 owner"
    );

    // 按 operation key 查：给出确定答案，且只落定一边。
    let status = dir.commit_status(&key);
    assert_eq!(
        status,
        CommitStatus::Committed {
            handle: candidate_handle.clone()
        },
        "按 key 查状态必须直接给出已提交，不能让调用方二选一"
    );
    assert!(
        dir.is_routable(&candidate_handle) && !dir.is_routable(&old.handle),
        "查清之后恰好一边可路由"
    );
    // 幂等：再查多少次都是同一个答案，状态不再抖动。
    for _ in 0..3 {
        assert_eq!(dir.commit_status(&key), status, "状态查询必须幂等");
    }
    assert!(
        dir.is_routable(&candidate_handle) && !dir.is_routable(&old.handle),
        "重复查询不许把两边都放行"
    );
}

#[tokio::test]
async fn unknown_rollback_outcome_restores_the_old_entry_on_query() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0006", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();
    let key = ReplacementOperationKey::for_handle(&old.handle);

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    // 服务端其实已回滚，但应答丢了。
    dir.lose_next_commit_reply(Some(UnknownCommitOutcome::RolledBack));

    let err = dir
        .commit_replacement(commit_for(
            &old.handle,
            &candidate,
            ReplacementOperation::RolledBack,
        ))
        .await
        .expect_err("结果未知时不得回成功");
    assert!(matches!(err, PortError::ProviderTimeout(_)));
    assert!(
        !dir.is_routable(&old.handle),
        "回滚结果未知时旧会话同样要停在屏障上，不能半信半疑地继续跑"
    );

    let status = dir.commit_status(&key);
    assert_eq!(
        status,
        CommitStatus::RolledBack {
            restored: old.handle.clone()
        },
        "按 key 查状态必须直接给出已回滚"
    );
    assert!(
        dir.is_routable(&old.handle) && !dir.is_routable(&candidate_handle),
        "回滚落定后只剩旧会话可路由"
    );
    assert!(dir
        .lookup(candidate_handle.db_session_id.clone())
        .await
        .expect("查询可路由条目不该报错")
        .is_none());
}

#[tokio::test]
async fn status_of_an_unknown_operation_key_is_pending() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let key = ReplacementOperationKey::for_handle(&old.handle);
    assert_eq!(
        dir.commit_status(&key),
        CommitStatus::Pending,
        "没有登记过的操作键必须是 Pending，而不是编一个答案"
    );
}

#[tokio::test]
async fn old_attachment_token_grants_nothing_on_the_new_session() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0007", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Committed,
    ))
    .await
    .expect("commit 应成功");

    assert!(
        matches!(
            attach_ok(&dir, &candidate_handle, old.token.clone()).await,
            AttachmentOutcomeAssert::Rejected(AttachmentRejection::NoTokenIssued)
        ),
        "旧会话的令牌在新会话上不能换到任何挂载权：令牌按会话签发"
    );
    assert!(
        matches!(
            attach_ok(&dir, &candidate_handle, foreign_token()).await,
            AttachmentOutcomeAssert::Rejected(AttachmentRejection::NoTokenIssued)
        ),
        "新会话在签发自己的令牌之前，谁的令牌都不认"
    );
}

#[tokio::test]
async fn replacement_after_expiry_is_refused_and_never_resurrects_the_old_entry() {
    let (clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0008", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();

    // 旧会话开着事务空闲期限。
    dir.arm_transaction_idle(&old.handle, Duration::from_secs(30), clock.instant())
        .expect("应能挂上事务空闲期限");
    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");

    // 时钟越过期限；到期判定由同一个 actor 串行做，这里显式触发它。
    clock.advance(Duration::from_secs(31));
    let adjudication = dir.adjudicate(&old.handle).expect("到期判定必须给出结论");
    assert!(
        adjudication.is_expired(),
        "越过事务空闲期限后必须判死，否则会拿一个死连接去替换"
    );
    assert!(!dir.is_routable(&old.handle), "判死之后旧条目立刻不可路由");

    let err = dir
        .commit_replacement(commit_for(
            &old.handle,
            &candidate,
            ReplacementOperation::Committed,
        ))
        .await
        .expect_err("旧会话已判死时不得再切换");
    assert!(
        matches!(err, PortError::CasConflict { .. }),
        "已判死的条目必须显式失败，而不是复活它去做替换"
    );
    assert!(
        !dir.is_routable(&candidate_handle),
        "失败的提交不许顺带把候选放行"
    );
    assert!(
        matches!(
            dir.commit_status(&ReplacementOperationKey::for_handle(&old.handle)),
            CommitStatus::Pending
        ),
        "没有落定的替换不许给出已提交的假答案"
    );
    assert!(
        dir.lookup(candidate_handle.db_session_id.clone())
            .await
            .expect("查询不该报错")
            .is_none(),
        "候选自始至终都没有进过可路由名单"
    );
}

// ---------------------------------------------------------------------------------------------
// 补放行：只有「确实还卡在屏障里」时才该动手
// ---------------------------------------------------------------------------------------------

/// 候选放行之后被调用方关掉，此后每一次按 key 查状态**都不得把它放回来**。
///
/// 屏障挡着的时候，按 key 查状态是唯一的解法，所以「提交其实已经落定、只是确认没送到
/// 调用方」这一格必然会走到「给候选补一次放行」。补放行存在的理由只有一条：放行那一步没跑到。
/// 而候选在被问到时有三种可能——还卡在屏障里（补上正合适）、已经可路由（早跑过了，补了是
/// 多余的）、已经被关闭或判死（补上去就是**复活**）。前两种补了没害处，第三种会让一个什么都
/// 执行不了的 id 重新出现在可路由名单里，而重试方随后拿着一枚「替换已提交」的回执去附着一个
/// 已经关掉的会话。所以补放行必须先问状态、自己决定动不动手，不能无条件重写。
///
/// 这个顺序不是构造出来刁难实现的：调用方关掉候选之后，重放会**再次**问同一个 key
/// （提交记录一直留着，因为记录的是「提交落定过」而不是「候选还活着」），于是每一次重试
/// 都会走进那一格。
#[tokio::test]
async fn 查状态不得复活已关闭的候选() {
    let (_clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0007", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();
    let key = ReplacementOperationKey::for_handle(&old.handle);

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    // 服务端其实已提交，只是确认没送到调用方：这一格之后只能靠查状态落定。
    dir.lose_next_commit_reply(Some(UnknownCommitOutcome::Committed));
    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Committed,
    ))
    .await
    .expect_err("结果未知时不得回成功");

    // 第一次查状态：提交落定，候选放行。此后它就是个正常活着的会话。
    let settled = CommitStatus::Committed {
        handle: candidate_handle.clone(),
    };
    assert_eq!(dir.commit_status(&key), settled);
    assert!(
        dir.is_routable(&candidate_handle),
        "前置条件：落定之后可路由"
    );

    // 调用方把它关掉。关闭是终态，此后任何路径都不得把它拉回来。
    dir.release(&candidate_handle, CloseDisposition::Closed)
        .await
        .expect("关闭候选必须成功");
    assert!(
        !dir.is_routable(&candidate_handle),
        "前置条件：关闭之后不可路由"
    );

    // 答案本身不受影响：提交确实发生过，候选没了并不会让它变成「没提交」。
    assert_eq!(
        dir.commit_status(&key),
        settled,
        "提交确实落定过，查状态必须照实答已提交"
    );
    assert!(
        !dir.is_routable(&candidate_handle),
        "候选已被关闭，查状态不得把它放回可路由"
    );
    assert!(
        !dir.is_routable(&old.handle),
        "旧条目同样不得复活：它已经被换掉，关着就是终态"
    );
    // 幂等：现实里重试随时可能发生，再查多少次都不该有任何一边动。
    for _ in 0..3 {
        assert_eq!(dir.commit_status(&key), settled);
        assert!(
            !dir.is_routable(&candidate_handle) && !dir.is_routable(&old.handle),
            "重复查询不得复活任何一边"
        );
    }
}

/// 已经放行过的候选被重复查状态时，不得顺手撤掉它的期限。
///
/// 补放行如果无条件重写整份条目状态，就会连期限一起清掉：候选明明活得好好的，空闲期限却
/// 被撤了，于是它变成一个**只有显式关闭才会消失**的会话——扫不到、过期不了、还占着名额。
/// 所以补放行只在「确实还卡在屏障里」时才动手，其余情况原样不碰。
#[tokio::test]
async fn 查状态不得撤掉活着的候选的期限() {
    let (clock, dir) = cycling_directory(64);
    let old = open(&dir).await;
    let candidate = candidate_owner("dbs_candidate_0008", NEXT_EPOCH, "2026-01-01T00:00:00.000Z");
    let candidate_handle = candidate.to_handle();
    let key = ReplacementOperationKey::for_handle(&old.handle);

    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Prepared,
    ))
    .await
    .expect("prepare 应成功");
    dir.lose_next_commit_reply(Some(UnknownCommitOutcome::Committed));
    dir.commit_replacement(commit_for(
        &old.handle,
        &candidate,
        ReplacementOperation::Committed,
    ))
    .await
    .expect_err("结果未知时不得回成功");
    assert_eq!(
        dir.commit_status(&key),
        CommitStatus::Committed {
            handle: candidate_handle.clone()
        }
    );

    // 放行之后给候选装上空闲期限：此刻它是个正常活着的会话。
    dir.arm_session_idle(
        &candidate_handle,
        Duration::from_secs(80),
        MonoInstant::ZERO,
    )
    .expect("装期限必须成功");

    // 再查一次。
    assert_eq!(
        dir.commit_status(&key),
        CommitStatus::Committed {
            handle: candidate_handle.clone()
        }
    );
    assert!(
        dir.is_routable(&candidate_handle),
        "已放行的候选必须仍然可路由"
    );

    // 期限还在，才谈得上「到期能被扫掉」：期限被撤掉的话这条永远判不出到期，
    // 会话于是占着名额又不再被回收。
    clock.advance(Duration::from_secs(81));
    assert!(
        dir.adjudicate(&candidate_handle)
            .expect("到期判定必须给出结论")
            .is_expired(),
        "空闲期限被查状态撤掉了：一个活着的会话从此不再被回收"
    );
}
