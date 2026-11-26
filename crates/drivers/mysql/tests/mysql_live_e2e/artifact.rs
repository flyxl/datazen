use super::{Fixture, resource::Ledger};
use datazen_driver_api::{
    DatabaseDriver, DriverError, QueryExecutionId, Value,
    namespace::NamespaceTarget,
    resource::{
        AcquireResourceRequest, Baseline, BudgetPort, CommandCall, IdentityScope, ResourceError,
        ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
    },
    session::{CancelDisposition, CloseDisposition},
};
use serde_json::{Value as Json, json};
use std::{
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct FileQuota {
    file: Mutex<std::fs::File>,
    headers_allowed: bool,
    writes_allowed: usize,
    writes: AtomicUsize,
}
impl FileQuota {
    fn new(path: &std::path::Path, headers_allowed: bool, writes_allowed: usize) -> Self {
        Self {
            file: Mutex::new(std::fs::File::create(path).unwrap()),
            headers_allowed,
            writes_allowed,
            writes: AtomicUsize::new(0),
        }
    }
    fn full() -> ResourceError {
        ResourceError::SinkRejected {
            reason: "test artifact byte budget exhausted".into(),
        }
    }
    fn append(&self, item: Json) -> Result<(), ResourceError> {
        writeln!(self.file.lock().unwrap(), "{item}").map_err(|_| Self::full())
    }
}
#[async_trait::async_trait]
impl ResultSink for FileQuota {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        if self.writes.fetch_add(1, Ordering::Relaxed) >= self.writes_allowed {
            return Err(Self::full());
        }
        let encoded = serde_json::to_vec(&chunk.rows).unwrap();
        assert!(
            encoded.len() <= 1024 * 1024,
            "real provider must bound bytes independently of its sink"
        );
        self.append(json!({"kind":"rows","index":chunk.statement_index,"length":encoded.len(),"count":chunk.rows.len()}))
    }
    async fn write_output(&self, item: Json) -> Result<(), ResourceError> {
        if !self.headers_allowed && item["kind"] == "statementColumns" {
            return Err(Self::full());
        }
        self.append(item)
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        self.append(json!({"kind":"complete"}))
    }
    async fn fail(&self, reason: &str) -> Result<(), ResourceError> {
        self.append(json!({"kind":"failure","reason":reason}))
    }
}

#[tokio::test]
async fn mysql_real_artifact_rejections_drain_the_same_connection_at_each_delivery_boundary() {
    let f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|d| d.driver_id() == "mysql")
        .unwrap();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let driver = factory.create();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = |purpose| AcquireResourceRequest {
        connection_config: f.cfg.clone(),
        target: NamespaceTarget::empty().with_database(f.database.clone()),
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
        scope: ResourceScope {
            purpose,
            max_physical_connections: 8,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
    };
    let fixed = provider
        .acquire_resource(&request(ResourcePurpose::InteractiveQuery), &budget)
        .await
        .unwrap();
    let lease = provider
        .acquire_resource(&request(ResourcePurpose::MetadataInspection), &budget)
        .await
        .unwrap();
    let h = provider.connection_handle(&fixed).unwrap();
    let original = driver
        .query(&h, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows;
    let dir = tempfile::tempdir().unwrap();
    let large = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<4) SELECT n, REPEAT('w',430000) AS wide FROM seq";
    let tiny = "SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3";
    for (label, headers, writes, sql, expected_attempts) in [
        ("metadata", false, 9, large, 0),
        ("first-packet", true, 0, large, 1),
        ("byte-window", true, 1, large, 2),
        ("end-flush", true, 1, tiny, 2),
    ] {
        let path = dir.path().join(format!("{label}.ndjson"));
        let artifact = FileQuota::new(&path, headers, writes);
        let err = provider
            .execute_on_resource(
                &fixed,
                &QueryExecutionId::new(label),
                &CommandCall::new("query", json!({"sql":sql})),
                &artifact,
            )
            .await
            .expect_err("actual separate artifact rejection");
        assert!(
            matches!(err,ResourceError::SinkRejected{reason} if reason=="test artifact byte budget exhausted")
        );
        assert_eq!(artifact.writes.load(Ordering::Relaxed), expected_attempts);
        assert_eq!(
            driver
                .query(&h, "SELECT CONNECTION_ID()")
                .await
                .expect("surviving exact native protocol socket")
                .rows,
            original
        );
    }
    let path = dir.path().join("success.ndjson");
    let artifact = FileQuota::new(&path, true, 9);
    provider
        .execute_on_resource(
            &fixed,
            &QueryExecutionId::new("full-artifact"),
            &CommandCall::new("query", json!({"sql":large})),
            &artifact,
        )
        .await
        .expect("actual four wide rows from mysql");
    let results: Vec<Json> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let batches: Vec<_> = results
        .iter()
        .filter(|e| e["kind"] == "rows")
        .map(|e| e["count"].as_u64().unwrap())
        .collect();
    assert_eq!(batches, [1, 2, 1]);
    let admin = CommandCall::new("list_tables", json!({"database":f.database}));
    assert!(matches!(
        provider
            .execute_on_resource(
                &fixed,
                &QueryExecutionId::new("retained-refuses-admin"),
                &admin,
                &artifact
            )
            .await,
        Err(ResourceError::OperationNotSupported { .. })
    ));
    provider
        .execute_on_resource(
            &lease,
            &QueryExecutionId::new("real-leased-admin"),
            &admin,
            &artifact,
        )
        .await
        .expect("leased command reports actual native catalog");
    let actual = std::fs::read_to_string(&path).unwrap();
    assert!(actual.contains("commandResult") && actual.contains("rune_e2e_fixture"));
    assert_eq!(
        provider.close_resource(&fixed).await.unwrap(),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider.close_resource(&lease).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_resource_cancels_one_running_query_and_keeps_its_original_native_socket() {
    let mut f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|d| d.driver_id() == "mysql")
        .unwrap();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let driver = factory.create();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let fixed = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: f.cfg.clone(),
                target: NamespaceTarget::empty().with_database(f.database.clone()),
                identity_scope: IdentityScope::default(),
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
        .unwrap();
    let connection = provider.connection_handle(&fixed).unwrap();
    let pid_rows = driver
        .query(&connection, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows;
    let Some(Value::Integer(pid)) = pid_rows[0][0] else {
        panic!("actual MySQL thread id")
    };
    let dir = tempfile::tempdir().unwrap();
    let file = Arc::new(FileQuota::new(&dir.path().join("cancel.ndjson"), true, 9));
    let execution = QueryExecutionId::new("resource-real-cancel");
    let marker = format!("wait_{}", uuid::Uuid::new_v4().simple());
    let worker = tokio::spawn({
        let (provider, fixed, execution, file) = (
            provider.clone(),
            fixed.clone(),
            execution.clone(),
            file.clone(),
        );
        let sql = format!("SELECT 1 + SLEEP(20) AS {marker}");
        async move {
            provider
                .execute_on_resource(
                    &fixed,
                    &execution,
                    &CommandCall::new("query", json!({"sql":sql})),
                    &*file,
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let statement: Option<String> = sqlx::query_scalar(
                "SELECT CAST(INFO AS CHAR) FROM information_schema.PROCESSLIST WHERE ID=?",
            )
            .bind(pid)
            .fetch_optional(&mut f.oracle)
            .await
            .expect("visible real MySQL query")
            .flatten();
            if statement.as_deref().is_some_and(|s| s.contains(&marker)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("native thread observed running");
    let cancel = provider
        .request_cancel(&fixed, &execution)
        .await
        .expect("real execution-addressed cancellation");
    assert_eq!(cancel.disposition, CancelDisposition::Requested);
    let terminal = tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("active MySQL query stops")
        .expect("worker");
    assert!(
        matches!(
            terminal,
            Err(ResourceError::Driver(DriverError::QueryCancelled))
        ),
        "{terminal:?}"
    );
    assert_eq!(
        driver
            .query(&connection, "SELECT CONNECTION_ID()")
            .await
            .expect("exact same MySQL threads after drain")
            .rows,
        pid_rows
    );
    assert_eq!(
        provider
            .request_cancel(&fixed, &execution)
            .await
            .unwrap()
            .disposition,
        CancelDisposition::NotRegistered
    );
    assert_eq!(
        provider.close_resource(&fixed).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_losing_native_backend_midstream_never_reopens_a_retained_identity() {
    let mut f = Fixture::open().await;
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|d| d.driver_id() == "mysql")
        .unwrap();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let driver = factory.create();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let req = AcquireResourceRequest {
        connection_config: f.cfg.clone(),
        target: NamespaceTarget::empty().with_database(f.database.clone()),
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 8,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
    };
    let h = provider.acquire_resource(&req, &budget).await.unwrap();
    let other = provider.acquire_resource(&req, &budget).await.unwrap();
    let phys = provider.connection_handle(&h).unwrap();
    let stable = provider.connection_handle(&other).unwrap();
    let foreign_before = driver
        .query(&stable, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows;
    let Some(Value::Integer(pid)) = driver
        .query(&phys, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows[0][0]
    else {
        panic!("actual MySQL target")
    };
    let marker = format!("dz_lost_{}", uuid::Uuid::new_v4().simple());
    let exec = QueryExecutionId::new("socket-disappears-while-running");
    let dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(FileQuota::new(&dir.path().join("lost.jsonl"), true, 10));
    let worker = tokio::spawn({
        let (provider, h, exec, sink) = (provider.clone(), h.clone(), exec.clone(), sink.clone());
        let sql = format!("SELECT 1+SLEEP(20) AS {marker}");
        async move {
            provider
                .execute_on_resource(
                    &h,
                    &exec,
                    &CommandCall::new("query", json!({"sql":sql})),
                    &*sink,
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(4),async{
        loop{
            let active:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID=? AND LOCATE(?, INFO)>0")
            .bind(pid).bind(&marker).fetch_one(&mut f.oracle).await.unwrap();
            if active==1{break;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("independent connection confirms actual owned MySQL query in progress");
    assert_eq!(
        provider
            .request_cancel(&other, &exec)
            .await
            .unwrap()
            .disposition,
        CancelDisposition::Unsupported
    );
    sqlx::raw_sql(&format!("KILL CONNECTION {pid}"))
        .execute(&mut f.oracle)
        .await
        .expect("terminate this isolated fixture connection only");
    let outcome = tokio::time::timeout(Duration::from_secs(6), worker)
        .await
        .expect("native disconnect unwinds")
        .unwrap();
    assert!(
        matches!(outcome,Err(ResourceError::Driver(DriverError::ConnectionFailed(message))) if message.contains("SessionLost"))
    );
    assert!(provider.observe_session(&h).await.is_err());
    assert!(
        provider
            .execute_on_resource(
                &h,
                &QueryExecutionId::new("no-lost-reopen"),
                &CommandCall::new("query", json!({"sql":"SELECT 1"})),
                &*sink
            )
            .await
            .is_err()
    );
    assert_eq!(
        driver
            .query(&stable, "SELECT CONNECTION_ID()")
            .await
            .unwrap()
            .rows,
        foreign_before
    );
    assert_eq!(
        provider.close_resource(&h).await.unwrap(),
        CloseDisposition::Closed
    );
    assert_eq!(
        provider.close_resource(&other).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mysql_undrainable_native_stream_closes_identity_when_control_connection_is_denied() {
    let mut f = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let (actor, secret) = (
        format!("dz_drain_{}", &tag[..15]),
        uuid::Uuid::new_v4().simple().to_string(),
    );
    let table = format!("dz_drain_{tag}");
    let lock = format!("dz_drain_lock_{tag}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY); INSERT INTO {table} VALUES (1),(2);
     CREATE USER '{actor}'@'127.0.0.1' IDENTIFIED BY '{secret}' WITH MAX_USER_CONNECTIONS 1;
     GRANT SELECT ON `{}`.* TO '{actor}'@'127.0.0.1'",
        f.database
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let hold: i64 = sqlx::query_scalar("SELECT GET_LOCK(?,0)")
        .bind(&lock)
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(hold, 1);
    let factory = datazen_driver_mysql::factories()
        .into_iter()
        .find(|d| d.driver_id() == "mysql")
        .unwrap();
    let provider = datazen_driver_api::require_resource_provider(factory).unwrap();
    let driver = factory.create();
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let mut config = f.cfg.clone();
    config.username = Some(actor.clone());
    config.password = Some(secret);
    config.max_pool_size = 1;
    config.connection_timeout = 1;
    let retained = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: config,
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database(f.database.clone()),
                baseline: Baseline::default(),
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
                    max_physical_connections: 1,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
            },
            &budget,
        )
        .await
        .expect("native data admitted but one-account limit rejects auxiliary connection");
    let real = provider.connection_handle(&retained).unwrap();
    let rows = driver
        .query(&real, "SELECT CONNECTION_ID()")
        .await
        .unwrap()
        .rows;
    let process = match rows[0][0] {
        Some(Value::Integer(id)) => u64::try_from(id).unwrap(),
        ref other => panic!("unexpected native thread ID: {other:?}"),
    };
    let observed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER=?")
            .bind(&actor)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
    assert_eq!(observed, 1);
    let dir = tempfile::tempdir().unwrap();
    let sink = FileQuota::new(&dir.path().join("rejected.jsonl"), false, 0);
    let expected = QueryExecutionId::new(format!("no-control-drain-{tag}"));
    let query = CommandCall::new(
        "query_stream",
        json!({"sql":format!(
          "SELECT IF(id=1,REPEAT('x',262144),CONCAT(GET_LOCK('{lock}',18),'tail')) AS payload FROM {table} FORCE INDEX (PRIMARY) ORDER BY id"
        )}),
    );
    let started = tokio::time::Instant::now();
    let seen = tokio::time::timeout(
        Duration::from_secs(13),
        provider.execute_on_resource(&retained, &expected, &query, &sink),
    )
    .await;
    let released: i64 = sqlx::query_scalar("SELECT RELEASE_LOCK(?)")
        .bind(&lock)
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(released, 1);
    let outcome = seen
        .expect("provider stops before the native statement's full lock timeout")
        .unwrap_err();
    assert!(matches!(
        outcome,
        ResourceError::Driver(DriverError::SessionLost)
    ));
    assert!(
        started.elapsed() >= Duration::from_secs(4),
        "must be the actual bounded protocol drain, not a validation failure"
    );
    assert!(
        driver.query(&real, "SELECT 1").await.is_err(),
        "no replacement socket under the lost handle"
    );
    let left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID=? AND USER=?",
    )
    .bind(process)
    .bind(&actor)
    .fetch_one(&mut f.oracle)
    .await
    .unwrap();
    if left > 0 {
        let _ = sqlx::raw_sql(&format!("KILL CONNECTION {process}"))
            .execute(&mut f.oracle)
            .await;
    }
    assert_eq!(
        provider.close_resource(&retained).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
    sqlx::raw_sql(&format!("DROP USER '{actor}'@'127.0.0.1'"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
}
