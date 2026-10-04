//! CM-74 路径级断言：`setSessionContext` 替换路径（§7.4-6 ⑥）。
//!
//! 这一组用例盯的是 **CM-74 `:1323` 那句断言**在替换路径上真的成立：句柄在物理资源关闭
//! **之前**已经在**原 resource** 上终结并从 actor 注销。既有覆盖 `registry_release.rs`
//! 只证明了「释放器在一个自己搭的会话上按这个顺序做」，本组把它推到**编排层**：候选建连 →
//! 目录原子发布 → 释放旧会话 → 回执，四步走完一遍。
//!
//! 另有一个更硬的判据：**目录给替换候选签不出 attachment 令牌**，§7.4 `:564` 的回执字段因此
//! 原本无法实现。本组既证明它在提交前确实签不出（`Prepared` 的候选不可路由），也证明提交后
//! 签出的那枚令牌在新会话上**真的作数**、旧令牌**真的不作数**。
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | 替换成功_旧句柄在原资源上终结后旧资源才关闭 | §9.4 步骤 1 → 3 | CM-74 `:1323`、§7.4 `:561` |
//! | 回执里的令牌在新会话上作数且旧令牌立即失效 | §7.4 `:564` | §7.4 `:564` |
//! | 屏障上的候选一律拒签令牌 | §12 `Prepared` → `CommitBarrier` | §7.4 `:561` |
//! | 提交前失败_候选销毁且旧会话恢复可用 | §7.4-6「提交前失败销毁候选」 | §7.4 `:561` |
//! | 重试同一旧句柄返回同一份回执且不重建候选 | §7.4 幂等 | §7.4 `:566` |
//! | driver 少报已终结句柄时替换仍发布但旧会话判失 | CM-74 clause 3 | §9.4 `:657` |

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
    as_backend, db_session_id, execute_request, handle_of, handle_ref, namespace, register_ready,
    BackendPlan, ScriptedBackend, SESSION_LIMIT,
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
        runtime_epoch: RuntimeEpoch::new("rte-00000001"),
        resource_epoch: 0,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

/// 与 `ContextReplacer` 里那份推导**逐字一致**。重放幂等靠的就是这个句柄派生出的
/// operation key，所以断言两端相等本身就是一条要守的契约。
fn directory_handle_of(handle: &SessionHandle) -> DirectoryHandle {
    DirectoryHandle::new(
        handle.db_session_id.clone(),
        RuntimeEpoch::new(format!("rte-{:08}", handle.runtime_epoch.get())),
    )
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
    // 候选是登记表**第二个**会话 ⇒ `Counter(2)` ⇒ `rte-00000002`。这条同时证明候选确实
    // 走过 `open_candidate`，而不是拿旧句柄改头换面。
    assert_eq!(
        receipt.session.runtime_epoch.as_str(),
        "rte-00000002",
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
        runtime_epoch: RuntimeEpoch::new("rte-00000002"),
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

    let candidate =
        DirectoryHandle::new(DbSessionId::new("dbs_2"), RuntimeEpoch::new("rte-00000002"));
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

    // 重放签发的是**新**令牌（目录只存摘要，上一枚原文不在任何人手里）。承重的不变量是
    // 同一个新会话句柄 + 不重开候选；这条令牌必须在新会话上真的作数，否则重试等于给了个
    // 不能用的回执。
    directory
        .attach_from_client(
            &second.session,
            PrincipalId::new("pr_1"),
            ClientInstanceId::new("cli-1"),
            second.attachment_token.clone(),
        )
        .expect("重放签发的令牌必须在同一新会话上作数");
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
