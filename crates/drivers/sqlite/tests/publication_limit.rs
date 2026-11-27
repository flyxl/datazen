//! Real retained SQLite provider, a disposable on-disk file, and a separate SQLx oracle.
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, IdentityScope,
    ResourceError, ResourceProvider, ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
};
use datazen_driver_api::{DriverError, QueryExecutionId, Value};
use serde_json::json;
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct FixtureDir(PathBuf);
impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Captured(Mutex<Vec<ResultChunk>>);
#[async_trait::async_trait]
impl ResultSink for Captured {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.0.lock().expect("row capture").push(chunk);
        Ok(())
    }
    async fn write_output(&self, _output: serde_json::Value) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        panic!("an expected-success stream reported a failure")
    }
}

#[derive(Default)]
struct Budget(Mutex<bool>);
#[async_trait::async_trait]
impl BudgetPort for Budget {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        assert_eq!(n, 1);
        let mut active = self.0.lock().expect("budget");
        assert!(!*active);
        *active = true;
        Ok(BudgetPermit {
            permit_id: "fixture-budget".into(),
            physical_connections: 1,
        })
    }
    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        assert_eq!(permit.permit_id, "fixture-budget");
        let mut active = self.0.lock().expect("budget");
        assert!(*active);
        *active = false;
        Ok(())
    }
}

#[tokio::test]
async fn real_sqlite_publication_keeps_compound_values_and_caps_only_output() {
    let dir = FixtureDir(
        std::env::temp_dir().join(format!("dz-sqlite-publication-{}", uuid::Uuid::new_v4())),
    );
    std::fs::create_dir(&dir.0).expect("create disposable test directory");
    let file = dir.0.join("fixture.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&file)
        .create_if_missing(true);
    let mut oracle = SqliteConnection::connect_with(&options)
        .await
        .expect("independent connection to actual file");
    sqlx::raw_sql(
        "CREATE TABLE sample (id INTEGER PRIMARY KEY); INSERT INTO sample VALUES (1),(2),(3);",
    )
    .execute(&mut oracle)
    .await
    .expect("disposable data");
    let config = serde_json::from_value(json!({
        "id": uuid::Uuid::new_v4().to_string(), "name": "temporary output budget fixture",
        "databaseType": "sqlite", "database": file.to_str().expect("fixture path"),
        "connectionTimeout": 5, "maxPoolSize": 1, "readOnly": true,
    }))
    .expect("fixture profile");
    let provider = datazen_driver_sqlite::factory()
        .resource_provider()
        .expect("linked provider");
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let resource = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: config,
                target: NamespaceTarget::empty(),
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
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
        .expect("actual SQLite resource");

    for (sql, expected, limit_key) in [
        (
            "SELECT id AS probe FROM sample WHERE id=1 UNION ALL VALUES (2)",
            vec![1, 2],
            "publishedRowLimit",
        ),
        (
            "SELECT id AS probe FROM sample WHERE id=1 UNION ALL VALUES (2), (3)",
            vec![1, 2, 3],
            "publishedRowLimit",
        ),
        ("SELECT id FROM sample ORDER BY id", vec![1, 2, 3], "limit"),
    ] {
        let reference = sqlx::query_scalar::<_, i64>(sql)
            .fetch_all(&mut oracle)
            .await
            .expect("independent exact authored result");
        assert_eq!(reference, expected);
        let mut input = json!({"sql":sql});
        input[limit_key] = 2.into();
        let sink = Captured::default();
        let result = provider
            .execute_on_resource(
                &resource,
                &QueryExecutionId::new(uuid::Uuid::new_v4().to_string()),
                &CommandCall::new("query", input),
                &sink,
            )
            .await
            .expect("actual resource query");
        assert!(result.completion_status.is_success());
        let chunks = sink.0.lock().expect("captured resource output");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].sql, sql);
        let expected = reference[..2]
            .iter()
            .map(|n| vec![Some(Value::Integer(*n))])
            .collect::<Vec<_>>();
        assert_eq!(chunks[0].rows, expected);
    }
    for (input, detail) in [
        (json!({"sql":"SELECT 1","publishedRowLimit":0}), "1..=10001"),
        (
            json!({"sql":"SELECT 1","publishedRowLimit":2,"limit":2}),
            "not both",
        ),
        (
            json!({"sql":"SELECT 1; SELECT 2","publishedRowLimit":2}),
            "exactly one",
        ),
    ] {
        let sink = Captured::default();
        let refused = provider
            .execute_on_resource(
                &resource,
                &QueryExecutionId::new(uuid::Uuid::new_v4().to_string()),
                &CommandCall::new("query", input),
                &sink,
            )
            .await;
        assert!(
            matches!(refused, Err(ResourceError::Driver(DriverError::InvalidConfig(reason)))
                         if reason.contains(detail))
        );
        assert!(sink
            .0
            .into_inner()
            .expect("no rejected result capture")
            .is_empty());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT id FROM sample ORDER BY id")
            .fetch_all(&mut oracle)
            .await
            .unwrap(),
        [1, 2, 3]
    );
    provider
        .close_resource(&resource)
        .await
        .expect("release live resource");
    assert!(!*ledger.0.lock().unwrap());
    oracle
        .close()
        .await
        .expect("close independent handle before cleanup");
}
