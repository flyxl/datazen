use super::backend::*;
use crate::execution::runtime::connection::ProviderError;
use async_trait::async_trait;
use std::sync::Arc;
pub(super) struct CommandBackend {
    pub inner: Arc<dyn SessionBackend>,
    pub command: String,
}
#[async_trait]
impl SessionBackend for CommandBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, ProviderError> {
        self.inner.open_for_command(request, &self.command).await
    }
    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, ProviderError> {
        self.inner.execute(request).await
    }
    async fn cancel(&self, request: CancelOnResource) -> Result<ResourceCancel, ProviderError> {
        self.inner.cancel(request).await
    }
    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, ProviderError> {
        self.inner.finalize_handles(request).await
    }
    async fn close(&self, request: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
        self.inner.close(request).await
    }
}
