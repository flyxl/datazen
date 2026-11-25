use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use datazen_driver_api::{
    namespace::NamespaceTarget,
    resource::{
        AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall,
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceProvider, ResourcePurpose,
        ResourceScope, ResultChunk, ResultSink,
    },
    session::{CloseDisposition, CompletionStatus, ContextChangeDisposition, TransactionOptions},
    ConnectionConfig, QueryExecutionId,
};
use serde_json::json;

#[derive(Default)]
struct Budget {
    taken: AtomicU32,
    released: AtomicU32,
}

#[async_trait]
impl BudgetPort for Budget {
    async fn acquire_physical_connections(
        &self,
        count: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        self.taken.fetch_add(count, Ordering::SeqCst);
        Ok(BudgetPermit {
            permit_id: "fixture-permit".into(),
            physical_connections: count,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        self.released
            .fetch_add(permit.physical_connections, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Default)]
struct Sink {
    chunks: Mutex<Vec<ResultChunk>>,
    completed: AtomicBool,
    failed: AtomicBool,
}

#[async_trait]
impl ResultSink for Sink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.chunks.lock().unwrap().push(chunk);
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.completed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        self.failed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

pub async fn run(provider: &dyn ResourceProvider, config: &ConnectionConfig, port: u16) {
    let target = NamespaceTarget::empty().with_database("db1");
    let request = DescribeResourceRequest {
        connection_config: config.clone(),
        identity_scope: IdentityScope::default(),
        target: target.clone(),
        purpose: ResourcePurpose::InteractiveQuery,
    };
    let description = provider.describe_resource(&request).await.unwrap();
    assert_eq!(description.provider_id, "redis");
    assert_eq!(description.namespace_shape.path_segments.len(), 1);
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let resource = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: config.clone(),
                target,
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
                    max_physical_connections: 1,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
                identity_scope: IdentityScope::default(),
                baseline: Baseline::new(Vec::new()),
            },
            &budget,
        )
        .await
        .unwrap();
    assert_eq!(ledger.taken.load(Ordering::SeqCst), 1);

    let observation = provider.observe_session(&resource).await.unwrap();
    assert!(observation.protocol_drained);
    let execution = QueryExecutionId::new("fixture-command");
    let sink = Sink::default();
    let result = provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new("execute", json!({"sql":"SET contract:resource from-db1"})),
            &sink,
        )
        .await
        .unwrap();
    assert_eq!(result.completion_status, CompletionStatus::Succeeded);
    assert!(sink.completed.load(Ordering::SeqCst));
    assert_eq!(sink.chunks.lock().unwrap().len(), 1);

    for (db, expected) in [(1, Some("from-db1")), (2, None)] {
        let client = redis::Client::open(format!("redis://127.0.0.1:{port}/{db}")).unwrap();
        let mut witness = client.get_multiplexed_async_connection().await.unwrap();
        let value: Option<String> = redis::cmd("GET")
            .arg("contract:resource")
            .query_async(&mut witness)
            .await
            .unwrap();
        assert_eq!(value.as_deref(), expected);
    }
    let db2 = NamespaceTarget::empty().with_database("db2");
    assert_eq!(
        provider.change_context(&resource, &db2).await.unwrap(),
        ContextChangeDisposition::Confirmed
    );
    let sink = Sink::default();
    provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new("query", json!({"sql":"GET contract:resource\nPING"})),
            &sink,
        )
        .await
        .unwrap();
    assert_eq!(sink.chunks.lock().unwrap().len(), 2);

    let refusal = Sink::default();
    assert!(provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new("info", json!({})),
            &refusal
        )
        .await
        .is_err());
    assert!(!refusal.completed.load(Ordering::SeqCst));
    let invalid = Sink::default();
    assert!(provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new("nonexistent_fixture_command", json!({})),
            &invalid
        )
        .await
        .is_err());
    assert!(invalid.failed.load(Ordering::SeqCst));
    assert!(provider
        .begin_transaction(
            &resource,
            &TransactionOptions {
                isolation_level: Some("serializable".into()),
                read_only: Some(false),
                defers_commit: false,
            }
        )
        .await
        .is_err());
    assert!(provider.commit_transaction(&resource).await.is_err());
    assert!(provider.rollback_transaction(&resource).await.is_err());
    assert!(provider
        .request_cancel(&resource, &execution)
        .await
        .is_err());
    provider
        .reset_resource(&resource, &Baseline::new(Vec::new()))
        .await
        .unwrap();
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(provider.observe_session(&resource).await.is_err());
    assert_eq!(ledger.released.load(Ordering::SeqCst), 1);
}
