//! 替换期间的**发放闸门**。
//!
//! 替换是「先建新、再切旧」，切换瞬间旧会话必须既不能再发新执行、
//! 又不能已经关掉（关掉的是替换编排自己的活）。闸门就是这两者之间那一格：
//!
//! ```text
//!      hold              commit              release
//!   Ready ──▶ Closing ────────────▶ Closing ────────▶（终态）
//!            闸门举着               旧条目改关闭路由
//!            · Execute 被拒          · 物理资源与已登记句柄
//!            · RegisterHandles 被拒    仍原封不动，由编排交给
//!            · SetIdleDeadline 被拒   唯一的释放例程
//!            · Evict 被拒
//!            · Close / View / 闸门命令本身照常
//! ```
//!
//! 闸门**不碰**已登记句柄：它们要求在原资源上终结，而终结动作
//! 归 [`super::release`]。闸门只负责「别再发新的」，不负责「收旧的」——
//! 两件事混在一起就会长出第二条释放路径，而第二条路径一定和第一条漂移。

use super::{ActorState, ExecCommand, SessionActor};
use crate::connection::{CloseMode, RuntimeError, SessionHandle, SessionState};

/// 闸门举着时，非替换编排的一切发放都被拒：切换前旧会话不得再发新执行。
const HELD: &str = "replacementInProgress";

/// 举起闸门：旧会话停止发放新执行，物理资源与已登记句柄**原封不动**。
pub(super) fn hold(state: &mut ActorState, handle: &SessionHandle) -> Result<(), RuntimeError> {
    super::check_epoch(state, handle)?;
    if state.physical.is_none() {
        return Err(RuntimeError::SessionClosed(state.db_session_id.to_string()));
    }
    if state.replacement_hold.is_none() {
        state.replacement_hold = Some(state.view.state);
        state.view.state = SessionState::Closing;
    }
    Ok(())
}

/// 放下闸门（提交前失败）：旧会话**原样**恢复发放。
///
/// 恢复的是举起闸门**之前**的 `SessionState`，不是写死一个 `Ready`：
/// 写死 `Ready` 会把「本来就不 Ready」的会话抬成 Ready，那是凭空造状态。
pub(super) fn resume(state: &mut ActorState, handle: &SessionHandle) -> Result<(), RuntimeError> {
    super::check_epoch(state, handle)?;
    if let Some(previous) = state.replacement_hold.take() {
        state.view.state = previous;
    }
    Ok(())
}

/// 闸门的**唯一**执行点：举着闸门就把被挡的命令就地拒掉并回话。
///
/// 这里是闸门唯一的判定与拒绝处：登记、提交、释放、视图四条命令全部经过
/// [`super::handle_exec`]，因此不存在「某条发放路径忘了查闸门」的分叉。
///
/// - `Ok(command)` = 放行，原命令归还调用方继续处理。
/// - `Err(())` = 已拒绝并已回话，调用方必须直接返回。
pub(super) fn gate(state: &ActorState, command: ExecCommand) -> Result<ExecCommand, ()> {
    // 各变体的 `reply` 载荷类型不同，必须逐个 arm；合并 arm 会要求同一个绑定
    // 满足多个 `Sender<Result<T, _>>`。
    if state.replacement_hold.is_none() {
        return Ok(command);
    }
    let error = RuntimeError::CloseRejected(HELD);
    match command {
        ExecCommand::Execute { reply, .. } => {
            let _ = reply.send(Err(error));
            Err(())
        }
        ExecCommand::RegisterHandles { reply, .. } => {
            let _ = reply.send(Err(error));
            Err(())
        }
        ExecCommand::SetIdleDeadline { reply, .. } => {
            let _ = reply.send(Err(error));
            Err(())
        }
        ExecCommand::Evict { reply, .. } => {
            let _ = reply.send(Err(error));
            Err(())
        }
        other => Ok(other),
    }
}

impl SessionActor {
    /// 提交前失败 ⇒ 销毁候选。
    ///
    /// 走的是**同一条**释放例程（[`super::release`]），不是另写一份关闭：
    /// 候选即使带句柄消失，也必须先在它自己登记的那条资源上终结。
    pub(crate) async fn destroy_candidate(
        &self,
        handle: &SessionHandle,
    ) -> Result<(), RuntimeError> {
        self.exec(|reply| ExecCommand::Close {
            handle: handle.clone(),
            mode: CloseMode::RollbackAndClose,
            reply,
        })
        .await
        .map(|_| ())
    }
}
