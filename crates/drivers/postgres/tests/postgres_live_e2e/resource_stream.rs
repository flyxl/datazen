use super::Fixture;
use datazen_driver_api::{
    async_trait,
    resource::{ResourceError, ResultChunk, ResultSink},
    ConnectionHandle, DatabaseDriver, QueryExecutionId, StatementResult, Value,
};
use datazen_driver_postgres::PostgresDriver;
use serde_json::{json, Value as Json};
use std::io::Write;
use std::sync::Mutex;

struct Artifact {
    file: Mutex<(std::fs::File, usize)>,
    path: std::path::PathBuf,
    budget: usize,
}

impl Artifact {
    fn new(directory: &tempfile::TempDir, name: &str, budget: usize) -> Self {
        let path = directory.path().join(name);
        let file = std::fs::File::create(&path).expect("isolated output artifact");
        Self {
            file: Mutex::new((file, 0)),
            path,
            budget,
        }
    }

    fn append(&self, output: &Json) -> Result<(), ResourceError> {
        let mut bytes = serde_json::to_vec(output).expect("verifiable JSON record");
        bytes.push(b'\n');
        let mut writer = self.file.lock().expect("artifact lock");
        if writer.1 + bytes.len() > self.budget {
            return Err(ResourceError::SinkRejected {
                reason: "output artifact quota exceeded".into(),
            });
        }
        writer
            .0
            .write_all(&bytes)
            .map_err(|e| ResourceError::SinkRejected {
                reason: format!("could not persist output artifact: {e}"),
            })?;
        writer.1 += bytes.len();
        Ok(())
    }

    fn records(&self) -> Vec<Json> {
        std::fs::read_to_string(&self.path)
            .expect("read exact persisted artifact")
            .lines()
            .map(|s| serde_json::from_str(s).expect("valid NDJSON"))
            .collect()
    }
}

#[async_trait]
impl ResultSink for Artifact {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.append(
            &json!({"kind":"rows", "statementIndex":chunk.statement_index,"rows":chunk.rows}),
        )
    }

    async fn write_output(&self, output: Json) -> Result<(), ResourceError> {
        self.append(&output)
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.append(&json!({"kind":"complete"}))
    }

    async fn fail(&self, reason: &str) -> Result<(), ResourceError> {
        self.append(&json!({"kind":"failed","reason":reason}))
    }
}

async fn stream(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    sql: &str,
    limit: Option<u32>,
    parameters: Option<&[Value]>,
    sink: &Artifact,
) -> Result<Vec<StatementResult>, ResourceError> {
    let id = QueryExecutionId::new(uuid::Uuid::new_v4().to_string());
    driver
        .prepare_query_execution(handle, &id)
        .await
        .expect("real execution registered");
    let result = driver
        .query_to_sink(handle, &id, sql, limit, parameters, sink)
        .await;
    driver
        .cleanup_query_execution(handle, &id)
        .await
        .expect("clean live registration");
    result
}

#[tokio::test]
async fn live_resource_stream_writes_bounded_artifacts_and_preserves_retained_session() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let pooled = driver
        .connect(&fixture.config)
        .await
        .expect("pooled PostgreSQL handle");
    let retained = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("fixed PostgreSQL handle");
    let artifacts = tempfile::tempdir().expect("disposable artifacts");
    let success = Artifact::new(&artifacts, "success.ndjson", 8 * 1024 * 1024);
    let result = stream(&driver,&pooled,
        "SELECT n AS id FROM generate_series(1,270) n; SELECT n, repeat('x', 400000) AS chunk FROM generate_series(1,4) n; SELECT NULL::integer AS empty_value WHERE FALSE",
        Some(269), None, &success,
    ).await.expect("real wire rows saved to artifact");
    assert_eq!(result.len(), 3);
    assert!(result[0].truncated && !result[1].truncated && !result[2].truncated);
    assert!(
        result.iter().all(|statement| statement.rows.is_empty()),
        "driver must not retain a second row copy"
    );
    assert_eq!(result[2].columns[0].name, "empty_value");
    let records = success.records();
    let lengths = |statement| {
        records
            .iter()
            .filter(|r| r["kind"] == "rows" && r["statementIndex"] == statement)
            .map(|r| r["rows"].as_array().expect("array").len())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        lengths(0),
        [1, 256, 12],
        "ordinary rows observe fixed batch limit"
    );
    assert_eq!(lengths(1), [1, 2, 1], "large rows observe byte budget");
    assert_eq!(lengths(2), Vec::<usize>::new());
    let ids: Vec<i32> = sqlx::query_scalar("SELECT n FROM generate_series(1,269) n")
        .fetch_all(&mut fixture.oracle)
        .await
        .expect("independent SQL row oracle");
    let seen: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "rows" && r["statementIndex"] == 0)
        .flat_map(|r| r["rows"].as_array().expect("rows"))
        .map(|row| {
            let cells: Vec<Option<Value>> =
                serde_json::from_value(row.clone()).expect("exact recorded row cells");
            match cells.as_slice() {
                [Some(Value::Integer(id))] => i32::try_from(*id).expect("value in bounded range"),
                _ => panic!("unexpected recorded typed row"),
            }
        })
        .collect();
    assert_eq!(seen, ids);
    let summaries: Vec<_> = records
        .iter()
        .filter(|row| row["kind"] == "statementSummary")
        .map(|r| r["rowsAffected"].as_u64())
        .collect();
    assert_eq!(summaries, [Some(270), Some(4), Some(0)]);
    let bytes: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "rows" && r["statementIndex"] == 1)
        .flat_map(|r| r["rows"].as_array().expect("array"))
        .map(|row| {
            let cells: Vec<Option<Value>> =
                serde_json::from_value(row.clone()).expect("large artifact row");
            match &cells[1] {
                Some(Value::String(text)) => text.len(),
                _ => panic!("expected real PostgreSQL text value"),
            }
        })
        .collect();
    assert_eq!(bytes, [400000; 4]);

    driver
        .execute(
            &retained,
            "CREATE TEMP TABLE stream_fixture (a boolean,p bytea,j jsonb)",
        )
        .await
        .expect("real retained-session temp table");
    let pid = driver
        .query(&retained, "SELECT pg_backend_pid()")
        .await
        .expect("original backend")
        .rows;
    let parameters = [
        Value::Bool(true),
        Value::Bytes(vec![0, 255, 39]),
        Value::Json(json!({"label":"it's 雪"})),
    ];
    let writes = Artifact::new(&artifacts, "bound.ndjson", 16 * 1024);
    let actual = stream(
        &driver,
        &retained,
        "INSERT INTO stream_fixture VALUES ($1,$2,$3)",
        None,
        Some(&parameters),
        &writes,
    )
    .await
    .expect("bound execution on pinned backend");
    assert_eq!(actual[0].rows_affected, Some(1));
    assert_eq!(
        driver
            .query(&retained, "SELECT a,p,j FROM stream_fixture")
            .await
            .expect("read retained temp row")
            .rows,
        vec![parameters.clone().into_iter().map(Some).collect::<Vec<_>>()]
    );
    let empty = Artifact::new(&artifacts, "typed-empty.ndjson", 4096);
    let typed = stream(
        &driver,
        &retained,
        "SELECT $1 AS active, $2 AS bytes, $3 AS doc WHERE false",
        None,
        Some(&parameters),
        &empty,
    )
    .await
    .expect("same prepared types even with no live rows");
    assert_eq!(
        typed[0]
            .columns
            .iter()
            .map(|column| column.data_type.as_str())
            .collect::<Vec<_>>(),
        ["BOOL", "BYTEA", "JSONB"]
    );
    assert!(typed[0].rows.is_empty());
    let metadata = empty
        .records()
        .into_iter()
        .find(|row| row["kind"] == "statementMetadata")
        .expect("persisted blank output metadata");
    assert_eq!(metadata["columns"][1]["dataType"], "BYTEA");
    let no_multi = Artifact::new(&artifacts, "multi-denied.ndjson", 1024);
    assert!(stream(
        &driver,
        &retained,
        "SELECT $1::bool; INSERT INTO stream_fixture VALUES (FALSE,NULL,NULL)",
        None,
        Some(&parameters[..1]),
        &no_multi
    )
    .await
    .is_err());
    assert!(no_multi.records().is_empty());

    let rejected = Artifact::new(&artifacts, "rejected.ndjson", 0);
    assert!(
        matches!(stream(&driver,&retained,"SELECT 1 AS quota_exhausted",None,None,&rejected).await,
        Err(ResourceError::SinkRejected{reason}) if reason=="output artifact quota exceeded")
    );
    assert!(rejected.records().is_empty());
    let huge = Artifact::new(&artifacts, "oversized.ndjson", 4 * 1024 * 1024);
    assert!(
        matches!(stream(&driver,&retained,"SELECT repeat('x',1048580) AS oversized",None,None,&huge).await,
        Err(ResourceError::SinkRejected{reason}) if reason.contains("exceeds the 1 MiB"))
    );
    assert!(!huge
        .records()
        .iter()
        .any(|record| record["kind"] == "rows" || record["kind"] == "statementSummary"));

    // Actual file quotas at different phases: a first row may not fit,
    // or earlier batches can be durable while a later byte-flush is refused.
    // No failed artifact may masquerade as a successful statement summary.
    for (name, quota, expected_batches, sql) in [
        (
            "first-row-quota.ndjson",
            4096,
            Vec::<usize>::new(),
            "SELECT repeat('x',20000) AS payload",
        ),
        (
            "later-batch-quota.ndjson",
            650_000,
            vec![1],
            "SELECT n,repeat('x',400000) AS payload FROM generate_series(1,4) n",
        ),
    ] {
        let constrained = Artifact::new(&artifacts, name, quota);
        assert!(
            matches!(stream(&driver,&retained,sql,None,None,&constrained).await,
            Err(ResourceError::SinkRejected{reason}) if reason == "output artifact quota exceeded")
        );
        let stored = constrained.records();
        assert!(stored
            .iter()
            .any(|record| record["kind"] == "statementColumns"));
        assert!(!stored
            .iter()
            .any(|record| record["kind"] == "statementSummary"));
        let saved = stored
            .iter()
            .filter(|row| row["kind"] == "rows")
            .map(|row| {
                row["rows"]
                    .as_array()
                    .expect("persisted partial artifact")
                    .len()
            })
            .collect::<Vec<_>>();
        assert_eq!(saved, expected_batches);
    }

    let bad = Artifact::new(&artifacts, "bad.ndjson", 4096);
    assert!(stream(
        &driver,
        &retained,
        "SELECT NULL::not_a_real_fixture_type",
        None,
        None,
        &bad
    )
    .await
    .is_err());
    assert!(bad.records().is_empty());
    assert_eq!(
        driver
            .query(&retained, "SELECT pg_backend_pid()")
            .await
            .expect("protocol post-drain is usable")
            .rows,
        pid
    );
    assert_eq!(
        driver
            .query(&retained, "SELECT count(*) FROM stream_fixture")
            .await
            .expect("retained temp data remains")
            .rows,
        vec![vec![Some(Value::Integer(1))]]
    );
    let leaked: Option<String> = sqlx::query_scalar("SELECT to_regclass('stream_fixture')::text")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("separate oracle");
    assert!(leaked.is_none());
    driver.disconnect(retained).await.expect("fixed cleanup");
    driver.disconnect(pooled).await.expect("pool cleanup");
}

#[tokio::test]
async fn losing_a_real_postgres_backend_midstream_tombstones_the_retained_checkout() {
    let mut fixture = Fixture::open().await;
    let driver = std::sync::Arc::new(PostgresDriver::new());
    let retained = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("retained streaming checkout");
    let pid = match driver
        .query(&retained, "SELECT pg_backend_pid()")
        .await
        .expect("live original backend")
        .rows[0][0]
    {
        Some(Value::Integer(pid)) => i32::try_from(pid).expect("actual postgres PID"),
        _ => panic!("unexpected postgres pid representation"),
    };
    let store = tempfile::tempdir().expect("disposable stream artifact");
    let artifact = std::sync::Arc::new(Artifact::new(&store, "connection-lost.ndjson", 16 * 1024));
    let worker = {
        let driver = driver.clone();
        let retained = retained.clone();
        let artifact = artifact.clone();
        tokio::spawn(async move {
            stream(
                &driver,
                &retained,
                "SELECT 21 AS may_never_arrive FROM pg_sleep(30)",
                None,
                None,
                &artifact,
            )
            .await
        })
    };
    use std::time::Duration;
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event='PgSleep')")
                .bind(pid).fetch_one(&mut fixture.oracle).await.expect("independent live backend witness");
            if active {break}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("real retained stream is executing");
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
            .bind(pid)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("dispose only the isolated live backend")
    );
    let failure = tokio::time::timeout(Duration::from_secs(7), worker)
        .await
        .expect("finite protocol drain")
        .expect("retained worker joined")
        .expect_err("lost retained session cannot report successful output");
    assert!(
        matches!(failure,ResourceError::Driver(datazen_driver_api::DriverError::ConnectionFailed(reason)) if reason.contains("SessionLost"))
    );
    assert!(!artifact
        .records()
        .iter()
        .any(|record| record["kind"] == "statementSummary"));
    assert!(driver.observe_fixed_session(&retained).await.is_err());
    let later = Artifact::new(&store, "later-rejected.ndjson", 4096);
    assert!(stream(
        &driver,
        &retained,
        "SELECT 1 AS must_not_be_reconnected",
        None,
        None,
        &later
    )
    .await
    .is_err());
    assert!(later.records().is_empty());
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1)"
    )
    .bind(pid)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("exact original backend is gone"));
    driver
        .disconnect(retained)
        .await
        .expect("resource can still be cleaned");
}

#[tokio::test]
async fn postgres_refuses_to_reuse_a_stream_when_server_policy_blocks_cancellation_and_drain() {
    let mut fixture = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("dz_drain_role_{tag}");
    let db = format!("dz_fixture_drain_{tag}");
    let secret = format!("fixture-drain-{tag}");
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD '{secret}'"))
        .execute(&mut fixture.oracle)
        .await
        .expect("actual scoped business role");
    sqlx::query(&format!("CREATE DATABASE {db}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("isolated database policy fixture");
    let mut oracle = fixture.connect_to(&db).await;
    // Scope this simulated production policy to the new disposable DB.
    sqlx::query("REVOKE EXECUTE ON FUNCTION pg_catalog.pg_cancel_backend(integer) FROM PUBLIC")
        .execute(&mut oracle)
        .await
        .expect("actual server denies role's cancellation primitive");
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT has_function_privilege($1,'pg_catalog.pg_cancel_backend(integer)','EXECUTE')"
    )
    .bind(&role)
    .fetch_one(&mut oracle)
    .await
    .expect("independent ACL witness"));
    let driver = PostgresDriver::new();
    let mut target = fixture.config.clone();
    target.database = Some(db.clone());
    target.username = Some(role.clone());
    target.password = Some(secret);
    let retained = driver
        .connect_fixed_session(&target)
        .await
        .expect("data and control run as actual restricted role");
    driver
        .execute(&retained, "SET statement_timeout = '25s'")
        .await
        .expect("bound maximum lifetime of fixture query");
    let pid = match driver
        .query(&retained, "SELECT pg_backend_pid()")
        .await
        .expect("actual original checkout")
        .rows[0][0]
    {
        Some(Value::Integer(pid)) => i32::try_from(pid).expect("PostgreSQL PID"),
        _ => panic!("unexpected live PID"),
    };
    let artifacts = tempfile::tempdir().expect("disposable result output");
    let refused = Artifact::new(&artifacts, "quota-denied.ndjson", 0);
    // The first row is larger than the server's protocol buffer and reaches
    // the sink before the second row's real backend delay.
    let output=tokio::time::timeout(std::time::Duration::from_secs(15),stream(
        &driver,&retained,
        "SELECT repeat('x',100000) AS payload, CASE WHEN n=1 THEN NULL::text ELSE (SELECT NULL::text FROM pg_sleep(22) WHERE n>1) END AS delayed FROM generate_series(1,2) n",
        None,None,&refused,
    )).await.expect("protocol cleanup must remain bounded");
    assert!(
        matches!(output,Err(ResourceError::Driver(datazen_driver_api::DriverError::ConnectionFailed(reason))) if reason.contains("SessionLost")),
        "incomplete drain must never return a reusable checkout"
    );
    assert!(refused.records().is_empty());
    assert!(driver.observe_fixed_session(&retained).await.is_err());
    driver
        .disconnect(retained)
        .await
        .expect("control socket and retired checkout can close");
    // The hardened role cannot cancel; the independent fixture administrator
    // may stop its remaining isolated PostgreSQL backend during cleanup.
    sqlx::query("SELECT pg_terminate_backend($1) FROM pg_stat_activity WHERE pid=$1")
        .bind(pid)
        .execute(&mut oracle)
        .await
        .expect("remove only old poisoned fixture backend");
    drop(oracle);
    sqlx::query(&format!("DROP DATABASE {db}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("dispose database-specific policy");
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("dispose dedicated role");
}

#[tokio::test]
async fn postgres_resource_commands_reject_unbudgeted_targets_and_ambiguous_bound_input() {
    use datazen_driver_api::{
        namespace::NamespaceTarget,
        resource::{
            AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, IdentityScope,
            ResourceProvider, ResourcePurpose, ResourceScope,
        },
        session::CloseDisposition,
        DriverError,
    };
    use datazen_driver_postgres::PostgresResourceProvider;
    use std::sync::Arc;

    #[derive(Default)]
    struct Budget(Mutex<std::collections::HashSet<String>>);
    #[async_trait]
    impl BudgetPort for Budget {
        async fn acquire_physical_connections(
            &self,
            n: u32,
        ) -> Result<BudgetPermit, ResourceError> {
            let id = uuid::Uuid::new_v4().to_string();
            assert!(self
                .0
                .lock()
                .expect("physical admission ledger")
                .insert(id.clone()));
            Ok(BudgetPermit {
                permit_id: id,
                physical_connections: n,
            })
        }
        async fn release_physical_connections(
            &self,
            permit: &BudgetPermit,
        ) -> Result<(), ResourceError> {
            assert!(self
                .0
                .lock()
                .expect("physical admission ledger")
                .remove(&permit.permit_id));
            Ok(())
        }
    }

    let mut fixture = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let local = fixture.config.database.as_deref().expect("marked source");
    let foreign = format!("dz_fixture_other_scope_{tag}");
    let name = format!("dz_scoped_rows_{tag}");
    let relation = format!("public.{name}");
    let forbidden_schema = format!("dz_unbudgeted_{tag}");
    sqlx::query(&format!("CREATE DATABASE {foreign}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("independent actual foreign catalog");
    let mut secondary = fixture.connect_to(&foreign).await;
    for (oracle, number) in [(&mut fixture.oracle, 11), (&mut secondary, 22)] {
        sqlx::raw_sql(&format!(
            "CREATE TABLE {relation}(n integer); INSERT INTO {relation} VALUES ({number})"
        ))
        .execute(oracle)
        .await
        .expect("disposable same-name relation in each database");
    }
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver);
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = |purpose| AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: NamespaceTarget::empty().with_database(local.to_owned()),
        scope: ResourceScope {
            purpose,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let leased = provider
        .acquire_resource(&request(ResourcePurpose::MetadataInspection), &budget)
        .await
        .expect("real leased catalog resource");
    let fixed = provider
        .acquire_resource(&request(ResourcePurpose::InteractiveQuery), &budget)
        .await
        .expect("real retained interactive resource");
    let files = tempfile::tempdir().expect("temporary real result sink files");
    let values = Artifact::new(&files, "leased-catalog.ndjson", 1024 * 1024);
    let listed = provider
        .execute_on_resource(
            &leased,
            &QueryExecutionId::new("actual-list-tables"),
            &CommandCall::new("list_tables", json!({"database":local,"schema":"public"})),
            &values,
        )
        .await
        .expect("leased real catalog command works");
    assert!(listed.completion_status.is_success());
    let metadata = values.records();
    assert!(metadata
        .iter()
        .any(|record| record["kind"] == "commandResult"
            && record["data"]["tables"]
                .as_array()
                .is_some_and(|tables| tables.iter().any(|entry| entry["name"] == name))));

    let refused = Artifact::new(&files, "scoped-rejections.ndjson", 1024 * 1024);
    let command = CommandCall::new(
        "query",
        json!({"database":foreign,"sql":format!("DELETE FROM {relation}")}),
    );
    assert!(
        matches!(provider.execute_on_resource(&leased,&QueryExecutionId::new("separate-catalog"),&command,&refused).await,
        Err(ResourceError::OperationNotSupported{reason,..}) if reason.contains("separately budgeted"))
    );
    let malformed = CommandCall::new(
        "query",
        json!({
            "database":local, "sql":format!("DELETE FROM {relation}"), "boundParameters":{"not":"a parameter array"}
        }),
    );
    assert!(
        matches!(provider.execute_on_resource(&fixed,&QueryExecutionId::new("bad-parameter-shape"),&malformed,&refused).await,
        Err(ResourceError::Driver(DriverError::InvalidConfig(reason))) if reason.contains("invalid bound parameters"))
    );
    let backup = CommandCall::new(
        "backup_dump",
        json!({
            "database":local,"options":datazen_driver_api::BackupDumpOptions::default()
        }),
    );
    assert!(
        matches!(provider.execute_on_resource(&leased,&QueryExecutionId::new("no-leased-backup"),&backup,&refused).await,
        Err(ResourceError::OperationNotSupported{reason,..}) if reason.contains("retained session required"))
    );
    let scope = CommandCall::new(
        "create_schema",
        json!({"database":local,"name":forbidden_schema}),
    );
    assert!(
        matches!(provider.execute_on_resource(&fixed,&QueryExecutionId::new("no-retained-admin"),&scope,&refused).await,
        Err(ResourceError::OperationNotSupported{reason,..}) if reason.contains("catalog/admin command"))
    );
    assert!(!refused.records().iter().any(|record| matches!(
        record["kind"].as_str(),
        Some("rows" | "commandResult" | "statementSummary" | "complete")
    )));
    assert!(
        provider
            .observe_session(&fixed)
            .await
            .expect("retained original scope still healthy")
            .context
            .namespace
            .database
            .as_deref()
            == Some(local)
    );
    for (oracle, expected) in [(&mut fixture.oracle, 11), (&mut secondary, 22)] {
        assert_eq!(
            sqlx::query_scalar::<_, i32>(&format!("SELECT n FROM {relation}"))
                .fetch_one(oracle)
                .await
                .expect("independent no-change oracle"),
            expected
        );
    }
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT to_regnamespace($1) IS NULL")
            .bind(&forbidden_schema)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("no unreviewed retained admin action")
    );
    assert_eq!(
        provider
            .close_resource(&fixed)
            .await
            .expect("close actual retained resource"),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider
            .close_resource(&leased)
            .await
            .expect("close actual leased resource"),
        CloseDisposition::Closed
    );
    assert!(ledger
        .0
        .lock()
        .expect("resource budget released")
        .is_empty());
    drop(secondary);
    sqlx::query(&format!("DROP DATABASE {foreign}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("no automatic foreign sockets survive");
    sqlx::query(&format!("DROP TABLE {relation}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("scoped cleanup");
}
