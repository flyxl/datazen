use super::results::RuntimeResultSink;
use crate::{connection::ProviderError, registry::backend::*};
use async_trait::async_trait;
use std::sync::Arc;
pub(super) struct ObservedBackend {
    pub inner: Arc<dyn SessionBackend>,
    pub sink: RuntimeResultSink,
}
#[async_trait]
impl SessionBackend for ObservedBackend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, ProviderError> {
        self.inner.open(r).await
    }
    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, ProviderError> {
        self.sink.started(&r.execution_id, &r.resource_id);
        let outcome = self.inner.execute(r).await?;
        self.sink.capture(outcome.clone());
        Ok(outcome)
    }
    async fn cancel(&self, r: CancelOnResource) -> Result<ResourceCancel, ProviderError> {
        self.inner.cancel(r).await
    }
    async fn finalize_handles(
        &self,
        r: FinalizeHandles,
    ) -> Result<HandleFinalization, ProviderError> {
        self.inner.finalize_handles(r).await
    }
    async fn close(&self, r: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
        self.inner.close(r).await
    }
}
