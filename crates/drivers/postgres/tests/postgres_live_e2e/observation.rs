use super::Fixture;
use std::sync::{Arc, Mutex};

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, IdentityScope, ResourceError,
    ResourceProvider, ResourcePurpose, ResourceScope,
};
use datazen_driver_api::session::{
    CloseDisposition, ObservationConfidence, TransactionOptions, TransactionState,
};
use datazen_driver_api::DatabaseDriver;
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};

#[derive(Default)]
struct Budget(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl BudgetPort for Budget {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        let permit_id = uuid::Uuid::new_v4().to_string();
        self.0.lock().unwrap().push(permit_id.clone());
        Ok(BudgetPermit {
            permit_id,
            physical_connections: n,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        let mut active = self.0.lock().unwrap();
        let index = active
            .iter()
            .position(|id| id == &permit.permit_id)
            .expect("only release an actual permit");
        active.swap_remove(index);
        Ok(())
    }
}

#[tokio::test]
async fn leased_session_and_pinned_transaction_report_only_live_postgres_facts() {
    let mut fixture = Fixture::open().await;
    let name = format!("dz_obs_{}", uuid::Uuid::new_v4().simple());
    let table = format!("{name}.live_rows");
    sqlx::raw_sql(&format!("CREATE SCHEMA {name};CREATE TABLE {table}(id integer PRIMARY KEY);INSERT INTO {table} VALUES(1);"))
        .execute(&mut fixture.oracle).await.expect("independent schema and data");
    let who: (String, String) =
        sqlx::query_as("SELECT current_database()::text, current_user::text")
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("independent session identity");
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver.clone());
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: NamespaceTarget::empty().with_database(who.0.clone()),
        scope: ResourceScope {
            purpose: ResourcePurpose::MetadataInspection,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let handle = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("leased real resource");
    let leased = provider
        .observe_session(&handle)
        .await
        .expect("actual leased session");
    assert_eq!(
        leased.context.namespace.database.as_deref(),
        Some(who.0.as_str())
    );
    assert_eq!(
        leased.context.effective_identity.as_deref(),
        Some(who.1.as_str())
    );
    assert_eq!(leased.context.confidence, ObservationConfidence::Partial);
    assert!(leased.context.namespace.schema.is_none() && leased.context.search_path.is_empty());

    provider
        .begin_transaction(
            &handle,
            &TransactionOptions {
                isolation_level: None,
                read_only: None,
                defers_commit: false,
            },
        )
        .await
        .expect("link a real pinned transaction");
    let session = provider
        .connection_handle(&handle)
        .expect("same underlying driver handle");
    driver
        .execute(
            &session,
            &format!("SET LOCAL search_path TO \"{name}\", public"),
        )
        .await
        .expect("set real transaction search path");
    assert_eq!(
        driver
            .execute(&session, "INSERT INTO live_rows VALUES(2)")
            .await
            .expect("live unqualified transaction write"),
        1
    );
    let pinned = provider
        .observe_session(&handle)
        .await
        .expect("actual pinned session");
    assert_eq!(
        pinned.context.namespace.database.as_deref(),
        Some(who.0.as_str())
    );
    assert_eq!(
        pinned.context.effective_identity.as_deref(),
        Some(who.1.as_str())
    );
    assert_eq!(
        pinned.context.namespace.schema.as_deref(),
        Some(name.as_str())
    );
    assert_eq!(
        pinned.context.search_path,
        [name.clone(), "public".to_owned()]
    );
    assert_eq!(pinned.context.confidence, ObservationConfidence::Confirmed);
    assert_eq!(pinned.context.transaction_state, TransactionState::Active);
    assert_eq!(pinned.context.autocommit, Some(false));
    assert_eq!(pinned.handles.len(), 1);
    let sql = format!("SELECT id FROM {table} ORDER BY id");
    let independently_visible = async |oracle: &mut sqlx::PgConnection| {
        sqlx::query_scalar::<_, i32>(&sql)
            .fetch_all(oracle)
            .await
            .expect("independent rows")
    };
    assert_eq!(
        independently_visible(&mut fixture.oracle).await,
        [1],
        "uncommitted row must not escape"
    );
    provider
        .close_handle(&handle, &pinned.handles[0])
        .await
        .expect("real rollback on handle close");
    assert_eq!(
        independently_visible(&mut fixture.oracle).await,
        [1],
        "closed transaction must roll back"
    );
    assert!(provider.commit_transaction(&handle).await.is_err());
    let after = provider
        .observe_session(&handle)
        .await
        .expect("still usable leased pool");
    assert_eq!(after.context.confidence, ObservationConfidence::Partial);
    assert!(after.handles.is_empty());
    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("release real pool"),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
    sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove observation fixture");
}

#[derive(Default)]
struct Outputs {
    statements: Mutex<Vec<datazen_driver_api::resource::StatementSummary>>,
    command: Mutex<Vec<serde_json::Value>>,
    completed: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl datazen_driver_api::resource::ResultSink for Outputs {
    async fn write(
        &self,
        _chunk: datazen_driver_api::resource::ResultChunk,
    ) -> Result<(), ResourceError> {
        panic!("these driver commands return summaries or metadata, not query rows")
    }

    async fn write_statement_summary(
        &self,
        statement: datazen_driver_api::resource::StatementSummary,
    ) -> Result<(), ResourceError> {
        self.statements.lock().unwrap().push(statement);
        Ok(())
    }

    async fn write_output(&self, data: serde_json::Value) -> Result<(), ResourceError> {
        self.command.lock().unwrap().push(data);
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.completed
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn fail(&self, _reason: &str) -> Result<(), ResourceError> {
        panic!("errors in these commands are returned directly")
    }
}

#[tokio::test]
async fn retained_command_summaries_and_aborted_transactions_match_real_postgres_state() {
    use datazen_driver_api::resource::CommandCall;
    use datazen_driver_api::session::EffectOutcome;
    use datazen_driver_api::QueryExecutionId;
    use serde_json::json;

    let mut fixture = Fixture::open().await;
    let schema = format!("dz_retained_{}", uuid::Uuid::new_v4().simple());
    let table = format!("{schema}.events");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; CREATE TABLE {table}(id int PRIMARY KEY); INSERT INTO {table} VALUES(1);"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("independent PostgreSQL fixture creation");
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver.clone());
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: NamespaceTarget::empty().with_database(fixture.config.database.clone().unwrap()),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let handle = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("retained live resource");
    let normal = TransactionOptions {
        isolation_level: None,
        read_only: None,
        defers_commit: false,
    };
    for refused in [
        TransactionOptions {
            isolation_level: Some("Serializable".into()),
            ..normal.clone()
        },
        TransactionOptions {
            read_only: Some(true),
            ..normal.clone()
        },
    ] {
        assert!(matches!(
            provider.begin_transaction(&handle, &refused).await,
            Err(ResourceError::OperationNotSupported { .. })
        ));
    }
    assert_eq!(
        provider
            .observe_session(&handle)
            .await
            .unwrap()
            .context
            .transaction_state,
        TransactionState::Idle
    );

    // Deliberately start the SQL transaction directly on the same retained
    // backend. A later real failure changes the server state, not a test flag.
    let connection = provider
        .connection_handle(&handle)
        .expect("retained connection");
    driver
        .execute(&connection, "BEGIN")
        .await
        .expect("actual SQL BEGIN");
    let insert = format!("INSERT INTO {table} VALUES(2)");
    let out = Outputs::default();
    let done = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("retained-write"),
            &CommandCall::new("execute", json!({"sql":insert})),
            &out,
        )
        .await
        .expect("real execute");
    assert_eq!(done.transaction_observation.state, TransactionState::Active);
    assert_eq!(out.statements.lock().unwrap()[0].rows_affected, Some(1));
    assert_eq!(out.statements.lock().unwrap()[0].sql, insert);
    assert_eq!(out.completed.load(std::sync::atomic::Ordering::SeqCst), 1);
    let duplicate = format!("INSERT INTO {table} VALUES(1)");
    assert!(matches!(
        provider
            .execute_on_resource(
                &handle,
                &QueryExecutionId::new("retained-duplicate"),
                &CommandCall::new("execute", json!({"sql":duplicate})),
                &Outputs::default()
            )
            .await,
        Err(ResourceError::Driver(_))
    ));
    let failed = provider
        .observe_session(&handle)
        .await
        .expect("read SQL aborted state");
    assert_eq!(failed.context.transaction_state, TransactionState::Aborted);
    let done = provider
        .commit_transaction(&handle)
        .await
        .expect("PG responds ROLLBACK to COMMIT on aborted tx");
    assert_eq!(done.effect, Some(EffectOutcome::RolledBack));
    assert_eq!(done.state, TransactionState::Idle);
    let external = format!("SELECT id FROM {table} ORDER BY id");
    assert_eq!(
        sqlx::query_scalar::<_, i32>(&external)
            .fetch_all(&mut fixture.oracle)
            .await
            .unwrap(),
        vec![1]
    );

    provider
        .begin_transaction(&handle, &normal)
        .await
        .expect("fresh transaction after rollback");
    let output = Outputs::default();
    let metadata = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("retained-schema"),
            &CommandCall::new("get_table_schema", json!({"relationSql":table})),
            &output,
        )
        .await
        .expect("same-connection metadata");
    assert!(metadata.completion_status.is_success());
    let results = output.command.lock().unwrap().clone();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["data"]["resolvedTarget"]["schema"], schema);
    assert_eq!(results[0]["data"]["resolvedTarget"]["table"], "events");
    assert_eq!(results[0]["data"]["resolvedTarget"]["sessionScoped"], false);
    let observation = provider
        .rollback_transaction(&handle)
        .await
        .expect("actual rollback acknowledgement");
    assert_eq!(observation.effect, Some(EffectOutcome::RolledBack));
    provider
        .close_resource(&handle)
        .await
        .expect("close retained session");
    assert!(ledger.0.lock().unwrap().is_empty());
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove isolated schema");
}

#[tokio::test]
async fn a_lost_retained_postgres_backend_is_not_silently_replaced() {
    use datazen_driver_api::{DriverError, Value};

    let mut fixture = Fixture::open().await;
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver.clone());
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: NamespaceTarget::empty().with_database(fixture.config.database.clone().unwrap()),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let lost = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("first live resource");
    let neighbor = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("separate live resource");
    let pid_for = async |h: &datazen_driver_api::resource::ResourceHandle| -> i32 {
        let handle = provider.connection_handle(h).expect("underlying PG handle");
        let result = driver
            .query(&handle, "SELECT pg_backend_pid()")
            .await
            .expect("actual backend PID");
        match result.rows[0][0] {
            Some(Value::Integer(value)) => value.try_into().unwrap(),
            ref other => panic!("expected PostgreSQL PID, got {other:?}"),
        }
    };
    let target_pid = pid_for(&lost).await;
    let neighbor_pid = pid_for(&neighbor).await;
    assert_ne!(target_pid, neighbor_pid);
    let fixture_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut fixture.oracle)
        .await
        .unwrap();
    assert_ne!(target_pid, fixture_pid);
    let killed: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1, 4000)")
        .bind(target_pid)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("remove only the owned fixture backend");
    assert!(killed);
    let remaining: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT FROM pg_stat_activity WHERE pid=$1)")
            .bind(target_pid)
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap();
    assert!(!remaining, "the PostgreSQL backend actually disappeared");
    assert!(matches!(
        provider.observe_session(&lost).await,
        Err(ResourceError::Driver(DriverError::SessionLost))
    ));
    assert_eq!(pid_for(&neighbor).await, neighbor_pid);
    assert_eq!(
        provider
            .close_resource(&lost)
            .await
            .expect("release lost resource"),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider
            .close_resource(&lost)
            .await
            .expect("idempotent close"),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider
            .close_resource(&neighbor)
            .await
            .expect("close independent resource"),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}
