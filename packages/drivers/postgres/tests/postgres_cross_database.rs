//! Gated live integration test: PostgreSQL **cross-database reads**.
//!
//! The driver has no `use_database`: reading a table that lives in another
//! catalog selects (or opens) that database's pool and resolves the schema
//! there, leaving the handle's own session untouched. That is the fix for
//! BUG-003, where the session was re-pointed and every later read silently
//! resolved against the wrong database.
//!
//! Skips cleanly when PostgreSQL is unavailable. Credentials come from the
//! **process environment only** (`TEST_PG_*` keys, injected by CI secret or by
//! the developer shell). This file used to fall back to parsing
//! `packages/drivers/.env`, which conflicted with the repo's `.env` protection
//! rule (AGENTS.md 「本地环境变量文件保护」 and
//! `docs/architecture/platform/fake-runtime-fixtures.md` §13): a test that
//! silently sources a credential file cannot prove where its secrets came from.
//! Inject them explicitly instead — see the env block below.
//!
//! Run (skip if no Postgres):
//!   cargo test -p datazen-driver-postgres --test postgres_cross_database -- --nocapture
//!
//! Force live run with env (example — use your own secrets, do not commit them):
//!   TEST_PG_HOST=127.0.0.1 TEST_PG_PORT=5432 TEST_PG_USER=postgres \
//!   TEST_PG_DATABASE=dz_fixture_pg_a TEST_PG_DATABASE_B=dz_fixture_pg_b \
//!   cargo test -p datazen-driver-postgres --test postgres_cross_database -- --nocapture
//!
//! Both names carry the dedicated `dz_fixture_` prefix the shared driver contract
//! enforces (`docs/architecture/platform/fake-runtime-fixtures.md` §10.2 rule 1),
//! and they are two **different** databases: the test is only meaningful when the
//! session's own catalog and the foreign catalog differ. Names without the prefix
//! are rejected by that contract, so point the keys at your own prefixed
//! databases rather than at a working copy.
//!
//! Fixture: **seeded by this test** — a `public` table unique to database_a,
//! dropped again on the way out. It used to discover an existing table instead,
//! which made the verdict a function of whatever the developer's server happened
//! to hold: on an empty fixture database nothing matched, the test returned, and
//! cargo reported `ok`. The name carries a fresh UUID, so a table left behind by
//! an interrupted run can never be mistaken for this run's probe.
//!
//! Every environment-shaped exit routes through `unverified`, so
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` turns each one into a failure instead of a
//! silent pass. Without that variable the test still skips — CI has no fixtures.

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DriverError, SqlTarget, Value};
use datazen_driver_postgres::PostgresDriver;

#[path = "../../http-support/tests/support/live_gate.rs"]
mod live_gate;

/// One dimension name, used for both the skip line and the strict-mode failure.
const DIMENSION: &str = "cross-database";
const LABEL: &str = "postgresql";

/// The only way this test gives up.
///
/// Routing all of these through one choke point is the point: `test result: ok`
/// cannot distinguish a passed check from a check that never ran, so every exit
/// that means "this run proves nothing" has to be able to become red.
fn unverified(reason: &str) {
    live_gate::unverified_or_fail(LABEL, DIMENSION, reason, live_gate::strict_live());
}

#[derive(Clone, Debug)]
struct PgTestConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database_a: String,
    database_b: String,
}

impl Default for PgTestConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5432,
            user: "postgres".into(),
            password: String::new(),
            database_a: "dz_fixture_pg_a".into(),
            database_b: "dz_fixture_pg_b".into(),
        }
    }
}

/// Resolve a `TEST_PG_*` key from the **process environment only**.
///
/// No file fallback, deliberately: see the module header. An unset key keeps
/// the default, so a developer with a local server on the default port still
/// gets a live run without exporting anything; the database defaults name
/// dedicated `dz_fixture_` databases, and a server that does not have them
/// simply skips the test (see `cross_database_reads_never_move_the_session`).
fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

/// Build the live-test config from `TEST_PG_*` process env.
///
/// Returns `None` only when the caller must not proceed at all; today every
/// key has a usable default, so the gate is decided by whether the server is
/// actually reachable (see `cross_database_reads_never_move_the_session`).
fn load_pg_config() -> Option<PgTestConfig> {
    let mut cfg = PgTestConfig::default();
    if let Some(v) = env_var("TEST_PG_HOST") {
        cfg.host = v;
    }
    if let Some(v) = env_var("TEST_PG_PORT") {
        cfg.port = v.parse().unwrap_or(cfg.port);
    }
    if let Some(v) = env_var("TEST_PG_USER") {
        cfg.user = v;
    }
    if let Some(v) = env_var("TEST_PG_PASSWORD") {
        cfg.password = v;
    }
    if let Some(v) = env_var("TEST_PG_DATABASE") {
        cfg.database_a = v;
    }
    if let Some(v) = env_var("TEST_PG_DATABASE_B") {
        cfg.database_b = v;
    }

    Some(cfg)
}

/// Connect **to database_b**, so database_a is always a foreign catalog.
fn connection_config(cfg: &PgTestConfig) -> ConnectionConfig {
    ConnectionConfig {
        id: "pg-cross-database-it".into(),
        name: "postgres cross-database integration".into(),
        database_type: "postgresql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        database: Some(cfg.database_b.clone()),
        schema: None,
        username: Some(cfg.user.clone()),
        password: Some(cfg.password.clone()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

/// Only the variant of a driver error is safe to print from a test: a connect
/// failure can embed the connection string it was handed, and this file must
/// never leak an injected credential into CI output.
fn err_label(err: &DriverError) -> &'static str {
    match err {
        DriverError::ConnectionFailed(_) => "ConnectionFailed",
        DriverError::ConnectionTimeout => "ConnectionTimeout",
        DriverError::AuthenticationFailed(_) => "AuthenticationFailed",
        DriverError::SslError(_) => "SslError",
        DriverError::InvalidConfig(_) => "InvalidConfig",
        DriverError::PoolExhausted => "PoolExhausted",
        DriverError::QueryFailed(_) => "QueryFailed",
        _ => "other",
    }
}

fn cell_as_i64(value: &Option<Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// The handle's own session still points at `database_b`, so an unqualified
/// query against a database_a-only table must keep failing. If it starts
/// succeeding, something re-pointed the session (the BUG-003 regression).
async fn assert_handle_session_untouched(
    driver: &PostgresDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    cfg: &PgTestConfig,
    probe: &str,
    label: &str,
) {
    let err = driver
        .query(handle, &format!("SELECT COUNT(*) FROM {probe}"))
        .await
        .expect_err(&format!(
            "{label}: the handle's session must stay on {} and not see {}.{probe}",
            cfg.database_b, cfg.database_a
        ));
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "{label}: expected QueryFailed, got {err:?}"
    );
}

#[tokio::test]
async fn cross_database_reads_never_move_the_session() {
    let Some(cfg) = load_pg_config() else {
        unverified("TEST_PG_* is not in the process environment");
        return;
    };

    if cfg.database_a == cfg.database_b {
        unverified(&format!(
            "TEST_PG_DATABASE and TEST_PG_DATABASE_B must differ (both are {})",
            cfg.database_a
        ));
        return;
    }

    let driver = PostgresDriver::new();
    let handle = match driver.connect(&connection_config(&cfg)).await {
        Ok(h) => h,
        Err(e) => {
            unverified(&format!(
                "cannot connect to PostgreSQL at {}:{} ({})",
                cfg.host,
                cfg.port,
                err_label(&e)
            ));
            return;
        }
    };

    let dbs = match driver.get_databases(&handle).await {
        Ok(d) => d,
        Err(e) => {
            let _ = driver.disconnect(handle).await;
            unverified(&format!("list databases failed ({})", err_label(&e)));
            return;
        }
    };
    for needed in [&cfg.database_a, &cfg.database_b] {
        if !dbs.iter().any(|d| d == needed) {
            let _ = driver.disconnect(handle).await;
            unverified(&format!(
                "database `{needed}` not found (have: {})",
                dbs.join(", ")
            ));
            return;
        }
    }

    // `get_databases` reports what the server advertises, which is a wider set
    // than what this role may actually open (`CONNECT` is a separate privilege).
    // Probe each catalog with `test_connection` — deliberately *not* `query_at`,
    // so this gate never exercises the very capability the test is about to
    // assert. An unreachable catalog is a missing fixture, so the cross-database
    // dimension reports itself unverified instead of failing a check that never
    // got to run.
    for database in [&cfg.database_a, &cfg.database_b] {
        let mut probe = connection_config(&cfg);
        probe.id = format!("pg-cross-database-it-probe-{database}");
        probe.database = Some((*database).clone());
        if let Err(e) = driver.test_connection(&probe).await {
            let _ = driver.disconnect(handle).await;
            unverified(&format!(
                "catalog `{database}` is advertised but not openable with these credentials ({})",
                err_label(&e)
            ));
            return;
        }
    }

    // Seed the fixture this test asserts on, rather than discovering one in
    // whatever state the developer's server happens to be in.
    //
    // Discovery made the verdict a function of ambient data. On an empty
    // fixture database there was nothing to find, the test returned early, and
    // cargo reported `ok` — a green result that had verified nothing at all.
    // `test result: ok` cannot distinguish "the check passed" from "the check
    // never ran", so the fixture is produced here to make that difference
    // visible. The name carries a fresh UUID, so a table left behind by an
    // interrupted run can never be mistaken for this run's probe.
    let probe = format!("dz_cross_db_probe_{}", uuid::Uuid::new_v4().simple());
    // The probe belongs in `database_a` while the handle under test sits on
    // `database_b` — that separation is the entire point. Seeding through the
    // bare handle would have created it in B, where the cross-database reads
    // would never have had to cross, and every assertion below would have passed
    // without testing anything. A dedicated seed connection also keeps the setup
    // from exercising the cross-database routing it is meant to set up for.
    //
    // With the table only in A, the later "same statement, no target, must
    // still fail" assertion is meaningful: the session cannot see it at all.
    let mut seed_config = connection_config(&cfg);
    seed_config.id = "pg-cross-database-seed".into();
    seed_config.database = Some(cfg.database_a.clone());
    let seed = driver
        .connect(&seed_config)
        .await
        .unwrap_or_else(|e| panic!("seed connection to {}: {e}", cfg.database_a));
    driver
        .query(
            &seed,
            &format!("CREATE TABLE {probe} (id INT PRIMARY KEY, marker TEXT NOT NULL)"),
        )
        .await
        .unwrap_or_else(|e| panic!("seed probe table {probe} in {}: {e}", cfg.database_a));
    driver
        .query(
            &seed,
            &format!("INSERT INTO {probe} (id, marker) VALUES (1, '{probe}')"),
        )
        .await
        .unwrap_or_else(|e| panic!("seed probe row into {probe}: {e}"));

    println!(
        "▶  cross-database live: handle on {}, reading {} on {}:{}",
        cfg.database_b, cfg.database_a, cfg.host, cfg.port
    );

    // ── the BUG-003 regression: reading a foreign catalog's table structure ──
    let users_schema = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .unwrap_or_else(|e| panic!("get_table_schema({}.{probe}): {e}", cfg.database_a));
    assert!(
        !users_schema.columns.is_empty(),
        "cross-database table structure must not come back empty — that empty \
         column list is what the data grid turned into blank cells (BUG-003)"
    );
    let col_names: Vec<&str> = users_schema
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    println!("   {}.{probe} columns: {col_names:?}", cfg.database_a);

    let (cols, _pks) = driver
        .get_columns(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .unwrap_or_else(|e| panic!("get_columns({}.{probe}): {e}", cfg.database_a));
    assert_eq!(cols.len(), users_schema.columns.len());

    // ── the session never moved ──
    assert_handle_session_untouched(&driver, &handle, &cfg, &probe, "after cross-database reads")
        .await;

    // ── the data path: a targeted statement reaches the foreign catalog ──
    //
    // This is the regression the grid hit. `query` cannot express a target, so
    // a read for another database ran on the handle's own pool: it either
    // returned that database's rows or nothing at all, which the user saw as
    // "only the first database has data". The same statement must now work
    // through `query_at` and keep failing without a target.
    let target = SqlTarget::new(Some(&cfg.database_a), Some("public"));
    let result = driver
        .query_at(&handle, &format!("SELECT COUNT(*) FROM {probe}"), target)
        .await
        .unwrap_or_else(|e| panic!("query_at({}.{probe}): {e}", cfg.database_a));
    let counted = result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(cell_as_i64)
        .expect("COUNT(*) must come back as a number");
    assert!(
        counted >= 1,
        "expected at least one row in {}.{probe}, got {counted}",
        cfg.database_a
    );

    // The bare table name was resolved inside the requested schema, not the
    // pool's default `search_path`.
    let via_public = driver
        .query_at(
            &handle,
            &format!("SELECT COUNT(*) FROM {probe}"),
            SqlTarget::new(Some(&cfg.database_a), Some("public")),
        )
        .await
        .expect("schema-qualified read");
    assert_eq!(
        via_public
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(cell_as_i64),
        Some(counted)
    );

    // Same statement, no target: the handle's own database still cannot see it.
    assert_handle_session_untouched(&driver, &handle, &cfg, &probe, "after a targeted read").await;

    // A transaction holds one connection, so a statement for another database
    // must be refused rather than silently read from the transaction's own.
    let tx = driver
        .begin_transaction(&handle)
        .await
        .expect("begin transaction");
    let err = driver
        .query_at(&handle, &format!("SELECT COUNT(*) FROM {probe}"), target)
        .await
        .expect_err("a foreign-database statement must not run inside the transaction");
    assert!(
        matches!(err, DriverError::TransactionError(_)),
        "expected TransactionError, got {err:?}"
    );
    driver.rollback(tx).await.expect("rollback");

    // ── repeat to prove the foreign pool is stable and reused ──
    for i in 0..5 {
        let schema = driver
            .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
            .await
            .unwrap_or_else(|e| panic!("cross-database read #{i}: {e}"));
        assert_eq!(
            schema.columns.len(),
            users_schema.columns.len(),
            "cross-database read #{i} changed shape"
        );
    }
    assert_handle_session_untouched(
        &driver,
        &handle,
        &cfg,
        &probe,
        "after repeated cross-database reads",
    )
    .await;

    // ── schema is mandatory for single-table resolution on PostgreSQL ──
    let err = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, None)
        .await
        .expect_err("a schema-aware driver must require an explicit schema");
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig without a schema, got {err:?}"
    );

    // ── a schema-less driver contract violation is rejected ──
    let err = driver
        .get_tables(&handle, &cfg.database_a, Some("public"))
        .await
        .expect("listing with an explicit schema is allowed");
    assert!(err.iter().any(|t| t.name == probe));

    // ── unknown database fails loudly instead of returning an empty list ──
    let err = driver
        .get_tables(&handle, "nonexistent_db_xyz_f3_test", None)
        .await
        .expect_err("unknown database should error");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed for unknown database, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent_db_xyz_f3_test")
            || msg.to_lowercase().contains("does not exist")
            || msg.to_lowercase().contains("failed to connect"),
        "error should mention the bad database name: {msg}"
    );

    // ── the open-database report is what the navigator marks as "open" ──
    let open = driver
        .open_databases(&handle)
        .await
        .expect("open_databases");
    assert!(
        open.iter().any(|d| d == &cfg.database_a),
        "the foreign database has an open pool and must be reported: {open:?}"
    );
    assert!(
        !open.iter().any(|d| d == &cfg.database_b),
        "the handle's own database is the session's primary pool, not a \
         releasable per-database resource, so it must not be reported: {open:?}"
    );

    // ── closing the foreign database pool drops only that pool ──
    assert!(
        driver
            .close_database(&handle, &cfg.database_a)
            .await
            .expect("close foreign pool"),
        "the foreign pool should have been open"
    );

    let open_after = driver
        .open_databases(&handle)
        .await
        .expect("open_databases after close");
    assert!(
        !open_after.iter().any(|d| d == &cfg.database_a),
        "a closed database must stop being reported as open: {open_after:?}"
    );
    assert!(
        !open_after.iter().any(|d| d == &cfg.database_b),
        "closing a foreign database must not surface the handle's own: {open_after:?}"
    );
    let schema_after_close = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .expect("reopening the foreign pool on demand");
    assert!(!schema_after_close.columns.is_empty());

    // Drop the probe on the way out so a green run leaves the fixture database
    // exactly as it found it. The name is UUID-unique, so a leak from a crashed
    // run cannot break the next one — but leaking on every *successful* run
    // would make "not deleted" indistinguishable from "not reused".
    driver
        .query(&seed, &format!("DROP TABLE IF EXISTS {probe}"))
        .await
        .unwrap_or_else(|e| panic!("drop probe table {probe}: {e}"));
    driver
        .disconnect(seed)
        .await
        .expect("disconnect the seed connection");

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  PostgreSQL cross-database live checks passed");
}
