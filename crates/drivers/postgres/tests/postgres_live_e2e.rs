//! Public PostgreSQL driver journeys against a disposable, marked PostgreSQL server.
//! Runs no unit tests and never treats a missing live fixture as a pass.

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, QueryStreamEvent, SslMode, Value};
use datazen_driver_postgres::PostgresDriver;
use serde_json::json;
use sqlx::{postgres::PgConnectOptions, Connection, PgConnection};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Fixture {
    oracle: PgConnection,
    config: ConnectionConfig,
}

impl Fixture {
    async fn open() -> Self {
        fn required(name: &str) -> String {
            std::env::var(name)
                .unwrap_or_else(|_| panic!("{name} is required for live PostgreSQL E2E"))
        }

        let host = required("TEST_PG_HOST");
        let port: u16 = required("TEST_PG_PORT")
            .parse()
            .expect("invalid TEST_PG_PORT");
        let database = required("TEST_PG_DATABASE");
        let user = required("TEST_PG_USER");
        let password = std::env::var("TEST_PG_PASSWORD").unwrap_or_default();
        assert_eq!(
            host, "127.0.0.1",
            "live E2E requires an isolated loopback fixture"
        );
        assert_ne!(port, 0, "live E2E requires a valid PostgreSQL port");
        assert!(
            database == "datazen_e2e" || database.starts_with("dz_fixture_"),
            "live E2E refuses non-fixture database names"
        );
        let options = PgConnectOptions::new()
            .host(&host)
            .port(port)
            .username(&user)
            .password(&password)
            .database(&database)
            .ssl_mode(sqlx::postgres::PgSslMode::Disable);
        let mut oracle =
            tokio::time::timeout(Duration::from_secs(8), PgConnection::connect_with(&options))
                .await
                .expect("live PostgreSQL fixture timed out")
                .unwrap_or_else(|_| panic!("could not connect to the live PostgreSQL fixture"));
        let marker = sqlx::query_scalar::<_, String>("SELECT marker FROM public.rune_e2e_fixture")
            .fetch_all(&mut oracle)
            .await
            .unwrap_or_else(|_| {
                panic!("could not verify the disposable PostgreSQL fixture marker")
            });
        assert_eq!(
            marker,
            ["datazen-rune-disposable"],
            "invalid disposable marker"
        );
        let config = serde_json::from_value(json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "name": "disposable live PostgreSQL E2E",
            "databaseType": "postgresql",
            "host": host,
            "port": port,
            "database": database,
            "username": user,
            "password": password,
            "sslMode": SslMode::Disable,
            "connectionTimeout": 5,
            "maxPoolSize": 4
        }))
        .unwrap_or_else(|_| panic!("could not create the PostgreSQL fixture config"));
        Self { oracle, config }
    }
}

fn string(value: impl Into<String>) -> Option<Value> {
    Some(Value::String(value.into()))
}

#[tokio::test]
async fn binary_and_streaming_queries_preserve_real_postgres_values() {
    let mut fixture = Fixture::open().await;
    let mut config = fixture.config.clone();
    config.read_only = true;
    let driver = PostgresDriver::new();
    let handle = driver.connect(&config).await.expect("driver connection");
    // Server casts form the independent oracle where the API promises text.
    // cast(cidr AS text) keeps the subnet even when it is the host width.
    let oracle = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    >(
        r#"SELECT
           '1234567890123456789012345.1200'::numeric::text,
           '-0.000010'::numeric::text, 'NaN'::numeric::text,
           '192.0.2.25/24'::inet::text, host('2001:db8::1'::inet),
           '203.0.113.9/32'::cidr::text,
           '08:00:2B:01:02:03'::macaddr::text,
           '08:00:2B:FF:FE:01:02:03'::macaddr8::text"#,
    )
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("independent value oracle");
    let sql = r#"SELECT
        9007199254740991::bigint AS safe_bound, 9007199254740992::bigint AS unsafe_high,
        (-9007199254740992)::bigint AS unsafe_low,
        23::integer AS int_value, 12::smallint AS small_value,
        1.25::real AS real_value, '-2.125'::double precision AS double_value,
        '1234567890123456789012345.1200'::numeric AS exact_amount,
        '-0.000010'::numeric AS tiny_negative, 'NaN'::numeric AS nan_amount,
        TRUE AS flag, '2024-02-29'::date AS leap_day, '12:34:56'::time AS raw_time,
        '2024-02-29 18:34:56'::timestamp AS raw_timestamp,
        '2024-02-29 18:34:56+02'::timestamptz AS zoned_timestamp,
        'e137a508-6655-46a1-8d96-f9b96063ab2a'::uuid AS uuid_value,
        '{"items":["雪",null],"enabled":true}'::jsonb AS json_document,
        '192.0.2.25/24'::inet AS ip4, '2001:db8::1'::inet AS ip6,
        '203.0.113.9/32'::cidr AS host_width_subnet,
        '08:00:2B:01:02:03'::macaddr AS mac_value, '08:00:2B:FF:FE:01:02:03'::macaddr8 AS mac8_value,
        '02:30:00'::interval AS duration, '00:00:00'::interval AS zero_duration,
        decode('00ff27', 'hex') AS binary_payload,
        ''::text AS empty_text, NULL::text AS absent_text,
        NULL::inet AS absent_ip, NULL::cidr AS absent_subnet,
        NULL::numeric AS absent_amount, NULL::interval AS absent_duration"#;
    let expected = vec![
        Some(Value::Integer(9_007_199_254_740_991)),
        string("9007199254740992"),
        string("-9007199254740992"),
        Some(Value::Integer(23)),
        Some(Value::Integer(12)),
        Some(Value::Float(1.25)),
        Some(Value::Float(-2.125)),
        string(oracle.0),
        string(oracle.1),
        string(oracle.2),
        Some(Value::Bool(true)),
        string("2024-02-29"),
        string("12:34:56"),
        string("2024-02-29 18:34:56"),
        string("2024-02-29T16:34:56+00:00"),
        string("e137a508-6655-46a1-8d96-f9b96063ab2a"),
        Some(Value::Json(json!({"items": ["雪", null], "enabled": true}))),
        string(oracle.3),
        string(oracle.4),
        string(oracle.5),
        string(oracle.6),
        string(oracle.7),
        string("02:30:00"),
        string("00:00:00"),
        Some(Value::Bytes(vec![0, 255, 39])),
        string(""),
        None,
        None,
        None,
        None,
        None,
    ];
    let materialized = driver
        .query(&handle, sql)
        .await
        .expect("typed driver query");
    assert_eq!(materialized.rows_affected, Some(1));
    assert_eq!(materialized.rows.len(), 1);
    assert_eq!(materialized.columns.len(), expected.len());
    for ((column, observed), expected) in materialized
        .columns
        .iter()
        .zip(&materialized.rows[0])
        .zip(&expected)
    {
        assert_eq!(
            observed, expected,
            "decoded PostgreSQL column {}",
            column.name
        );
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    driver
        .query_stream(&handle, sql, Some(1), {
            let events = Arc::clone(&events);
            Arc::new(move |event| events.lock().expect("event lock").push(event))
        })
        .await
        .expect("typed streaming query");
    let events = events.lock().expect("event lock").clone();
    assert!(matches!(
        events.first(), Some(QueryStreamEvent::StatementStart { index: 0, columns, .. })
        if columns.iter().map(|c| &c.name).eq(materialized.columns.iter().map(|c| &c.name))
    ));
    let streamed: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            QueryStreamEvent::Rows { index: 0, rows } => Some(rows.as_slice()),
            _ => None,
        })
        .flatten()
        .cloned()
        .collect();
    assert_eq!(streamed, vec![expected]);
    assert!(events.iter().any(|event| matches!(
        event,
        QueryStreamEvent::StatementEnd {
            index: 0,
            truncated: false,
            ..
        }
    )));
    assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    driver.disconnect(handle).await.expect("driver cleanup");
}

#[path = "postgres_live_e2e/admin.rs"]
mod admin;

#[path = "postgres_live_e2e/structure.rs"]
mod structure;

#[path = "postgres_live_e2e/migration.rs"]
mod migration;

#[path = "postgres_live_e2e/objects.rs"]
mod objects;

#[path = "postgres_live_e2e/sync.rs"]
mod sync;

#[path = "postgres_live_e2e/execution.rs"]
mod execution;
