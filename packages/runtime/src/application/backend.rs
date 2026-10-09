use super::results::lock;
use super::results::RuntimeResultSink;
use crate::{connection::ProviderError, registry::backend::*};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[derive(Clone)]
pub(super) struct PhysicalOutcome {
    pub id: String,
    pub release: Option<CloseResourceOutcome>,
    pub effect: crate::connection::EffectOutcome,
}
pub(super) type PhysicalLedger = Arc<Mutex<HashMap<u64, PhysicalOutcome>>>;
pub(super) struct ObservedBackend {
    pub inner: Arc<dyn SessionBackend>,
    pub sink: RuntimeResultSink,
    pub physical: PhysicalLedger,
}
#[async_trait]
impl SessionBackend for ObservedBackend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, ProviderError> {
        let epoch = r.runtime_epoch;
        let opened = self.inner.open(r).await?;
        lock(&self.physical).insert(
            epoch,
            PhysicalOutcome {
                id: opened.resource_id.clone(),
                release: None,
                effect: crate::connection::EffectOutcome::Unknown,
            },
        );
        Ok(opened)
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
        let id = r.resource_id.clone();
        let outcome = self.inner.finalize_handles(r).await?;
        for record in lock(&self.physical).values_mut().filter(|v| v.id == id) {
            record.effect = outcome.effect_outcome;
        }
        Ok(outcome)
    }
    async fn close(&self, r: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
        let id = r.resource_id.clone();
        let outcome = self.inner.close(r).await?;
        for record in lock(&self.physical).values_mut().filter(|v| v.id == id) {
            record.release = Some(outcome);
        }
        Ok(outcome)
    }
}
