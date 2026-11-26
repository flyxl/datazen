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
