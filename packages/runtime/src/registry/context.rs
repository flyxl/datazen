//! §7.4 `setSessionContext` 的运行时契约层：**候选资源 + §12 原子发布**。
//!
//! 这一层此前不存在：`setSessionContext` 在 `platform-api` / `application` /
//! `backend-client` 三处都有契约与调用方，唯独 runtime 侧没有执行者。补它不能靠
//! 造一个同名入口糊住判据——那只是把一个洞改名。真正缺的是**次序**：把 §12 已经
//! 建好的替换状态机真正驱动起来。
//!
//! ## 次序（§7.4-6 的十项在这里逐条落点）
//!
//! ```text
//!   定位旧会话 ─▶ 上下文修订校验 ─▶ 幂等短路 ─▶ 额度预检
//!        │
//!        ▼  ① 候选物理资源（open_candidate，不进可见表 ⇒ 不可见、不可定位、不可执行）
//!        ▼  ②③④ 旧 actor 举发放闸门（停止发放新执行；句柄与资源原封不动）
//!        ▼  §12 Prepared：候选 id 被占住，候选不可路由
//!        ▼  §12 Committed：原子切换，恰好一边可路由
//!        ▼  ⑥ 旧会话走**唯一**的 §9.4 释放例程（close_registered）
//!        ▼  publish_candidate：候选此刻才进可见表
//!        ▼  签发新会话自己的 attachment 令牌 ⇒ 挂载 ⇒ 回执
//! ```
//!
//! 三个设计决定值得写下来，因为它们正是 CM-74 在这条路径上成立的原因：
//!
//! - **第 ⑥ 步不重写释放逻辑。** 它调用既有的 `close_registered` → `ExecCommand::Close`
//!   → `actor::release`，也就是 §9.4 唯一那条四步例程。CM-74 要求的「句柄在物理
//!   资源关闭前已在**原 resource** 上终结并从 actor 注销」因此是**构造性**成立的：
//!   替换路径没有第二条关闭路径可以漂移。
//! - **闸门不碰句柄。** 闸门只停止「发新的」，收「旧的」一律归 §9.4。两者混写就会
//!   长出第二条释放路径，而第二条路径一定和第一条漂移。
//! - **回执里的令牌是**新会话自己**签发的。** 候选不经 `open_session`（那条路在登记
//!   的同一刻就把令牌交出去了），§12 的提交也不签发；不补这一下，§7.4 回执的
//!   `attachmentToken` 就是个谁也用不上的字段。
//!
//! ## 幂等是 §12 原生的，不是外挂的
//!
//! `ReplacementOperationKey::for_handle` 由**旧句柄**确定性地派生，重试同一个旧句柄
//! 必然落到同一个 key 上，`commit_status` 于是直接给出 `Committed { handle }`。
//! 同一份回执被恢复，候选**永不重建**。这里不去碰 `gateway/idempotency.rs`（CM-70
//! 在飞），幂等性完全由目录承担。
//!
//! 幂等短路排在**所有**旧会话检查**之前**，这是它必须待的位置：替换一旦提交，旧会话
//! 就注销了，先定位就会先撞上 `UnknownSession`，重放永远走不到短路。
//!
//! 恢复的是**同一份回执的语义**，不是同一枚令牌：目录只留摘要，上一次那枚令牌早已
//! 不在任何人手里，重签会把摘要换掉、旧令牌即刻作废。承重的部分是同一个新会话句柄——
//! 不重开候选、不重复提交。§7.4 本就写明旧令牌不授予新 session 附着权，重签落的安全那一侧。
//!
//! ## 两种句柄
//!
//! runtime 内部用 `connection::SessionHandle`（`dbSessionId` + `Counter`），
//! 目录用 `dto::session::SessionHandle`（`dbSessionId` + 字符串 `RuntimeEpoch`）。
//! 二者是既有事实，本模块在边界上做一次**确定性**换算：operation key 由前者派生，
//! 因此重试必然命中同一个 key。
//!
//! ## 失败语义
//!
//! - **提交前失败**：候选销毁（同样走 §9.4）、闸门放下、旧会话**原样**继续。
//!   提交前后是唯一的分界线——分界线之前什么都没变。
//! - **提交后失败**：只允许恢复同一份回执。新会话已经是既成事实，替换不得倒退。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ClientInstanceId, DbSessionId, PrincipalId, RuntimeEpoch as PlatformEpoch,
};
use datazen_platform_api::ports::session_directory::{
    ReplacementCommit, ReplacementOperation, ReplacementOutcome, SessionDirectory, SessionOwner,
};
use tracing::warn;

use crate::connection::{CloseMode, ExecutionTarget, RuntimeError, SessionHandle, SessionView};
use crate::directory::{
    AttachmentRejection, CommitStatus, InMemorySessionDirectory, ReplacementOperationKey,
    SessionHandle as DirectoryHandle,
};
use crate::registry::actor::OpenRequest;
use crate::registry::port::SessionPort;
use crate::registry::{SessionActor, SessionRegistry};

/// §7.4 编排需要的目录能力。
///
/// 之所以在 runtime 侧另立一个窄端口，而不是往 platform-api 的 `SessionDirectory`
/// 上加方法：§12 的 `CommitStatus` 与 `ReplacementOperationKey` 是 runtime 的类型，
/// 把它们搬进契约层等于让契约层反过来依赖运行时表示。加端口比搬类型便宜，
/// 而且这一层需要的能力就是下面这五个，多一个都不给：编排全程向目录要的，
/// 一项不多、一项不少。
#[async_trait]
pub trait ReplacementDirectory: Send + Sync + 'static {
    /// 取旧会话的 owner。`None` = 目录里没有这个条目。
    async fn owner_of(
        &self,
        db_session_id: &DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError>;

    /// 按 operation key 查替换的落定状态（§7.4 幂等的依据）。
    fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus;

    /// §12 的 `Prepared` / `Committed` / `RolledBack` 原子提交。
    async fn commit(&self, commit: ReplacementCommit) -> Result<ReplacementOutcome, PortError>;

    /// 给**已发布**的新会话签发 attachment 令牌。§7.4 回执里的那个字段。
    ///
    /// 之所以是端口方法而不是让调用方自带：候选不经 `open_session`，
    /// 提交协议也不签发，调用方手里根本不存在一枚对新会话有效的令牌。
    async fn issue_attachment_token(
        &self,
        handle: &DirectoryHandle,
    ) -> Result<AttachmentToken, AttachmentRejection>;

    /// 在新会话上挂载，验的是**新会话自己签发**的那枚令牌。
    async fn attach_client(
        &self,
        handle: &DirectoryHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<(), AttachmentRejection>;
}

#[async_trait]
impl ReplacementDirectory for InMemorySessionDirectory {
    async fn owner_of(
        &self,
        db_session_id: &DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError> {
        SessionDirectory::lookup(self, db_session_id.clone()).await
    }

    fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus {
        InMemorySessionDirectory::commit_status(self, key)
    }

    async fn commit(&self, commit: ReplacementCommit) -> Result<ReplacementOutcome, PortError> {
        SessionDirectory::commit_replacement(self, commit).await
    }

    async fn issue_attachment_token(
        &self,
        handle: &DirectoryHandle,
    ) -> Result<AttachmentToken, AttachmentRejection> {
        InMemorySessionDirectory::issue_attachment_token(self, handle)
    }

    async fn attach_client(
        &self,
        handle: &DirectoryHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<(), AttachmentRejection> {
        InMemorySessionDirectory::attach_from_client(
            self,
            handle,
            principal_id,
            client_instance_id,
            token,
        )
        .map(|_| ())
    }
}

/// §7.4 替换请求。
pub struct ContextChangeRequest {
    /// 被替换的旧会话。替换 operation key 由它确定性地派生（§7.4-6 幂等）。
    pub handle: SessionHandle,
    /// §7.4 的乐观并发闸门：不匹配即拒，绝不基于「大概没变」去替换。
    pub expected_context_revision: u64,
    /// 期望的新命名空间（连接与对象目标沿用旧会话）。
    pub desired: ExecutionTarget,
    /// 候选 `dbSessionId`，由调用方（directory 的发号方）给出。
    pub candidate_db_session_id: DbSessionId,
    /// 发起替换的归属身份。提交后用它在新会话上挂载，证明新会话真的可路由。
    pub principal_id: PrincipalId,
    pub client_instance_id: ClientInstanceId,
}

/// §7.4 回执：`ContextChangeReceipt { session, replacedSessionId, attachmentToken }`。
///
/// `Debug` 不是装饰：这是一份 `Result` 的成功侧，调用方要能把它 `expect_err` 出去。
#[derive(Debug)]
pub struct ContextChangeReceipt {
    /// 新会话。
    pub session: DirectoryHandle,
    /// 被替换掉的旧会话 id。
    pub replaced_session_id: DbSessionId,
    /// 在新会话上有效的挂载令牌。
    pub attachment_token: AttachmentToken,
}

/// 替换编排器。持有 registry 与 directory 两个依赖：registry 管可见表，
/// directory 持有替换状态机；两者都不反向依赖本模块。
pub struct ContextReplacer {
    registry: Arc<SessionRegistry>,
    directory: Arc<dyn ReplacementDirectory>,
}

impl ContextReplacer {
    pub fn new(registry: Arc<SessionRegistry>, directory: Arc<dyn ReplacementDirectory>) -> Self {
        Self {
            registry,
            directory,
        }
    }

    /// §7.4-6 全流程。
    pub async fn replace(
        &self,
        request: ContextChangeRequest,
    ) -> Result<ContextChangeReceipt, RuntimeError> {
        let old_id = request.handle.db_session_id.clone();
        let old_directory_handle = directory_handle(&request.handle);

        // 幂等短路排在**最前**，这是它必须待的位置：替换一旦提交，旧会话就注销了，
        // 任何先定位旧会话的检查（上下文修订、归属）都会先撞上 `UnknownSession`，
        // 重放将永远走不到这里。operation key 只由**旧句柄**决定，目录已经记着答案。
        match self
            .directory
            .commit_status(&ReplacementOperationKey::for_handle(&old_directory_handle))
        {
            CommitStatus::Committed { handle } => {
                // 重签而不是复用旧令牌：目录只留摘要，上一次那枚早已不在任何人手里。
                // 幂等的承重部分是**同一个新会话句柄**（不会重开候选、不会重复提交），
                // 而 §7.4 明说旧令牌不授予新 session 附着权——重签只会换掉摘要，
                // 旧令牌立刻作废，是安全的那一侧。
                let token = self
                    .directory
                    .issue_attachment_token(&handle)
                    .await
                    .map_err(attachment_error)?;
                return Ok(ContextChangeReceipt {
                    session: handle,
                    replaced_session_id: old_id,
                    attachment_token: token,
                });
            }
            // 上一次回滚了：这**不是**重试，是一个新请求，重新走一遍。
            CommitStatus::RolledBack { .. } | CommitStatus::Pending => {}
        }

        // 定位旧会话。定位不到就是 `UnknownSession`——**不**退化成「找相似 id 的那个」。
        let old_view = self.registry.session_view(&request.handle).await?;
        if old_view.context_revision.get() != request.expected_context_revision {
            return Err(RuntimeError::ContextRevisionMismatch {
                expected: request.expected_context_revision,
                actual: old_view.context_revision.get(),
            });
        }
        let old_owner = self
            .directory
            .owner_of(&old_id)
            .await
            .map_err(port_error)?
            .ok_or_else(|| RuntimeError::UnknownSession(old_id.to_string()))?;

        if self.registry.remaining_quota() == 0 {
            return Err(RuntimeError::BudgetExhausted("replacementNeedsSlot"));
        }

        // ① 候选物理资源：开了、起了 actor，但**不在可见表里**。
        let (actor, view, runtime_epoch) = self
            .registry
            .open_candidate(open_request(&request, &old_view, &old_owner))
            .await?;
        let candidate_handle = view.handle.clone();
        let candidate_directory_handle = directory_handle(&candidate_handle);
        let new_owner = SessionOwner {
            db_session_id: request.candidate_db_session_id.clone(),
            runtime_epoch: PlatformEpoch::new(format!("rte-{:08}", runtime_epoch.get())),
            resource_epoch: old_owner.resource_epoch + 1,
            ..old_owner.clone()
        };

        // ②③④ 旧 actor 举闸门。此后旧会话不再发新执行，句柄与资源仍原封不动。
        if let Err(error) = self.registry.hold_for_replacement(&request.handle).await {
            self.abort_candidate(&actor, &candidate_handle).await;
            return Err(error);
        }

        // §12 Prepared / Committed。两步之间任何一步失败都走「提交前失败」分支。
        let key = ReplacementOperationKey::for_handle(&old_directory_handle);
        if let Err(error) = self
            .directory
            .commit(ReplacementCommit {
                old: old_directory_handle.clone(),
                new_owner: new_owner.clone(),
                operation: ReplacementOperation::Prepared,
            })
            .await
        {
            self.abort_candidate(&actor, &candidate_handle).await;
            let _ = self
                .registry
                .resume_after_replacement(&request.handle)
                .await;
            return Err(port_error(error));
        }
        if let Err(error) = self
            .directory
            .commit(ReplacementCommit {
                old: old_directory_handle.clone(),
                new_owner: new_owner.clone(),
                operation: ReplacementOperation::Committed,
            })
            .await
        {
            // Prepared 已落 ⇒ 必须回滚，不能留一条悬空的 replacement 记录。
            let _ = self
                .directory
                .commit(ReplacementCommit {
                    old: old_directory_handle.clone(),
                    new_owner: new_owner.clone(),
                    operation: ReplacementOperation::RolledBack,
                })
                .await;
            warn!(%key, "§7.4-6 提交失败，已回滚到候选前状态");
            self.abort_candidate(&actor, &candidate_handle).await;
            let _ = self
                .registry
                .resume_after_replacement(&request.handle)
                .await;
            return Err(port_error(error));
        }

        // ⑥ §9.4：旧会话走**唯一**那条释放例程。已提交，所以这里失败也不回头——
        // 失败的语义是「旧会话已丢失/已隔离」，表项在 `close_registered` 里一并注销，
        // 新会话仍是唯一真相。
        if let Err(error) = self
            .registry
            .close_registered(&request.handle, CloseMode::RollbackAndClose)
            .await
        {
            warn!(
                old = %old_id,
                %error,
                "§7.4-6 提交后释放旧会话未得 Clean：新会话已是既成事实，不倒退"
            );
        }

        // 候选此刻才进可见表。
        self.registry
            .publish_candidate(new_owner.worker_id.clone(), runtime_epoch, view, actor)
            .map_err(|error| {
                warn!(%error, "§7.4-6 候选发布失败：新会话不可见，已提交但无宿主入口");
                error
            })?;

        // 给**已发布**的新会话签发它自己的令牌，再拿这枚令牌挂载：回执里的
        // `attachmentToken` 必须**真的**在新会话上作数，否则这个字段就是一句空话。
        // 签发在提交之后、挂载之前：屏障上的候选还路由不到，谁去签都是拒签。
        let token = self
            .directory
            .issue_attachment_token(&candidate_directory_handle)
            .await
            .map_err(attachment_error)?;
        self.directory
            .attach_client(
                &candidate_directory_handle,
                request.principal_id.clone(),
                request.client_instance_id.clone(),
                token.clone(),
            )
            .await
            .map_err(attachment_error)?;

        Ok(ContextChangeReceipt {
            session: candidate_directory_handle,
            replaced_session_id: old_id,
            attachment_token: token,
        })
    }

    /// §7.4-6 第 10 项：提交前失败 ⇒ 销毁候选。
    ///
    /// 销毁**同样**走 §9.4：候选若带句柄消失，也必须先在它自己登记的那条资源上
    /// 终结，而不是直接丢一个物理资源。
    async fn abort_candidate(&self, actor: &SessionActor, candidate: &SessionHandle) {
        if let Err(error) = actor.destroy_candidate(candidate).await {
            warn!(
                candidate = %candidate.db_session_id,
                %error,
                "§7.4-6 候选销毁未得 Clean：额度照退，物理资源由 actor 退出收尾"
            );
        }
        self.registry.refund_candidate();
    }
}

/// 候选的 `OpenRequest`：沿用旧会话的连接/归属/配置，只换命名空间。
///
/// `owner` 取自 `SessionView`（runtime 侧的权威表示），不是目录投影里的
/// `OwnerRef`——那是另一套更窄的枚举，抄过来会丢字段。
fn open_request(
    request: &ContextChangeRequest,
    old_view: &SessionView,
    old_owner: &SessionOwner,
) -> OpenRequest {
    OpenRequest {
        db_session_id: request.candidate_db_session_id.clone(),
        worker_id: old_owner.worker_id.clone(),
        connection_id: old_owner.connection_id.clone(),
        config_revision: old_view.config_revision.clone(),
        owner: old_view.owner.clone(),
        initial_target: request.desired.clone(),
        expires_at: old_view.expires_at.clone(),
        idle_deadline_ms: None,
    }
}

/// runtime 句柄 → 目录句柄。**确定性**换算：同一个 runtime 句柄永远得到同一个
/// 目录句柄，因而 `ReplacementOperationKey::for_handle` 也永远得到同一个 key。
fn directory_handle(handle: &SessionHandle) -> DirectoryHandle {
    DirectoryHandle::new(
        handle.db_session_id.clone(),
        PlatformEpoch::new(format!("rte-{:08}", handle.runtime_epoch.get())),
    )
}

fn port_error(error: PortError) -> RuntimeError {
    match error {
        PortError::NotFound(id) => RuntimeError::UnknownSession(id),
        PortError::TokenInvalid => RuntimeError::CloseRejected("attachmentTokenRejected"),
        // `RuntimeError` 只收 `&'static str`，所以端口原话进日志，错误面上留稳定理由。
        other => {
            warn!(reason = %other, "§7.4-6 目录端口拒绝了替换操作");
            RuntimeError::InvariantBroken("directoryRejectedReplacement")
        }
    }
}

fn attachment_error(rejection: AttachmentRejection) -> RuntimeError {
    warn!(?rejection, "§7.4-6 无法给新会话签发 attachment 令牌");
    RuntimeError::CloseRejected(attachment_reason(&rejection))
}

fn attachment_reason(rejection: &AttachmentRejection) -> &'static str {
    match rejection {
        AttachmentRejection::NoToken => "attachmentTokenMissing",
        AttachmentRejection::NoTokenIssued => "attachmentTokenNotIssued",
        AttachmentRejection::TokenMismatch => "attachmentTokenRejected",
        AttachmentRejection::PrincipalMismatch => "attachmentPrincipalMismatch",
        AttachmentRejection::OwnerMismatch => "attachmentOwnerMismatch",
        _ => "attachmentRejected",
    }
}
