//! 备份输出端口（D6 item 5 切片 b，2026-10-26）。
//!
//! `BackupOutputBinder` 自 modules `services::ports` 归位（唯一定义在
//! 此，旧路径改再导出垫片）：绑定的实现需要 session 归属判定、执行条目
//! 可见性与 sink 生命周期，这些都在 runtime 内部，由宿主提供实现。
//! 返回的守卫是**能力对象**——drop 即撤销，实现方负责把 in-flight 的写
//! 等完并拒收迟到输出（共享层不认识实现的 Drop 逻辑，只依赖这个语义）。

use crate::execution::application::BackupOutput;
use crate::execution::domain::target::ExecutionTarget;
use crate::execution::domain::SessionHandle;
use crate::shared::context::RequestContext;
use crate::shared::error::ApiError;

/// 备份输出绑定（消费者定义端口）。
#[async_trait::async_trait]
pub trait BackupOutputBinder: Send + Sync + 'static {
    async fn bind(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
        target: &ExecutionTarget,
        callback: BackupOutput,
    ) -> Result<Box<dyn BackupOutputGuard>, ApiError>;
}

/// 备份输出守卫：存活期间输出可用，drop 即撤销。
pub trait BackupOutputGuard: Send + Sync {}
