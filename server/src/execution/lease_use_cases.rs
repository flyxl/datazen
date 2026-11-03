//! WR-D4/E：租约委托用例面 —— `ConnectionUseCases` 的 server 形态。
//!
//! 端口语义（`RuntimeLeaseSource` 的硬约束）：runtime 可能被吊销，**每调用
//! 现取**用例面，禁止把 `RuntimeConnectionUseCases` 缓存在本适配器里。本
//! 结构因此只持有租约源端口：每个用例方法先 `resolve()` 再逐参转发——
//! 「同一用例、另一适配器」（§4.7 端口表的 server 行）。
//!
//! bootstrap 装配：`SessionsState`/执行域路由的 `Arc<dyn ConnectionUseCases>`
//! 由本适配器供给（与 MCP 租约源共用同一个 `RuntimeLeaseSource` 实例与
//! 共享 `RuntimeResultSink`）。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::connection::ProfileView;
use datazen::execution::application::requests::{
    AttachmentRequest, CloseSessionRequest, ExecuteAtTargetRequest, ExecuteInSessionRequest,
    OpenSessionRequest, SetSessionContextRequest,
};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::{
    CancelReceipt, CloseReceipt, ContextChangeReceipt, EventSequence, EventSubscription,
    ExecutionReceipt, ExecutionView, IdempotentOperation, OpenSessionReceipt, SessionHandle,
    SessionView, SubmissionToken,
};
use datazen::shared::context::RequestContext;
use datazen::shared::error::ApiError;
use datazen::shared::id::{ExecutionId, StreamId};

use crate::execution::lease::RuntimeLeaseSource;

/// 每调用现取租约、再转发用例的适配器。
pub struct LeaseConnectionUseCases {
    lease_source: Arc<dyn RuntimeLeaseSource>,
}

impl LeaseConnectionUseCases {
    /// 构造。`lease_source` 与 MCP 租约源同实例（bootstrap 装配纪律）。
    pub fn new(lease_source: Arc<dyn RuntimeLeaseSource>) -> Self {
        Self { lease_source }
    }

    async fn resolve(
        &self,
    ) -> Result<Arc<dyn ConnectionUseCases>, ApiError> {
        let cases = self
            .lease_source
            .resolve()
            .await
            .map_err(|message| ApiError::new(datazen::shared::error::ApiErrorCode::ServiceUnavailable, message))?;
        Ok(cases as Arc<dyn ConnectionUseCases>)
    }
}

#[async_trait]
impl ConnectionUseCases for LeaseConnectionUseCases {
    async fn list_connections(
        &self,
        ctx: &RequestContext,
    ) -> Result<Vec<ProfileView>, ApiError> {
        self.resolve().await?.list_connections(ctx).await
    }

    async fn issue_submission_token(
        &self,
        ctx: &RequestContext,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, ApiError> {
        self.resolve()
            .await?
            .issue_submission_token(ctx, operation, handle)
            .await
    }

    async fn open_session(
        &self,
        ctx: &RequestContext,
        request: OpenSessionRequest,
    ) -> Result<OpenSessionReceipt, ApiError> {
        self.resolve().await?.open_session(ctx, request).await
    }

    async fn attach_session(
        &self,
        ctx: &RequestContext,
        request: AttachmentRequest,
    ) -> Result<SessionView, ApiError> {
        self.resolve().await?.attach_session(ctx, request).await
    }

    async fn detach_session(
        &self,
        ctx: &RequestContext,
        request: AttachmentRequest,
    ) -> Result<SessionView, ApiError> {
        self.resolve().await?.detach_session(ctx, request).await
    }

    async fn get_session(
        &self,
        ctx: &RequestContext,
        handle: SessionHandle,
    ) -> Result<SessionView, ApiError> {
        self.resolve().await?.get_session(ctx, handle).await
    }

    async fn execute_in_session(
        &self,
        ctx: &RequestContext,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        self.resolve().await?.execute_in_session(ctx, request).await
    }

    async fn execute_at_target(
        &self,
        ctx: &RequestContext,
        request: ExecuteAtTargetRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        self.resolve().await?.execute_at_target(ctx, request).await
    }

    async fn set_session_context(
        &self,
        ctx: &RequestContext,
        request: SetSessionContextRequest,
    ) -> Result<ContextChangeReceipt, ApiError> {
        self.resolve().await?.set_session_context(ctx, request).await
    }

    async fn close_session(
        &self,
        ctx: &RequestContext,
        request: CloseSessionRequest,
    ) -> Result<CloseReceipt, ApiError> {
        self.resolve().await?.close_session(ctx, request).await
    }

    async fn get_execution(
        &self,
        ctx: &RequestContext,
        execution_id: ExecutionId,
    ) -> Result<ExecutionView, ApiError> {
        self.resolve().await?.get_execution(ctx, execution_id).await
    }

    async fn cancel_execution(
        &self,
        ctx: &RequestContext,
        execution_id: ExecutionId,
    ) -> Result<CancelReceipt, ApiError> {
        self.resolve().await?.cancel_execution(ctx, execution_id).await
    }

    async fn subscribe_events(
        &self,
        ctx: &RequestContext,
        stream_id: StreamId,
        after_sequence: Option<EventSequence>,
    ) -> Result<EventSubscription, ApiError> {
        self.resolve()
            .await?
            .subscribe_events(ctx, stream_id, after_sequence)
            .await
    }
}
