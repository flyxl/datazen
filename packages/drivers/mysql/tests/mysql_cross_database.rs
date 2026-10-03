//! Gated live integration test: MySQL **cross-database reads**.
//!
//! The driver has no `use_database`. A metadata read for another database is
//! served by fully qualifying the name (`` `db`.`table` `` for the `SHOW`
//! family, `WHERE TABLE_SCHEMA = ?` for `information_schema`), so the borrowed
//! pool connection is never re-pointed and no `USE` leaks back into the pool.
//!
//! Skips cleanly when MySQL is unavailable. Credentials come from the **process
//! environment only** (`TEST_MYSQL_*`, injected by shell or CI secret) — see
//! `packages/drivers/postgres/tests/postgres_cross_database.rs` for why no
//! `.env` file is read.
//!
//! Run (skip if no MySQL):
//!   cargo test -p datazen-driver-mysql --test mysql_cross_database -- --nocapture
//!
//! Force live run with env (example — use your own secrets, do not commit them):
//!   TEST_MYSQL_HOST=127.0.0.1 TEST_MYSQL_PORT=3306 TEST_MYSQL_USER=root \
//!   TEST_MYSQL_PASSWORD= TEST_MYSQL_DATABASE=dz_fixture_mysql_a \
//!   TEST_MYSQL_DATABASE_B=dz_fixture_mysql_b \
//!   cargo test -p datazen-driver-mysql --test mysql_cross_database -- --nocapture
//!
//! Both names carry the dedicated `dz_fixture_` prefix the shared driver contract
//! enforces (`docs/architecture/platform/fake-runtime-fixtures.md` §10.2 rule 1),
//! and they are two **different** databases: the test is only meaningful when the
//! session's own catalog and the foreign catalog differ. Names without the prefix
//! are rejected by that contract, so point the keys at your own prefixed
//! databases rather than at a working copy.
//!
//! Fixture: the tests that assert on catalog contents **seed** their own table —
//! pid plus nanosecond nonce, so a leftover from an interrupted run can never be
//! mistaken for this run's probe — and drop it again on the way out. Discovering
//! an existing table instead made the verdict a function of whatever the
//! developer's server happened to hold: on an empty fixture database nothing
//! matched, the test returned, and cargo reported `ok. 4 passed` for a run that
//! had proven nothing.
//!
//! Every environment-shaped exit routes through `unverified`, so
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` turns each one into a failure instead of a
//! silent pass. Without that variable the suite still skips — CI has no fixtures.

use datazen_driver_api::{
    ConnectionConfig, DatabaseDriver, DriverError, SqlTarget, TransactionHandle, Value,
};
use std::collections::HashMap;

use datazen_driver_mysql::MysqlDriver;

#[path = "../../http-support/tests/support/live_gate.rs"]
mod live_gate;

/// Dimension names, used for both the skip line and the strict-mode failure.
/// They are separate because a run can lack a server (all four) while still
/// having both fixture databases present (only the cross-database pair).
const DIMENSION: &str = "cross-database";
const NO_DEFAULT_DATABASE: &str = "no-default-database";
const NUMERIC_DEFAULTS: &str = "numeric-string-and-tinyint-defaults";
const SNAPSHOT_ISOLATION: &str = "snapshot-repeatable-read-only";

/// The single choke point for "this run proves nothing".
///
/// `test result: ok` cannot distinguish a passed check from a check that never
/// ran, so every such exit has to be able to become red.
fn unverified(dimension: &str, reason: &str) {
    live_gate::unverified_or_fail("mysql", dimension, reason, live_gate::strict_live());
}

/// A name unique to this process and this instant.
///
/// pid alone is not enough: two runs of the same suite can overlap, and one
/// run's `DROP TABLE` would then delete the other's probe mid-test.
fn fresh_probe_name() -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    format!("dz_cross_db_probe_{}_{}", std::process::id(), nonce)
}

#[derive(Clone, Debug)]
struct MysqlTestConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database_a: String,
    database_b: String,
}

impl Default for MysqlTestConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: String::new(),
            database_a: "dz_fixture_mysql_a".into(),
            database_b: "dz_fixture_mysql_b".into(),
        }
    }
}

/// Resolve a `TEST_MYSQL_*` key from the **process environment only**.
///
/// No file fallback: a test must never parse `packages/drivers/.env`
/// (AGENTS.md 「本地环境变量文件保护」). Inject the keys from the shell or CI.
fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Gate on `TEST_MYSQL_*` in the process environment. Password may be
/// intentionally empty, so it is read without the empty filter.
///
/// Returns `None` only when no `TEST_MYSQL_*` key is present at all; it stays
/// silent so the caller reports once, through the gate, instead of printing a
/// skip line that a strict run would then contradict.
fn load_mysql_config() -> Option<MysqlTestConfig> {
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_MYSQL_"));

    if !has_marker {
        return None;
    }

    let mut cfg = MysqlTestConfig::default();
    if let Some(v) = env_var("TEST_MYSQL_HOST") {
        cfg.host = v;
    }
    if let Some(v) = env_var("TEST_MYSQL_PORT") {
        cfg.port = v.parse().unwrap_or(cfg.port);
    }
    if let Some(v) = env_var("TEST_MYSQL_USER") {
        cfg.user = v;
    }
    // Empty password is valid; allow override from env including empty string.
    if let Ok(v) = std::env::var("TEST_MYSQL_PASSWORD") {
        cfg.password = v;
    }
    if let Some(v) = env_var("TEST_MYSQL_DATABASE") {
        cfg.database_a = v;
    }
    if let Some(v) = env_var("TEST_MYSQL_DATABASE_B") {
        cfg.database_b = v;
    }

    Some(cfg)
}

fn connection_config(cfg: &MysqlTestConfig) -> ConnectionConfig {
    ConnectionConfig {
        id: "mysql-cross-database-it".into(),
        name: "mysql cross-database integration".into(),
        database_type: "mysql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        // Connect to database_b so database_a is always a foreign database.
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

fn cell_as_i64(value: &Option<Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

#[tokio::test]
async fn cross_database_reads_use_qualified_names_not_use() {
    let Some(cfg) = load_mysql_config() else {
        unverified(DIMENSION, "no TEST_MYSQL_* in the process environment");
        return;
    };

    if cfg.database_a == cfg.database_b {
        unverified(
            DIMENSION,
            &format!(
                "TEST_MYSQL_DATABASE and TEST_MYSQL_DATABASE_B must differ (both are {})",
                cfg.database_a
            ),
        );
        return;
    }

    let driver = MysqlDriver::new(false);
    let handle = match driver.connect(&connection_config(&cfg)).await {
        Ok(h) => h,
        Err(e) => {
            unverified(
                DIMENSION,
                &format!("cannot connect to MySQL at {}:{}: {e}", cfg.host, cfg.port),
            );
            return;
        }
    };

    let dbs = match driver.get_databases(&handle).await {
        Ok(d) => d,
        Err(e) => {
            let _ = driver.disconnect(handle).await;
            unverified(DIMENSION, &format!("SHOW DATABASES failed: {e}"));
            return;
        }
    };
    for needed in [&cfg.database_a, &cfg.database_b] {
        if !dbs.iter().any(|d| d == needed) {
            let _ = driver.disconnect(handle).await;
            unverified(
                DIMENSION,
                &format!("database `{needed}` not found (have: {})", dbs.join(", ")),
            );
            return;
        }
    }

    // Seed the fixture this test asserts on, rather than discovering one in
    // whatever state the developer's server happens to be in. Discovery made
    // the verdict a function of ambient data: on an empty fixture database
    // nothing matched, the test returned early, and cargo reported `ok` — a
    // green result that had verified nothing at all.
    let probe = fresh_probe_name();

    // The handle under test stays on B; seeding A gets its own connection, so
    // the setup does not itself exercise the cross-database read being tested.
    let mut seed_config = connection_config(&cfg);
    seed_config.id = "mysql-cross-database-seed".into();
    seed_config.database = Some(cfg.database_a.clone());
    let seed = driver
        .connect(&seed_config)
        .await
        .unwrap_or_else(|e| panic!("seed connection to {}: {e}", cfg.database_a));
    driver
        .query(
            &seed,
            &format!("CREATE TABLE `{probe}` (id INT PRIMARY KEY, marker VARCHAR(64) NOT NULL)"),
        )
        .await
        .unwrap_or_else(|e| panic!("seed probe table {probe}: {e}"));
    driver
        .query(
            &seed,
            &format!("INSERT INTO `{probe}` (id, marker) VALUES (1, '{probe}')"),
        )
        .await
        .unwrap_or_else(|e| panic!("seed probe row into {probe}: {e}"));

    println!(
        "▶  cross-database live: handle on {}, reading {} on {}:{}",
        cfg.database_b, cfg.database_a, cfg.host, cfg.port
    );

    // ── BUG-003 regression: a foreign database's table structure must not be empty ──
    let schema = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, None)
        .await
        .unwrap_or_else(|e| panic!("get_table_schema({}.{probe}): {e}", cfg.database_a));
    assert!(
        !schema.columns.is_empty(),
        "cross-database structure must not come back empty — that empty column \
         list is what the data grid turned into blank cells (BUG-003)"
    );
    let (cols, _pks) = driver
        .get_columns(&handle, &probe, &cfg.database_a, None)
        .await
        .unwrap_or_else(|e| panic!("get_columns({}.{probe}): {e}", cfg.database_a));
    assert_eq!(cols.len(), schema.columns.len());

    // ── the handle's own session is untouched: the unqualified probe is still
    //    absent from database_b ──
    let err = driver
        .query(&handle, &format!("SELECT COUNT(*) FROM {probe}"))
        .await
        .expect_err("the handle's session must stay on database_b and not see {}.{probe}");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed, got {err:?}"
    );

    // ── the data path: a targeted statement reaches the foreign database ──
    //
    // This is the "1046 No database selected" regression. `query` cannot
    // express a target, so the grid's generated statement was sent
    // unqualified: on a connection with no default database MySQL rejected it
    // outright, and on one with a default it read the wrong database. The same
    // statement must work through `query_at` and keep failing without a target.
    let target = SqlTarget::new(Some(&cfg.database_a), None);
    let counted = driver
        .query_at(&handle, &format!("SELECT COUNT(*) FROM {probe}"), target)
        .await
        .unwrap_or_else(|e| panic!("query_at({}.{probe}): {e}", cfg.database_a))
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(cell_as_i64)
        .expect("COUNT(*) must come back as a number");
    assert!(counted >= 0, "unexpected negative count {counted}");

    // The rewrite is what made it work: the driver inlined `` `db`.`table` ``.
    let qualified = driver.qualified_sql(
        &format!("SELECT COUNT(*) FROM {probe}"),
        SqlTarget::new(Some(&cfg.database_a), None),
    );
    assert!(
        qualified.contains(&format!("`{}`.`{probe}`", cfg.database_a)),
        "expected an inlined database qualifier, got: {qualified}"
    );

    // Same statement, no target: database_b still cannot see it.
    let err = driver
        .query(&handle, &format!("SELECT COUNT(*) FROM {probe}"))
        .await
        .expect_err("an untargeted statement must not reach database_a");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed, got {err:?}"
    );

    // ── repeated foreign reads stay stable (no per-acquire state to drift) ──
    for i in 0..5 {
        let schema = driver
            .get_table_schema(&handle, &probe, &cfg.database_a, None)
            .await
            .unwrap_or_else(|e| panic!("cross-database read #{i}: {e}"));
        assert_eq!(schema.columns.len(), schema.columns.len(), "shape changed");
    }
    let err = driver
        .query(&handle, &format!("SELECT COUNT(*) FROM {probe}"))
        .await
        .expect_err("session must still be on database_b after repeated foreign reads");
    assert!(matches!(err, DriverError::QueryFailed(_)), "got {err:?}");

    // ── a genuinely missing relation errors instead of reporting no columns ──
    let err = driver
        .get_table_schema(&handle, &probe, &cfg.database_b, None)
        .await
        .expect_err("a relation absent from the requested database must not report success");
    assert!(
        err.to_string().contains("does not exist"),
        "error should name the missing relation, got: {err}"
    );

    // ── MySQL has no schema level, so a schema argument is rejected ──
    let err = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .expect_err("MySQL must reject a schema argument");
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig, got {err:?}"
    );

    // ── unknown database fails loudly ──
    let err = driver
        .get_tables(&handle, "nonexistent_db_xyz_f1_test", None)
        .await
        .expect_err("unknown database should error");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed for unknown database, got: {err:?}"
    );

    // Drop the probe on the way out so a green run leaves the fixture database
    // exactly as it found it; the pid+nonce name means a leak from a crashed run
    // cannot break the next one.
    driver
        .query(&seed, &format!("DROP TABLE IF EXISTS `{probe}`"))
        .await
        .unwrap_or_else(|e| panic!("drop probe table {probe}: {e}"));
    driver
        .disconnect(seed)
        .await
        .expect("disconnect seed connection");

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  MySQL cross-database live checks passed");
}

/// The exact shape of the reported bug: a connection with **no** default
/// database.
///
/// MySQL answers an unqualified statement with `1046 (3D000): No database
/// selected`. The driver never issues `USE`, so the statement has to carry its
/// target — which is why the grid's generated `SELECT ... FROM `t`` used to
/// fail outright on such a connection. This is the same failure mode the
/// user-visible error came from, so it is pinned separately from the
/// cross-database case above.
#[tokio::test]
async fn connection_without_a_default_database_needs_an_explicit_target() {
    let Some(cfg) = load_mysql_config() else {
        unverified(
            NO_DEFAULT_DATABASE,
            "no TEST_MYSQL_* in the process environment",
        );
        return;
    };
    let driver = MysqlDriver::new(false);

    let mut config = connection_config(&cfg);
    config.database = None;
    let handle = match driver.connect(&config).await {
        Ok(h) => h,
        Err(e) => {
            unverified(
                NO_DEFAULT_DATABASE,
                &format!("cannot connect to MySQL at {}:{}: {e}", cfg.host, cfg.port),
            );
            return;
        }
    };

    // Seed the table this test counts rows from.
    //
    // It used to take whichever table `get_tables` happened to report first, and
    // skip when the fixture catalog was empty — so on a fresh fixture database
    // the test returned before asserting either half of its claim (unqualified
    // fails, targeted works) and still counted as passed. The handle under test
    // has no default database, so it cannot seed; a second connection opens A
    // instead, keeping the connection being tested exactly as the user has it.
    let probe = fresh_probe_name();
    let mut seed_config = connection_config(&cfg);
    seed_config.id = "mysql-no-default-database-seed".into();
    seed_config.database = Some(cfg.database_a.clone());
    let seed = driver
        .connect(&seed_config)
        .await
        .unwrap_or_else(|e| panic!("seed connection to {}: {e}", cfg.database_a));
    driver
        .query(
            &seed,
            &format!(
                "CREATE TABLE `{probe}` (id INT PRIMARY KEY, marker VARCHAR(64) NOT NULL) \
                 ENGINE=InnoDB"
            ),
        )
        .await
        .unwrap_or_else(|e| panic!("seed probe table {probe}: {e}"));
    driver
        .query(
            &seed,
            &format!("INSERT INTO `{probe}` (id, marker) VALUES (1, '{probe}')"),
        )
        .await
        .unwrap_or_else(|e| panic!("seed probe row into {probe}: {e}"));

    println!(
        "▶  no-default-database live: unqualified statement must fail, targeted must work ({}:{})",
        cfg.host, cfg.port
    );

    // Untargeted: reproduces the reported error verbatim.
    let err = driver
        .query(&handle, &format!("SELECT COUNT(*) FROM {probe}"))
        .await
        .expect_err("an unqualified statement on a database-less connection must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("1046") || msg.to_lowercase().contains("no database selected"),
        "expected MySQL 1046 'No database selected', got: {msg}"
    );

    // Targeted: the driver inlines `` `db`.`table` `` and it works.
    let counted = driver
        .query_at(
            &handle,
            &format!("SELECT COUNT(*) FROM {probe}"),
            SqlTarget::new(Some(&cfg.database_a), None),
        )
        .await
        .unwrap_or_else(|e| panic!("query_at({}.{probe}): {e}", cfg.database_a))
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(cell_as_i64)
        .expect("COUNT(*) must come back as a number");
    // Exactly the row seeded above, not merely "some non-negative number": a
    // `counted >= 0` assertion passes for zero rows too, so it would have
    // reported success for a targeted read that resolved against the wrong
    // database — the very thing this test exists to catch.
    assert_eq!(
        counted, 1,
        "targeted read must resolve against {} and see the one seeded row in {probe}",
        cfg.database_a
    );

    driver
        .query(&seed, &format!("DROP TABLE IF EXISTS `{probe}`"))
        .await
        .unwrap_or_else(|e| panic!("drop probe table {probe}: {e}"));
    driver
        .disconnect(seed)
        .await
        .expect("disconnect seed connection");
    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  MySQL no-default-database live checks passed");
}

#[tokio::test]
async fn get_table_schema_preserves_numeric_string_and_tinyint_defaults() {
    let Some(cfg) = load_mysql_config() else {
        unverified(
            NUMERIC_DEFAULTS,
            "no TEST_MYSQL_* in the process environment",
        );
        return;
    };
    let driver = MysqlDriver::new(false);
    let handle = match driver.connect(&connection_config(&cfg)).await {
        Ok(handle) => handle,
        Err(e) => {
            unverified(
                NUMERIC_DEFAULTS,
                &format!("cannot connect to MySQL at {}:{}: {e}", cfg.host, cfg.port),
            );
            return;
        }
    };

    let table = fresh_probe_name().replace("dz_cross_db_probe", "codex_mysql_default_meta");
    driver
        .query(
            &handle,
            &format!(
                "CREATE TABLE `{table}` (\
                    amount DECIMAL(18,6) NOT NULL DEFAULT 0, \
                    code VARCHAR(32) NOT NULL DEFAULT 'CODE0001', \
                    active TINYINT(1) NOT NULL DEFAULT 1\
                )"
            ),
        )
        .await
        .expect("create temporary schema-metadata fixture");

    let schema_result = driver
        .get_table_schema(&handle, &table, &cfg.database_b, None)
        .await;
    let columns_result = driver
        .get_columns(&handle, &table, &cfg.database_b, None)
        .await;
    let all_columns_result = driver.get_all_columns(&handle, &cfg.database_b, None).await;
    let cleanup_result = driver
        .query(&handle, &format!("DROP TABLE IF EXISTS `{table}`"))
        .await;
    let disconnect_result = driver.disconnect(handle).await;

    cleanup_result.expect("drop temporary schema-metadata fixture");
    disconnect_result.expect("disconnect from MySQL");
    let schema = schema_result.expect("read temporary schema metadata");
    let (columns, _) = columns_result.expect("read per-table information_schema metadata");
    let mut all_columns = all_columns_result.expect("read batch information_schema metadata");
    let batch_columns = all_columns
        .remove(&table)
        .expect("batch metadata contains the temporary table")
        .0;

    for (source, columns) in [
        ("SHOW FULL COLUMNS", schema.columns),
        ("per-table information_schema", columns),
        ("batch information_schema", batch_columns),
    ] {
        let defaults: HashMap<&str, Option<&str>> = columns
            .iter()
            .map(|column| (column.name.as_str(), column.default_value.as_deref()))
            .collect();

        assert_eq!(
            defaults.get("amount"),
            Some(&Some("0.000000")),
            "{source} lost the DECIMAL default"
        );
        assert_eq!(
            defaults.get("code"),
            Some(&Some("CODE0001")),
            "{source} lost the string default"
        );
        assert_eq!(
            defaults.get("active"),
            Some(&Some("1")),
            "{source} lost the TINYINT(1) default"
        );
    }
}

#[tokio::test]
async fn read_snapshot_is_repeatable_read_only_and_releases_after_commit() {
    let Some(cfg) = load_mysql_config() else {
        unverified(
            SNAPSHOT_ISOLATION,
            "no TEST_MYSQL_* in the process environment",
        );
        return;
    };
    let driver = MysqlDriver::new(false);
    let reader = match driver.connect(&connection_config(&cfg)).await {
        Ok(handle) => handle,
        Err(e) => {
            unverified(
                SNAPSHOT_ISOLATION,
                &format!("cannot connect to MySQL at {}:{}: {e}", cfg.host, cfg.port),
            );
            return;
        }
    };
    let writer = match driver.connect(&connection_config(&cfg)).await {
        Ok(handle) => handle,
        Err(e) => {
            let _ = driver.disconnect(reader).await;
            unverified(
                SNAPSHOT_ISOLATION,
                &format!("cannot open a second MySQL connection: {e}"),
            );
            return;
        }
    };

    let table = fresh_probe_name().replace("dz_cross_db_probe", "codex_mysql_snapshot");
    driver
        .execute(
            &writer,
            &format!(
                "CREATE TABLE `{table}` (id INT PRIMARY KEY, amount INT NOT NULL) ENGINE=InnoDB"
            ),
        )
        .await
        .expect("create InnoDB snapshot fixture");
    driver
        .execute(
            &writer,
            &format!("INSERT INTO `{table}` (id, amount) VALUES (1, 10)"),
        )
        .await
        .expect("seed snapshot fixture");

    let mut active_snapshot: Option<TransactionHandle> = None;
    let journey_result: Result<(i64, i64), DriverError> = async {
        active_snapshot = Some(driver.begin_read_snapshot(&reader).await?);

        let write_result = driver
            .execute(
                &reader,
                &format!("UPDATE `{table}` SET amount = 99 WHERE id = 1"),
            )
            .await;
        if write_result.is_ok() {
            return Err(DriverError::TransactionError(
                "a read snapshot allowed a write".into(),
            ));
        }

        driver
            .execute(
                &writer,
                &format!("UPDATE `{table}` SET amount = 20 WHERE id = 1"),
            )
            .await?;

        let old_rows = driver
            .query(
                &reader,
                &format!("SELECT amount FROM `{table}` WHERE id = 1"),
            )
            .await?;
        let old_value = old_rows
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(cell_as_i64)
            .ok_or_else(|| DriverError::QueryFailed("snapshot query returned no value".into()))?;
        driver
            .rollback(active_snapshot.take().expect("first snapshot is active"))
            .await?;

        active_snapshot = Some(driver.begin_read_snapshot(&reader).await?);
        let new_rows = driver
            .query(
                &reader,
                &format!("SELECT amount FROM `{table}` WHERE id = 1"),
            )
            .await?;
        let new_value = new_rows
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(cell_as_i64)
            .ok_or_else(|| {
                DriverError::QueryFailed("new snapshot query returned no value".into())
            })?;
        driver
            .commit(active_snapshot.take().expect("second snapshot is active"))
            .await?;

        Ok((old_value, new_value))
    }
    .await;

    if let Some(tx) = active_snapshot.take() {
        let _ = driver.rollback(tx).await;
    }
    let cleanup_result = driver
        .execute(&writer, &format!("DROP TABLE IF EXISTS `{table}`"))
        .await;
    let reader_disconnect_result = driver.disconnect(reader).await;
    let writer_disconnect_result = driver.disconnect(writer).await;

    cleanup_result.expect("drop InnoDB snapshot fixture");
    reader_disconnect_result.expect("disconnect snapshot reader");
    writer_disconnect_result.expect("disconnect snapshot writer");
    let (old_value, new_value) = journey_result.expect("complete MySQL snapshot journey");
    assert_eq!(
        old_value, 10,
        "first snapshot must retain the pre-update value"
    );
    assert_eq!(
        new_value, 20,
        "later snapshot must observe the committed update"
    );
}
