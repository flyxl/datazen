//! 共享 feature 层（`datazen-feature-core`）**消费者定义端口**的宿主实现。
//!
//! 为什么这些实现住在这里：`ArtifactChunkReader` 与 `BackupOutputBinder` 是共享
//! 层为了不点名 `datazen_runtime::application::RuntimeConnectionUseCases` 而定义的
//! trait（R1：领域包不得依赖连接运行时）。回答它们需要 Runtime 门面，而门面归宿主
//! 装配——所以实现落在宿主，共享层只认 trait 与契约 DTO。
//!
//! 两个实现都是**薄壳**：没有缓存、没有额外授权判断，把调用原样转给门面。任何
//! 在薄壳里加的逻辑都会让「共享层看到的语义」与「Runtime 的语义」分叉。

use datazen_feature_core::services::ports::{
    ArtifactChunkReader, BackupOutputBinder, BackupOutputGuard,
};
use datazen_platform_api::{
    context::RequestContext,
    dto::artifact::BackupOutput,
    dto::execution::ResultChunk,
    dto::session::SessionHandle,
    error::ApiError,
    id::{ArtifactId, Counter},
    target::ExecutionTarget,
};
use datazen_runtime::application::{BackupOutputBinding, RuntimeConnectionUseCases};
use std::sync::Arc;

/// 结果块读取：转发给 Runtime 门面的 `read_artifact_chunk`（含其授权与分页）。
pub struct HostArtifactChunks(Arc<RuntimeConnectionUseCases>);

impl HostArtifactChunks {
    pub fn new(runtime: Arc<RuntimeConnectionUseCases>) -> Self {
        Self(runtime)
    }
    /// 以 `Arc<dyn ArtifactChunkReader>` 形式返回，供共享层入口直接取用。
    pub fn shared(self) -> Arc<dyn ArtifactChunkReader> {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl ArtifactChunkReader for HostArtifactChunks {
    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact: &ArtifactId,
        index: Counter,
    ) -> Result<ResultChunk, ApiError> {
        self.0
            .read_artifact_chunk(ctx, artifact.clone(), index)
            .await
    }
}

/// 备份输出绑定：转发给 Runtime 门面的 `bind_backup_output`。
pub struct HostBackupBindings(Arc<RuntimeConnectionUseCases>);

impl HostBackupBindings {
    pub fn new(runtime: Arc<RuntimeConnectionUseCases>) -> Self {
        Self(runtime)
    }
    pub fn shared(self) -> Arc<dyn BackupOutputBinder> {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl BackupOutputBinder for HostBackupBindings {
    async fn bind(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
        target: &ExecutionTarget,
        callback: BackupOutput,
    ) -> Result<Box<dyn BackupOutputGuard>, ApiError> {
        let binding = self
            .0
            .bind_backup_output(ctx, handle, target, callback)
            .await?;
        Ok(Box::new(HostBackupOutput(binding)))
    }
}

/// 守卫薄壳：drop 时由 `BackupOutputBinding` 自己的 `Drop` 撤销能力
/// （等完 in-flight 写、拒收迟到输出）。这里**不能**额外实现 `Drop`。
struct HostBackupOutput(BackupOutputBinding);

impl BackupOutputGuard for HostBackupOutput {}
