//! 表编辑的依赖端口（D4-d，自 feature-core services/ports 迁入）。

use crate::execution::ResultChunk;
use crate::shared::context::RequestContext;
use crate::shared::error::ApiError;
use crate::shared::id::{ArtifactId, Counter};

/// 结果块读取（消费者定义端口）。
///
/// `ConnectionUseCases` 覆盖会话与执行面，**不**覆盖结果内容：读一块要过授权、
/// 要按 sink 的发布水位分页，那是 runtime 的内建能力。共享层若直接命名
/// `datazen_modules::execution::application::RuntimeConnectionUseCases`，R1 立刻失守；这个
/// 端口把问题交给**持有 Runtime 门面的那一侧**回答，共享层只认契约类型
/// [`ResultChunk`]。
#[async_trait::async_trait]
pub trait ArtifactChunkReader: Send + Sync + 'static {
    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact: &ArtifactId,
        index: Counter,
    ) -> Result<ResultChunk, ApiError>;
}
