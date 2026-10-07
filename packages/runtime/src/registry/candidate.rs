//! 替换编排用的登记表原语。
//!
//! 四条方法只有一处用途：[`crate::registry::context`] 的 `setSessionContext` 路径，
//! 所以它们全是 `pub(super)`——可见范围就是 `registry` 模块本身，对外仍然只有
//! [`SessionPort`](crate::registry::port::SessionPort) 一个入口。
//!
//! 其中 [`SessionRegistry::open_candidate`] 承担全部意义：
//!
//! ```text
//!   open_candidate            原子切换              publish_candidate
//!   ─────────────            ────────────            ────────────────
//!   起 actor、占额度          Prepared / Committed      插进可见表
//!   **不进表**                ——间隙——                此刻宿主才找得到它
//! ```
//!
//! 间隙里候选对宿主**不可见**：没有第二个入口能定位到它，也就没有「切换前的
//! 幽灵会话」可执行。`publish_candidate` 与 `SessionRecord` 留在 `registry.rs`，
//! 因为那条路必须认得表项的私有形状。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::actor::{spawn_actor, ExecCommand, OpenRequest};
use super::epoch::RuntimeEpoch;
use super::{SessionActor, SessionRegistry};
use crate::connection::{RuntimeError, SessionHandle, SessionView};

impl SessionRegistry {
    /// 开一个候选物理资源并起 actor，**不进可见表**。
    ///
    /// 与 `open_and_publish` 只差最后一步：候选在原子切换之前对宿主
    /// **不可见**，因此没有第二个入口能定位到它。
    pub(super) async fn open_candidate(
        &self,
        request: OpenRequest,
    ) -> Result<(SessionActor, SessionView, RuntimeEpoch), RuntimeError> {
        self.quota.try_reserve()?;
        let runtime_epoch = RuntimeEpoch::new(self.epoch_seq.fetch_add(1, Ordering::SeqCst) + 1);
        let open_input = request.clone();
        let actor = spawn_actor(
            request,
            runtime_epoch,
            Arc::clone(&self.backend),
            self.outbox.clone(),
        );
        match actor
            .exec(|reply| ExecCommand::Open {
                request: open_input,
                reply,
            })
            .await
        {
            Ok(view) => Ok((actor, view, runtime_epoch)),
            Err(error) => {
                self.quota.release();
                Err(error)
            }
        }
    }

    /// 候选销毁后把占掉的额度退回去。
    pub(super) fn refund_candidate(&self) {
        self.quota.release();
    }

    /// 旧 actor 举发放闸门（物理资源与已登记句柄原封不动）。
    pub(super) async fn hold_for_replacement(
        &self,
        handle: &SessionHandle,
    ) -> Result<(), RuntimeError> {
        self.locate(&handle.db_session_id)?
            .actor
            .exec(|reply| ExecCommand::HoldForReplacement {
                handle: handle.clone(),
                reply,
            })
            .await
    }

    /// 提交前失败 ⇒ 旧会话原样恢复发放。
    pub(super) async fn resume_after_replacement(
        &self,
        handle: &SessionHandle,
    ) -> Result<(), RuntimeError> {
        self.locate(&handle.db_session_id)?
            .actor
            .exec(|reply| ExecCommand::ResumeAfterReplacement {
                handle: handle.clone(),
                reply,
            })
            .await
    }
}
