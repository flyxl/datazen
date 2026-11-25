use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use datazen_driver_api::{
    namespace::NamespaceTarget,
    resource::{
        AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall,
        DescribeResourceRequest, IdentityScope, InitializationRequirement, ResourceError,
        ResourceProvider, ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
    },
    session::{
        CloseDisposition, CompletionStatus, ContextChangeDisposition, ResetDisposition,
        SessionHandleKind, SessionHandleRef, TransactionOptions,
    },
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
    refuses_rows: bool,
}

#[async_trait]
impl ResultSink for Sink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        if self.refuses_rows {
            return Err(ResourceError::SinkRejected {
                reason: "fixture refuses rows".into(),
            });
        }
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
    assert_eq!(provider.provider_id(), description.provider_id);
    assert_eq!(provider.capabilities().provider_id, description.provider_id);
    assert_eq!(
        provider.namespace_shape().path_segments.len(),
        description.namespace_shape.path_segments.len()
    );
    assert_eq!(description.namespace_shape.path_segments.len(), 1);
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = AcquireResourceRequest {
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
    };
    let unsupported_baseline = Baseline::new(vec![InitializationRequirement {
        id: "fixture-init".into(),
        sql: "PING".into(),
        mandatory: true,
        idempotent: true,
    }]);
    let mut bad = request.clone();
    bad.scope.max_physical_connections = 0;
    assert!(matches!(
        provider.acquire_resource(&bad, &budget).await,
        Err(ResourceError::BudgetDenied { .. })
    ));
    let mut bad = request.clone();
    bad.scope.holds_open_transaction = true;
    assert!(provider.acquire_resource(&bad, &budget).await.is_err());
    let mut bad = request.clone();
    bad.baseline = unsupported_baseline.clone();
    assert!(provider.acquire_resource(&bad, &budget).await.is_err());
    let mut bad = request.clone();
    bad.target = NamespaceTarget::empty().with_database("x");
    assert!(provider.acquire_resource(&bad, &budget).await.is_err());
    assert_eq!(ledger.taken.load(Ordering::SeqCst), 0);

    let mut bad = request.clone();
    bad.connection_config.port = Some(0);
    assert!(provider.acquire_resource(&bad, &budget).await.is_err());
    let mut bad = request.clone();
    bad.target = NamespaceTarget::empty().with_database("db999999");
    assert!(provider.acquire_resource(&bad, &budget).await.is_err());
    assert_eq!(ledger.taken.load(Ordering::SeqCst), 2);
    assert_eq!(ledger.released.load(Ordering::SeqCst), 2);
    let resource = provider.acquire_resource(&request, &budget).await.unwrap();
    assert_eq!(ledger.taken.load(Ordering::SeqCst), 3);

    let observation = provider.observe_session(&resource).await.unwrap();
    assert!(observation.protocol_drained);
    let next = provider.observe_session(&resource).await.unwrap();
    assert!(next.context_revision > observation.context_revision);
    assert!(!provider
        .connection_handle(&resource)
        .unwrap()
        .pool_id
        .is_empty());
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
    let malformed = NamespaceTarget::empty().with_database("dbbad");
    let refused_target = provider
        .change_context(&resource, &malformed)
        .await
        .unwrap_err();
    assert!(matches!(
        refused_target,
        ResourceError::NamespaceTargetRejected { .. }
    ));
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

    let refusing = Sink {
        refuses_rows: true,
        ..Default::default()
    };
    assert!(matches!(
        provider
            .execute_on_resource(
                &resource,
                &execution,
                &CommandCall::new("query", json!({"sql":"PING"})),
                &refusing
            )
            .await,
        Err(ResourceError::SinkRejected { .. })
    ));
    assert!(!refusing.completed.load(Ordering::SeqCst));
    assert!(provider
        .change_context(&resource, &NamespaceTarget::empty().with_schema("refused"))
        .await
        .is_err());
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
    assert!(provider
        .reset_resource(&resource, &unsupported_baseline)
        .await
        .is_err());
    assert_eq!(
        provider
            .reset_resource(&resource, &Baseline::new(Vec::new()))
            .await
            .unwrap(),
        ResetDisposition::Discard
    );
    let out_of_range = provider
        .change_context(
            &resource,
            &NamespaceTarget::empty().with_database("db999999"),
        )
        .await
        .unwrap_err();
    assert!(
        out_of_range.to_string().contains("999999"),
        "{out_of_range}"
    );
    let default_db = NamespaceTarget::empty();
    assert_eq!(
        provider
            .change_context(&resource, &default_db)
            .await
            .unwrap(),
        ContextChangeDisposition::Confirmed
    );
    let marker = Sink::default();
    provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new(
                "execute",
                json!({"sql":"SET contract:resource:root on-db0"}),
            ),
            &marker,
        )
        .await
        .unwrap();
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/0")).unwrap();
    let mut oracle = client.get_multiplexed_async_connection().await.unwrap();
    let value: String = redis::cmd("GET")
        .arg("contract:resource:root")
        .query_async(&mut oracle)
        .await
        .unwrap();
    assert_eq!(value, "on-db0");

    let named = Sink::default();
    provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new(
                "execute",
                json!({"sql":"CLIENT SETNAME datazen-resource-fixture"}),
            ),
            &named,
        )
        .await
        .unwrap();
    let clients: String = redis::cmd("CLIENT")
        .arg("LIST")
        .query_async(&mut oracle)
        .await
        .unwrap();
    let id = clients
        .lines()
        .find(|line| {
            line.split_whitespace()
                .any(|token| token == "name=datazen-resource-fixture")
        })
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("id="))
        .unwrap();
    let killed: u32 = redis::cmd("CLIENT")
        .arg(&["KILL", "ID", id])
        .query_async(&mut oracle)
        .await
        .unwrap();
    assert_eq!(killed, 1);
    assert!(matches!(
        provider.observe_session(&resource).await,
        Err(ResourceError::Driver(_))
    ));
    let dropped = Sink::default();
    let error = provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new("execute", json!({"sql":"PING"})),
            &dropped,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ResourceError::Driver(_)));
    assert!(dropped.failed.load(Ordering::SeqCst));
    assert!(!dropped.completed.load(Ordering::SeqCst));
    let unissued = SessionHandleRef::new(
        "fixture-cursor",
        SessionHandleKind::Cursor,
        resource.resource_key(),
        resource.runtime_epoch(),
    );
    assert!(provider.close_handle(&resource, &unissued).await.is_err());
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(provider.close_handle(&resource, &unissued).await.is_ok());
    assert!(provider.observe_session(&resource).await.is_err());
    assert_eq!(ledger.released.load(Ordering::SeqCst), 3);
    let mut default = request;
    default.target = NamespaceTarget::empty();
    let resource = provider.acquire_resource(&default, &budget).await.unwrap();
    let sink = Sink::default();
    provider
        .execute_on_resource(
            &resource,
            &execution,
            &CommandCall::new("query", json!({"sql":"GET contract:catalog"})),
            &sink,
        )
        .await
        .unwrap();
    {
        let chunks = sink.chunks.lock().unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0].rows,
            vec![vec![Some(datazen_driver_api::Value::String(
                "written".into()
            ))]]
        );
    }
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert_eq!(ledger.released.load(Ordering::SeqCst), 4);
}
