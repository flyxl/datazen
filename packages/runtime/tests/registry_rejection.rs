//! CM-74 替换路径的**拒绝侧**断言：被拒的替换一点痕迹都不许留。
//!
//! 这一组是从 `registry_context.rs` 里分出来的，单文件 800 行上限所致——但分出来的理由
//! 不是「装不下」，而是这三条用例盯的东西与其余七条**不同类**：
//! 其余七条证明替换**成功**时四步的顺序与令牌的作数范围；这三条证明替换**没成功**时
//! 系统既没有偷偷换掉旧会话、没有偷偷吃掉一格额度，也没有把一枚**死令牌**当成功回执发回。
//! 把两类断言混在一个文件里，「拒绝路径没人测」这件事会被「成功路径测得很全」掩盖过去。
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | 修订号不符一律拒绝且旧会话一点不动 | 乐观并发闸门 | §4.4 `:392`、§7.1 `:526` |
//! | 候选发布失败时额度回到原值且撞号的那一行不动 | 提交后退款 | §7.4-6 `:561`、§12 |
//! | 新会话已从目录里作废后同键重放不得发回死令牌 | 幂等回放的存活性复核 | §7.4 `:566`、§13.1 `:800` |
//!
//! 闸门的权威就是那两行，没有第三处：`:392` 说 `configRevision/contextRevision`
//! 「用于版本与上下文冲突，**不能证明物理资源连续性**」——所以拿它当乐观闸门是正当用途；
//! `:526` 说「后续排队请求仍要**重新校验** `contextRevision`」——所以漏了校验就是漏了协议。
//!
//! ## 这些断言靠什么活着（变异清单，勿删）
//!
//! 台账合并时会被删掉，所以「哪几个变异会让本组变红」必须写在文件里。W-06 说的就是
//! 这件事：缺陷清单可以只活在台账里，「哪几个变异会让本组变红」不行。四条都实测过：
//!
//! - **删掉上下文修订闸门**（`context.rs` 里那个 `if` 整个拿掉）：**只有「修订号不符」
//!   那条红**，返回一枚指向新会话的回执。本组其余断言全绿——因为 `registry_context.rs`
//!   的既有用例一律传权威修订号（当时六条，现七条，含 T5b）。**这就是 W-04 本身**：
//!   §7.4 的并发安全承诺曾被六条全绿的用例「覆盖」，而它们从不肯传错修订号——把闸门整个删掉，整套 CM-74 用例依旧
//!   全绿。承诺没被证明过，只被数过。反证只有这一条，所以这一条不能删。
//! - **去掉 `publish_candidate` 失败分支里的 `refund_candidate`**：**只有「候选发布失败」
//!   那条红**，`remaining_quota` 少一格（`left: 2 / right: 3`）。
//! - **把孤儿态判据从「世代相等」弱化成「id 在册」**：只有「候选发布失败」那条红，
//!   重试直接返回 `Ok(ContextChangeReceipt)`，且回执里的 `session` 是**撞号那一行**
//!   （`dbs_2`）——比不发回执更糟：等于把别人的会话连同新签的令牌发了出去。
//! - **删掉 `replayed_token` 里对缓存令牌的存活性复核**（`context.rs` 里那个
//!   `owner_of(...).is_some()`，连带 `remove(&cache_key)` 那个丢弃分支）：只有「新会话
//!   已从目录里作废后同键重放不得发回死令牌」那条红，`replace` 返回 `Ok` 且回执里是那枚
//!   **再也附着不上**的令牌；`registry_context.rs` 那七条对此**全绿**——它们从没把关过
//!   新会话。反过来把丢弃分支换成 `panic!`，panic 会真的响：这条分支第一次有了执行证据，
//!   而此前它只在台账里被论证过。**这就是本条自己的由来**：论证不算证据，台账删掉之后
//!   连「论证过」都不会剩下。被论证过的安全分支，必须在本组里有一条能把它变红的用例。
//!
//! 第二、三条尤其值得留着：它们钉的不是「有个判断」，而是「判断的**粒度**对」。
//!
//! ★ 这两条是 CM-74 闸门用例，且它们**在更早的一轮里是新建文件**：只 diff
//! `registry_context.rs`（那次是纯文档改动、零代码、零删除用例）会安静地漏掉本文件，
//! 看上去「一条用例都没丢」。判断 CM-74 覆盖有没有回退，本文件必须一起看。

#![allow(dead_code)]
mod common;
mod registry_fixtures;

use std::sync::Arc;

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::id::{
    ClientInstanceId, DbSessionId, EditorSessionId, OrganizationId, PrincipalId, RuntimeEpoch,
    Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::{InvalidationReason, SessionDirectory};
use datazen_runtime::connection::{
    ConnectionId, Counter, ExecutionTarget, ObjectTarget, RuntimeError, SessionHandle,
};
use datazen_runtime::registry::{
    ContextChangeRequest, ContextReplacer, SessionPort, SessionRegistry,
};
use registry_fixtures::{
    as_backend, db_session_id, directory_handle_of, epoch_string, execute_request, handle_of,
    handle_ref, namespace, register_ready, BackendPlan, ScriptedBackend, FIRST_SESSION_EPOCH,
    SESSION_LIMIT,
};

/// 旧会话在**目录**侧的条目。身份必须与 `registry_fixtures::open_request` 的 owner 对齐
/// （`pr_1` / `cli-1` / `ed-1`），否则测的就不是替换而是夹具串台。
fn old_owner() -> datazen_platform_api::ports::session_directory::SessionOwner {
    datazen_platform_api::ports::session_directory::SessionOwner {
        db_session_id: db_session_id(),
        organization_id: OrganizationId::new("org_1"),
        principal_id: PrincipalId::new("pr_1"),
        connection_id: ConnectionId::new("conn_1"),
        owner: OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("cli-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        },
        worker_id: WorkerId::new("w_1"),
        // 与 `ContextReplacer::directory_handle` 的派生式严格对齐：登记表第一个会话拿
        // `Counter(1)`，于是 `rte-{:08}` 落在 `rte-00000001`。
        runtime_epoch: RuntimeEpoch::new(epoch_string(FIRST_SESSION_EPOCH)),
        resource_epoch: 0,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

/// 换一个库表作为期望上下文——`setSessionContext` 的意义就在这里。
fn desired_target() -> ExecutionTarget {
    ExecutionTarget {
        connection_id: ConnectionId::new("conn_1"),
        namespace: namespace(),
        object: Some(ObjectTarget {
            kind: "table".to_owned(),
            name: "customers".to_owned(),
            signature: String::new(),
        }),
    }
}

fn context_request(
    handle: &SessionHandle,
    expected_context_revision: u64,
    candidate_db_session_id: DbSessionId,
) -> ContextChangeRequest {
    ContextChangeRequest {
        handle: handle.clone(),
        expected_context_revision,
        desired: desired_target(),
        candidate_db_session_id,
        principal_id: PrincipalId::new("pr_1"),
        client_instance_id: ClientInstanceId::new("cli-1"),
    }
}

/// 旧会话登记 + 发一次执行，把两个句柄登记在它自己身上。
async fn old_session_with_two_handles(registry: &Arc<SessionRegistry>) -> (SessionHandle, Counter) {
    let view = register_ready(registry, db_session_id()).await;
    let handle = handle_of(&view);
    registry
        .submit_execution(execute_request(&handle, 1))
        .await
        .expect("带句柄的执行必须完成");
    let context_revision = registry
        .session_view(&handle)
        .await
        .expect("旧会话必须还在")
        .context_revision;
    (handle, context_revision)
}

fn registry_with(backend: &Arc<ScriptedBackend>) -> Arc<SessionRegistry> {
    Arc::new(SessionRegistry::new(as_backend(backend), SESSION_LIMIT))
}

// ---------------------------------------------------------------------------------------------

// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 修订号不符一律拒绝且旧会话一点不动() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(2, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let (_clock, directory) = common::cycling_directory(64);
    let directory = Arc::new(directory);
    directory
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");

    let (old_handle, context_revision) = old_session_with_two_handles(&registry).await;
    let quota_before = registry.remaining_quota();
    let candidate_id = DbSessionId::new("dbs_2");
    let replacer = ContextReplacer::new(registry.clone(), directory.clone());

    // 故意传一个**错**的修订号。这是本组 6 条既有用例全都没覆盖的一面：它们一律传
    // `context_revision.get()`（权威值），所以闸门哪怕被整个删掉，6 条仍然全绿。
    let wrong = context_revision.get() + 1;
    let error = replacer
        .replace(context_request(&old_handle, wrong, candidate_id.clone()))
        .await
        .expect_err("修订号不符必须拒绝，绝不允许基于『大概没变』替换");

    // 拒绝的理由必须是**可比对的**修订冲突，不是碰巧撞上的别的失败。
    match error {
        RuntimeError::ContextRevisionMismatch { expected, actual } => {
            assert_eq!(expected, wrong, "回执必须写明调用方期待什么");
            assert_eq!(actual, context_revision.get(), "回执必须写明实际是什么");
        }
        other => panic!("必须是 ContextRevisionMismatch，实际是 {other:?}"),
    }

    // 拒绝之后**旧会话一点不动**：额度不变、表项还在、目录仍可路由。
    assert_eq!(
        registry.remaining_quota(),
        quota_before,
        "被闸门拦下的替换不得动额度：候选从未开启，也没有东西可退"
    );
    assert_eq!(
        registry.registered_ids(),
        vec![db_session_id()],
        "旧会话必须仍在册；被拒的替换不得注销它"
    );
    assert!(
        registry.session_view(&old_handle).await.is_ok(),
        "旧会话必须仍可查询"
    );
    assert!(
        directory.is_routable(&directory_handle_of(&old_handle)),
        "目录里旧会话必须仍可路由"
    );
    assert!(
        !directory.contains(&candidate_id),
        "候选必须从未在目录里出现过"
    );
    // 闸门之后一步都不许走：旧 actor 的闸门没被举起来，也就没有「再放下」这件事。
    let traces = backend.traces().await;
    assert!(
        traces.closed_with.is_empty(),
        "被拒的替换不得触碰旧物理资源"
    );
}

// ---------------------------------------------------------------------------------------------
// T8：候选发布失败的额度归还（W-01）
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 候选发布失败时额度回到原值且撞号的那一行不动() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(2, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let (_clock, directory) = common::cycling_directory(64);
    let directory = Arc::new(directory);
    directory
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");

    let (old_handle, context_revision) = old_session_with_two_handles(&registry).await;

    // 先把候选 id（`dbs_2`）占住，`publish_candidate` 必然撞号。这正是 W-01 的成因：
    // 全仓只有 `abort_candidate`（提交**前**）调过 `refund_candidate`，提交**后**的
    // 这条失败路径上没有任何额度退还，撞号一次就永久吃掉一格。
    let colliding = register_ready(&registry, DbSessionId::new("dbs_2")).await;
    let colliding_handle = handle_of(&colliding);
    let replacer = ContextReplacer::new(registry.clone(), directory.clone());

    replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            DbSessionId::new("dbs_2"),
        ))
        .await
        .expect_err("候选 id 撞号必须让替换失败，不能悄悄覆盖在册行");

    // 承重断言：额度账必须与**在册行数**对上，一格都不能多占。
    //
    // 这里不能断言「等于调用前的快照」：旧会话已经走完 §9.4 被注销了，它那一格由
    // `forget` 合法归还，所以调用后比调用前**多**一格才对。真正的判据是账实相符——
    // `SESSION_LIMIT - 在册行数`：
    //   4 − 1（只剩撞号那行）= 3   ← 候选那一格已被 `refund_candidate` 退回
    //   4 − 2                     ← W-01 复发：候选那一格卡死，账比实多占一格
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "发布失败必须退还候选占掉的额度；额度只有 `refund_candidate` 这一条退路，\
         候选从未进表所以 `forget` 对它无效"
    );
    assert_eq!(
        registry.remaining_quota() + registry.registered_ids().len(),
        SESSION_LIMIT,
        "额度账必须与在册行数严格账实相符：多占一格就是永久泄漏，且无人会想起它"
    );

    // 撞号的那一行**不受牵连**：它是别人的会话，不能被替换顺手摘掉。
    assert!(
        registry.is_registered(&DbSessionId::new("dbs_2")),
        "撞号的在册行必须原样保留"
    );
    assert!(
        registry.session_view(&colliding_handle).await.is_ok(),
        "撞号的在册行必须仍可查询"
    );

    // 孤儿态如实描述：旧会话已注销（§9.4 已走过），候选从未进表。
    assert_eq!(
        registry.registered_ids(),
        vec![DbSessionId::new("dbs_2")],
        "旧会话已走完 §9.4 被注销；候选从未发布，故不在册"
    );

    // 重试不得退化成「把孤儿句柄当回执发出去」：目录已提交，注册表却认不出这一行。
    let retry = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            DbSessionId::new("dbs_2"),
        ))
        .await
        .expect_err("重试必须给出可判定的失败，而不是一枚无宿主入口的句柄");
    match retry {
        RuntimeError::InvariantBroken(detail) => assert_eq!(
            detail, "committedWithoutHostEntry",
            "重试必须停在孤儿态上，不能签发回执"
        ),
        other => panic!("必须是 committedWithoutHostEntry，实际是 {other:?}"),
    }
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "重试也不得再吃额度：短路在开候选之前就返回了，一格都不该多占"
    );
}

// ---------------------------------------------------------------------------------------------
// T9：幂等回放的存活性复核——新会话没了就按拒绝处理，不发回死令牌
// ---------------------------------------------------------------------------------------------

/// 替换**曾经**成功、而那一代会话随后从目录里消失之后，同键重放必须给 `Err`，不是那枚令牌。
///
/// §13.1 `:800` 承诺「响应丢失时，在原 runtime 内同键返回同 session/token/候选提交结果」，
/// 承诺的前提是那一代会话**还在**。会话没了之后前提没了，此时若照原样返回，等于把一枚
/// 目录侧已经作废的令牌当成功回执发回去：调用方拿到 `Ok`，紧接着挂载必失败——比当场
/// 报错更难排查，因为它看起来是成功的。
///
/// 这条用例存在的原因不是「顺手补一条」，而是 `replayed_token` 里那几行存活性复核此前
/// 只被论证、没被钉过：把整个复核删掉，其余用例**全绿**，而台账里恰恰写着这条分支
/// 「不会把死令牌当成功回执发回」。论证不算证据。
///
/// 要逼出这条分支只能**作废**新会话，不能**关闭**它。关闭（§12 的 `release`）把条目留在
/// 表里只让它不再可路由，而重放的第一步是问目录「这次替换提交下来没有」，`commit_status`
/// 一见已提交就把候选条目重新 `publish()` 回可路由。于是被关闭的候选在重放途中会自己活
/// 过来：存活性复核根本不会被叫醒，那枚令牌照旧发回。这条口径问题属目录侧、已按只报告
/// 不修记入台账待裁定项；这里改用会**删掉**条目的 `invalidate`，因为「目录已经不认识这次
/// 会话」正是复核存在的理由。
#[tokio::test]
async fn 新会话已从目录里作废后同键重放不得发回死令牌() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(2, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let (_clock, directory) = common::cycling_directory(64);
    let directory = Arc::new(directory);
    directory
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");

    let (old_handle, context_revision) = old_session_with_two_handles(&registry).await;
    let candidate_id = DbSessionId::new("dbs_2");
    let replacer = ContextReplacer::new(registry.clone(), directory.clone());

    // 第一次替换必须真成功：本组要看的不是「替换失败」，而是「替换**成功过**、那一代会话
    // 随后没了」的时候重放会怎么答。
    let first = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("首次替换必须成功");
    let dead_token = first.attachment_token.clone();
    // 回执里的 `session` 就是目录句柄（`dto::SessionHandle`），`attach_from_client`
    // 收的正是它，所以直接用回执里这一枚。
    let new_session = first.session.clone();

    // 作废新会话：少数会**删掉**条目的处置，此后目录对这次会话的说法只有「不知道」。
    directory
        .invalidate(candidate_id.clone(), InvalidationReason::WorkerLost)
        .await
        .expect("作废新会话必须成功");
    assert!(
        !directory.contains(&candidate_id),
        "作废删条目：目录里不该再留着这次会话"
    );
    assert!(
        directory
            .lookup(candidate_id.clone())
            .await
            .expect("查目录必须成功")
            .is_none(),
        "存活性复核问的就是这个口径：查得到 owner 才算还在"
    );

    // 先证明那枚令牌**真的死了**：按回执里的用法把它挂回新会话必然失败。少了这一步，
    // 「发回死令牌」只是听上去糟糕，断言也就只是在钉一个想象出来的后果。
    directory
        .attach_from_client(
            &new_session,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            dead_token.clone(),
        )
        .expect_err("会话已作废：那枚令牌必然挂不上，把它当成功回执发回等于骗调用方");

    // 同键重放：owner 内存里此刻**确实**存着那枚令牌（存活性复核是唯一的出口），目录还
    // 留着「已提交」的操作记录、注册表也认这一代，所以短路会一路走到令牌回放那里。
    // 这一步唯一能返回的 `Ok` 就是那枚死令牌的回执，因此只能是 `Err`。
    let error = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect_err("新会话已作废，重放必须按拒绝处理，绝不能把死令牌当成功回执发回");
    match error {
        RuntimeError::CloseRejected(reason) => assert_eq!(
            reason, "attachmentRejected",
            "必须是签发侧判定的拒签，而不是碰巧撞上的别的失败"
        ),
        other => panic!("必须是 attachmentRejected，实际是 {other:?}"),
    }
}
