use super::Fixture;
use datazen_driver_api::{
    DatabaseDriver, QueryExecutionId, Value,
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
};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub(super) struct Ledger(pub(super) Mutex<BTreeMap<String, u32>>);
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
    assert!(
        std::fs::read_to_string(&p)
            .expect("read written artifact")
            .contains("statementSummary")
    );

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
    assert!(
        driver
            .execute(&connection, &format!("INSERT INTO {table} VALUES(5)"))
            .await
            .is_err()
    );
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
    assert!(
        saved
            .iter()
            .any(|d| d["kind"] == "statementMetadata" && d["truncated"] == true)
    );
    let empty = CommandCall::new(
        "query",
        json!({"sql":"SELECT CAST(? AS SIGNED) AS typed WHERE FALSE","boundParameters":[Value::Integer(41)]}),
    );
    provider
        .execute_on_resource(&fixed, &id("empty-native-bound"), &empty, &sink)
        .await
        .expect("native empty bound result");
    assert!(
        read()
            .iter()
            .any(|d| d["kind"] == "statementMetadata" && d["columns"][0]["name"] == "typed")
    );
    let malformed = CommandCall::new(
        "query",
        json!({"sql":"SELECT ?; SELECT 2","boundParameters":[Value::Integer(1)]}),
    );
    assert!(
        provider
            .execute_on_resource(&fixed, &id("ambiguous-native-sql"), &malformed, &sink)
            .await
            .is_err()
    );
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

#[tokio::test]
async fn mysql_backup_snapshot_refuses_to_commit_user_transaction_and_persists_a_real_artifact() {
    use datazen_driver_api::{BackupDumpOptions, DriverError};
    let mut f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|p| p.driver_id() == "mysql")
        .unwrap();
    let driver = factory.create();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
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
        .expect("actual retained MySQL resource");
    let handle = provider
        .connection_handle(&fixed)
        .expect("actual driver handle");
    let table = format!("dz_backup_user_work_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .expect("independent created table");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("backup.ndjson");
    let artifact = Artifact(Mutex::new(std::fs::File::create(&path).unwrap()));
    let opts = BackupDumpOptions {
        single_transaction: true,
        schema_only: true,
        create_database: true,
        ..Default::default()
    };
    let call = |options: &BackupDumpOptions| {
        CommandCall::new(
            "backup_dump",
            json!({"database":f.database,"options":options}),
        )
    };
    for (id, invalid) in [
        ("missing-db", json!({"options":opts})),
        (
            "invalid-options",
            json!({"database":f.database,"options":"invalid"}),
        ),
        (
            "conflicting-options",
            json!({"database":f.database,"options":BackupDumpOptions{schema_only:true,data_only:true,..Default::default()}}),
        ),
        (
            "wrong-catalog",
            json!({"database":std::env::var("TEST_MYSQL_DATABASE_B").unwrap(),"options":opts}),
        ),
    ] {
        assert!(matches!(
            provider
                .execute_on_resource(
                    &fixed,
                    &QueryExecutionId::new(id),
                    &CommandCall::new("backup_dump", invalid),
                    &artifact
                )
                .await,
            Err(ResourceError::Driver(DriverError::InvalidConfig(_)))
        ));
    }
    driver
        .execute(&handle, "START TRANSACTION")
        .await
        .expect("stage user SQL transaction not registered in provider");
    driver
        .execute(&handle, &format!("INSERT INTO {table} VALUES(17)"))
        .await
        .expect("uncommitted user edit");
    let nested = provider
        .execute_on_resource(
            &fixed,
            &QueryExecutionId::new("nested-backup"),
            &call(&opts),
            &artifact,
        )
        .await;
    let visible: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(
        visible, 0,
        "a backup must not implicitly commit this real user edit"
    );
    assert!(matches!(
        nested,
        Err(ResourceError::Driver(DriverError::TransactionError(_)))
    ));
    assert!(
        matches!(
            driver
                .dump_database_with_progress(&handle, &f.database, &opts, &mut |_| {})
                .await,
            Err(DriverError::TransactionError(_))
        ),
        "direct backup must reject the same live nesting"
    );
    assert_eq!(
        driver
            .query(&handle, &format!("SELECT id FROM {table}"))
            .await
            .expect("original transaction still usable")
            .rows,
        [vec![Some(Value::Integer(17))]]
    );
    driver
        .execute(&handle, "ROLLBACK")
        .await
        .expect("user retains control of original edit");
    let visible: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(visible, 0);
    provider
        .execute_on_resource(
            &fixed,
            &QueryExecutionId::new("successful-backup"),
            &call(&opts),
            &artifact,
        )
        .await
        .expect("snapshot runs once safely afterward");
    let saved = std::fs::read_to_string(&path).expect("actual persisted SQL artifact events");
    assert!(saved.contains("CREATE DATABASE IF NOT EXISTS") && saved.contains(&table));
    driver
        .execute(&handle, &format!("INSERT INTO {table} VALUES(23)"))
        .await
        .expect("snapshot no longer holds the session read only");
    assert_eq!(
        sqlx::query_scalar::<_, i32>(&format!("SELECT id FROM {table}"))
            .fetch_one(&mut f.oracle)
            .await
            .expect("actual durable new write"),
        23
    );
    assert_eq!(
        provider.close_resource(&fixed).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_leased_resource_routes_bound_work_to_native_scope_and_settles_read_snapshot() {
    use datazen_driver_api::resource::InitializationRequirement;
    let mut f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|p| p.driver_id() == "mysql")
        .unwrap();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let peer = std::env::var("TEST_MYSQL_DATABASE_B").unwrap();
    let table = format!("dz_lease_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE TABLE {table}(id INT PRIMARY KEY,val INT); INSERT INTO {table} VALUES(1,11);
      CREATE TABLE {peer}.{table}(id INT PRIMARY KEY,val INT); INSERT INTO {peer}.{table} VALUES(1,22);"))
      .execute(&mut f.oracle).await.unwrap();
    let target = NamespaceTarget::empty().with_database(peer.clone());
    let mut request = AcquireResourceRequest {
        connection_config: f.cfg.clone(),
        target: target.clone(),
        identity_scope: IdentityScope::default(),
        baseline: Baseline::new(vec![InitializationRequirement {
            id: "native_time_zone".into(),
            sql: "SET time_zone = '+01:00'".into(),
            mandatory: true,
            idempotent: true,
        }]),
        scope: ResourceScope {
            purpose: ResourcePurpose::TransferJob,
            max_physical_connections: 8,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
    };
    let shape = provider.namespace_shape();
    assert!(shape.canonicalize(&target).is_ok());
    let descriptor = provider
        .describe_resource(&DescribeResourceRequest {
            connection_config: f.cfg.clone(),
            target: target.clone(),
            identity_scope: IdentityScope::default(),
            purpose: ResourcePurpose::TransferJob,
        })
        .await
        .unwrap();
    assert_eq!(
        descriptor.session_continuity,
        datazen_driver_api::capabilities::SessionContinuity::Leased
    );
    assert!(
        matches!(provider.acquire_resource(&request,&budget).await,Err(ResourceError::OperationNotSupported{ref reason,..}) if reason.contains("baseline replay"))
    );
    assert!(ledger.0.lock().unwrap().is_empty());
    request.baseline = Baseline::default();
    let leased = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("leased native MySQL checkout");
    assert_eq!(ledger.0.lock().unwrap().len(), 1);
    let observed = provider.observe_session(&leased).await.unwrap();
    assert_eq!(
        observed.context.namespace.database.as_deref(),
        Some(peer.as_str())
    );
    assert_eq!(observed.context.confidence, ObservationConfidence::Partial);
    assert!(matches!(
        provider.commit_transaction(&leased).await,
        Err(ResourceError::InvalidResourceState { .. })
    ));
    assert!(matches!(
        provider.rollback_transaction(&leased).await,
        Err(ResourceError::InvalidResourceState { .. })
    ));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("leased.jsonl");
    let sink = Artifact(Mutex::new(std::fs::File::create(&path).unwrap()));
    let id = |label: &str| QueryExecutionId::new(format!("lease-{label}-{table}"));
    let bound = CommandCall::new(
        "query_stream",
        json!({"sql":format!("SELECT val FROM {table} WHERE id=?"),
       "boundParameters":[Value::Integer(1)],"limit":2}),
    );
    let inserted = CommandCall::new(
        "execute",
        json!({"sql":format!("INSERT INTO {table}(id,val) VALUES(?,?)"),
       "boundParameters":[Value::Integer(2),Value::Integer(37)]}),
    );
    let id_reused = id("preflight");
    for command in ["query", "execute"] {
        let malformed = CommandCall::new(
            command,
            json!({"sql":"SELECT ?","boundParameters":{"not":"a list"}}),
        );
        assert!(matches!(
            provider
                .execute_on_resource(&leased, &id_reused, &malformed, &sink)
                .await,
            Err(ResourceError::Driver(_))
        ));
    }
    let without_sql = CommandCall::new("query", json!({"sql":"   "}));
    assert!(matches!(
        provider
            .execute_on_resource(&leased, &id_reused, &without_sql, &sink)
            .await,
        Err(ResourceError::OperationNotSupported { .. })
    ));
    let backup = CommandCall::new("backup_dump", json!({"database":peer}));
    assert!(matches!(
        provider
            .execute_on_resource(&leased, &id_reused, &backup, &sink)
            .await,
        Err(ResourceError::OperationNotSupported { .. })
    ));
    assert!(
        provider
            .execute_on_resource(&leased, &id_reused, &inserted, &sink)
            .await
            .unwrap()
            .completion_status
            .is_success()
    );
    let options = TransactionOptions {
        read_only: Some(true),
        ..Default::default()
    };
    let snapshot = provider
        .begin_transaction(&leased, &options)
        .await
        .expect("actual native read-only snapshot");
    assert_eq!(snapshot.state, TransactionState::Active);
    let before = provider
        .execute_on_resource(&leased, &id("before"), &bound, &sink)
        .await
        .unwrap();
    assert!(before.completion_status.is_success());
    sqlx::query(&format!("UPDATE {peer}.{table} SET val=71 WHERE id=1"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let after = provider
        .execute_on_resource(&leased, &id("after"), &bound, &sink)
        .await
        .unwrap();
    assert!(after.completion_status.is_success());
    let read_only = provider
        .execute_on_resource(&leased, &id("read-only"), &inserted, &sink)
        .await
        .unwrap_err();
    assert!(
        matches!(read_only,ResourceError::Driver(ref error) if error.to_string().contains("READ ONLY")||error.to_string().contains("read-only"))
    );
    assert_eq!(
        provider.commit_transaction(&leased).await.unwrap().state,
        TransactionState::Idle
    );
    provider
        .execute_on_resource(&leased, &id("settled"), &bound, &sink)
        .await
        .unwrap();
    let actual = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Json>(l).unwrap())
        .filter(|event| event["kind"] == "rows")
        .collect::<Vec<_>>();
    assert_eq!(
        actual
            .iter()
            .map(|e| e["rows"][0][0].clone())
            .collect::<Vec<_>>(),
        [22, 22, 71].map(|v| serde_json::to_value(Value::Integer(v)).unwrap())
    );
    let native: (i32, i32, i64) = sqlx::query_as(&format!(
        "SELECT (SELECT val FROM {table} WHERE id=1),(SELECT val FROM {peer}.{table} WHERE id=1),
        (SELECT COUNT(*) FROM {peer}.{table} WHERE id=2 AND val=37)"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .unwrap();
    assert_eq!(native, (11, 71, 1));
    assert_eq!(
        provider.close_resource(&leased).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(provider.observe_session(&leased).await.is_err());
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_native_checkout_timeout_does_not_run_waiting_commands_or_poison_sole_backend() {
    use std::{fmt::Debug, time::Duration};
    let mut f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|p| p.driver_id() == "mysql")
        .unwrap();
    let driver = factory.create();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("dz_wait_{nonce}");
    let lock = format!("dz_limit_timeout_{nonce}");
    let write = format!("INSERT INTO {table} VALUES (6)");
    let bound_write = format!("INSERT INTO {table} VALUES (?)");
    sqlx::query(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let reserved: i64 = sqlx::query_scalar("SELECT GET_LOCK(?,0)")
        .bind(&lock)
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(reserved, 1);
    let mut cfg = f.cfg.clone();
    cfg.connection_timeout = 1;
    cfg.max_pool_size = 1;
    let leased = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: cfg,
                target: NamespaceTarget::empty().with_database(f.database.clone()),
                scope: ResourceScope {
                    purpose: ResourcePurpose::TransferJob,
                    max_physical_connections: 1,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
                identity_scope: IdentityScope::default(),
                baseline: Baseline::default(),
            },
            &budget,
        )
        .await
        .unwrap();
    let h = provider.connection_handle(&leased).unwrap();
    let worker = tokio::spawn({
        let (driver, h, lock) = (driver.clone(), h.clone(), lock.clone());
        async move {
            driver
                .query(&h, &format!("SELECT GET_LOCK('{lock}',10)"))
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(4),async {
       loop {
          let waiting:i64=sqlx::query_scalar(
             "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE STATE='User lock' AND ID<>CONNECTION_ID() AND INFO LIKE ?")
            .bind(format!("%{lock}%")).fetch_one(&mut f.oracle).await.unwrap();
          if waiting==1 {break;}
          tokio::time::sleep(Duration::from_millis(20)).await;
       }
    }).await.expect("the only native pool slot is holding an observed real lock wait");
    let id = QueryExecutionId::new(format!("registered-wait-{nonce}"));
    driver.prepare_query_execution(&h, &id).await.unwrap();
    let artifact = tempfile::tempdir().unwrap();
    let sink = Artifact(Mutex::new(
        std::fs::File::create(artifact.path().join("never.jsonl")).unwrap(),
    ));
    let resource_id = QueryExecutionId::new(format!("leased-wait-{nonce}"));
    let query = CommandCall::new("query", json!({"sql":"SELECT 1"}));
    let parameters = [Value::Integer(6)];
    fn timeout<T: Debug, E: Debug>(result: Result<T, E>) {
        let error = format!(
            "{:?}",
            result.expect_err("sole native connection is still held")
        );
        assert!(
            error.contains("timed out") || error.contains("PoolTimedOut"),
            "{error}"
        );
    }
    let all = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            driver.query(&h, "SELECT 1"),
            driver.query_multi(&h, &write, None),
            driver.query_stream(&h, &write, None, Arc::new(|_| {})),
            driver.query_stream_with_execution(&h, &id, &write, None, Arc::new(|_| {})),
            driver.query_with_params(&h, "SELECT ?", &parameters),
            driver.execute(&h, &write),
            driver.execute_with_params(&h, &bound_write, &parameters),
            driver.begin_transaction(&h),
            driver.begin_read_snapshot(&h),
            driver.get_table_schema(&h, &table, &f.database, None),
            driver.get_databases(&h),
            driver.get_server_info(&h),
            provider.execute_on_resource(&leased, &resource_id, &query, &sink),
        )
    })
    .await;
    let released: i64 = sqlx::query_scalar("SELECT RELEASE_LOCK(?)")
        .bind(&lock)
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(released, 1);
    let (r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, r11, r12, r13) =
        all.expect("each lease attempt is bounded");
    timeout(r1);
    timeout(r2);
    timeout(r3);
    timeout(r4);
    timeout(r5);
    timeout(r6);
    timeout(r7);
    timeout(r8);
    timeout(r9);
    timeout(r10);
    timeout(r11);
    timeout(r12);
    timeout(r13);
    assert_eq!(
        worker.await.unwrap().unwrap().rows,
        [vec![Some(Value::Integer(1))]]
    );
    assert!(
        std::fs::read(artifact.path().join("never.jsonl"))
            .unwrap()
            .is_empty()
    );
    let persisted: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(persisted, 0);
    assert_eq!(
        driver.execute(&h, &write).await.unwrap(),
        1,
        "the original pool is reusable once native owner finishes"
    );
    let tx = driver.begin_transaction(&h).await.unwrap();
    let transactional = QueryExecutionId::new(format!("transaction-gone-{nonce}"));
    driver
        .prepare_query_execution(&h, &transactional)
        .await
        .unwrap();
    let pending = format!("INSERT INTO {table} VALUES(77)");
    driver.execute(&h, &pending).await.unwrap();
    let before: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id=77"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(before, 0);
    driver.rollback(tx).await.unwrap();
    assert!(
        matches!(driver.query_stream_with_execution(&h,&transactional,&pending,None,Arc::new(|_|{})).await,
       Err(datazen_driver_api::DriverError::Unsupported(ref message)) if message.contains("transaction execution connection"))
    );
    let after: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id=77"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(
        after, 0,
        "a queued run cannot fall through to autocommit after its transaction closed"
    );
    assert_eq!(
        provider.close_resource(&leased).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}
