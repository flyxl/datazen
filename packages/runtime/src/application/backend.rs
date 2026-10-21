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
    async fn open_for_command(
        &self,
        r: OpenResource,
        command: &str,
    ) -> Result<OpenedResource, ProviderError> {
        let epoch = r.runtime_epoch;
        let opened = self.inner.open_for_command(r, command).await?;
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
    async fn reclaim_idle_resources(&self) {
        self.inner.reclaim_idle_resources().await;
    }
    /// 只读观测口必须显式转发。
    ///
    /// 这一层是 `SessionBackend` 的装饰器，而 trait 方法带默认实现（默认 `None`）：
    /// 忘记转发不会编译失败，只会让应用层的快照**永远为空**——那正是"加了口却读不到"
    /// 的静默失败。下方 `observed_backend_forwards_the_observability_snapshot` 钉住它。
    async fn observability_snapshot(&self) -> Option<crate::resource::RuntimeObservabilityReport> {
        self.inner.observability_snapshot().await
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{PoolObservabilityEntry, RuntimeObservabilityReport};

    /// Minimal backend: only the observability port answers. Any other call is a
    /// test bug and returns a protocol error, so it cannot pass silently.
    struct SnapshotOnly {
        report: Option<RuntimeObservabilityReport>,
    }

    fn marker_report() -> RuntimeObservabilityReport {
        RuntimeObservabilityReport::from_ledger(
            vec![PoolObservabilityEntry {
                pool_key_digest: "marker:c1:n1".into(),
                model: "stateful",
                max_idle: 2,
                in_use: 1,
                idle: 0,
                closing: 0,
                quarantined: 0,
                issued: 1,
            }],
            1,
            1,
            0,
            0,
            0,
        )
    }

    #[async_trait]
    impl SessionBackend for SnapshotOnly {
        async fn open(&self, _r: OpenResource) -> Result<OpenedResource, ProviderError> {
            Err(ProviderError::ProtocolError("unused".into()))
        }
        async fn execute(&self, _r: ExecuteOnResource) -> Result<ResourceExecution, ProviderError> {
            Err(ProviderError::ProtocolError("unused".into()))
        }
        async fn cancel(&self, _r: CancelOnResource) -> Result<ResourceCancel, ProviderError> {
            Err(ProviderError::ProtocolError("unused".into()))
        }
        async fn finalize_handles(
            &self,
            _r: FinalizeHandles,
        ) -> Result<HandleFinalization, ProviderError> {
            Err(ProviderError::ProtocolError("unused".into()))
        }
        async fn close(&self, _r: CloseResource) -> Result<CloseResourceOutcome, ProviderError> {
            Err(ProviderError::ProtocolError("unused".into()))
        }
        async fn observability_snapshot(&self) -> Option<RuntimeObservabilityReport> {
            self.report.clone()
        }
    }

    /// 装饰器忘记转发 `observability_snapshot` 不会编译失败——trait 的默认实现返回
    /// `None`，症状只是"应用层快照永远是 null"。这条测试把那个静默失败变成红的。
    #[tokio::test]
    async fn observed_backend_forwards_the_observability_snapshot() {
        let wrapper = ObservedBackend {
            inner: Arc::new(SnapshotOnly {
                report: Some(marker_report()),
            }),
            sink: RuntimeResultSink::default(),
            physical: Arc::new(Mutex::new(HashMap::new())),
        };
        let report = wrapper
            .observability_snapshot()
            .await
            .expect("the decorator must forward the snapshot, not fall back to None");
        assert_eq!(report.physical_total, 1);
        assert_eq!(report.pools[0].pool_key_digest, "marker:c1:n1");
    }

    /// 反向：内层真的没有该端口时，装饰器必须如实返回 `None`（而不是编一份空报告）。
    #[tokio::test]
    async fn observed_backend_reports_none_when_the_inner_backend_has_no_snapshot() {
        let wrapper = ObservedBackend {
            inner: Arc::new(SnapshotOnly { report: None }),
            sink: RuntimeResultSink::default(),
            physical: Arc::new(Mutex::new(HashMap::new())),
        };
        assert!(wrapper.observability_snapshot().await.is_none());
    }
}
