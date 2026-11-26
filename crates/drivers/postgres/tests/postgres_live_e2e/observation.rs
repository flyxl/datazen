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

#[tokio::test]
async fn real_admission_refuses_unpaid_or_invalid_scope_without_leaking_postgres_sessions() {
    use datazen_driver_api::resource::{DescribeResourceRequest, InitializationRequirement};

    let mut fixture = Fixture::open().await;
    let name = format!("dz_admit_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {name}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("real schema for a configured admission");
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver.clone());
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let target = NamespaceTarget::empty()
        .with_database(fixture.config.database.clone().unwrap())
        .with_schema(name.clone());
    let request = AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: target.clone(),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let described = provider
        .describe_resource(&DescribeResourceRequest {
            connection_config: request.connection_config.clone(),
            target,
            identity_scope: request.identity_scope.clone(),
            purpose: request.scope.purpose,
        })
        .await
        .expect("describe legitimate PostgreSQL target");
    assert!(described.initialization_requirements.is_empty());

    let unopened = async |oracle: &mut sqlx::PgConnection, database: &str| -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE datname=$1 AND pid<>pg_backend_pid()",
        )
        .bind(database)
        .fetch_one(oracle)
        .await
        .expect("independent sessions")
    };
    let database = fixture.config.database.clone().unwrap();
    let before = unopened(&mut fixture.oracle, &database).await;
    let mut over_budget = request.clone();
    over_budget.scope.max_physical_connections = 0;
    assert!(matches!(
        provider.acquire_resource(&over_budget, &budget).await,
        Err(ResourceError::BudgetDenied { .. })
    ));
    let mut forbidden_baseline = request.clone();
    forbidden_baseline.baseline = Baseline::new(vec![InitializationRequirement {
        id: "unsupported-replay".into(),
        sql: "SET application_name = 'not admitted'".into(),
        mandatory: true,
        idempotent: true,
    }]);
    assert!(matches!(
        provider
            .acquire_resource(&forbidden_baseline, &budget)
            .await,
        Err(ResourceError::OperationNotSupported { .. })
    ));
    let mut bad_auth = request.clone();
    bad_auth.connection_config.password = Some(uuid::Uuid::new_v4().to_string());
    assert!(matches!(
        provider.acquire_resource(&bad_auth, &budget).await,
        Err(ResourceError::Driver(_))
    ));
    assert!(ledger.0.lock().unwrap().is_empty());
    assert_eq!(unopened(&mut fixture.oracle, &database).await, before);

    let handle = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("valid scope still opens");
    let observed = provider
        .observe_session(&handle)
        .await
        .expect("actual configured schema");
    assert_eq!(
        observed.context.namespace.schema.as_deref(),
        Some(name.as_str())
    );
    let connection = provider
        .connection_handle(&handle)
        .expect("owned live PG connection");
    let query = driver
        .query(&connection, "SELECT current_schema()::text")
        .await
        .expect("actual session schema");
    assert_eq!(
        query.rows,
        vec![vec![Some(datazen_driver_api::Value::String(name.clone()))]]
    );
    provider
        .close_resource(&handle)
        .await
        .expect("confirm physical closure");
    assert!(ledger.0.lock().unwrap().is_empty());
    assert_eq!(unopened(&mut fixture.oracle, &database).await, before);
    sqlx::query(&format!("DROP SCHEMA {name}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn commit_after_backend_loss_is_unknown_and_never_reports_success() {
    use datazen_driver_api::session::EffectOutcome;
    use datazen_driver_api::Value;

    let mut fixture = Fixture::open().await;
    let table = format!("public.dz_lost_tx_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table}(id int PRIMARY KEY)"))
        .execute(&mut fixture.oracle)
        .await
        .expect("marked fixture table");
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
    let resource = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("real retained connection");
    let connection = provider
        .connection_handle(&resource)
        .expect("open fixture connection");
    let pid = driver
        .query(&connection, "SELECT pg_backend_pid()")
        .await
        .expect("PG assigned a backend");
    let [Some(Value::Integer(pid))] = pid.rows[0].as_slice() else {
        panic!("fixture PID not returned")
    };
    let pid: i32 = (*pid).try_into().unwrap();
    provider
        .begin_transaction(
            &resource,
            &TransactionOptions {
                isolation_level: None,
                read_only: None,
                defers_commit: false,
            },
        )
        .await
        .expect("live PostgreSQL transaction");
    assert_eq!(
        driver
            .execute(&connection, &format!("INSERT INTO {table} VALUES(7)"))
            .await
            .expect("server acknowledged uncommitted write"),
        1
    );
    let select = format!("SELECT count(*) FROM {table}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&select)
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap(),
        0
    );
    let available: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1, 4000)")
        .bind(pid)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("stop only test backend");
    assert!(available, "retained backend was terminated before COMMIT");
    let receipt = provider
        .commit_transaction(&resource)
        .await
        .expect("unconfirmed commit returns an honest receipt");
    assert_eq!(receipt.state, TransactionState::Unknown);
    assert_eq!(receipt.effect, Some(EffectOutcome::Unknown));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&select)
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap(),
        0,
        "external verification sees no committed row"
    );
    provider
        .close_resource(&resource)
        .await
        .expect("clean up lost connection");
    assert!(ledger.0.lock().unwrap().is_empty());
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove marked fixture table");
}

#[tokio::test]
async fn pg_wire_alias_factories_can_close_their_own_live_resources_and_transactions() {
    use datazen_driver_api::Value;

    let mut fixture = Fixture::open().await;
    let oracle: i32 = sqlx::query_scalar("SELECT 43")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent PG response");
    let all = datazen_driver_postgres::factories();
    let ordinary = all
        .iter()
        .find(|f| f.driver_id() == "postgresql")
        .expect("native factory");
    let native = ordinary.resource_provider().expect("native provider");
    // Both aliases deliberately use a real PostgreSQL wire fixture here.
    // This verifies this crate's lifecycle wiring, not the other products'
    // server-specific SQL or compatibility.
    for alias in ["questdb", "cloudberry"] {
        let factory = all
            .iter()
            .find(|f| f.driver_id() == alias)
            .expect("registered alias factory");
        let driver = factory.create();
        let provider = factory
            .resource_provider()
            .expect("registered aliased provider");
        assert_eq!(provider.provider_id(), alias);
        assert_eq!(
            factory.resource_capabilities(),
            provider.capabilities().capabilities
        );
        let ledger = Arc::new(Budget::default());
        let budget: Arc<dyn BudgetPort> = ledger.clone();
        let mut config = fixture.config.clone();
        config.id = uuid::Uuid::new_v4().to_string();
        config.database_type = alias.into();
        let request = AcquireResourceRequest {
            connection_config: config.clone(),
            target: NamespaceTarget::empty().with_database(config.database.clone().unwrap()),
            scope: ResourceScope {
                purpose: ResourcePurpose::InteractiveQuery,
                max_physical_connections: 16,
                holds_open_transaction: false,
                pin_for_streaming: false,
            },
            identity_scope: IdentityScope::default(),
            baseline: Baseline::default(),
        };
        let descriptor = provider
            .describe_resource(&datazen_driver_api::resource::DescribeResourceRequest {
                connection_config: config,
                target: request.target.clone(),
                identity_scope: request.identity_scope.clone(),
                purpose: request.scope.purpose,
            })
            .await
            .expect("same real server identity scope");
        assert_eq!(descriptor.provider_id, alias);
        let resource = provider
            .acquire_resource(&request, &budget)
            .await
            .expect("aliased real PG connection");
        assert!(
            native.connection_handle(&resource).is_err(),
            "native provider must not own the alias handle"
        );
        let connection = provider
            .connection_handle(&resource)
            .expect("factory owns its compatible native pool");
        let rows = driver
            .query(&connection, "SELECT 43")
            .await
            .expect("actual server query via the alias driver");
        assert_eq!(
            rows.rows,
            vec![vec![Some(Value::Integer(i64::from(oracle)))]]
        );
        provider
            .begin_transaction(
                &resource,
                &TransactionOptions {
                    isolation_level: None,
                    read_only: None,
                    defers_commit: false,
                },
            )
            .await
            .expect("aliased real transaction");
        let state = provider
            .observe_session(&resource)
            .await
            .expect("alias observes real transaction");
        assert_eq!(state.context.transaction_state, TransactionState::Active);
        assert!(native
            .close_handle(&resource, &state.handles[0])
            .await
            .is_err());
        provider
            .close_handle(&resource, &state.handles[0])
            .await
            .expect("alias can close its own transaction");
        assert_eq!(
            provider
                .observe_session(&resource)
                .await
                .unwrap()
                .context
                .transaction_state,
            TransactionState::Idle
        );
        assert!(native.close_resource(&resource).await.is_err());
        assert_eq!(
            provider
                .close_resource(&resource)
                .await
                .expect("alias can close its own physical pool"),
            CloseDisposition::Closed
        );
        assert!(ledger.0.lock().unwrap().is_empty());
    }
}

#[derive(Default)]
struct PausedAdmission {
    ledger: Budget,
    acquired: std::sync::atomic::AtomicUsize,
    release_retries: std::sync::atomic::AtomicUsize,
    waiting: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl BudgetPort for PausedAdmission {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        let index = self
            .acquired
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if index == 1 {
            self.waiting.notify_one();
            self.resume.notified().await;
        }
        if index == 0 {
            let permit_id = format!("data-{}", uuid::Uuid::new_v4());
            self.ledger.0.lock().unwrap().push(permit_id.clone());
            Ok(BudgetPermit {
                permit_id,
                physical_connections: n,
            })
        } else {
            self.ledger.acquire_physical_connections(n).await
        }
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        if permit.permit_id.starts_with("data-")
            && self
                .release_retries
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                == 0
        {
            return Err(ResourceError::BudgetDenied {
                requested: permit.physical_connections,
                reason: "fixture budget ledger briefly unavailable".into(),
            });
        }
        self.ledger.release_physical_connections(permit).await
    }
}

#[tokio::test]
async fn abandoning_real_postgres_admission_closes_sockets_and_retries_budget_release() {
    use std::sync::atomic::Ordering::SeqCst;
    use std::time::Duration;
    let mut fixture = Fixture::open().await;
    let database = fixture.config.database.clone().unwrap();
    let before = async |oracle: &mut sqlx::PgConnection| -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE datname=$1 AND pid<>pg_backend_pid()",
        )
        .bind(&database)
        .fetch_one(oracle)
        .await
        .unwrap()
    };
    let baseline = before(&mut fixture.oracle).await;
    let provider = Arc::new(PostgresResourceProvider::new(Arc::new(
        PostgresDriver::new(),
    )));
    let ledger = Arc::new(PausedAdmission::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: NamespaceTarget::empty().with_database(database.clone()),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let pending = tokio::spawn(async move { provider.acquire_resource(&request, &budget).await });
    tokio::time::timeout(Duration::from_secs(8), ledger.waiting.notified())
        .await
        .expect("real data socket opened before control permits");
    assert!(
        before(&mut fixture.oracle).await > baseline,
        "independent PG sees the partially opened resource"
    );
    assert_eq!(
        ledger.ledger.0.lock().unwrap().len(),
        1,
        "one already-owned data permit"
    );
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    ledger.resume.notify_one();
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if ledger.ledger.0.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("owned cleanup completes after caller cancellation");
    assert_eq!(ledger.acquired.load(SeqCst), 2);
    assert!(
        ledger.release_retries.load(SeqCst) >= 2,
        "failed host-ledger release was not mistaken for success"
    );
    tokio::time::timeout(Duration::from_secs(4), async {
        while before(&mut fixture.oracle).await != baseline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("independent PG confirms no remaining sockets");
}
