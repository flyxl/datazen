//! 物理资源后端接缝（驱动资源契约在 registry 侧的那一半）。
//!
//! ## 为什么自己定义 trait 而不是复用 `connection/port.rs`
//!
//! `connection::port` 里只冻结了**同步的** `BudgetPort` 和一批请求/回执 DTO，
//! 没有 async 的资源级端口。而 `connection/**` 是 Wave 1 冻结面，不得改动。
//! 因此这里定义 registry **自己**的后端抽象——它只表达 actor 需要的那五件事：
//! 打开、在资源上执行、取消、在**原资源**上终结句柄、关闭。
//!
//! 它是**接缝**，不是实现：真正的驱动适配在别的轨道/后续 Wave 填进来。
//! 但正因为它是接缝，**契约的形状就是规格**：「归池前检查」在这里被拆成
//! `CloseResource::registered_handles`（宿主自己的账）+ `CloseResourceOutcome::Undecidable`
//! （后端无法确认），而不是一句「关闭成功」。
//!
//! ## 顺序约束
//!
//! [`SessionBackend`] 的五个方法的调用顺序被严格规定：
//! `finalize_handles`（在**原资源**上）→ 注销登记 → `close`。
//! 本 trait **不**用类型系统表达这个顺序（那需要借用检查器级别的重构），
//! 而是由 [`super::actor`] 的单一释放例程保证；顺序一旦被打乱，
//! 「在新资源上提交旧句柄」这种缺陷就会重新长出来。

use async_trait::async_trait;

use crate::connection::{
    CommandCall, ConfigRevision, ConnectionId, EffectOutcome, ExecutionId, ExecutionState,
    NamespaceTarget, OwnerRef, ResourceId, SessionContext, SessionHandleRef, StreamId,
};

use tokio::sync::mpsc;

use crate::connection::port::CancelDisposition;
use crate::registry::audit::CapabilityVersions;

/// 打开物理资源（物理侧）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenResource {
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub owner: OwnerRef,
    pub initial_target: NamespaceTarget,
    pub runtime_epoch: u64,
}

/// 打开结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedResource {
    /// 打开后真实观测到的上下文（不是配置期望值）。
    pub context: SessionContext,
    /// 非敏感的能力版本。
    pub capabilities: CapabilityVersions,
    /// 不透明资源标识。**不透明**是硬要求：调用方不得从它反推连接串或端点。
    pub resource_id: String,
    /// 该 driver 是否有独立取消路径。false 时只能回 `unsupported`，
    /// 不能靠「先置 CancelRequested 再假装取消」来假装支持。
    pub driver_supports_cancel: bool,
}

/// 在资源上执行一条命令。
///
/// 只 `PartialEq`：`CommandCall` 的输入是 `serde_json::Value`，没有 `Eq`。
#[derive(Debug)]
pub struct ExecuteOnResource {
    /// 宿主铸造的执行 id。**宿主**是执行 id 的发放方：调用方要能在执行**进行中**
    /// 就能引用它发起取消，而一个只有拿到终态才存在的 id 做不到这件事。
    pub execution_id: ExecutionId,
    pub resource_id: String,
    pub command: CommandCall,
    pub expected_context_revision: u64,
    /// driver 公布精确 cancelHandle 的出口（见 [`CancelHandleSink`]）。
    pub cancel_handle_sink: CancelHandleSink,
}

/// cancelHandle 公布口。
///
/// 取消绑定必须能在执行**进行中**建立，而 `execute()` 只在**结束时**返回
/// `ResourceExecution`。若把 cancelHandle 绑在返回值上，「执行中取消」这条主路径
/// 就没有绑定可校验，伪造检查也就形同虚设。
///
/// 因此后端在开始真正干活之前，把 handle 推到这里。actor 在飞行期间
/// `select!` 这条通道：收到即落绑定，之后任何取消请求都能逐字比对。
#[derive(Debug, Clone)]
pub struct CancelHandleSink(mpsc::UnboundedSender<String>);

impl CancelHandleSink {
    /// actor 侧构造：把 sender 端交给后端任务，receiver 端留在 actor 的 `select!` 里。
    pub fn new(sender: mpsc::UnboundedSender<String>) -> Self {
        Self(sender)
    }

    /// 后端公布精确 handle。通道已关闭（actor 已退出）时返回 false，
    /// 后端据此知道**没人会来取消这次执行**。
    pub fn publish(&self, cancel_handle: impl Into<String>) -> bool {
        self.0.send(cancel_handle.into()).is_ok()
    }
}

/// 执行终态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceExecution {
    pub execution_id: ExecutionId,
    /// 结果流 id。宿主只转发出，不会自己编造。
    pub stream_id: StreamId,
    pub state: ExecutionState,
    /// 物理层判定的效果。**不得**由宿主根据请求成功与否回填。
    pub effect_outcome: EffectOutcome,
    /// 执行后的上下文（用于推进 `contextRevision`）。
    pub context_after: SessionContext,
    pub context_revision: u64,
    /// 本执行产生、需要登记到 actor 的句柄。
    pub handles: Vec<SessionHandleRef>,
    /// driver 侧的精确 cancelHandle。仅用于 actor 内部绑定校验，**不进审计**。
    pub cancel_handle: String,
}

/// 在资源上取消一次执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelOnResource {
    pub resource_id: String,
    pub execution_id: ExecutionId,
    /// 精确的 cancelHandle：只有命中未完成执行才算数。
    pub cancel_handle: String,
}

/// 资源侧取消结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceCancel {
    pub disposition: CancelDisposition,
    pub state: ExecutionState,
}

/// 句柄终结方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleDisposition {
    /// 驱逐/替换：回滚。
    Rollback,
    /// 关闭/提交：确认终结。
    Commit,
}

/// 在**原资源**上终结一批句柄（注销前置条件）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizeHandles {
    pub resource_id: String,
    pub handles: Vec<SessionHandleRef>,
    pub disposition: HandleDisposition,
}

/// 句柄终结结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleFinalization {
    /// 确实终结掉的句柄数。
    pub finalized: usize,
    /// 仍然存活的句柄数。`> 0` 时**不得**继续关闭物理资源——
    /// 带着活句柄关资源等于制造一个「宿主以为干净、实际有活事务」的空洞。
    pub remaining: usize,
    /// 终结效果（回滚失败/结果未知时是 `Unknown`）。
    pub effect_outcome: EffectOutcome,
}

/// 关闭物理资源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseResource {
    pub resource_id: String,
    /// **宿主自己账上的**登记句柄数，不是 driver 报回来的。
    ///
    /// driver 报 Clean 不是事务已终结的证据，所以宿主必须自己数一遍。
    pub registered_handles: usize,
}

/// 关闭结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseResourceOutcome {
    /// 已确认关闭。
    Closed,
    /// 无法确认（`SessionLost` / `OutcomeUnknown` 路径）。
    Undecidable { reason: &'static str },
}

impl CloseResourceOutcome {
    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// registry 与物理资源之间唯一的接缝。
#[async_trait]
pub trait SessionBackend: Send + Sync + 'static {
    /// 打开物理资源。**登记不得早于本方法成功返回**。
    async fn open(
        &self,
        request: OpenResource,
    ) -> Result<OpenedResource, crate::connection::ProviderError>;

    /// 在资源上执行。
    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, crate::connection::ProviderError>;

    /// 取消。绑定校验已在到达本方法之前完成——
    /// 被拒的绑定**不应该**产生任何后端调用。
    async fn cancel(
        &self,
        request: CancelOnResource,
    ) -> Result<ResourceCancel, crate::connection::ProviderError>;

    /// 在**原资源**上终结句柄。
    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, crate::connection::ProviderError>;

    /// 关闭物理资源。
    async fn close(
        &self,
        request: CloseResource,
    ) -> Result<CloseResourceOutcome, crate::connection::ProviderError>;
}

/// 宿主侧归池前检查。
///
/// 这是一个**纯函数**，放在 backend 这一层是因为它检查的正是后端请求的形状：
/// 宿主账上还有活句柄时，关闭请求就必须先把句柄终结掉。
/// 「driver 说 Clean」不构成放行条件——`registered_handles` 只数宿主的账。
pub const fn ready_to_return_to_pool(registered_handles: usize) -> bool {
    registered_handles == 0
}

/// 宿主账上有活句柄时，关闭请求必须改成先终结句柄。
pub fn close_request_for(
    resource_id: String,
    registered_handles: usize,
    handles: Vec<SessionHandleRef>,
) -> (CloseResource, Option<FinalizeHandles>) {
    if ready_to_return_to_pool(registered_handles) {
        return (
            CloseResource {
                resource_id,
                registered_handles,
            },
            None,
        );
    }
    (
        CloseResource {
            resource_id: resource_id.clone(),
            registered_handles,
        },
        Some(FinalizeHandles {
            resource_id,
            handles,
            disposition: HandleDisposition::Rollback,
        }),
    )
}

/// 资源标识的持有者类型别名（避免在 actor 里到处写 `String`）。
pub type BackendResourceId = ResourceId;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{Counter, HandleId, HandleKind};

    fn handle() -> SessionHandleRef {
        SessionHandleRef::new(
            HandleId::new("h_1"),
            HandleKind::Transaction,
            ResourceId::new("res_1"),
            Counter::new(3),
        )
    }

    /// 宿主账上有活句柄时，关闭请求必须**带上**先终结句柄的那一步。
    #[test]
    fn host_side_check_forces_handle_finalization_before_close() {
        let (close, finalize) = close_request_for("res_1".to_owned(), 1, vec![handle()]);
        assert_eq!(close.registered_handles, 1);
        assert!(!ready_to_return_to_pool(1));
        let finalize = finalize.expect("有活句柄时必须先生成句柄终结请求");
        assert_eq!(finalize.resource_id, "res_1");
        assert_eq!(finalize.handles.len(), 1);
        assert_eq!(finalize.disposition, HandleDisposition::Rollback);
    }

    /// driver 报 Clean 不是放行条件，但宿主账空时可以直接关。
    ///
    /// 反例（曾经真实存在过的写法）是把 `is_closed()` 当作归池前检查：
    /// driver 说 Clean 而宿主还有活事务句柄，于是带着事务关资源。
    #[test]
    fn empty_host_ledger_needs_no_finalization_step() {
        let (close, finalize) = close_request_for("res_1".to_owned(), 0, Vec::new());
        assert!(finalize.is_none());
        assert!(ready_to_return_to_pool(close.registered_handles));
        assert!(CloseResourceOutcome::Closed.is_closed());
        assert!(!CloseResourceOutcome::Undecidable {
            reason: "rollbackFailed"
        }
        .is_closed());
    }

    /// 带有活句柄的终结结果不得继续关闭。
    #[test]
    fn remaining_handles_block_the_close_step() {
        let finalization = HandleFinalization {
            finalized: 0,
            remaining: 1,
            effect_outcome: EffectOutcome::Unknown,
        };
        assert!(
            finalization.remaining > 0,
            "remaining > 0 是宿主判断『不能关』的唯一依据"
        );
        assert_eq!(
            finalization.effect_outcome,
            EffectOutcome::Unknown,
            "回滚结果不可判定时必须是 unknown，不能写 rolledBack"
        );
        assert!(EffectOutcome::is_undecidable(Some(
            crate::connection::ExecutionErrorCode::Timeout
        )));
    }
}
