//! CM-74 路径级断言：`setSessionContext` 替换路径（§7.4-6 ⑥）。
//!
//! 这一组用例盯的是 CM-74 块（`:1320` 标题）里**断言行 `:1324`** 那句话在替换路径上
//! 真的成立：句柄在物理资源关闭**之前**已经在**原 resource** 上终结并从 actor 注销。
//! 既有覆盖 `registry_release.rs` 只证明了「释放器在一个自己搭的会话上按这个顺序做」，
//! 本组把它推到**编排层**：候选建连 → 目录原子发布 → 释放旧会话 → 回执，四步走完一遍。
//!
//! CM-74 那三行要分清，引错行就是引用错的话：
//!
//! - `:1322` 是**前置**：fake driver 的命令分别返回事务句柄与游标句柄各一个，均已按
//!   §6.5 登记；另备一个未登记的句柄。
//! - `:1323` 是**步骤**：推进到 idle 期限触发淘汰，在淘汰中途发起 commit；再分别走
//!   归池、`setSessionContext` 替换、`closeSession` 三条路径；最后提交未登记句柄。
//! - `:1324` 才是**断言**：句柄在物理资源关闭前已在原 resource 上回滚/关闭并从 actor
//!   注销；commit 落在旧 resource 或明确失败；driver 返回 Clean 时若宿主仍有已登记句柄，
//!   宿主检查必须失败；未登记句柄被拒；actor 终止后不重建任何句柄。
//!
//! 本组对的是 `:1324`。引 `:1323` 会引到步骤叙述上——「把归池排在第一步」这类读法
//! 就是把 `:1323` 的第二句当成了 `:1324` 的判据。
//!
//! 另有一个更硬的判据：**目录给替换候选签不出 attachment 令牌**，§7.4 `:564` 的回执字段因此
//! 原本无法实现。本组既证明它在提交前确实签不出（`Prepared` 的候选不可路由），也证明提交后
//! 签出的那枚令牌在新会话上**真的作数**、旧令牌**真的不作数**。
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | 替换成功_旧句柄在原资源上终结后旧资源才关闭 | §9.4 步骤 1 → 3 | CM-74 `:1324`（断言行）、§7.4 `:561` |
//! | 回执里的令牌在新会话上作数且旧令牌立即失效 | §7.4 `:564` | §7.4 `:564` |
//! | 屏障上的候选一律拒签令牌 | §12 `Prepared` → `CommitBarrier` | §7.4 `:561` |
//! | 提交前失败_候选销毁且旧会话恢复可用 | §7.4-6「提交前失败销毁候选」 | §7.4 `:561` |
//! | 重试同一旧句柄返回同一份回执且不重建候选 | §7.4 幂等 | §7.4 `:566` |
//! | 重试不换令牌_第一次那枚在重试之后仍然作数 | §7.4 幂等 + §13.1 响应分类 | §7.4 `:566`、§13.1 `:800` |
//! | driver 少报已终结句柄时替换仍发布但旧会话判失 | CM-74 clause 3 | §9.4 `:657` |
//!
//! **拒绝侧**（修订号不符、候选发布失败撞号、新会话已关闭后同键重放）在
//! `registry_rejection.rs`：它们盯的是替换**没成功**时系统不留痕，与本组盯的「替换发生
//! 时的顺序」不同类。改闸门、改退款或改幂等回放的**存活性复核**时，两个文件要一起跑——
//! 只跑本文件会漏掉它们（下面变异清单最后一条的反证就住在那个文件里）。
//!
//! ★ 复盘事实（W-03 之后的残留，写在这里以免随台账一起消失）：CM-74 的用例**不只在
//! 本文件**。`registry_rejection.rs` 的三条（T7 修订号不符 / T8 发布失败撞号与孤儿态 /
//! T9 新会话作废后同键重放）
//! 同样是 CM-74 闸门用例，而且在更早的一轮里是**新建文件**。**只 diff
//! `registry_context.rs` 会安静地漏掉它们**——本文件那次 diff 是纯文档 +14 行、零代码、
//! 零删除用例，看上去什么也没丢。要判断 CM-74 覆盖有没有回退，必须两个文件一起看。
//!
//! ## 这些断言靠什么活着（变异清单，勿删）
//!
//! W-06：这一段本身就是结论。缺陷清单可以只活在台账里，**「哪几个变异会让这组变红」
//! 不行**——台账合并时会被删掉，所以这段必须留在文件里。实测记录：
//!
//! - **幂等短路后移**（放到 `session_view` 之后）：T5 红，`UnknownSession("dbs_1")`。
//! - **删掉幂等重放、退回每次重签一枚令牌**：T5b 红，`TokenMismatch`——这正是 R2
//!   长期存在的「重签」实害，重签会把目录里的摘要换掉，调用方手上最初那枚当场作废。
//! - **删掉 `publish_candidate`**：T1/T5/T6 红（回执里没有新会话、额度对不上）。
//! - **闸门放行 `Close`/`View`**：T4 红，`CloseRejected("replacementInProgress")`。
//! - **删掉 `replayed_token` 里对缓存令牌的存活性复核**（`context.rs` 里那个
//!   `owner_of(...).is_some()`）：本组七条**全绿**，`context.rs` 里那个条目随之失效却没人
//!   喊。这条是本组自己的缺口，所以反证住在 `registry_rejection.rs` 的「新会话已从目录里
//!   作废后同键重放不得发回死令牌」：它先作废新会话再同键重放，实测那条红、其余全绿。
//! - **删掉上下文修订闸门**：本组七条**全绿**（含 T5b）——它们一律传权威修订号。这不是本组的缺口，
//!   是缺陷本身：闸门的反证在 `registry_rejection.rs`，没有它那就是个绿着的断言。

#![allow(dead_code)]
mod common;
mod registry_fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ClientInstanceId, DbSessionId, EditorSessionId, OrganizationId, PrincipalId,
    RuntimeEpoch, Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::{
    ReplacementCommit, ReplacementOperation, ReplacementOutcome, SessionDirectory, SessionOwner,
};
use datazen_runtime::connection::{
    ConnectionId, Counter, ExecuteInSessionRequest, ExecutionTarget, ObjectTarget, RuntimeError,
    SessionHandle,
};
use datazen_runtime::directory::{
    AttachmentRejection, CommitStatus, InMemorySessionDirectory, ReplacementOperationKey,
    SessionHandle as DirectoryHandle,
};
use datazen_runtime::registry::{
    ContextChangeRequest, ContextReplacer, ReplacementDirectory, SessionPort, SessionRegistry,
};
use registry_fixtures::{
    as_backend, db_session_id, directory_handle_of, epoch_string, execute_request, handle_of,
    handle_ref, namespace, register_ready, BackendPlan, ScriptedBackend, FIRST_SESSION_EPOCH,
    SECOND_SESSION_EPOCH, SESSION_LIMIT,
};

/// 旧会话在**目录**侧的条目。身份必须与 `registry_fixtures::open_request` 的 owner 对齐
/// （`pr_1` / `cli-1` / `ed-1`），否则回执里的令牌在新会话上验身份会直接被拒，
/// 测的就不是替换而是夹具串台。
fn old_owner() -> SessionOwner {
    SessionOwner {
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
// T1：CM-74 的顺序断言在替换路径上成立
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 替换成功_旧句柄在原资源上终结后旧资源才关闭() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            // driver 报 Clean：它认为自己把两个都终结了。
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

    let receipt = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("替换必须成功");

    // 回执指向新会话，旧会话的 id 落在 `replacedSessionId`。
    assert_eq!(
        receipt.session.db_session_id, candidate_id,
        "回执必须指向新会话"
    );
    assert_eq!(
        receipt.replaced_session_id,
        db_session_id(),
        "回执必须写明被替换掉的是谁"
    );
    // 候选是登记表**第二个**会话 ⇒ `Counter(2)`。这条同时证明候选确实
    // 走过 `open_candidate`，而不是拿旧句柄改头换面。
    assert_eq!(
        receipt.session.runtime_epoch.as_str(),
        epoch_string(SECOND_SESSION_EPOCH),
        "回执里的新会话必须是新铸的那一个"
    );

    let traces = backend.traces().await;

    // 步骤 1：句柄在**它们登记时的那个 resource** 上终结，带的就是登记的那两个句柄。
    let finalize = traces
        .finalized
        .iter()
        .find(|request| request.resource_id == "res_1")
        .expect("旧句柄必须在原资源 res_1 上终结");
    assert_eq!(finalize.handles.len(), 2, "终结请求必须带全部登记句柄");

    // 步骤 3：物理资源随后才关；关闭请求里的登记数是**释放之后**的账。
    let close = traces
        .closed_with
        .iter()
        .find(|request| request.resource_id == "res_1")
        .expect("旧物理资源必须被关闭");
    assert_eq!(
        close.registered_handles, 0,
        "关闭请求里的句柄数是释放后的账：0 才说明终结发生在关之前"
    );
    assert!(
        traces.closed_with.iter().all(|r| r.resource_id == "res_1"),
        "本用例只应关闭旧资源"
    );

    // 步骤 4：旧会话从登记表注销，额度归还给新会话。
    assert_eq!(
        registry.registered_ids(),
        vec![candidate_id],
        "旧会话必须从登记表注销"
    );
    assert!(
        registry.session_view(&old_handle).await.is_err(),
        "旧会话注销后 view 必须拿不到"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "旧会话归还一格、候选占住一格，净账必须正好剩 SESSION_LIMIT-1；\
         等于 SESSION_LIMIT 说明候选建连没占额度（漏记），等于 SESSION_LIMIT-2 说明旧额度没归还"
    );

    // 目录侧：新会话可路由，旧会话已被替换关闭。
    assert!(directory.is_routable(&receipt.session), "新会话必须可路由");
    assert!(
        !directory.is_routable(&directory_handle_of(&old_handle)),
        "被替换的旧会话必须不再可路由"
    );
}

// ---------------------------------------------------------------------------------------------
// T2：回执里的令牌在新会话上作数，旧令牌立刻失效（§7.4 `:564`）
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 回执里的令牌在新会话上作数且旧令牌立即失效() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let (_clock, directory) = common::cycling_directory(64);
    let directory = Arc::new(directory);
    directory
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");

    let (old_handle, context_revision) = old_session_with_two_handles(&registry).await;
    let old_directory_handle = directory_handle_of(&old_handle);
    // 旧会话自己的一枚令牌，替换前先在旧会话上用掉一次，证明它当时确实有效。
    let old_token = directory
        .issue_attachment_token(&old_directory_handle)
        .expect("旧会话此时可路由，必须签得出令牌");
    directory
        .attach_from_client(
            &old_directory_handle,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            old_token.clone(),
        )
        .expect("旧令牌在替换前必须可用");

    let replacer = ContextReplacer::new(registry.clone(), directory.clone());
    let receipt = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            DbSessionId::new("dbs_2"),
        ))
        .await
        .expect("替换必须成功");

    assert!(
        !receipt.attachment_token.is_empty(),
        "回执必须带一枚非空令牌"
    );
    // 载荷实打实走一遍校验：路由、摘要、principal、归属四关都在 `authorize_attachment` 里，
    // 任何一关不过都是 `Err`。已挂载返回 `AlreadyAttached` 也算过——挂载状态是幂等的，
    // 摘要与身份仍然逐项验过。
    directory
        .attach_from_client(
            &receipt.session,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            receipt.attachment_token.clone(),
        )
        .expect("回执里的令牌必须在**新会话**上作数");

    // §7.4 `:564`：「旧 token 不授予新 session 附着权」。
    let rejection = directory
        .attach_from_client(
            &receipt.session,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            old_token,
        )
        .expect_err("旧令牌绝不能在新会话上作数");
    assert!(
        matches!(rejection, AttachmentRejection::TokenMismatch),
        "旧令牌在新会话上必须按摘要不符被拒，实际：{rejection:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// T3：屏障上的候选一律拒签令牌（§12 `Prepared`）
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 屏障上的候选一律拒签令牌() {
    let (_clock, directory) = common::cycling_directory(64);
    let old = directory
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");

    let candidate_owner = SessionOwner {
        db_session_id: DbSessionId::new("dbs_2"),
        runtime_epoch: RuntimeEpoch::new(epoch_string(SECOND_SESSION_EPOCH)),
        resource_epoch: old_owner().resource_epoch + 1,
        ..old_owner()
    };
    directory
        .commit_replacement(ReplacementCommit {
            old: old.clone(),
            new_owner: candidate_owner,
            operation: ReplacementOperation::Prepared,
        })
        .await
        .expect("Prepared 必须被接受");

    let candidate = DirectoryHandle::new(
        DbSessionId::new("dbs_2"),
        RuntimeEpoch::new(epoch_string(SECOND_SESSION_EPOCH)),
    );
    let rejection = directory
        .issue_attachment_token(&candidate)
        .expect_err("屏障上的候选一律拒签");
    assert!(
        matches!(rejection, AttachmentRejection::NotRoutable(_)),
        "必须按不可路由拒签，实际：{rejection:?}"
    );

    // 反向：屏障**不得**波及旧会话，否则一次替换准备就把正在服务的会话掀了。
    assert!(
        directory.is_routable(&old),
        "Prepared 期间旧会话必须照旧可路由"
    );
    assert!(
        directory.issue_attachment_token(&old).is_ok(),
        "旧会话在屏障期间仍可签发"
    );
}

// ---------------------------------------------------------------------------------------------
// T4：提交前失败——候选销毁、旧会话恢复可用
// ---------------------------------------------------------------------------------------------

/// 在 `Committed` 上打一个 CAS 冲突，其余全部照常委派。
///
/// 打在 `Committed` 而不是 `Prepared` 上，才能让 `Prepared` 真的落进目录，候选条目
/// 因此经历完整的「隐形 → 回滚 → 消失」，而不是压根没进去过。
struct FailOnCommitDirectory {
    inner: Arc<InMemorySessionDirectory>,
    seen_operations: std::sync::Mutex<Vec<ReplacementOperation>>,
    /// 屏障窗口内旧会话的执行尝试：记下每一次尝试拿到的理由。
    ///
    /// 端口方法在 `Prepared` 提交那一刻被调用，而那一刻旧会话**正处在替换屏障里**
    /// （编排是 ⑦hold → ⑧Prepared）。所以这是唯一能站在屏障内部往外看一眼的时机。
    execute_while_held: Arc<std::sync::Mutex<Vec<String>>>,
    /// 屏障期间要探的那次执行，屏障开跑前填进来。
    probe: std::sync::Mutex<Option<ExecuteInSessionRequest>>,
    registry: Arc<SessionRegistry>,
}

#[async_trait]
impl ReplacementDirectory for FailOnCommitDirectory {
    async fn owner_of(
        &self,
        db_session_id: &datazen_platform_api::id::DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError> {
        self.inner.owner_of(db_session_id).await
    }

    fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus {
        self.inner.commit_status(key)
    }

    async fn commit(&self, commit: ReplacementCommit) -> Result<ReplacementOutcome, PortError> {
        self.seen_operations
            .lock()
            .expect("用例内锁")
            .push(commit.operation);
        // 只在 `Prepared` 这一刻探：这一刻旧会话正处在屏障里（编排是 ⑦hold → ⑧Prepared）。
        if commit.operation == ReplacementOperation::Prepared {
            // 守卫必须先出作用域：跨 `.await` 持锁，这个 future 就不 Send 了。
            let probe = self.probe.lock().expect("用例内锁").clone();
            if let Some(request) = probe {
                let reason = match self.registry.submit_execution(request).await {
                    Ok(_) => "UNEXPECTED_OK".to_string(),
                    Err(error) => format!("{error:?}"),
                };
                self.execute_while_held
                    .lock()
                    .expect("用例内锁")
                    .push(reason);
            }
        }
        if commit.operation == ReplacementOperation::Committed {
            return Err(PortError::cas_conflict(
                "replacement",
                "cm74 用例：故意拒绝提交",
            ));
        }
        self.inner.commit_replacement(commit).await
    }

    async fn issue_attachment_token(
        &self,
        handle: &DirectoryHandle,
    ) -> Result<AttachmentToken, AttachmentRejection> {
        self.inner.issue_attachment_token(handle)
    }

    async fn attach_client(
        &self,
        handle: &DirectoryHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<(), AttachmentRejection> {
        self.inner
            .attach_from_client(handle, principal_id, client_instance_id, token)
            .map(|_| ())
    }
}

#[tokio::test]
async fn 提交前失败_候选销毁且旧会话恢复可用() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let (_clock, directory) = common::cycling_directory(64);
    let inner = Arc::new(directory);
    inner
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");
    let port = Arc::new(FailOnCommitDirectory {
        inner,
        seen_operations: std::sync::Mutex::new(Vec::new()),
        execute_while_held: Arc::new(std::sync::Mutex::new(Vec::new())),
        probe: std::sync::Mutex::new(None),
        registry: registry.clone(),
    });

    let (old_handle, context_revision) = old_session_with_two_handles(&registry).await;
    // 装上探针：屏障窗口内往旧会话发一次执行，理由会被记下来。
    *port.probe.lock().expect("用例内锁") =
        Some(execute_request(&old_handle, context_revision.get()));
    let replacer = ContextReplacer::new(registry.clone(), port.clone());
    let candidate_id = DbSessionId::new("dbs_2");

    let outcome = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await;

    let error = outcome.expect_err("提交被拒时不得返回成功");
    assert!(
        matches!(
            error,
            RuntimeError::InvariantBroken("directoryRejectedReplacement")
        ),
        "目录拒绝须原样落成静态理由，实际：{error:?}"
    );
    assert_eq!(
        *port.seen_operations.lock().expect("用例内锁"),
        vec![
            ReplacementOperation::Prepared,
            ReplacementOperation::Committed,
            ReplacementOperation::RolledBack
        ],
        "失败必须走完 Prepared → Committed(被拒) → RolledBack 三步"
    );

    // 屏障**进入**侧：探针是在 `Prepared` 那一刻发的，此刻旧会话已 hold。
    // 没有这一条，下面的「屏障退出后恢复可执行」就只是「本来就一直可执行」，
    // 状态机只有出口没有入口——正是 AGENTS.md 明令禁止的那种单向逻辑。
    assert_eq!(
        *port.execute_while_held.lock().expect("用例内锁"),
        vec![format!(
            "{:?}",
            RuntimeError::CloseRejected("replacementInProgress")
        )],
        "屏障窗口内旧会话必须拒执行，且理由必须是 replacementInProgress"
    );

    // 旧会话原样继续：行在、view 在、还能再发执行（证明替换屏障已退出，不是只进不出）。
    assert!(
        registry.is_registered(&db_session_id()),
        "旧会话必须留在登记表"
    );
    registry
        .session_view(&old_handle)
        .await
        .expect("旧会话必须照常可查");
    registry
        .submit_execution(execute_request(&old_handle, context_revision.get()))
        .await
        .expect("屏障退出后旧会话必须恢复可执行");

    // 候选：登记表没有、目录没有、额度已归还。
    assert!(
        !registry.is_registered(&candidate_id),
        "被销毁的候选绝不能留在登记表"
    );
    assert!(
        !port.inner.contains(&candidate_id),
        "回滚后目录里不得残留候选条目"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "候选额度必须归还：回滚后只剩旧会话占一格，净账正好剩 SESSION_LIMIT-1"
    );

    // 后端只挨了一次 close：关的是候选。旧会话一次都没关过。
    assert_eq!(
        backend.close_calls(),
        1,
        "只该销毁候选，旧会话一个物理资源都不能碰"
    );
    assert!(
        port.inner.is_routable(&directory_handle_of(&old_handle)),
        "旧会话在目录里必须照旧可路由"
    );
}

// ---------------------------------------------------------------------------------------------
// T5：重试同一旧句柄——同一份回执，不重建候选
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 重试同一旧句柄返回同一份回执且不重建候选() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
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

    let first = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("首次替换必须成功");
    // 重试带**同一个候选 id**：若实现去重开候选，这里会因为 id 已被占用而失败。
    let second = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("重放必须靠幂等短路返回，而不是重走一遍");

    assert_eq!(
        second.session, first.session,
        "重放必须返回同一份回执里的同一个新会话"
    );
    assert_eq!(
        second.replaced_session_id, first.replaced_session_id,
        "重放必须认同一份被替换关系"
    );
    assert_eq!(
        backend.open_calls(),
        2,
        "旧会话 + 候选各建连一次；重放再建候选就说明幂等短路没生效"
    );
    assert_eq!(backend.close_calls(), 1, "旧会话只关一次");
    assert_eq!(
        registry.registered_ids(),
        vec![candidate_id.clone()],
        "重放不得动登记表"
    );
    assert!(
        directory.contains(&candidate_id),
        "重放后新会话必须仍在目录里"
    );
    // 被替换的旧条目**留在表里但不可路由**（§12 要留下替换关系），所以「旧会话没了」
    // 这件事只能按路由判，不能按是否在表里判——后者会永远为真，测不出任何东西。
    assert!(
        !directory.is_routable(&directory_handle_of(&old_handle)),
        "重放不得把旧会话重新变成可路由"
    );

    // 重放发回的是**同一枚**令牌（§7.4 `:566` 同键重试返回原 receipt、§13.1 `:800`
    // 同键返回同 session/token）。承重的不变量是新会话句柄、候选不重建、令牌不换新——
    // 这枚令牌必须在新会话上真的作数，否则重试等于给了个不能用的回执。令牌相等的
    // 断言在 T5b，那里还带一枚对照：目录每次签发确实都是新的一枚。
    assert_eq!(
        second.attachment_token, first.attachment_token,
        "重放不得换一枚新令牌：§7.4 `:566` 要求返回**原**回执，§13.1 `:800` 要求同键返回同 token"
    );
    directory
        .attach_from_client(
            &second.session,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            second.attachment_token.clone(),
        )
        .expect("重放发回的令牌必须在同一新会话上作数");
}

// ---------------------------------------------------------------------------------------------
// T5b：重试不换令牌——第一次那枚在重试之后仍然作数
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn 重试不换令牌_第一次那枚在重试之后仍然作数() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
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

    let first = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("首次替换必须成功");
    let second = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("重放必须靠幂等短路返回，而不是重走一遍");

    // 判据一：重放返回的就是**同一枚**令牌。§7.4 `:566`「同键重试返回原 receipt」，
    // §13.1 `:800`「在原 runtime 内同键返回同 session/token/候选提交结果」。
    assert_eq!(
        second.attachment_token, first.attachment_token,
        "同键重试必须发回同一枚令牌，不是重签一枚"
    );

    // 判据二（承重的那条）：**第一次那枚**在重试之后仍然作数。
    //
    // 这条比判据一更难绕过，也才是实害所在：网络层丢掉响应时调用方手上只有最初那枚。
    // 若实现每次重签，目录里的摘要会被换掉、最初那枚当场作废，而重试又是唯一的出路——
    // 于是「替换已提交」永远附着不上。此处能 `attach_from_client` 成功，是因为重试没有
    // 动过目录里的摘要。
    directory
        .attach_from_client(
            &second.session,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            first.attachment_token.clone(),
        )
        .expect("第一次那枚令牌必须在重试之后仍然作数（重签会让它当场作废）");

    // 对照：目录**每次签发都是新的一枚**，所以判据一不是恒真式。这一步刻意放在最后——
    // 它会换掉目录里的摘要，只影响它自己之后的附着。
    let freshly_issued = directory
        .issue_attachment_token(&second.session)
        .expect("新会话已发布，签发必然成功");
    assert_ne!(
        freshly_issued, first.attachment_token,
        "对照：目录重复签发会换一枚新令牌，所以上面的相等只能是 owner 回放出来的"
    );
}

// ---------------------------------------------------------------------------------------------
// T6：driver 少报已终结句柄——CM-74 clause 3 在替换路径上
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn driver少报句柄时替换仍发布但旧会话判失() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            // driver 报 Clean 却只终结了 1 个：§9.4 的宿主检查必须在**这里**失败。
            .finalizes(1, 0),
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

    let receipt = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("提交已落定，替换不得因为旧会话判失而回滚");

    assert_eq!(receipt.session.db_session_id, candidate_id);
    assert_eq!(
        registry.registered_ids(),
        vec![candidate_id],
        "旧会话判失也要注销"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "旧会话额度必须归还，不能永久卡死；候选占住一格后净账正好剩 SESSION_LIMIT-1"
    );

    // 关闭动作**照走**：四步跑完才写 `Lost`，不可判定的是某个句柄的命运，不是「有没有关掉」。
    let traces = backend.traces().await;
    assert!(
        traces.closed_with.iter().any(|r| r.resource_id == "res_1"),
        "旧物理资源必须已被关闭"
    );
}
