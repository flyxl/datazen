use super::{Fixture, resource::Ledger};
use datazen_driver_api::{
    ConnectionConfig, DatabaseDriver, QueryExecutionId, Value,
    namespace::NamespaceTarget,
    resource::{
        AcquireResourceRequest, Baseline, BudgetPort, IdentityScope, ResourceError,
        ResourceProvider, ResourcePurpose, ResourceScope,
    },
    session::{
        CancelDisposition, CloseDisposition, EffectOutcome, ObservationConfidence,
        TransactionOptions, TransactionState,
    },
};
use std::sync::Arc;

fn factory() -> &'static dyn datazen_driver_api::DatabaseDriverFactory {
    datazen_driver_mysql::factories()
        .into_iter()
        .find(|f| f.driver_id() == "mysql")
        .unwrap()
}
fn request(config: &ConnectionConfig, purpose: ResourcePurpose) -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config.clone(),
        target: NamespaceTarget::empty().with_database(config.database.clone().unwrap()),
        scope: ResourceScope {
            purpose,
            max_physical_connections: 8,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    }
}
fn provider() -> Arc<dyn ResourceProvider> {
    datazen_driver_api::require_resource_provider(factory()).unwrap()
}

#[tokio::test]
async fn mysql_performance_schema_proves_raw_transaction_without_guessing_uninstrumented_state() {
    let mut f = Fixture::open().await;
    let driver = factory().create();
    let provider = provider();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    for definition in [
        "UPDATE performance_schema.setup_instruments SET ENABLED='YES', TIMED='YES' WHERE NAME='transaction'",
        "UPDATE performance_schema.setup_consumers SET ENABLED='YES' WHERE NAME IN ('events_transactions_current','thread_instrumentation','global_instrumentation')",
    ] {
        sqlx::query(definition)
            .execute(&mut f.oracle)
            .await
            .expect("authentic disposable runtime instrumentation");
    }

    let resource = provider
        .acquire_resource(&request(&f.cfg, ResourcePurpose::InteractiveQuery), &budget)
        .await
        .expect("real retained session");
    let handle = provider.connection_handle(&resource).unwrap();
    let table = format!("dz_observed_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let start = provider
        .observe_session(&resource)
        .await
        .expect("idle proven on real control socket");
    assert_eq!(start.context.transaction_state, TransactionState::Idle);
    let native_pid = driver
        .query(&handle, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let Some(Value::Integer(pid)) = native_pid else {
        panic!("native exact CONNECTION_ID()")
    };
    driver
        .execute(&handle, "START TRANSACTION")
        .await
        .expect("user statement starts native transaction");
    driver
        .execute(&handle, &format!("INSERT INTO {table} VALUES(19)"))
        .await
        .unwrap();
    let status:String=sqlx::query_scalar(
        "SELECT CAST(e.STATE AS CHAR) FROM performance_schema.threads t JOIN performance_schema.events_transactions_current e ON e.THREAD_ID=t.THREAD_ID WHERE t.PROCESSLIST_ID=?"
    ).bind(pid).fetch_one(&mut f.oracle).await.expect("independent MySQL attests actual active transaction");
    assert_eq!(status, "ACTIVE");
    let observed = provider
        .observe_session(&resource)
        .await
        .expect("read actual wire and independent control");
    assert_eq!(observed.context.transaction_state, TransactionState::Active);
    assert_eq!(
        observed.context.confidence,
        ObservationConfidence::Confirmed
    );
    assert_eq!(observed.handles.len(), 1);
    provider
        .close_handle(&resource, &observed.handles[0])
        .await
        .expect("roll back adopted real transaction only");
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let idle = provider
        .observe_session(&resource)
        .await
        .expect("actual MySQL rollback");
    assert_eq!(idle.context.transaction_state, TransactionState::Idle);
    assert!(idle.handles.is_empty());
    sqlx::query("UPDATE performance_schema.setup_consumers SET ENABLED='NO' WHERE NAME='events_transactions_current'")
        .execute(&mut f.oracle).await.expect("genuinely disable the disposable server's consumer");
    driver.execute(&handle, "START TRANSACTION").await.unwrap();
    driver
        .execute(&handle, &format!("INSERT INTO {table} VALUES(23)"))
        .await
        .unwrap();
    let unknown = provider
        .observe_session(&resource)
        .await
        .expect("still a real reachable session");
    assert_eq!(unknown.context.transaction_state, TransactionState::Unknown);
    assert_eq!(unknown.context.confidence, ObservationConfidence::Partial);
    let undone = provider
        .rollback_transaction(&resource)
        .await
        .expect("wire rollback even with unavailable observer");
    assert_eq!(undone.effect, Some(EffectOutcome::RolledBack));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        0
    );
    sqlx::query("UPDATE performance_schema.setup_consumers SET ENABLED='YES' WHERE NAME='events_transactions_current'")
        .execute(&mut f.oracle).await.unwrap();
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_provider_does_not_acknowledge_commit_of_a_lost_backend_or_leak_admission() {
    let mut f = Fixture::open().await;
    let driver = factory().create();
    let provider = provider();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let mut invalid = f.cfg.clone();
    invalid.password = Some(uuid::Uuid::new_v4().to_string());
    invalid.connection_timeout = 1;
    assert!(matches!(
        provider
            .acquire_resource(
                &request(&invalid, ResourcePurpose::InteractiveQuery),
                &budget
            )
            .await,
        Err(ResourceError::Driver(_))
    ));
    assert!(
        ledger.0.lock().unwrap().is_empty(),
        "failed real authentication must release the charged permit"
    );
    let resource = provider
        .acquire_resource(&request(&f.cfg, ResourcePurpose::InteractiveQuery), &budget)
        .await
        .unwrap();
    let handle = provider.connection_handle(&resource).unwrap();
    let table = format!("dz_lost_write_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(
        provider
            .request_cancel(&resource, &QueryExecutionId::new("missing-owned-execution"))
            .await
            .unwrap()
            .disposition,
        CancelDisposition::NotRegistered
    );
    provider
        .begin_transaction(&resource, &TransactionOptions::default())
        .await
        .expect("start actual MySQL write");
    driver
        .execute(&handle, &format!("INSERT INTO {table} VALUES(7)"))
        .await
        .unwrap();
    let Some(Value::Integer(pid)) = driver
        .query(&handle, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows[0][0]
    else {
        panic!("real fixture PID")
    };
    sqlx::query(&format!("KILL CONNECTION {pid}"))
        .execute(&mut f.oracle)
        .await
        .expect("terminate only the fixture-owned backend");
    let terminal = provider
        .commit_transaction(&resource)
        .await
        .expect("effect receipt still honest");
    assert_eq!(terminal.state, TransactionState::Unknown);
    assert_eq!(terminal.effect, Some(EffectOutcome::Unknown));
    assert!(!terminal.committed());
    assert!(
        provider.observe_session(&resource).await.is_err(),
        "must not silently replace lost MySQL checkout"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[derive(Default)]
struct PausedLedger {
    inner: Ledger,
    pending: tokio::sync::Notify,
    proceed: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl BudgetPort for PausedLedger {
    async fn acquire_physical_connections(
        &self,
        n: u32,
    ) -> Result<datazen_driver_api::resource::BudgetPermit, ResourceError> {
        self.inner.acquire_physical_connections(n).await
    }
    async fn release_physical_connections(
        &self,
        permit: &datazen_driver_api::resource::BudgetPermit,
    ) -> Result<(), ResourceError> {
        self.pending.notify_one();
        self.proceed.notified().await;
        self.inner.release_physical_connections(permit).await
    }
}
#[tokio::test]
async fn mysql_parallel_close_does_not_claim_full_settlement_before_real_socket_and_budget_close() {
    let mut f = Fixture::open().await;
    let driver = factory().create();
    let provider = provider();
    let ledger = Arc::new(PausedLedger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let owned = provider
        .acquire_resource(&request(&f.cfg, ResourcePurpose::InteractiveQuery), &budget)
        .await
        .unwrap();
    let h = provider.connection_handle(&owned).unwrap();
    let Some(Value::Integer(pid)) = driver
        .query(&h, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows[0][0]
    else {
        panic!("real mysql owned pid")
    };
    let release = ledger.pending.notified();
    let first = tokio::spawn({
        let (p, h) = (provider.clone(), owned.clone());
        async move { p.close_resource(&h).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(4), release)
        .await
        .expect("socket cleanup happened and accounting awaits acknowledgement");
    assert_eq!(
        provider.close_resource(&owned).await.unwrap(),
        CloseDisposition::CloseUnconfirmed
    );
    assert!(provider.observe_session(&owned).await.is_err());
    assert_eq!(ledger.inner.0.lock().unwrap().len(), 1);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let held: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID=?",
            )
            .bind(pid)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
            if held == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
    })
    .await
    .expect("independent MySQL confirms the original socket is already gone");
    ledger.proceed.notify_one();
    assert_eq!(first.await.unwrap().unwrap(), CloseDisposition::Closed);
    assert_eq!(
        provider.close_resource(&owned).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.inner.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_fixed_typed_begin_does_not_implicitly_commit_native_transactions() {
    use datazen_driver_api::{DriverError, Value};
    let mut f = Fixture::open().await;
    let driver = datazen_driver_mysql::MysqlDriver::new(false);
    let fixed = driver
        .connect_fixed_session(&f.cfg)
        .await
        .expect("real pinned session");
    let pooled = driver
        .connect(&f.cfg)
        .await
        .expect("separate pooled session");
    let table = format!("dz_nested_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY) ENGINE=InnoDB"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let visible = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_scalar::<_, i32>(&format!("SELECT id FROM {table} ORDER BY id"))
            .fetch_all(oracle)
            .await
            .unwrap()
    };

    // The real server's next-transaction read-only setting must survive the
    // nesting guard. Set it independently and verify the InnoDB write refusal.
    driver
        .execute(&fixed, "SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    let read_only = driver
        .begin_transaction(&fixed)
        .await
        .expect("accept idle next READ ONLY");
    assert!(
        matches!(
            driver.begin_transaction(&fixed).await,
            Err(DriverError::TransactionError(ref detail)) if detail.contains("already open")
        ),
        "an active transaction need not have written rows"
    );
    assert!(
        matches!(driver.execute(&fixed,&format!("INSERT INTO {table} VALUES (3)")).await,
       Err(DriverError::QueryFailed(detail)) if detail.contains("READ ONLY")||detail.contains("read-only"))
    );
    driver
        .rollback(read_only)
        .await
        .expect("rollback read-only transaction");
    assert!(visible(&mut f.oracle).await.is_empty());

    let tx = driver
        .begin_transaction(&fixed)
        .await
        .expect("start retained transaction");
    driver
        .execute(&fixed, &format!("INSERT INTO {table} VALUES(7)"))
        .await
        .unwrap();
    assert!(visible(&mut f.oracle).await.is_empty());
    assert!(
        matches!(driver.begin_transaction(&fixed).await,Err(DriverError::TransactionError(ref msg)) if msg.contains("already open"))
    );
    assert!(
        visible(&mut f.oracle).await.is_empty(),
        "refused nested typed BEGIN must never persist pending changes"
    );
    assert_eq!(
        driver
            .query(&fixed, &format!("SELECT id FROM {table}"))
            .await
            .unwrap()
            .rows,
        [vec![Some(Value::Integer(7))]]
    );
    driver.rollback(tx).await.unwrap();
    assert!(visible(&mut f.oracle).await.is_empty());

    driver.execute(&fixed, "START TRANSACTION").await.unwrap();
    driver
        .execute(&fixed, &format!("INSERT INTO {table} VALUES(9)"))
        .await
        .unwrap();
    assert!(matches!(
        driver.begin_transaction(&fixed).await,
        Err(DriverError::TransactionError(_))
    ));
    assert!(
        visible(&mut f.oracle).await.is_empty(),
        "native raw transaction is not determined by the driver's previous typed calls"
    );
    driver.execute(&fixed, "ROLLBACK").await.unwrap();
    assert!(visible(&mut f.oracle).await.is_empty());

    let pool_tx = driver.begin_transaction(&pooled).await.unwrap();
    assert!(matches!(
        driver.begin_transaction(&pooled).await,
        Err(DriverError::TransactionError(_))
    ));
    assert!(matches!(
        driver.begin_read_snapshot(&pooled).await,
        Err(DriverError::TransactionError(_))
    ));
    let id = pool_tx.id.clone();
    let resource = pool_tx.connection_id.clone();
    driver.rollback(pool_tx).await.unwrap();
    let ended = || datazen_driver_api::TransactionHandle {
        id: id.clone(),
        connection_id: resource.clone(),
    };
    assert!(matches!(
        driver.rollback(ended()).await,
        Err(DriverError::TransactionError(_))
    ));
    assert!(matches!(
        driver.commit(ended()).await,
        Err(DriverError::TransactionError(_))
    ));

    let writable = driver.begin_transaction(&fixed).await.unwrap();
    driver
        .execute(&fixed, &format!("INSERT INTO {table} VALUES(22)"))
        .await
        .unwrap();
    driver.commit(writable).await.unwrap();
    assert_eq!(visible(&mut f.oracle).await, [22]);
    driver.disconnect(pooled).await.unwrap();
    driver.disconnect(fixed).await.unwrap();
}
