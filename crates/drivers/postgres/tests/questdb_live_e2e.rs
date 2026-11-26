//! Real QuestDB product tests for the PostgreSQL wire alias.
use datazen_driver_api::{ConnectionConfig, DatabaseDriverFactory, Value};
use serde_json::json;
use sqlx::{
    postgres::{PgConnectOptions, PgSslMode},
    Connection, PgConnection,
};

fn factory() -> &'static dyn DatabaseDriverFactory {
    datazen_driver_postgres::factories()
        .into_iter()
        .find(|f| f.driver_id() == "questdb")
        .expect("QuestDB factory is registered")
}

async fn fixture() -> (PgConnection, ConnectionConfig) {
    let port: u16 = std::env::var("DATAZEN_TEST_QUESTDB_PORT")
        .expect("real disposable QuestDB fixture required")
        .parse()
        .expect("valid fixture port");
    let password = std::env::var("DATAZEN_TEST_QUESTDB_PASSWORD").expect("fixture password");
    let expected_marker = std::env::var("DATAZEN_TEST_QUESTDB_MARKER").expect("fixture marker");
    let mut oracle = PgConnection::connect_with(
        &PgConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .database("qdb")
            .username("admin")
            .password(&password)
            .ssl_mode(PgSslMode::Disable),
    )
    .await
    .expect("independent QuestDB protocol connection");
    let build: String = sqlx::query_scalar("SELECT build()")
        .fetch_one(&mut oracle)
        .await
        .expect("QuestDB native build function");
    assert!(build.contains("QuestDB"), "{build}");
    let marker: String = sqlx::query_scalar("SELECT token FROM datazen_qdb_marker")
        .fetch_one(&mut oracle)
        .await
        .expect("isolated fixture marker");
    assert_eq!(marker, expected_marker);

    let cfg: ConnectionConfig = serde_json::from_value(json!({
        "id":uuid::Uuid::new_v4().to_string(), "name":"disposable QuestDB",
        "databaseType":"questdb", "host":"127.0.0.1", "port":port, "database":"qdb",
        "username":"admin", "password":password, "sslMode":"disable",
        "connectionTimeout":4, "maxPoolSize":2
    }))
    .expect("QuestDB connection configuration");
    (oracle, cfg)
}

#[tokio::test]
async fn native_questdb_connection_executes_and_reads_time_series() {
    let (mut oracle, cfg) = fixture().await;
    let driver = factory().create();
    assert_eq!(driver.driver_type(), "questdb");
    let handle = driver
        .connect(&cfg)
        .await
        .expect("QuestDB alias connects to genuine QuestDB");
    let table = format!("dzen_ticks_{}", uuid::Uuid::new_v4().simple());
    driver.execute(&handle, &format!("CREATE TABLE {table} (sensor SYMBOL, reading INT, ts TIMESTAMP) TIMESTAMP(ts) PARTITION BY DAY BYPASS WAL"))
        .await.expect("create native time series");
    driver.execute(&handle, &format!("INSERT INTO {table} VALUES ('alpha',11,'2024-01-01T00:00:00.000000Z'), ('alpha',17,'2024-01-01T00:01:00.000000Z'), ('beta',3,'2024-01-01T00:01:10.000000Z')"))
        .await.expect("append native timestamp rows");
    let sql = format!("SELECT sum(reading) FROM {table} WHERE sensor='alpha'");
    let read = driver
        .query(&handle, &sql)
        .await
        .expect("alias reads real QuestDB rows");
    assert_eq!(read.rows, [vec![Some(Value::Integer(28))]]);
    let actual: i64 = sqlx::query_scalar(&sql)
        .fetch_one(&mut oracle)
        .await
        .expect("independent readback after DataZen writes");
    assert_eq!(actual, 28);
    let sampled = driver
        .query(
            &handle,
            &format!("SELECT ts, sum(reading) FROM {table} SAMPLE BY 1m"),
        )
        .await
        .expect("native QuestDB SAMPLE BY, not a vanilla PostgreSQL surrogate");
    assert_eq!(sampled.rows.len(), 2);
    assert_eq!(sampled.rows[1][1], Some(Value::Integer(20)));
    driver
        .execute(&handle, &format!("DROP TABLE {table}"))
        .await
        .expect("remove owned fixture table");
    driver
        .disconnect(handle)
        .await
        .expect("disconnect native QuestDB alias");
}

#[tokio::test]
async fn native_questdb_streams_bound_rows_and_returns_the_engine_plan() {
    use datazen_driver_api::QueryStreamEvent;
    use std::sync::{Arc, Mutex};
    let (mut oracle, cfg) = fixture().await;
    let driver = factory().create();
    let handle = driver.connect(&cfg).await.expect("genuine QuestDB");
    let sql = "SELECT x k, x * 10 v FROM long_sequence(5) WHERE x >= $1";
    let mut expected = sqlx::query_as::<_, (i64, i64)>(sql)
        .bind(2_i64)
        .fetch_all(&mut oracle)
        .await
        .expect("independent QuestDB prepared query");
    let rows = Arc::new(Mutex::new(Vec::new()));
    let stream = Arc::clone(&rows);
    driver
        .query_stream_with_params(
            &handle,
            sql,
            &[Value::Integer(2)],
            Some(3),
            Arc::new(move |event| {
                if let QueryStreamEvent::Rows { rows, .. } = event {
                    stream.lock().unwrap().extend(rows);
                }
            }),
        )
        .await
        .expect("real bounded QuestDB streaming query with a prepared value");
    expected.truncate(3);
    assert_eq!(
        *rows.lock().unwrap(),
        expected
            .into_iter()
            .map(|(k, v)| vec![Some(Value::Integer(k)), Some(Value::Integer(v))])
            .collect::<Vec<_>>()
    );
    let plan_sql = "SELECT x FROM long_sequence(7) WHERE x > 3";
    let native = sqlx::query_scalar::<_, String>(&format!("EXPLAIN {plan_sql}"))
        .fetch_all(&mut oracle)
        .await
        .expect("QuestDB native query plan");
    let planned = driver
        .explain(&handle, plan_sql)
        .await
        .expect("QuestDB native text and JSON plan");
    assert_eq!(planned.plan_text, native.join("\n"));
    let native_json: String = sqlx::query_scalar(&format!("EXPLAIN (FORMAT JSON) {plan_sql}"))
        .fetch_one(&mut oracle)
        .await
        .expect("native QuestDB JSON explain");
    assert_eq!(
        planned.plan_json,
        Some(serde_json::from_str(&native_json).expect("native JSON plan"))
    );
    driver.disconnect(handle).await.expect("disconnect QuestDB");
}

#[tokio::test]
async fn native_questdb_bound_writes_and_script_recovery_keep_exact_values() {
    let (mut oracle, cfg) = fixture().await;
    let driver = factory().create();
    let info = driver
        .test_connection(&cfg)
        .await
        .expect("real connectivity check");
    assert_eq!(info.server_type, "questdb");
    let independent_version: String = sqlx::query_scalar("SELECT build()")
        .fetch_one(&mut oracle)
        .await
        .expect("genuine QuestDB version, not protocol compatibility label");
    assert_eq!(info.server_version, independent_version);

    let raw_password = cfg.password.clone().expect("fixture password");
    let mut refused = cfg.clone();
    refused.password = Some(format!("{raw_password}-invalid"));
    refused.connection_timeout = 2;
    assert!(
        driver.test_connection(&refused).await.is_err(),
        "server must reject wrong fixture password"
    );

    let handle = driver.connect(&cfg).await.expect("open genuine QuestDB");
    let live_info = driver
        .get_server_info(&handle)
        .await
        .expect("server info on native QuestDB");
    assert_eq!(live_info.server_type, "questdb");
    assert_eq!(live_info.server_version, independent_version);
    let table = format!("dzen_bind_{}", uuid::Uuid::new_v4().simple());
    let code = "north; '渠' \\ sensor";
    driver
        .execute(
            &handle,
            &format!("CREATE TABLE {table} (sensor VARCHAR, reading LONG, enabled BOOLEAN)"),
        )
        .await
        .expect("native varchar and long table");
    let insert = format!("INSERT INTO {table} (sensor, reading, enabled) VALUES ($1, $2, $3)");
    assert_eq!(
        driver
            .execute_with_params(
                &handle,
                &insert,
                &[
                    Value::String(code.into()),
                    Value::Integer(41),
                    Value::Bool(true)
                ]
            )
            .await
            .expect("bind literal, long and boolean"),
        1
    );
    let invalid = driver
        .query(&handle, &format!("SELECT missing_test_column FROM {table}"))
        .await;
    assert!(invalid.is_err(), "actual server rejected the invalid query");
    let script =
        format!("SELECT sensor, reading, enabled FROM {table}; SELECT count() FROM {table}");
    let batch = driver
        .query_multi(&handle, &script, None)
        .await
        .expect("same driver connection after native SQL error");
    assert_eq!(batch.results.len(), 2);
    assert_eq!(
        batch.results[0].rows,
        [vec![
            Some(Value::String(code.into())),
            Some(Value::Integer(41)),
            Some(Value::Bool(true))
        ]]
    );
    assert_eq!(batch.results[1].rows, [vec![Some(Value::Integer(1))]]);
    let stored: (String, i64, bool) =
        sqlx::query_as(&format!("SELECT sensor, reading, enabled FROM {table}"))
            .fetch_one(&mut oracle)
            .await
            .expect("independent exact persisted values");
    assert_eq!(stored, (code.into(), 41, true));
    driver
        .execute(&handle, &format!("DROP TABLE {table}"))
        .await
        .expect("drop disposable QuestDB table");
    driver
        .disconnect(handle)
        .await
        .expect("release QuestDB pool after error");
}
