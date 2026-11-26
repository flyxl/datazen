use super::Fixture;
use datazen_driver_api::{
    namespace::NamespaceTarget,
    resource::{
        AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall,
        DescribeResourceRequest, IdentityScope, ResourceError, ResourcePurpose, ResourceScope,
        ResultChunk, ResultSink,
    },
    session::{
        CloseDisposition, ContextChangeDisposition, ObservationConfidence, ResetDisposition,
        TransactionOptions, TransactionState,
    },
    DatabaseDriver, QueryExecutionId, Value,
};
use serde_json::{json, Value as Json};
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Ledger(Mutex<BTreeMap<String, u32>>);
#[async_trait::async_trait]
impl BudgetPort for Ledger {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        let id = uuid::Uuid::new_v4().to_string();
        self.0.lock().unwrap().insert(id.clone(), n);
        Ok(BudgetPermit {
            permit_id: id,
            physical_connections: n,
        })
    }
    async fn release_physical_connections(&self, p: &BudgetPermit) -> Result<(), ResourceError> {
        assert_eq!(
            self.0.lock().unwrap().remove(&p.permit_id),
            Some(p.physical_connections)
        );
        Ok(())
    }
}
struct Artifact(Mutex<std::fs::File>);
#[async_trait::async_trait]
impl ResultSink for Artifact {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.write_output(
            json!({"kind":"rows","statementIndex":chunk.statement_index,"rows":chunk.rows}),
        )
        .await
    }
    async fn write_output(&self, event: Json) -> Result<(), ResourceError> {
        writeln!(self.0.lock().unwrap(), "{event}").map_err(|error| {
            ResourceError::SinkRejected {
                reason: error.to_string(),
            }
        })?;
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        self.write_output(json!({"kind":"complete"})).await
    }
    async fn fail(&self, reason: &str) -> Result<(), ResourceError> {
        self.write_output(json!({"kind":"failure","reason":reason}))
            .await
    }
}

#[tokio::test]
async fn mysql_provider_retains_real_transaction_context_and_closes_named_rollback() {
    let mut f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|p| p.driver_id() == "mysql")
        .expect("live MySQL factory");
    let driver = factory.create();
    let provider =
        datazen_driver_api::require_resource_provider(factory).expect("factory resource contract");
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let foreign = std::env::var("TEST_MYSQL_DATABASE_B").expect("second actual catalog");
    let table = format!("dz_provider_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE TABLE {foreign}.{table}(id INT PRIMARY KEY); INSERT INTO {foreign}.{table} VALUES(1)"))
        .execute(&mut f.oracle).await.expect("independent transactional target");
    let requested = NamespaceTarget::empty().with_database(f.database.clone());
    let desired = NamespaceTarget::empty().with_database(foreign.clone());
    let desc = provider
        .describe_resource(&DescribeResourceRequest {
            connection_config: f.cfg.clone(),
            identity_scope: IdentityScope::default(),
            target: requested.clone(),
            purpose: ResourcePurpose::InteractiveQuery,
        })
        .await
        .expect("advertised real retained scope");
    assert_eq!(
        desc.session_continuity,
        datazen_driver_api::capabilities::SessionContinuity::Fixed
    );
    let request = |purpose, max| AcquireResourceRequest {
        connection_config: f.cfg.clone(),
        identity_scope: IdentityScope::default(),
        target: requested.clone(),
        scope: ResourceScope {
            purpose,
            max_physical_connections: max,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        baseline: Baseline::default(),
    };
    assert!(matches!(
        provider
            .acquire_resource(&request(ResourcePurpose::InteractiveQuery, 0), &budget)
            .await,
        Err(ResourceError::BudgetDenied { .. })
    ));
    let fixed = provider
        .acquire_resource(&request(ResourcePurpose::InteractiveQuery, 8), &budget)
        .await
        .expect("acquire actual fixed connection");
    let leased = provider
        .acquire_resource(&request(ResourcePurpose::MetadataInspection, 8), &budget)
        .await
        .expect("acquire actual leased connection");
    assert_eq!(
        provider
            .observe_session(&leased)
            .await
            .expect("leased observation")
            .context
            .confidence,
        ObservationConfidence::Partial
    );
    let original = provider
        .observe_session(&fixed)
        .await
        .expect("fixed observation");
    assert_eq!(
        original.context.confidence,
        ObservationConfidence::Confirmed
    );
    assert_eq!(
        original.context.namespace.database.as_deref(),
        Some(f.database.as_str())
    );
    assert_eq!(
        provider
            .change_context(&fixed, &NamespaceTarget::empty())
            .await
            .expect("empty context"),
        ContextChangeDisposition::Unsupported
    );
    let connection = provider
        .connection_handle(&fixed)
        .expect("physical fixed handle");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY); INSERT INTO {table} VALUES(99)"
    ))
    .execute(&mut f.oracle)
    .await
    .expect("deliberate same-name decoy in old database");
    let warmed = format!("SELECT id FROM {table} WHERE id > ? ORDER BY id");
    assert_eq!(
        driver
            .query_with_params(&connection, &warmed, &[Value::Integer(0)])
            .await
            .expect("prepare unqualified SQL in old catalog")
            .rows,
        [vec![Some(Value::Integer(99))]]
    );
    assert_eq!(
        provider
            .change_context(&fixed, &desired)
            .await
            .expect("same socket switches native context"),
        ContextChangeDisposition::Confirmed
    );
    assert_eq!(
        provider
            .observe_session(&fixed)
            .await
            .expect("observed after switch")
            .context
            .namespace
            .database
            .as_deref(),
        Some(foreign.as_str())
    );
    assert_eq!(
        provider
            .reset_resource(&fixed, &Baseline::default())
            .await
            .expect("reset contract"),
        ResetDisposition::Discard
    );
    assert_eq!(
        driver
            .query_with_params(&connection, &warmed, &[Value::Integer(0)])
            .await
            .expect("same SQL now uses changed catalog")
            .rows,
        [vec![Some(Value::Integer(1))]]
    );
    let bad = TransactionOptions {
        isolation_level: Some("unsupported fixture level".into()),
        read_only: None,
        defers_commit: false,
    };
    assert!(matches!(
        provider.begin_transaction(&fixed, &bad).await,
        Err(ResourceError::OperationNotSupported { .. })
    ));
    let opts = TransactionOptions {
        isolation_level: Some("REPEATABLE READ".into()),
        read_only: Some(false),
        defers_commit: false,
    };
    provider
        .begin_transaction(&fixed, &opts)
        .await
        .expect("actual MySQL read/write transaction");
    assert!(matches!(
        provider.begin_transaction(&fixed, &opts).await,
        Err(ResourceError::InvalidResourceState { .. })
    ));
    assert_eq!(
        provider
            .change_context(&fixed, &requested)
            .await
            .expect("no context change while transaction open"),
        ContextChangeDisposition::RequiresReplacement
    );
    let tx = provider
        .observe_session(&fixed)
        .await
        .expect("actual transaction observation");
    assert_eq!(tx.transaction.state, TransactionState::Active);
    let named = tx
        .handles
        .first()
        .expect("registered transaction handle")
        .clone();
    assert!(matches!(
        provider.close_handle(&leased, &named).await,
        Err(ResourceError::InvalidResourceState { .. })
    ));

    let fdir = tempfile::tempdir().unwrap();
    let p = fdir.path().join("wire.ndjson");
    let artifact = Artifact(Mutex::new(std::fs::File::create(&p).unwrap()));
    let cmd = |tag: &str| QueryExecutionId::new(tag);
    let report = provider
        .execute_on_resource(
            &fixed,
            &cmd("write-rollback"),
            &CommandCall::new(
                "execute",
                json!({"sql":format!("INSERT INTO {table} VALUES (2)")}),
            ),
            &artifact,
        )
        .await
        .expect("route write over the actual retained session");
    assert!(report.completion_status.is_success());
    let external = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_scalar::<_, i32>(&format!("SELECT id FROM {foreign}.{table} ORDER BY id"))
            .fetch_all(oracle)
            .await
            .expect("independent native visibility")
    };
    assert_eq!(external(&mut f.oracle).await, [1]);
    let query = driver
        .query(&connection, &format!("SELECT id FROM {table} ORDER BY id"))
        .await
        .expect("read in same native transaction");
    assert_eq!(
        query.rows,
        [vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]]
    );
    provider
        .close_handle(&fixed, &named)
        .await
        .expect("physically roll back named transaction");
    assert_eq!(external(&mut f.oracle).await, [1]);
    assert!(matches!(
        provider.close_handle(&fixed, &named).await,
        Err(ResourceError::InvalidResourceState { .. })
    ));

    provider
        .begin_transaction(&fixed, &opts)
        .await
        .expect("open another physical transaction");
    driver
        .execute(&connection, &format!("INSERT INTO {table} VALUES(3)"))
        .await
        .expect("stage committed real write");
    provider
        .commit_transaction(&fixed)
        .await
        .expect("commit tracked transaction");
    assert_eq!(external(&mut f.oracle).await, [1, 3]);
    assert!(std::fs::read_to_string(&p)
        .expect("read written artifact")
        .contains("statementSummary"));

    let read_only = TransactionOptions {
        isolation_level: None,
        read_only: Some(true),
        defers_commit: false,
    };
    provider
        .begin_transaction(&fixed, &read_only)
        .await
        .expect("real read-only snapshot");
    let before = driver
        .query(&connection, &format!("SELECT id FROM {table} ORDER BY id"))
        .await
        .expect("establish snapshot");
    sqlx::query(&format!("INSERT INTO {foreign}.{table} VALUES(4)"))
        .execute(&mut f.oracle)
        .await
        .expect("independently commit after first snapshot read");
    let during = driver
        .query(&connection, &format!("SELECT id FROM {table} ORDER BY id"))
        .await
        .expect("read stable snapshot");
    assert_eq!(during.rows, before.rows);
    assert!(driver
        .execute(&connection, &format!("INSERT INTO {table} VALUES(5)"))
        .await
        .is_err());
    provider
        .rollback_transaction(&fixed)
        .await
        .expect("end snapshot with real rollback");
    assert_eq!(external(&mut f.oracle).await, [1, 3, 4]);
    assert_eq!(
        provider
            .close_resource(&fixed)
            .await
            .expect("close physical retained driver"),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider
            .close_resource(&fixed)
            .await
            .expect("idempotent closed resource"),
        CloseDisposition::Closed
    );
    provider
        .close_handle(&fixed, &named)
        .await
        .expect("closed same-owner transaction is idempotent");
    provider
        .close_resource(&leased)
        .await
        .expect("close real leased resource");
    assert!(
        ledger.0.lock().unwrap().is_empty(),
        "all admitted connection budgets must settle exactly once"
    );
}

#[tokio::test]
async fn mysql_resource_stream_persists_batch_metadata_and_recovers_after_quota_error() {
    let f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|p| p.driver_id() == "mysql")
        .unwrap();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let driver = factory.create();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let fixed = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: f.cfg.clone(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database(f.database.clone()),
                baseline: Baseline::default(),
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
                    max_physical_connections: 8,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
            },
            &budget,
        )
        .await
        .expect("actual retained result checkout");
    let connection = provider
        .connection_handle(&fixed)
        .expect("actual connection");
    let before = driver
        .query(&connection, "SELECT CONNECTION_ID()")
        .await
        .expect("exact MySQL backend")
        .rows;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("result.ndjson");
    let sink = Artifact(Mutex::new(std::fs::File::create(&path).unwrap()));
    let id = |label| QueryExecutionId::new(label);
    let prepared = CommandCall::new(
        "query",
        json!({
            "sql":"WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<270) SELECT n,REPEAT('q', 800) AS payload FROM seq WHERE n>=? ORDER BY n",
            "boundParameters":[Value::Integer(2)],"limit":260
        }),
    );
    let completion = provider
        .execute_on_resource(&fixed, &id("native-batches"), &prepared, &sink)
        .await
        .expect("persist real bounded rows");
    assert!(completion.completion_status.is_success());
    let read = || {
        std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Json>(line).unwrap())
            .collect::<Vec<_>>()
    };
    let saved = read();
    let rows = saved
        .iter()
        .filter(|d| d["kind"] == "rows")
        .flat_map(|d| d["rows"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 260);
    assert_eq!(rows[0][0], serde_json::to_value(Value::Integer(2)).unwrap());
    assert_eq!(
        rows[259][0],
        serde_json::to_value(Value::Integer(261)).unwrap()
    );
    assert!(saved
        .iter()
        .any(|d| d["kind"] == "statementMetadata" && d["truncated"] == true));
    let empty = CommandCall::new(
        "query",
        json!({"sql":"SELECT CAST(? AS SIGNED) AS typed WHERE FALSE","boundParameters":[Value::Integer(41)]}),
    );
    provider
        .execute_on_resource(&fixed, &id("empty-native-bound"), &empty, &sink)
        .await
        .expect("native empty bound result");
    assert!(read()
        .iter()
        .any(|d| d["kind"] == "statementMetadata" && d["columns"][0]["name"] == "typed"));
    let malformed = CommandCall::new(
        "query",
        json!({"sql":"SELECT ?; SELECT 2","boundParameters":[Value::Integer(1)]}),
    );
    assert!(provider
        .execute_on_resource(&fixed, &id("ambiguous-native-sql"), &malformed, &sink)
        .await
        .is_err());
    let too_wide = CommandCall::new(
        "query",
        json!({"sql":"SELECT REPEAT('a', 1200000) AS over_limit"}),
    );
    let err = provider
        .execute_on_resource(&fixed, &id("native-row-quota"), &too_wide, &sink)
        .await
        .expect_err("no unbounded single-row artifact");
    assert!(matches!(err,ResourceError::SinkRejected {reason} if reason.contains("batch limit")));
    assert_eq!(
        driver
            .query(&connection, "SELECT CONNECTION_ID()")
            .await
            .expect("driver proves wire drain and never reconnects unobserved")
            .rows,
        before
    );
    assert_eq!(
        driver
            .query(&connection, "SELECT 17 AS after_quota")
            .await
            .unwrap()
            .rows,
        [vec![Some(Value::Integer(17))]]
    );
    assert_eq!(
        provider.close_resource(&fixed).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}
