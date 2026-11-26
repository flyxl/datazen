//! Real Apache Cloudberry only. The native PostgreSQL wire suite is separate.
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, IdentityScope, ResourceError,
    ResourcePurpose, ResourceScope,
};
use datazen_driver_api::session::{CloseDisposition, TransactionOptions, TransactionState};
use datazen_driver_api::{
    ConnectionConfig, DatabaseDriverFactory, QueryExecutionId, QueryStreamCallback,
    QueryStreamEvent, SqlTarget, Value,
};
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::{Connection, PgConnection};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Fixture {
    oracle: PgConnection,
    config: ConnectionConfig,
}

impl Fixture {
    async fn open() -> Self {
        let port = std::env::var("TEST_CLOUDBERRY_PORT")
            .expect("Cloudberry fixture port is required")
            .parse::<u16>()
            .expect("Cloudberry fixture port must be valid");
        let database = std::env::var("TEST_CLOUDBERRY_DATABASE")
            .expect("Cloudberry fixture database is required");
        assert!(
            database.starts_with("dz_fixture_cloudberry_"),
            "Cloudberry E2E requires a disposable fixture database"
        );
        let username = std::env::var("TEST_CLOUDBERRY_USER").expect("Cloudberry fixture user");
        let password =
            std::env::var("TEST_CLOUDBERRY_PASSWORD").expect("Cloudberry fixture password");
        assert!(port != 0 && !username.is_empty() && !password.is_empty());
        let mut oracle = tokio::time::timeout(
            Duration::from_secs(8),
            PgConnection::connect_with(
                &PgConnectOptions::new()
                    .host("127.0.0.1")
                    .port(port)
                    .username(&username)
                    .password(&password)
                    .database(&database)
                    .ssl_mode(PgSslMode::Disable),
            ),
        )
        .await
        .expect("Cloudberry fixture connection timed out")
        .unwrap_or_else(|_| panic!("cannot connect to real Apache Cloudberry fixture"));
        assert!(sqlx::query_scalar::<_, String>("SELECT version()")
            .fetch_one(&mut oracle)
            .await
            .expect("verify server identity")
            .contains("Apache Cloudberry"));
        let (role, local_mode, active_segments): (String, bool, i64) = sqlx::query_as(
            "SELECT current_setting('gp_role'), current_setting('gp_internal_is_singlenode')::bool,
                    (SELECT count(*) FROM gp_segment_configuration WHERE content >= 0 AND role = 'p' AND status = 'u')",
        )
        .fetch_one(&mut oracle)
        .await
        .expect("real Cloudberry distributed coordinator");
        assert_eq!(role, "dispatch");
        assert!(!local_mode && active_segments >= 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT marker FROM public.rune_e2e_fixture")
                .fetch_all(&mut oracle)
                .await
                .expect("disposable marker must exist"),
            ["datazen-rune-disposable"]
        );
        let config = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "name": "disposable live Apache Cloudberry E2E",
            "databaseType": "cloudberry",
            "host": "127.0.0.1",
            "port": port,
            "database": database,
            "username": username,
            "password": password,
            "sslMode": "disable",
            "connectionTimeout": 5,
            "maxPoolSize": 4
        }))
        .expect("Cloudberry alias config");
        Self { oracle, config }
    }
}

fn factory() -> &'static dyn DatabaseDriverFactory {
    datazen_driver_postgres::factories()
        .into_iter()
        .find(|f| f.driver_id() == "cloudberry")
        .expect("registered Cloudberry factory")
}

fn string(s: impl Into<String>) -> Option<Value> {
    Some(Value::String(s.into()))
}

#[tokio::test]
async fn distributed_table_catalog_query_plan_and_stream_use_cloudberry_segments() {
    let mut fixture = Fixture::open().await;
    let f = factory();
    let driver = f.create();
    assert_eq!(driver.driver_type(), "cloudberry");
    let server = driver
        .test_connection(&fixture.config)
        .await
        .expect("real alias login");
    assert_eq!(server.server_type, "cloudberry");
    assert!(server.server_version.contains("Apache Cloudberry"));
    let database = fixture.config.database.as_deref().unwrap();
    let schema = format!("dz_cb_rows_{}", uuid::Uuid::new_v4().simple());
    let relation = format!("{schema}.metrics");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {relation}(id int PRIMARY KEY, label text NOT NULL, payload jsonb) DISTRIBUTED BY (id);
         CREATE INDEX metrics_label ON {relation}(label);
         INSERT INTO {relation} SELECT n, 'item-'||n, jsonb_build_object('id',n) FROM generate_series(1,128) n;"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("independent genuine distributed fixture");
    let ownership: Vec<(i32, i64)> = sqlx::query_as(&format!(
        "SELECT gp_segment_id, count(*) FROM {relation} GROUP BY gp_segment_id ORDER BY gp_segment_id"
    ))
    .fetch_all(&mut fixture.oracle)
    .await
    .expect("independent physical segment readback");
    assert_eq!(
        ownership.iter().map(|row| row.0).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(ownership.iter().map(|row| row.1).sum::<i64>(), 128);

    let db = driver
        .connect(&fixture.config)
        .await
        .expect("real alias connection");
    assert!(driver
        .get_databases(&db)
        .await
        .unwrap()
        .contains(&database.to_owned()));
    assert!(driver
        .get_schemas(&db, database)
        .await
        .unwrap()
        .unwrap()
        .contains(&schema));
    let tables = driver
        .get_tables(&db, database, Some(&schema))
        .await
        .unwrap();
    assert!(tables.iter().any(|t| t.name == "metrics"), "{tables:?}");
    let table = driver
        .get_table_schema(&db, "metrics", database, Some(&schema))
        .await
        .unwrap();
    assert_eq!(
        table
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "label", "payload"]
    );
    assert_eq!(table.primary_keys, ["id"]);
    assert!(table
        .indexes
        .iter()
        .any(|index| index.name == "metrics_label"));
    let target = SqlTarget::new(Some(database), Some(&schema));
    let selected = driver
        .query_with_params_at(
            &db,
            "SELECT id,label,payload FROM metrics WHERE id=$1::int",
            &[Value::Integer(41)],
            target,
        )
        .await
        .expect("qualified bound query through Cloudberry coordinator");
    assert_eq!(
        selected.rows,
        [vec![
            Some(Value::Integer(41)),
            string("item-41"),
            Some(Value::Json(serde_json::json!({"id":41})))
        ]]
    );
    let empty = driver
        .query_with_params_at(
            &db,
            "SELECT id,label,payload FROM metrics WHERE id=$1::int",
            &[Value::Integer(999)],
            target,
        )
        .await
        .expect("empty result retains typed metadata");
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 3);

    let run = QueryExecutionId::new("cloudberry-distributed-stream");
    driver.prepare_query_execution(&db, &run).await.unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let callback: QueryStreamCallback = Arc::new(move |event| observed.lock().unwrap().push(event));
    driver.query_stream_with_execution(&db, &run, &format!(
        "SELECT gp_segment_id, count(*) FROM {relation} GROUP BY gp_segment_id ORDER BY gp_segment_id"),
        None, callback,
    ).await.expect("actual distributed result stream");
    let expected = ownership
        .into_iter()
        .map(|(id, count)| {
            vec![
                Some(Value::Integer(i64::from(id))),
                Some(Value::Integer(count)),
            ]
        })
        .collect::<Vec<_>>();
    {
        let events = events.lock().unwrap();
        let rows = events
            .iter()
            .filter_map(|event| match event {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.clone()),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(rows, expected);
        assert!(events
            .iter()
            .any(|event| matches!(event, QueryStreamEvent::Done { .. })));
    }
    let plan = driver
        .explain(&db, &format!("SELECT * FROM {relation}"))
        .await
        .expect("native Cloudberry EXPLAIN");
    assert!(plan.plan_text.contains("Gather Motion"), "{plan:?}");
    assert!(plan.plan_json.is_some());
    driver.disconnect(db).await.unwrap();
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}

#[derive(Default)]
struct Ledger(Mutex<BTreeMap<String, u32>>);

#[async_trait::async_trait]
impl BudgetPort for Ledger {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        let permit_id = uuid::Uuid::new_v4().to_string();
        assert!(self
            .0
            .lock()
            .unwrap()
            .insert(permit_id.clone(), n)
            .is_none());
        Ok(BudgetPermit {
            permit_id,
            physical_connections: n,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        assert_eq!(
            self.0.lock().unwrap().remove(&permit.permit_id),
            Some(permit.physical_connections)
        );
        Ok(())
    }
}

#[tokio::test]
async fn cloudberry_resource_owns_a_physical_distributed_transaction_and_rolls_back() {
    let mut fixture = Fixture::open().await;
    let f = factory();
    let driver = f.create();
    let provider = f
        .resource_provider()
        .expect("registered Cloudberry resource provider");
    let ordinary = datazen_driver_postgres::factories()
        .into_iter()
        .find(|factory| factory.driver_id() == "postgresql")
        .unwrap()
        .resource_provider()
        .unwrap();
    assert_eq!(provider.provider_id(), "cloudberry");
    let table = format!("dz_cb_rollback_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE TABLE {table}(id int PRIMARY KEY, val text) DISTRIBUTED BY (id); INSERT INTO {table} VALUES (1,'original')"))
        .execute(&mut fixture.oracle).await.expect("actual distributed transaction fixture");
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let resource = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: fixture.config.clone(),
                target: NamespaceTarget::empty()
                    .with_database(fixture.config.database.clone().unwrap()),
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
                    max_physical_connections: 16,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
                identity_scope: IdentityScope::default(),
                baseline: Baseline::default(),
            },
            &budget,
        )
        .await
        .expect("leased Cloudberry resource");
    assert!(ordinary.connection_handle(&resource).is_err());
    let connection = provider
        .connection_handle(&resource)
        .expect("aliased real driver handle");
    let owner = provider
        .begin_transaction(
            &resource,
            &TransactionOptions {
                isolation_level: None,
                read_only: None,
                defers_commit: false,
            },
        )
        .await
        .expect("distributed transaction");
    let before = provider.observe_session(&resource).await.unwrap();
    assert_eq!(before.context.transaction_state, TransactionState::Active);
    assert_eq!(before.transaction.transaction_id, owner.transaction_id);
    assert_eq!(before.handles.len(), 1);
    assert_eq!(
        driver
            .execute_with_params(
                &connection,
                &format!("UPDATE {table} SET val=$1::text WHERE id=1"),
                &[Value::String("staged".into())]
            )
            .await
            .unwrap(),
        1
    );
    let query = format!("SELECT val FROM {table} WHERE id=1");
    assert_eq!(
        driver.query(&connection, &query).await.unwrap().rows,
        [vec![string("staged")]]
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(&query)
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap(),
        "original"
    );
    provider
        .close_handle(&resource, &before.handles[0])
        .await
        .expect("alias owns its transaction close");
    assert_eq!(
        provider
            .observe_session(&resource)
            .await
            .unwrap()
            .context
            .transaction_state,
        TransactionState::Idle
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(&query)
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap(),
        "original"
    );
    assert_eq!(
        provider.close_resource(&resource).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(resource.is_closed());
    assert!(ledger.0.lock().unwrap().is_empty());
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}

#[tokio::test]
async fn registered_cloudberry_cancel_reaches_only_its_live_query() {
    let mut fixture = Fixture::open().await;
    let driver = factory().create();
    let main = driver
        .connect(&fixture.config)
        .await
        .expect("cancel fixture");
    let mut side_config = fixture.config.clone();
    side_config.id = uuid::Uuid::new_v4().to_string();
    let side = driver
        .connect(&side_config)
        .await
        .expect("independent alias session");
    let execution = QueryExecutionId::new("cloudberry-native-cancel");
    driver
        .prepare_query_execution(&main, &execution)
        .await
        .unwrap();
    let trace = format!("dz_cb_cancel_{}", uuid::Uuid::new_v4().simple());
    let long_sql = format!("SELECT pg_sleep(20) /* {trace} */");
    let pending = {
        let driver = driver.clone();
        let connection = main.clone();
        let execution = execution.clone();
        tokio::spawn(async move {
            driver
                .query_stream_with_execution(
                    &connection,
                    &execution,
                    &long_sql,
                    None,
                    Arc::new(|_| {}),
                )
                .await
        })
    };
    let pid = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let pid: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE pid <> pg_backend_pid()
                   AND state='active' AND query LIKE $1",
            )
            .bind(format!("%{trace}%"))
            .fetch_optional(&mut fixture.oracle)
            .await
            .unwrap();
            if let Some(pid) = pid {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("independent Cloudberry observation of actual active query");
    assert!(pid > 0);
    assert!(matches!(
        driver.cancel_query_with_execution(&side, &execution).await,
        Err(datazen_driver_api::DriverError::QueryExecutionSessionMismatch)
    ));
    driver
        .cancel_query_with_execution(&main, &execution)
        .await
        .expect("cancel that actual coordinator PID");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), pending)
            .await
            .unwrap()
            .unwrap(),
        Err(datazen_driver_api::DriverError::QueryCancelled)
    ));
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND state='active' AND query LIKE $2)")
        .bind(pid).bind(format!("%{trace}%")).fetch_one(&mut fixture.oracle).await.unwrap();
    assert!(!active, "real statement has stopped on Cloudberry");
    assert_eq!(
        driver.query(&side, "SELECT 43").await.unwrap().rows,
        [vec![Some(Value::Integer(43))]]
    );
    driver.disconnect(side).await.unwrap();
    driver.disconnect(main).await.unwrap();
}
