//! 判据**条款 (2)**（「commit 落在旧 resource
//! 或明确失败，不出现在新 resource」）在「替换提交 × 并发空闲驱逐」这条交错上的行为。
//!
//! 本组回答两件事，两件事的**结论不同**，所以分开写：
//!
//! 1. **「旧句柄的终结落到新 resource 上」这一半：结构上不可达。** 完整论证写在
//!    [`src/registry/context.rs`] 模块文档的「条款 (2) 的可达性」一节——那份论证活在
//!    代码旁而不是台账里，因为台账随合并删除。这里不构造它：构造不出来的东西不该
//!    假装有一个「通过」它的用例。**但不可达的理由里有一块是承重件**：新会话之所以
//!    躲得过并发驱逐，靠的不是「替换期间它不可见」（那个窗口在 `publish_candidate`
//!    之后就结束了，此后它就是一条普通在册会话），而是 `open_candidate` 用的
//!    `OpenRequest` 把 `idle_deadline_ms` 置成 `None`。承重件必须由用例钉住，
//!    否则它就只是一句注释——见 `发布后的候选没有空闲期限_驱逐对它是不动`。
//!
//! 2. **「明确失败」这一半：可达，且失败点在驱逐侧而不是替换侧。** §7.4-6 的发放闸门
//!    在替换期间把 `Evict` 拒成 `CloseRejected("replacementInProgress")`（`actor/context.rs`
//!    的 `gate`）。此时唯一正确的登记账面是「什么都没发生」：行留着、额度仍占着，
//!    因为替换编排第 ⑥ 步马上就要走那条唯一的 §9.4 释放例程去终结旧句柄。
//!    把这一格并进「已丢失」（摘行 + 还额度）就会留下真实缺陷：`close_registered`
//!    从此定位不到旧会话，**旧物理资源连同它的已登记句柄再也没有人处置**。
//!
//! ## 造法：站在屏障内部往外看一眼
//!
//! 探针挂在目录的 `Prepared` 提交那一刻——编排是 ⑥hold（举闸门）→ ⑦Prepared，所以
//! 这一刻旧会话**正在**屏障里，`Evict` 会被闸门拒。与 `registry_context.rs` 里那个
//! 「屏障窗口内发一次执行」的探针同一套路数。**没有任何靠时间推进制造出来的窗口**：
//! 时钟值是显式注入的（`evict_idle_at` 只吃调用方给的绝对毫秒），答复由 actor 自己给出。
//!
//! ## 这些断言靠什么活着（变异清单，勿删）
//!
//! 台账合并时会被删掉，所以「哪几个变异让本组变红」必须留在文件里：
//!
//! - **删掉 `evict_idle_at` 里 `Err(RuntimeError::CloseRejected(_))` 那个 arm**（让它
//!   落到 `forget`）：`屏障期间到来的驱逐不得摘行_旧句柄仍由替换例程在原资源终结` **红**——
//!   探针看到行已被摘、额度多还一格，并且整场替换结束时 `closed_with` 里**没有** `res_1`、
//!   `finalize_calls() == 0`：旧物理资源带着两个已登记句柄泄漏。
//! - **删掉闸门对 `Evict` 的拒绝**（`actor/context.rs` 的 `gate` 不再挡 `Evict`）：
//!   同一条**红**，但红法不同——驱逐在屏障里真的把 §9.4 跑完了，于是探针看到
//!   `evicted_len == 1` 且 `close_calls == 1`，而 `replace` 第 ⑥ 步的 `close_registered`
//!   拿到 `UnknownSession`（本例的 `expect` 会喊）。两种红法指向同一件事：
//!   这一格的答复**必须**是「什么都没发生」。
//! - **把 `evict_idle_at` 改成「所有答复一律不动」**（删掉 `Ok(Some)` 与 `SessionLost`
//!   两格）：`对照_没有屏障时到期驱逐照常摘行还额度` 与 `registry_release.rs` 的
//!   `归池时driver报clean但宿主仍有登记句柄不得算归池成功` 都会红——本组不是单向逻辑。
//! - **把第 1 步的取样退回 `drain_handles`（取走即注销）**：
//!   `旧句柄只在其登记时的资源上终结_新资源从不收到终结请求` 红在
//!   `closed_with[0].registered_handles` 那道**正例**断言上吗？不——那里两种实现都是 0。
//!   真正会红的是 `registry_release.rs` 里 driver 少报那两条（恒 0 ⇒ 判据失去可观测面）。
//!   本组的这一条钉的是**批次归属**（`resource_id`），与FU2 的账目形状正交。
//! - **把 `context.rs` 的 `open_request` 里 `idle_deadline_ms: None` 改成 `Some(..)`**：
//!   `发布后的候选没有空闲期限_驱逐对它是不动` **红**（候选被驱逐摘掉、额度多还一格）。
//!   这条是第 1 点那节论证的**承重件**反证。
//! - **把 `release` 第 1 步的分组依据从「句柄登记时的资源」换成「当前物理资源」**：
//!   `旧句柄只在其登记时的资源上终结_新资源从不收到终结请求` **红**——那就是条款 (2)
//!   「出现在新 resource」的形状本身。

#![allow(dead_code)]
mod common;
mod registry_fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ClientInstanceId, DbSessionId, EditorSessionId, OrganizationId, PrincipalId,
    RuntimeEpoch, Timestamp,
};
use datazen_platform_api::ports::session_directory::{
    ReplacementCommit, ReplacementOperation, ReplacementOutcome, SessionDirectory, SessionOwner,
};
use datazen_runtime::connection::{
    ConnectionId, Counter, ExecutionTarget, ObjectTarget, SessionHandle,
};
use datazen_runtime::directory::{
    CommitStatus, InMemorySessionDirectory, ReplacementOperationKey,
    SessionHandle as DirectoryHandle,
};
use datazen_runtime::registry::{
    ContextChangeRequest, ContextReplacer, ReplacementDirectory, SessionPort, SessionRegistry,
};
use registry_fixtures::{
    as_backend, db_session_id, epoch_string, execute_request, handle_of, handle_ref, namespace,
    open_request, register_ready, worker_id, BackendPlan, ScriptedBackend, FIRST_SESSION_EPOCH,
    IDLE_DEADLINE_MS, SECOND_SESSION_EPOCH, SESSION_LIMIT,
};

fn registry_with(backend: &Arc<ScriptedBackend>) -> Arc<SessionRegistry> {
    Arc::new(SessionRegistry::new(as_backend(backend), SESSION_LIMIT))
}

/// 旧会话在**目录**侧的条目。身份必须与 `registry_fixtures::open_request` 的 owner 对齐
/// （`pr_1` / `cli-1` / `ed-1`），否则回执里的令牌在新会话上验身份会被直接拒。
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
        worker_id: worker_id(),
        runtime_epoch: RuntimeEpoch::new(epoch_string(FIRST_SESSION_EPOCH)),
        resource_epoch: 0,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

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

/// 旧会话登记 + 一次执行，把**两个句柄**登记在它自己身上（都绑在 `res_1`）。
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

/// 屏障内一次驱逐尝试**当时**的登记表与后端账面。
///
/// 字段缺一不可：只看「返回值空不空」的话，`evicted` 为空在**正确**分类和
/// 「所有答复一律不动」两种实现下都成立——那正是这条判据最容易被假装覆盖的形状。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BarrierProbe {
    /// 探针里 `evict_idle_at` 的返回值长度。
    evicted_len: usize,
    /// 探针之后旧会话是否仍在册。
    old_still_registered: bool,
    /// 探针之后的剩余额度。
    remaining_quota: usize,
    /// 探针时后端已收到的终结请求批次数。
    finalize_calls: usize,
    /// 探针时后端已收到的关闭请求数。
    close_calls: usize,
}

/// 在 `Prepared` 提交那一刻（旧会话正举着发放闸门）跑一次**空闲驱逐**的目录替身。
struct ProbingDirectory {
    inner: Arc<InMemorySessionDirectory>,
    registry: Arc<SessionRegistry>,
    backend: Arc<ScriptedBackend>,
    /// 屏障期间要探的那次驱逐用的时钟值，屏障开跑前填进来。
    at_ms: std::sync::Mutex<Option<u64>>,
    probes: std::sync::Mutex<Vec<BarrierProbe>>,
}

impl ProbingDirectory {
    fn set_clock(&self, at_ms: u64) {
        *self.at_ms.lock().expect("用例内锁") = Some(at_ms);
    }

    fn probes(&self) -> Vec<BarrierProbe> {
        self.probes.lock().expect("用例内锁").clone()
    }
}

#[async_trait]
impl ReplacementDirectory for ProbingDirectory {
    async fn owner_of(
        &self,
        db_session_id: &DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError> {
        self.inner.owner_of(db_session_id).await
    }

    fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus {
        self.inner.commit_status(key)
    }

    async fn commit(&self, commit: ReplacementCommit) -> Result<ReplacementOutcome, PortError> {
        // 只在 `Prepared` 这一刻探：编排是 ⑥hold → ⑦Prepared，此刻旧会话在屏障里。
        if commit.operation == ReplacementOperation::Prepared {
            // 守卫必须先出作用域：跨 `.await` 持锁，这个 future 就不 `Send` 了。
            let at_ms = *self.at_ms.lock().expect("用例内锁");
            if let Some(at_ms) = at_ms {
                let evicted = self.registry.evict_idle_at(at_ms).await;
                self.probes.lock().expect("用例内锁").push(BarrierProbe {
                    evicted_len: evicted.len(),
                    old_still_registered: self.registry.is_registered(&db_session_id()),
                    remaining_quota: self.registry.remaining_quota(),
                    finalize_calls: self.backend.finalize_calls(),
                    close_calls: self.backend.close_calls(),
                });
            }
        }
        self.inner.commit_replacement(commit).await
    }

    async fn issue_attachment_token(
        &self,
        handle: &DirectoryHandle,
    ) -> Result<AttachmentToken, datazen_runtime::directory::AttachmentRejection> {
        self.inner.issue_attachment_token(handle)
    }

    async fn attach_client(
        &self,
        handle: &DirectoryHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<(), datazen_runtime::directory::AttachmentRejection> {
        self.inner
            .attach_from_client(handle, principal_id, client_instance_id, token)
            .map(|_| ())
    }
}

// ---------------------------------------------------------------------------------------------
// 条款 (2) 可达的那一半：屏障里的驱逐答复必须是「什么都没发生」
// ---------------------------------------------------------------------------------------------

/// §7.4-6 屏障期间到来的空闲驱逐**不得**摘行、不得还额度：旧句柄仍由替换例程在原资源上终结。
///
/// 判据是 CM-74 `:1324` 条款 (2) 的后半句「**或明确失败**」：驱逐被闸门拒掉时它既没有
/// 落在新 resource 上、也没有落在旧 resource 上——它**什么都没做**。此时唯一正确的账面
/// 是「旧会话仍在册、额度仍占着」。
///
/// 三种错法留下三种不同的账目错误，所以每一项都要看：
///
/// | 错法 | 探针看到 | 最终看到 |
/// | --- | --- | --- |
/// | `CloseRejected` 当「已丢失」摘行还额度 | 行没了、额度多一格 | 旧资源从未被关闭、句柄从未被终结（**泄漏**） |
/// | 闸门不挡 `Evict` | 驱逐真的跑了 §9.4 | 旧句柄在**提交之前**就被关掉，第 ⑥ 步拿 `UnknownSession` |
/// | 分类正确（现状） | 行在、额度在 | 终结与关闭都只发生在 `res_1` 上，各一次 |
#[tokio::test]
async fn 屏障期间到来的驱逐不得摘行_旧句柄仍由替换例程在原资源终结() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(2, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let (_clock, directory) = common::cycling_directory(64);
    let port = Arc::new(ProbingDirectory {
        inner: Arc::new(directory),
        registry: registry.clone(),
        backend: backend.clone(),
        at_ms: std::sync::Mutex::new(None),
        probes: std::sync::Mutex::new(Vec::new()),
    });
    port.inner
        .register(old_owner())
        .await
        .expect("旧会话必须在目录里");

    let (old_handle, context_revision) = old_session_with_two_handles(&registry).await;
    // 时钟**到期**：这次驱逐不是因为没到期才空手而归的。
    port.set_clock(IDLE_DEADLINE_MS);
    let candidate_id = DbSessionId::new("dbs_2");
    let replacer = ContextReplacer::new(registry.clone(), port.clone());

    let receipt = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("屏障内的驱逐不得打断替换：提交必须照常完成");

    // 探针必须真的跑过（否则下面的断言只是「没测」）。
    let probes = port.probes();
    assert_eq!(
        probes.len(),
        1,
        "探针必须恰好在 `Prepared` 那一刻跑一次；跑了 0 次说明本用例什么都没测"
    );
    let probe = &probes[0];
    assert_eq!(
        probe.evicted_len, 0,
        "被闸门拒掉的驱逐不得把旧会话算成已驱逐"
    );
    assert!(
        probe.old_still_registered,
        "派发前的拒绝必须留着表项：摘了行，替换第 ⑥ 步就定位不到旧会话，\
         旧物理资源连同它的已登记句柄再也没有人处置"
    );
    assert_eq!(
        probe.remaining_quota,
        SESSION_LIMIT - 2,
        "此刻旧会话（1 格）与候选（1 格）都还占着额度：屏障内的驱逐一格都不许还"
    );
    // 屏障内**零后端动作**：既没发终结请求，也没发关闭请求。
    assert_eq!(
        probe.finalize_calls, 0,
        "闸门拒掉的驱逐不得自己发一次句柄终结——那是第二条释放路径"
    );
    assert_eq!(probe.close_calls, 0, "闸门拒掉的驱逐不得关任何物理资源");

    assert_eq!(receipt.session.db_session_id, candidate_id);

    // 收尾：替换照常成功，且旧句柄只在**旧资源**上终结一次、旧资源只被关一次。
    let traces = backend.traces().await;
    assert_eq!(
        traces.finalized.len(),
        1,
        "整场替换只应有**一次**终结批次：屏障内的驱逐不得再来一次"
    );
    assert_eq!(
        traces.finalized[0].resource_id, "res_1",
        "旧句柄必须在它们登记时的资源上终结"
    );
    assert_eq!(traces.finalized[0].handles.len(), 2);
    assert_eq!(
        traces
            .closed_with
            .iter()
            .map(|r| r.resource_id.as_str())
            .collect::<Vec<_>>(),
        vec!["res_1"],
        "被替换掉的旧物理资源只关一次，且绝不允许漏关"
    );

    // 账：旧会话归还一格、候选占一格 ⇒ 净 `SESSION_LIMIT - 1`；行只剩候选。
    assert_eq!(registry.registered_ids(), vec![candidate_id]);
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);
    assert!(
        registry.session_view(&old_handle).await.is_err(),
        "旧会话注销后不得再可查"
    );
}

/// 对照：屏障**之外**的到期驱逐照常工作。
///
/// 没有这条，上面那条就只是「驱逐从此永远不动手」也能变绿——那是本项目反复踩过的
/// 单向逻辑（有进入、无退出）。这里用一个没在替换里的会话证明正常驱逐仍然成立。
#[tokio::test]
async fn 对照_没有屏障时到期驱逐照常摘行还额度() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1")])
            .finalizes(1, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);
    registry
        .submit_execution(execute_request(&handle, 1))
        .await
        .expect("带句柄的执行必须完成");

    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS).await;

    assert_eq!(evicted.len(), 1, "无屏障 ⇒ 到期驱逐必须照常成功");
    assert_eq!(backend.close_calls(), 1);
    assert!(!registry.is_registered(&db_session_id()));
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "驱逐成功必须归还额度"
    );
    assert!(registry.session_view(&handle).await.is_err());
}

// ---------------------------------------------------------------------------------------------
// 条款 (2) 不可达的那一半：可观测面与承重件
// ---------------------------------------------------------------------------------------------

/// 旧句柄只在其登记时的资源上终结——新资源（候选）从不收到终结请求。
///
/// 这一条**不是**在证明「那个交错发生了但结果正确」：那个交错（并发驱逐把旧句柄终结到
/// **新** resource 上）结构上不可达，论证见 `src/registry/context.rs` 模块文档。
/// 本例钉的是使那个结论成立的**可观测面**，读的是后端请求里的**取值**而不是行号或顺序编号：
///
/// 1. 每一条 `FinalizeHandles` 的 `resource_id` 都必须等于**句柄登记时**那个资源（`res_1`）；
/// 2. 候选那一代的资源是 `res_2`，它**从不**出现在终结批次里；
/// 3. 宿主账全空时 `CloseResource.registered_handles` 才等于 0——这是**正例**，
///    与 `registry_release.rs` 里 driver 少报 ⇒ 该值非 0 的那两条互为对照。
#[tokio::test]
async fn 旧句柄只在其登记时的资源上终结_新资源从不收到终结请求() {
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
    let replacer = ContextReplacer::new(registry.clone(), directory.clone());
    let candidate_id = DbSessionId::new("dbs_2");

    let receipt = replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("替换必须成功");

    // 候选是登记表**第二个**世代 ⇒ 夹具按世代号命名资源，所以它是 `res_2`。
    assert_eq!(
        receipt.session.runtime_epoch.as_str(),
        epoch_string(SECOND_SESSION_EPOCH),
        "本例第 2 点的前提：候选那一代必须是 {SECOND_SESSION_EPOCH}，于是它的资源是 res_2"
    );

    let traces = backend.traces().await;
    assert!(
        !traces.finalized.is_empty(),
        "旧会话登记了两个句柄 ⇒ 必须至少发出一次终结请求"
    );
    for batch in &traces.finalized {
        assert_eq!(
            batch.resource_id, "res_1",
            "终结请求只能落在句柄**登记时**的那个资源上；出现在 res_2 就是条款 (2) 被打破"
        );
    }
    let closed: Vec<&str> = traces
        .closed_with
        .iter()
        .map(|r| r.resource_id.as_str())
        .collect();
    assert_eq!(closed, vec!["res_1"], "新资源 res_2 不得被替换收尾关掉");
    assert_eq!(
        traces.closed_with[0].registered_handles, 0,
        "全部句柄都被如实确认终结 ⇒ 宿主账此时**真的**为空"
    );
}

/// 发布后的候选**没有空闲期限**，所以并发驱逐对它是一个不动作。
///
/// 这条是「新资源不会被驱逐路径关掉」那一半论证的**承重件**。候选之所以躲得过并发
/// 驱逐，靠的不是「替换期间它不可见」——那个窗口在 `publish_candidate` 之后就结束了，
/// 此后它就是一条普通的在册会话；靠的是 `open_candidate` 用的 `OpenRequest` 把
/// `idle_deadline_ms` 置成 `None`，于是 actor 侧 `evict_idle` 对「没有期限」回 `Ok(None)`，
/// 而 `Ok(None)` 在登记表这一侧是一次正常的空操作。
///
/// 把 `context.rs` 里 `open_request` 的 `idle_deadline_ms: None` 改成 `Some(..)`，
/// 本例立刻红（候选被驱逐摘掉、额度多还一格）——**到那一步上面那段论证就不再成立**，
/// 注释必须改写。所以这不是装饰性用例：它是那条不可达论证唯一可执行的支点。
#[tokio::test]
async fn 发布后的候选没有空闲期限_驱逐对它是不动() {
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
    replacer
        .replace(context_request(
            &old_handle,
            context_revision.get(),
            candidate_id.clone(),
        ))
        .await
        .expect("替换必须成功");
    assert_eq!(registry.registered_ids(), vec![candidate_id.clone()]);
    assert_eq!(backend.close_calls(), 1, "替换只关旧资源一次");

    // 时钟推到**旧会话**那个期限之后的很远位置：候选若不设期限就不该被动到。
    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS * 10).await;
    assert!(
        evicted.is_empty(),
        "候选没有空闲期限 ⇒ 任何时钟值下驱逐对它都是不动作；实际驱逐了 {} 项",
        evicted.len()
    );
    assert!(
        registry.is_registered(&candidate_id),
        "候选必须仍在册：被一次「无期限」的驱逐摘掉，就是本例要防的那一格"
    );
    assert_eq!(
        backend.close_calls(),
        1,
        "候选的物理资源不得被驱逐关掉——关掉它，旧句柄的终结就再没有对照可言"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "额度不得凭空多还一格"
    );
}

/// 正例对照：同一个时钟值下，**设了期限**的普通会话确实被驱逐。
///
/// 上面那条「候选没有期限所以不动」只有在「有期限就一定会动」的前提下才有信息量：
/// 否则它可能只是「驱逐对谁都不动」。这一条把 `open_request` 里带期限的会话与
/// 候选在同一条判据上分开——差异只有一个变量：**期限有没有**。
#[tokio::test]
async fn 正例对照_带期限的会话在同一时钟下确实被驱逐() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    registry
        .register_session(open_request(db_session_id()))
        .await
        .expect("带期限的登记必须成功");

    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS * 10).await;
    assert_eq!(
        evicted.len(),
        1,
        "设了期限 ⇒ 同一时钟值下驱逐必须动手；本组因此不是「驱逐对谁都不动」"
    );
    assert!(registry.registered_ids().is_empty());
}
