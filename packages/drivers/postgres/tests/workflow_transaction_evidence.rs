//! Driver-level evidence for the Workflow proof recipe; not a desktop Job journey.
//! Requires a disposable MIGRATION_TEST_* fixture; missing fixtures fail closed.
use datazen_driver_api::{namespace::NamespaceTarget, resource::*, session::*, *};
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};
use std::sync::Arc;
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;
struct Budget;
#[async_trait::async_trait]
impl BudgetPort for Budget {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        Ok(BudgetPermit {
            permit_id: uuid::Uuid::new_v4().to_string(),
            physical_connections: n,
        })
    }
    async fn release_physical_connections(&self, _: &BudgetPermit) -> Result<(), ResourceError> {
        Ok(())
    }
}
#[derive(Default)]
struct Sink(std::sync::Mutex<Vec<Vec<Option<Value>>>>);
#[async_trait::async_trait]
impl ResultSink for Sink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.0.lock().expect("sink poisoned").extend(chunk.rows);
        Ok(())
    }
    async fn write_output(&self, _: serde_json::Value) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        Ok(())
    }
}
async fn sql(p: &PostgresResourceProvider, h: &ResourceHandle, text: &str) -> bool {
    p.execute_on_resource(
        h,
        &QueryExecutionId::new(uuid::Uuid::new_v4().to_string()),
        &CommandCall::new(
            if text.starts_with("SELECT") {
                "query"
            } else {
                "execute"
            },
            serde_json::json!({"sql": text}),
        ),
        &Sink::default(),
    )
    .await
    .map(|c| c.completion_status.is_success())
    .unwrap_or(false)
}
fn options() -> TransactionOptions {
    TransactionOptions {
        isolation_level: None,
        read_only: None,
        defers_commit: false,
    }
}
#[tokio::test]
async fn retained_recipe_commit_rollback_catalog_race_and_closed_session() {
    let Some(mut config) = migration_gate::require_config(
        "postgresql",
        "workflow-transaction-proof",
        "postgresql",
        "workflow-proof",
        &[],
    ) else {
        return;
    };
    config.max_pool_size = 1;
    let driver = Arc::new(PostgresDriver::new());
    let observer = driver
        .connect(&config)
        .await
        .expect("fixture connection failed");
    let schema = format!("wf_{}", uuid::Uuid::new_v4().simple());
    driver
        .execute(&observer, &format!("CREATE SCHEMA {schema}"))
        .await
        .expect("schema creation failed");
    driver
        .execute(
            &observer,
            &format!("CREATE TABLE {schema}.items(id INTEGER, label TEXT)"),
        )
        .await
        .expect("heap creation failed");
    config.schema = Some(schema.clone());
    let target = NamespaceTarget::empty()
        .with_database(config.database.clone().expect("database required"))
        .with_schema(&schema);
    let p = PostgresResourceProvider::new(driver.clone());
    let budget: Arc<dyn BudgetPort> = Arc::new(Budget);
    let request = AcquireResourceRequest {
        connection_config: config,
        target: target.clone(),
        scope: ResourceScope {
            purpose: ResourcePurpose::AgentSession,
            max_physical_connections: 4,
            holds_open_transaction: true,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::new(vec![]),
    };
    let h = p
        .acquire_resource(&request, &budget)
        .await
        .expect("retained acquire failed");
    let plan = driver
        .workflow_transaction_plan(
            &["INSERT INTO items(id, label) VALUES (1, 'committed')".into()],
            &target,
        )
        .expect("recipe rejected")
        .expect("missing recipe");
    let direct = driver
        .begin_transaction(&observer)
        .await
        .expect("direct proof begin failed");
    for lock in &plan.lock_sql {
        driver
            .execute(&observer, lock)
            .await
            .expect("direct proof lock failed");
    }
    let safe = driver
        .query(&observer, &plan.metadata_probe_sql)
        .await
        .expect("direct proof probe failed");
    assert!(matches!(safe.rows[0][0], Some(Value::Bool(true))));
    driver
        .rollback(direct)
        .await
        .expect("direct proof rollback failed");
    p.begin_transaction(&h, &options())
        .await
        .expect("begin failed");
    for lock in &plan.lock_sql {
        assert!(sql(&p, &h, lock).await, "relation lock failed");
    }
    // This call checks execution only; true/false decoder assertions below
    // independently cover the guard result.
    assert!(
        sql(&p, &h, &plan.metadata_probe_sql).await,
        "catalog probe failed"
    );
    // lock_timeout is server-side: failure proves incompatible DDL cannot pass
    // the retained relation lock, without a timing-only sleep assertion.
    driver
        .execute(&observer, "SET lock_timeout = '150ms'")
        .await
        .expect("lock timeout failed");
    // DriverError stores query text rather than structured SQLSTATE. Catch the
    // exact server code inside PostgreSQL: every other error propagates, and
    // successful DDL raises an uncaught P0001. Only 55P03 makes this DO succeed.
    driver
        .execute(
            &observer,
            &format!(
                "DO $proof$ BEGIN \
                 ALTER TABLE {schema}.items ADD COLUMN extra INTEGER; \
                 RAISE EXCEPTION 'DDL escaped retained lock'; \
                 EXCEPTION WHEN SQLSTATE '55P03' THEN NULL; END; $proof$"
            ),
        )
        .await
        .expect("DDL must fail specifically with lock_not_available (55P03)");
    assert!(sql(&p, &h, &plan.statements[0]).await);
    assert!(p
        .commit_transaction(&h)
        .await
        .expect("commit observation failed")
        .committed());
    let rows = driver
        .query(&observer, &format!("SELECT id FROM {schema}.items"))
        .await
        .expect("readback failed");
    assert_eq!(rows.rows.len(), 1);
    p.begin_transaction(&h, &options())
        .await
        .expect("second begin failed");
    assert!(sql(&p, &h, &plan.statements[0]).await);
    assert!(
        !sql(
            &p,
            &h,
            &format!("INSERT INTO {schema}.items(id) VALUES ('invalid integer')")
        )
        .await
    );
    let rollback = p
        .rollback_transaction(&h)
        .await
        .expect("rollback observation failed");
    assert_eq!(rollback.state, TransactionState::Idle);
    assert_ne!(rollback.effect, Some(EffectOutcome::Unknown));
    assert_eq!(
        driver
            .query(&observer, &format!("SELECT id FROM {schema}.items"))
            .await
            .expect("rollback readback failed")
            .rows
            .len(),
        1
    );
    p.close_resource(&h).await.expect("close failed");
    assert!(
        p.commit_transaction(&h).await.is_err(),
        "closed resource accepted commit"
    );
    assert!(
        p.rollback_transaction(&h).await.is_err(),
        "closed resource accepted rollback"
    );
    // Kill only the PID returned by this retained resource. This establishes
    // lost-session honesty, not a network race after COMMIT reached the server.
    for commit in [true, false] {
        let lost = p
            .acquire_resource(&request, &budget)
            .await
            .expect("lost-session acquire failed");
        p.begin_transaction(&lost, &options())
            .await
            .expect("lost-session begin failed");
        assert!(sql(&p, &lost, &plan.statements[0]).await);
        let sink = Sink::default();
        p.execute_on_resource(
            &lost,
            &QueryExecutionId::new(uuid::Uuid::new_v4().to_string()),
            &CommandCall::new(
                "query",
                serde_json::json!({"sql": "SELECT pg_backend_pid()"}),
            ),
            &sink,
        )
        .await
        .expect("PID lookup failed");
        let pid = match &sink.0.lock().expect("sink poisoned")[0][0] {
            Some(Value::Integer(pid)) => *pid,
            _ => panic!("PID must be an integer"),
        };
        let killed = driver
            .query(
                &observer,
                &format!("SELECT pg_catalog.pg_terminate_backend({pid})"),
            )
            .await
            .expect("session termination failed");
        assert!(matches!(killed.rows[0][0], Some(Value::Bool(true))));
        let outcome = if commit {
            p.commit_transaction(&lost).await
        } else {
            p.rollback_transaction(&lost).await
        }
        .expect("lost-session outcome missing");
        assert_eq!(outcome.state, TransactionState::Unknown);
        assert_eq!(outcome.effect, Some(EffectOutcome::Unknown));
        let _ = p.close_resource(&lost).await;
        assert_eq!(
            driver
                .query(&observer, &format!("SELECT id FROM {schema}.items"))
                .await
                .expect("lost-session readback failed")
                .rows
                .len(),
            1
        );
    }
    // DDL wins before locking: the identical recipe must observe a false guard.
    driver
        .execute(&observer, &format!("CREATE INDEX ON {schema}.items(id)"))
        .await
        .expect("index creation failed");
    let tx = driver
        .begin_transaction(&observer)
        .await
        .expect("probe begin failed");
    for lock in &plan.lock_sql {
        driver
            .execute(&observer, lock)
            .await
            .expect("probe lock failed");
    }
    let probe = driver
        .query(&observer, &plan.metadata_probe_sql)
        .await
        .expect("probe failed");
    assert!(matches!(probe.rows[0][0], Some(Value::Bool(false))));
    driver.rollback(tx).await.expect("probe rollback failed");
    driver
        .execute(&observer, &format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .expect("cleanup failed");
    driver
        .disconnect(observer)
        .await
        .expect("disconnect failed");
}
