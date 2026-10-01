//! Live coverage for the SQL Server driver's **write path, transactions and DDL
//! object lifecycle** against a real Azure SQL Database instance.
//!
//! Configuration comes from the **process environment only**. A local env
//! file is read *only* when you opt in by naming it:
//!
//! ```text
//! TEST_SQLSERVER_ENV_FILE=/path/to/your/env/file cargo test -p datazen-driver-sqlserver
//! ```
//!
//! Regression focus: T-SQL only accepts the programmable-object definitions
//! (`CREATE`/`ALTER` `SCHEMA`, `VIEW`, `PROCEDURE`, `FUNCTION`, `TRIGGER`,
//! `RULE`, `DEFAULT`) as the **first statement of a batch**. tiberius sends
//! `Client::query`/`Client::execute` through the `sp_executesql` RPC, which the
//! server rejects with error 156 (`Incorrect syntax near the keyword 'SCHEMA'`).
//! `SqlServerDriver` now routes those statements through a real batch
//! (`simple_query`). Every lifecycle below therefore drives the objects through
//! `driver.execute(...)` — the public path the fix repaired — and not through a
//! hand-written `EXEC`.
//!
//! Harness contract: `live_config()` returns `None` (no instance) and
//! `write_allowed()` returns `false` (no write switch) **skip** the test; they
//! never fail it. Every object is a `dz_test_*` scratch name owned by this
//! process (`LiveConfig::scratch` embeds the pid) and is dropped again even when
//! an assertion panics — see [`with_cleanup`], which also verifies through the
//! catalog that nothing it created is left behind.
//!
//! Run from the repository root:
//! ```text
//! cargo test -p datazen-driver-sqlserver --test live_write_and_ddl -- --nocapture --test-threads=2
//! ```

mod common;

use std::sync::Arc;

use common::{
    cell, cell_i64, cell_string, connect, drop_quietly, live_config, scalar, with_resume_retry,
    write_allowed, LiveConfig,
};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver as _, DriverError, Value};
use datazen_driver_sqlserver::SqlServerDriver;
use futures_util::FutureExt as _;
use serde_json::json;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Bracket-quote a T-SQL identifier (T-SQL escapes `]` as `]]`).
fn qi(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// `[schema].[name]`.
fn qn(schema: &str, name: &str) -> String {
    format!("{}.{}", qi(schema), qi(name))
}

/// `[dbo].[name]` — the scratch schema every test uses unless it creates one.
fn dbo(name: &str) -> String {
    format!("[dbo].{}", qi(name))
}

/// Marker that ties a scratch name to this test binary's process. A catalog
/// sweep based on it can never collide with another agent's concurrently
/// running test binary (their `LiveConfig::scratch` embeds another pid).
fn pid_marker() -> String {
    format!("_{}_", std::process::id())
}

/// Defensive cleanup prelude: never leave a transaction open on the session.
fn rollback_if_open() -> String {
    "IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION".to_string()
}

/// `SELECT COUNT(*)` probe over any object kind in `sys.objects`
/// (tables, views, procedures, functions, triggers, sequences).
fn object_probe(qualified: &str) -> String {
    format!("SELECT COUNT(*) FROM sys.objects WHERE object_id = OBJECT_ID(N'{qualified}')")
}

/// `SELECT COUNT(*)` probe for a schema.
fn schema_probe(schema: &str) -> String {
    format!("SELECT COUNT(*) FROM sys.schemas WHERE name = N'{schema}'")
}

/// `SELECT COUNT(*)` probe for an alias/CLR user-defined type.
fn type_probe(name: &str) -> String {
    format!("SELECT COUNT(*) FROM sys.types WHERE name = N'{name}' AND is_user_defined = 1")
}

/// `SELECT COUNT(*)` probe for a sequence (also present in `sys.objects`).
fn sequence_probe(name: &str) -> String {
    format!(
        "SELECT COUNT(*) FROM sys.sequences WHERE object_id = OBJECT_ID(N'{}')",
        dbo(name)
    )
}

/// Run `body` on a fresh connection and then always drop the caller's scratch
/// objects — including when an assertion panics (the panic is re-raised after
/// the cleanup, so a failure is never swallowed).
///
/// `probes` are `SELECT COUNT(*) …` catalog queries that must return 0 once the
/// cleanup ran; a non-zero count fails the test, so a test can never silently
/// leave a `dz_test_*` object behind. The probes are the authoritative cleanup
/// assertion: they are scoped to the exact objects this test created, so they
/// stay correct while other agents run their own live suites in parallel.
async fn with_cleanup<F, Fut>(
    cfg: &LiveConfig,
    label: &str,
    cleanup: Vec<String>,
    probes: Vec<String>,
    body: F,
) where
    F: FnOnce(Arc<SqlServerDriver>, ConnectionHandle) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (driver, handle) = connect(cfg).await;
    let outcome = std::panic::AssertUnwindSafe(body(Arc::clone(&driver), handle.clone()))
        .catch_unwind()
        .await;
    if outcome.is_err() {
        eprintln!("⚠️  {label}: body failed, running cleanup anyway");
    }
    for sql in cleanup {
        drop_quietly(&driver, &handle, &sql).await;
    }

    let mut leftovers: Vec<String> = Vec::new();
    for probe in probes {
        match scalar(&driver, &handle, &probe).await {
            Ok(value) => {
                let remaining = cell_i64(&value).unwrap_or(-1);
                if remaining != 0 {
                    leftovers.push(format!("{remaining} row(s) from `{probe}`"));
                }
            }
            Err(error) => leftovers.push(format!("cleanup probe `{probe}` failed: {error}")),
        }
    }

    let _ = driver.disconnect(handle).await;
    if let Err(payload) = outcome {
        std::panic::resume_unwind(payload);
    }
    assert!(
        leftovers.is_empty(),
        "{label}: cleanup left scratch object(s) behind: {leftovers:?}"
    );
}

/// Every column of every row, rendered through `common::cell`.
async fn catalog_rows(
    driver: &SqlServerDriver,
    handle: &ConnectionHandle,
    sql: &str,
) -> Vec<Vec<String>> {
    let result = driver
        .query(handle, sql)
        .await
        .unwrap_or_else(|e| panic!("catalog query failed: {e}\n{sql}"));
    result
        .rows
        .iter()
        .map(|row| row.iter().map(cell).collect())
        .collect()
}

/// `SELECT COUNT(*) FROM <from_and_where>` as an integer.
async fn count(driver: &SqlServerDriver, handle: &ConnectionHandle, from_and_where: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {from_and_where}");
    let value = scalar(driver, handle, &sql)
        .await
        .unwrap_or_else(|e| panic!("{sql} failed: {e}"));
    cell_i64(&value).unwrap_or_else(|| panic!("{sql} did not decode to an integer: {value:?}"))
}

/// Normalise a decimal rendering so `1.20` and `1.2` compare equal; keeps at
/// least one fractional digit so it can never read as an integer.
fn normalize_decimal(text: &str) -> String {
    match text.split_once('.') {
        Some((int, frac)) => {
            let frac = frac.trim_end_matches('0');
            if frac.is_empty() {
                format!("{int}.0")
            } else {
                format!("{int}.{frac}")
            }
        }
        None => format!("{text}.0"),
    }
}

/// `OBJECT_ID(N'…') IS NOT NULL` as a bool.
async fn object_exists(driver: &SqlServerDriver, handle: &ConnectionHandle, name: &str) -> bool {
    let value = scalar(
        driver,
        handle,
        &format!("SELECT CASE WHEN OBJECT_ID(N'{name}') IS NULL THEN 0 ELSE 1 END AS [e]"),
    )
    .await
    .expect("OBJECT_ID probe failed");
    cell_i64(&value) == Some(1)
}

// ---------------------------------------------------------------------------
// 1. DDL lifecycle
// ---------------------------------------------------------------------------

/// `CREATE SCHEMA` → use it → `DROP SCHEMA` (the exact statement kind that used
/// to fail with error 156 through `sp_executesql`).
#[tokio::test]
async fn schema_lifecycle_create_use_drop() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/DROP SCHEMA lifecycle") {
        return;
    }

    let schema = cfg.scratch("sch");
    let table = cfg.scratch("scht");
    let st = qn(&schema, &table);
    let cleanup = vec![
        rollback_if_open(),
        format!("DROP TABLE IF EXISTS {st}"),
        format!("DROP SCHEMA IF EXISTS {}", qi(&schema)),
    ];
    let probes = vec![schema_probe(&schema), object_probe(&st)];
    let (s, st) = (schema.clone(), st.clone());
    with_cleanup(
        &cfg,
        "schema lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            let created = driver
                .execute(&handle, &format!("CREATE SCHEMA {}", qi(&s)))
                .await
                .expect("CREATE SCHEMA must go out as a real batch (regression: error 156)");
            assert_eq!(created, 0, "DDL reports zero rows affected");

            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!("sys.schemas WHERE name = N'{}'", s)
                )
                .await,
                1,
                "the new schema must be visible in sys.schemas"
            );

            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("CREATE TABLE {st} ([id] INT NOT NULL, [v] NVARCHAR(20) NULL)")
                    )
                    .await
                    .expect("a table inside the scratch schema"),
                0
            );
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("INSERT INTO {st} ([id], [v]) VALUES (1, N'inside')")
                    )
                    .await
                    .expect("insert into the scratch schema table"),
                1
            );
            let read = scalar(
                &driver,
                &handle,
                &format!("SELECT [v] FROM {st} WHERE [id] = 1"),
            )
            .await
            .expect("read back from the scratch schema");
            assert_eq!(cell_string(&read).as_deref(), Some("inside"));

            // A duplicate CREATE SCHEMA is a clean server error, not a panic.
            let duplicate = driver
                .execute(&handle, &format!("CREATE SCHEMA {}", qi(&s)))
                .await
                .expect_err("a duplicate CREATE SCHEMA must fail");
            let message = duplicate.to_string();
            assert!(
                message.contains("2714") || message.contains("already exists"),
                "duplicate schema must report 2714/already exists, got: {message}"
            );

            // DROP SCHEMA requires an empty schema: drop the table first.
            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP TABLE {st}"))
                    .await
                    .expect("DROP TABLE"),
                0
            );
            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP SCHEMA {}", qi(&s)))
                    .await
                    .expect("DROP SCHEMA"),
                0
            );
            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!("sys.schemas WHERE name = N'{}'", s)
                )
                .await,
                0,
                "the schema must be gone"
            );
            assert!(
                !object_exists(&driver, &handle, &st).await,
                "the table must be gone with its schema"
            );
        },
    )
    .await;
}

/// `CREATE TABLE` (PK + identity + default + computed + NOT NULL) →
/// `ALTER TABLE ADD/DROP COLUMN` → `DROP TABLE`.
#[tokio::test]
async fn table_lifecycle_create_alter_drop() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/ALTER/DROP TABLE lifecycle") {
        return;
    }

    let table = cfg.scratch("tt");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    let (pk, df) = (format!("pk_{table}"), format!("df_{table}"));
    with_cleanup(&cfg, "table lifecycle", cleanup, probes, move |driver, handle| async move {
        let ddl = format!(
            "CREATE TABLE {t} (\
             [id] INT IDENTITY(1,1) NOT NULL CONSTRAINT {} PRIMARY KEY, \
             [name] NVARCHAR(50) NOT NULL CONSTRAINT {} DEFAULT (N'x'), \
             [qty] INT NOT NULL, \
             [total] AS ([qty] * 2) PERSISTED)",
            qi(&pk),
            qi(&df)
        );
        assert_eq!(driver.execute(&handle, &ddl).await.expect("CREATE TABLE"), 0);

        let shape = catalog_rows(
            &driver,
            &handle,
            &format!(
                "SELECT \
                 (SELECT COUNT(*) FROM sys.columns c WHERE c.object_id = OBJECT_ID(N'{t}') AND c.is_identity = 1), \
                 (SELECT COUNT(*) FROM sys.columns c WHERE c.object_id = OBJECT_ID(N'{t}') AND c.is_computed = 1), \
                 (SELECT COUNT(*) FROM sys.default_constraints d WHERE d.parent_object_id = OBJECT_ID(N'{t}')), \
                 (SELECT COUNT(*) FROM sys.key_constraints k WHERE k.parent_object_id = OBJECT_ID(N'{t}') AND k.type = 'PK'), \
                 (SELECT COUNT(*) FROM sys.columns c WHERE c.object_id = OBJECT_ID(N'{t}') AND c.name = 'name' AND c.is_nullable = 1)"
            ),
        )
        .await;
        assert_eq!(
            shape[0],
            vec!["1", "1", "1", "1", "0"],
            "identity / computed / default / primary key / NOT NULL shape"
        );

        // The default constraint and the computed column both apply on insert.
        assert_eq!(
            driver
                .execute(&handle, &format!("INSERT INTO {t} ([qty]) VALUES (3)"))
                .await
                .expect("insert using the default constraint"),
            1
        );
        let row = catalog_rows(&driver, &handle, &format!("SELECT [name], [total] FROM {t}")).await;
        assert_eq!(row[0], vec!["x", "6"], "default value and computed value");

        // ALTER TABLE ADD COLUMN
        assert_eq!(
            driver
                .execute(&handle, &format!("ALTER TABLE {t} ADD [added] NVARCHAR(20) NULL"))
                .await
                .expect("ALTER TABLE ADD COLUMN"),
            0
        );
        let added = scalar(
            &driver,
            &handle,
            &format!("SELECT COL_LENGTH(N'{t}', N'added') AS [len]"),
        )
        .await
        .expect("COL_LENGTH after ADD");
        assert!(
            cell_i64(&added).is_some(),
            "the added column must be visible to COL_LENGTH, got {added:?}"
        );
        assert_eq!(
            driver
                .execute(
                    &handle,
                    &format!("UPDATE {t} SET [added] = N'here' WHERE [id] = 1")
                )
                .await
                .expect("write the added column"),
            1
        );

        // ALTER TABLE DROP COLUMN
        assert_eq!(
            driver
                .execute(&handle, &format!("ALTER TABLE {t} DROP COLUMN [added]"))
                .await
                .expect("ALTER TABLE DROP COLUMN"),
            0
        );
        let dropped = scalar(
            &driver,
            &handle,
            &format!("SELECT COL_LENGTH(N'{t}', N'added') AS [len]"),
        )
        .await
        .expect("COL_LENGTH after DROP");
        assert!(
            cell_i64(&dropped).is_none(),
            "the dropped column must be gone, got {dropped:?}"
        );

        assert_eq!(
            driver
                .execute(&handle, &format!("DROP TABLE {t}"))
                .await
                .expect("DROP TABLE"),
            0
        );
        assert!(
            !object_exists(&driver, &handle, &t).await,
            "the table must be gone"
        );
    })
    .await;
}

/// `CREATE VIEW` (via `driver.query`, i.e. the shared `run` path) → `SELECT` →
/// `ALTER VIEW` → `CREATE OR ALTER VIEW` behind a leading comment →
/// `DROP VIEW`.
#[tokio::test]
async fn view_lifecycle_create_select_alter_drop() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/ALTER/DROP VIEW lifecycle") {
        return;
    }

    let view = cfg.scratch("vw");
    let v = dbo(&view);
    let cleanup = vec![rollback_if_open(), format!("DROP VIEW IF EXISTS {v}")];
    let probes = vec![object_probe(&v)];
    with_cleanup(
        &cfg,
        "view lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            let created = driver
                .query(&handle, &format!("CREATE VIEW {v} AS SELECT 1 AS [n]"))
                .await
                .expect("CREATE VIEW must go out as a real batch (regression: error 156)");
            assert!(created.rows.is_empty(), "a view definition returns no rows");

            let first = scalar(&driver, &handle, &format!("SELECT [n] FROM {v}"))
                .await
                .expect("SELECT from the view");
            assert_eq!(cell_i64(&first), Some(1));

            assert_eq!(
                driver
                    .execute(&handle, &format!("ALTER VIEW {v} AS SELECT 2 AS [n]"))
                    .await
                    .expect("ALTER VIEW must also be a real batch"),
                0
            );
            let second = scalar(&driver, &handle, &format!("SELECT [n] FROM {v}"))
                .await
                .expect("SELECT from the altered view");
            assert_eq!(cell_i64(&second), Some(2));

            // `CREATE OR ALTER VIEW` (SQL Server 2016 SP1+), behind a leading block
            // comment — the batch detector must skip comments, and the batch path
            // must carry non-ASCII text intact.
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!(
                        "/* leading comment */ CREATE OR ALTER VIEW {v} AS SELECT N'中文 🚀' AS [n]"
                    )
                    )
                    .await
                    .expect("CREATE OR ALTER VIEW behind a comment"),
                0
            );
            let third = scalar(&driver, &handle, &format!("SELECT [n] FROM {v}"))
                .await
                .expect("SELECT from the CREATE OR ALTER view");
            assert_eq!(
                cell_string(&third).as_deref(),
                Some("中文 🚀"),
                "a unicode literal must survive the batch path"
            );

            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP VIEW {v}"))
                    .await
                    .expect("DROP VIEW"),
                0
            );
            assert!(
                !object_exists(&driver, &handle, &v).await,
                "the view must be gone"
            );
        },
    )
    .await;
}

/// `CREATE PROCEDURE` with a parameter → `EXEC` and read the output →
/// `CREATE OR ALTER PROCEDURE` → `DROP PROCEDURE`.
#[tokio::test]
async fn procedure_lifecycle_exec_drop() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/ALTER/DROP PROCEDURE lifecycle") {
        return;
    }

    let procedure = cfg.scratch("prc");
    let p = dbo(&procedure);
    let cleanup = vec![rollback_if_open(), format!("DROP PROCEDURE IF EXISTS {p}")];
    let probes = vec![object_probe(&p)];
    with_cleanup(
        &cfg,
        "procedure lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            let body = format!(
                "CREATE PROCEDURE {p} @in INT, @out INT OUTPUT AS BEGIN SET NOCOUNT ON; \
             SET @out = @in * 3; END"
            );
            assert_eq!(
                driver
                    .execute(&handle, &body)
                    .await
                    .expect("CREATE PROCEDURE must go out as a real batch (regression: error 156)"),
                0
            );
            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!("sys.objects WHERE object_id = OBJECT_ID(N'{p}') AND type = 'P'")
                )
                .await,
                1,
                "the procedure must be catalogued as a stored procedure"
            );

            let call = format!(
                "DECLARE @r INT; EXEC {p} @in = 5, @out = @r OUTPUT; SELECT @r AS [result]"
            );
            let out = scalar(&driver, &handle, &call)
                .await
                .expect("EXEC the procedure");
            assert_eq!(cell_i64(&out), Some(15), "@out must be @in * 3");

            // CREATE OR ALTER (SQL Server 2016 SP1+) must work through the same path.
            let altered = format!(
                "CREATE OR ALTER PROCEDURE {p} @in INT, @out INT OUTPUT AS BEGIN \
             SET @out = @in * 4; END"
            );
            assert_eq!(
                driver
                    .execute(&handle, &altered)
                    .await
                    .expect("CREATE OR ALTER PROCEDURE"),
                0
            );
            let out = scalar(&driver, &handle, &call)
                .await
                .expect("EXEC the altered procedure");
            assert_eq!(
                cell_i64(&out),
                Some(20),
                "@out must be @in * 4 after CREATE OR ALTER"
            );

            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP PROCEDURE {p}"))
                    .await
                    .expect("DROP PROCEDURE"),
                0
            );
            assert!(
                !object_exists(&driver, &handle, &p).await,
                "the procedure must be gone"
            );
        },
    )
    .await;
}

/// Scalar `CREATE FUNCTION` → call it in a `SELECT` → `DROP FUNCTION`.
#[tokio::test]
async fn scalar_function_lifecycle() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/DROP FUNCTION (scalar) lifecycle") {
        return;
    }

    let function = cfg.scratch("fn");
    let f = dbo(&function);
    let cleanup = vec![rollback_if_open(), format!("DROP FUNCTION IF EXISTS {f}")];
    let probes = vec![object_probe(&f)];
    with_cleanup(
        &cfg,
        "scalar function lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            let body = format!("CREATE FUNCTION {f}(@x INT) RETURNS INT AS BEGIN RETURN @x + 1; END");
            assert_eq!(
                driver
                    .execute(&handle, &body)
                    .await
                    .expect("CREATE FUNCTION must go out as a real batch (regression: error 156)"),
                0
            );
            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!(
                        "sys.objects WHERE object_id = OBJECT_ID(N'{f}') AND type IN ('FN','IF','TF')"
                    )
                )
                .await,
                1,
                "the function must be catalogued"
            );
            let called = scalar(&driver, &handle, &format!("SELECT {f}(41) AS [r]"))
                .await
                .expect("call the scalar function");
            assert_eq!(cell_i64(&called), Some(42));

            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP FUNCTION {f}"))
                    .await
                    .expect("DROP FUNCTION"),
                0
            );
            assert!(
                !object_exists(&driver, &handle, &f).await,
                "the function must be gone"
            );
        },
    )
    .await;
}

/// Inline table-valued function — the same `CREATE FUNCTION` batch rule, but a
/// different body shape and a different catalog `type` (`IF`).
#[tokio::test]
async fn table_valued_function_lifecycle() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/DROP FUNCTION (table-valued) lifecycle") {
        return;
    }

    let function = cfg.scratch("tvf");
    let f = dbo(&function);
    let cleanup = vec![rollback_if_open(), format!("DROP FUNCTION IF EXISTS {f}")];
    let probes = vec![object_probe(&f)];
    with_cleanup(
        &cfg,
        "table-valued function lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            let body =
                format!("CREATE FUNCTION {f}(@n INT) RETURNS TABLE AS RETURN (SELECT @n AS [n])");
            assert_eq!(
                driver
                    .execute(&handle, &body)
                    .await
                    .expect("inline table-valued CREATE FUNCTION must be a real batch"),
                0
            );
            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!("sys.objects WHERE object_id = OBJECT_ID(N'{f}') AND type = 'IF'")
                )
                .await,
                1,
                "an inline TVF is catalogued with type 'IF'"
            );
            let row = scalar(&driver, &handle, &format!("SELECT [n] FROM {f}(7) AS t"))
                .await
                .expect("call the inline TVF");
            assert_eq!(cell_i64(&row), Some(7));

            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP FUNCTION {f}"))
                    .await
                    .expect("DROP FUNCTION (TVF)"),
                0
            );
        },
    )
    .await;
}

/// `CREATE TRIGGER` (AFTER INSERT) → prove it fired into a log table →
/// `DROP TRIGGER` and prove it stopped firing.
#[tokio::test]
async fn trigger_lifecycle_after_insert_fires() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/DROP TRIGGER lifecycle") {
        return;
    }

    let table = cfg.scratch("trg");
    let log = cfg.scratch("trlog");
    let trigger = cfg.scratch("tr");
    let (t, l, tr) = (dbo(&table), dbo(&log), dbo(&trigger));
    let cleanup = vec![
        rollback_if_open(),
        format!("DROP TRIGGER IF EXISTS {tr}"),
        format!("DROP TABLE IF EXISTS {l}"),
        format!("DROP TABLE IF EXISTS {t}"),
    ];
    let probes = vec![object_probe(&tr), object_probe(&t), object_probe(&l)];
    with_cleanup(
        &cfg,
        "trigger lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            driver
                .execute(
                    &handle,
                    &format!("CREATE TABLE {t} ([id] INT NOT NULL, [qty] INT NOT NULL)"),
                )
                .await
                .expect("CREATE TABLE for the trigger");
            driver
                .execute(
                    &handle,
                    &format!("CREATE TABLE {l} ([note] NVARCHAR(50) NOT NULL, [qty] INT NOT NULL)"),
                )
                .await
                .expect("CREATE TABLE for the trigger log");

            let ddl = format!(
                "CREATE TRIGGER {tr} ON {t} AFTER INSERT AS BEGIN SET NOCOUNT ON; \
             INSERT INTO {l} ([note], [qty]) SELECT N'fired', [qty] FROM inserted; END"
            );
            assert_eq!(
                driver
                    .execute(&handle, &ddl)
                    .await
                    .expect("CREATE TRIGGER must go out as a real batch (regression: error 156)"),
                0
            );

            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("INSERT INTO {t} ([id], [qty]) VALUES (1, 5)")
                    )
                    .await
                    .expect("insert that must fire the trigger"),
                1
            );
            let logged =
                catalog_rows(&driver, &handle, &format!("SELECT [note], [qty] FROM {l}")).await;
            assert_eq!(
                logged,
                vec![vec!["fired".to_string(), "5".to_string()]],
                "the AFTER INSERT trigger must have written exactly one log row"
            );

            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP TRIGGER {tr}"))
                    .await
                    .expect("DROP TRIGGER"),
                0
            );
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("INSERT INTO {t} ([id], [qty]) VALUES (2, 6)")
                    )
                    .await
                    .expect("insert after the trigger was dropped"),
                1
            );
            assert_eq!(
                count(&driver, &handle, &l).await,
                1,
                "a dropped trigger must not fire again"
            );
        },
    )
    .await;
}

/// `CREATE SEQUENCE` and `CREATE TYPE ... FROM ...` → use both in a real table
/// → drop them in dependency order.
#[tokio::test]
async fn sequence_and_alias_type_lifecycle() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CREATE/DROP SEQUENCE and TYPE lifecycle") {
        return;
    }

    let (seq, ty, table) = (cfg.scratch("seq"), cfg.scratch("ty"), cfg.scratch("aty"));
    let (s, y, t) = (dbo(&seq), dbo(&ty), dbo(&table));
    let cleanup = vec![
        rollback_if_open(),
        format!("DROP TABLE IF EXISTS {t}"),
        format!("DROP TYPE IF EXISTS {y}"),
        format!("DROP SEQUENCE IF EXISTS {s}"),
    ];
    let probes = vec![sequence_probe(&seq), type_probe(&ty), object_probe(&t)];
    with_cleanup(
        &cfg,
        "sequence/type lifecycle",
        cleanup,
        probes,
        move |driver, handle| async move {
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("CREATE SEQUENCE {s} AS INT START WITH 100 INCREMENT BY 5")
                    )
                    .await
                    .expect("CREATE SEQUENCE"),
                0
            );
            let first = scalar(&driver, &handle, &format!("SELECT NEXT VALUE FOR {s} AS [n]"))
                .await
                .expect("first NEXT VALUE FOR");
            assert_eq!(cell_i64(&first), Some(100), "START WITH 100");
            let second = scalar(&driver, &handle, &format!("SELECT NEXT VALUE FOR {s} AS [n]"))
                .await
                .expect("second NEXT VALUE FOR");
            assert_eq!(cell_i64(&second), Some(105), "INCREMENT BY 5");
            assert_eq!(
                count(&driver, &handle, &format!("sys.sequences WHERE name = N'{}'", seq)).await,
                1
            );

            assert_eq!(
                driver
                    .execute(&handle, &format!("CREATE TYPE {y} FROM NVARCHAR(30) NOT NULL"))
                    .await
                    .expect("CREATE TYPE FROM"),
                0
            );
            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!("sys.types WHERE name = N'{}' AND is_user_defined = 1", ty)
                )
                .await,
                1,
                "the alias type must be catalogued"
            );

            // Use both: an alias-typed column and a sequence-backed value.
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!(
                            "CREATE TABLE {t} ([id] INT NOT NULL, [v] {y} NOT NULL DEFAULT (N'seeded'))"
                        )
                    )
                    .await
                    .expect("create a table over the alias type"),
                0
            );
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("INSERT INTO {t} ([id]) VALUES (NEXT VALUE FOR {s})")
                    )
                    .await
                    .expect("insert using the sequence and the alias-type default"),
                1
            );
            let row = catalog_rows(&driver, &handle, &format!("SELECT [id], [v] FROM {t}")).await;
            assert_eq!(
                row[0],
                vec!["110", "seeded"],
                "sequence value and alias-type default"
            );

            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP TABLE {t}"))
                    .await
                    .expect("DROP TABLE"),
                0
            );
            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP TYPE {y}"))
                    .await
                    .expect("DROP TYPE"),
                0
            );
            assert_eq!(
                driver
                    .execute(&handle, &format!("DROP SEQUENCE {s}"))
                    .await
                    .expect("DROP SEQUENCE"),
                0
            );
        },
    )
    .await;
}

/// Negative control: dropping a nonexistent object is a clean
/// `DriverError::QueryFailed` with the server's "does not exist" message
/// (3701 / 15151), never a panic, and the session stays usable.
#[tokio::test]
async fn dropping_missing_objects_reports_clean_errors() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "DROP of nonexistent objects (negative control)") {
        return;
    }

    let missing = cfg.scratch("gone");
    let missing_schema = cfg.scratch("gonesch");
    let cleanup = vec![rollback_if_open()];
    with_cleanup(
        &cfg,
        "negative control",
        cleanup,
        Vec::new(),
        move |driver, handle| async move {
            let cases = vec![
                ("DROP TABLE", format!("DROP TABLE {}", dbo(&missing))),
                ("DROP VIEW", format!("DROP VIEW {}", dbo(&missing))),
                (
                    "DROP PROCEDURE",
                    format!("DROP PROCEDURE {}", dbo(&missing)),
                ),
                ("DROP FUNCTION", format!("DROP FUNCTION {}", dbo(&missing))),
                ("DROP SEQUENCE", format!("DROP SEQUENCE {}", dbo(&missing))),
                (
                    "DROP SCHEMA",
                    format!("DROP SCHEMA {}", qi(&missing_schema)),
                ),
            ];
            for (label, sql) in cases {
                match driver.execute(&handle, &sql).await {
                    Ok(rows) => {
                        panic!("{label} on a nonexistent object must fail, got Ok({rows}): {sql}")
                    }
                    Err(DriverError::QueryFailed(message)) => {
                        assert!(
                            message.contains("does not exist")
                                || message.contains("3701")
                                || message.contains("15151"),
                            "{label} must report a clean missing-object error, got: {message}"
                        );
                        eprintln!("ℹ️  {label} on a missing object: {message}");
                    }
                    Err(other) => panic!("{label}: expected QueryFailed, got {other:?}"),
                }
            }

            let alive = scalar(&driver, &handle, "SELECT 42 AS [n]")
                .await
                .expect("the session must survive a failed DROP");
            assert_eq!(cell_i64(&alive), Some(42));
        },
    )
    .await;
}

// ---------------------------------------------------------------------------
// 2. DML, row counts and value decoding
// ---------------------------------------------------------------------------

/// Insert/update/delete return the exact affected count, a zero-row statement
/// returns 0, and the final state is verified with `SELECT COUNT(*)`.
#[tokio::test]
async fn dml_row_counts_are_exact() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "DML row counts") {
        return;
    }

    let table = cfg.scratch("dml");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    with_cleanup(
        &cfg,
        "DML row counts",
        cleanup,
        probes,
        move |driver, handle| async move {
            driver
                .execute(
                    &handle,
                    &format!("CREATE TABLE {t} ([id] INT NOT NULL, [v] NVARCHAR(20) NULL)"),
                )
                .await
                .expect("CREATE TABLE");

            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!(
                            "INSERT INTO {t} ([id], [v]) VALUES (1, N'a'), (2, N'b'), (3, N'c')"
                        )
                    )
                    .await
                    .expect("multi-row INSERT"),
                3,
                "a three-row VALUES list must report three affected rows"
            );
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("INSERT INTO {t} ([id], [v]) SELECT [id] + 10, [v] FROM {t}")
                    )
                    .await
                    .expect("INSERT ... SELECT"),
                3
            );
            assert_eq!(
                driver
                    .execute(
                        &handle,
                        &format!("UPDATE {t} SET [v] = N'z' WHERE [id] >= 2")
                    )
                    .await
                    .expect("UPDATE with a WHERE clause"),
                5
            );
            assert_eq!(
                driver
                    .execute(&handle, &format!("DELETE FROM {t} WHERE [id] = 3"))
                    .await
                    .expect("DELETE with a WHERE clause"),
                1
            );
            assert_eq!(
                driver
                    .execute(&handle, &format!("DELETE FROM {t} WHERE [id] = 999"))
                    .await
                    .expect("DELETE matching nothing"),
                0,
                "a statement matching no row must report 0"
            );
            assert_eq!(
                driver
                    .execute(&handle, &format!("UPDATE {t} SET [v] = N'q' WHERE 1 = 0"))
                    .await
                    .expect("UPDATE matching nothing"),
                0
            );

            assert_eq!(count(&driver, &handle, &t).await, 5, "final row count");
            assert_eq!(
                count(&driver, &handle, &format!("{t} WHERE [v] = N'z'")).await,
                4,
                "final state of the updated rows"
            );
        },
    )
    .await;
}

/// Unicode/emoji, NULL, decimal precision, `bigint` extremes and `varbinary`
/// survive a write → read round trip through the driver's value decoding
/// (`varbinary` decodes to a `0x…` hex string).
#[tokio::test]
async fn write_types_round_trip_through_decoding() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "type round trip through a real table") {
        return;
    }

    let table = cfg.scratch("vals");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    with_cleanup(&cfg, "type round trip", cleanup, probes, move |driver, handle| async move {
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE {t} (\
                     [u] NVARCHAR(100) NULL, \
                     [n] INT NULL, \
                     [d] DECIMAL(38,10) NULL, \
                     [big] BIGINT NULL, \
                     [small] BIGINT NULL, \
                     [vb] VARBINARY(16) NULL, \
                     [flag] BIT NULL)"
                ),
            )
            .await
            .expect("CREATE TABLE for the type probe");

        let insert = format!(
            "INSERT INTO {t} ([u], [n], [d], [big], [small], [vb], [flag]) VALUES \
             (N'中文 🚀 naïve Ω ''q''', NULL, CAST(12345678901234567890.1234567890 AS DECIMAL(38,10)), \
             CAST(9223372036854775807 AS BIGINT), CAST(-9223372036854775808 AS BIGINT), \
             CAST(0x0102FF AS VARBINARY(16)), 1)"
        );
        assert_eq!(
            driver.execute(&handle, &insert).await.expect("wide-literal INSERT"),
            1
        );

        let rows = catalog_rows(
            &driver,
            &handle,
            &format!("SELECT [u], [n], [d], [big], [small], [vb], [flag] FROM {t}"),
        )
        .await;
        assert_eq!(rows.len(), 1, "exactly one probe row");
        let row = &rows[0];
        assert_eq!(row[0], "中文 🚀 naïve Ω 'q'", "unicode/emoji round trip");
        assert_eq!(row[1], "NULL", "NULL must stay NULL");
        assert_eq!(
            normalize_decimal(&row[2]),
            normalize_decimal("12345678901234567890.1234567890"),
            "DECIMAL(38,10) must not lose precision"
        );
        assert_eq!(row[3], i64::MAX.to_string(), "bigint maximum");
        assert_eq!(row[4], i64::MIN.to_string(), "bigint minimum");
        assert_eq!(row[5], "0x0102ff", "varbinary must decode to a 0x hex string");
        assert_eq!(row[6], "true", "bit must decode to a bool");
    })
    .await;
}

/// Identity columns: an explicit value is rejected, an omitted value is
/// generated, and `SET IDENTITY_INSERT ON/OFF` allows an explicit value.
#[tokio::test]
async fn identity_column_behaviour() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "identity column behaviour") {
        return;
    }

    let table = cfg.scratch("idt");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    with_cleanup(&cfg, "identity behaviour", cleanup, probes, move |driver, handle| async move {
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE {t} ([id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, [v] INT NOT NULL)"
                ),
            )
            .await
            .expect("CREATE TABLE with an identity column");

        let rejected = driver
            .execute(&handle, &format!("INSERT INTO {t} ([id], [v]) VALUES (5, 1)"))
            .await
            .expect_err("an explicit identity value must be rejected while IDENTITY_INSERT is OFF");
        match &rejected {
            DriverError::QueryFailed(message) => assert!(
                message.contains("544") || message.contains("IDENTITY_INSERT"),
                "expected error 544 / IDENTITY_INSERT, got: {message}"
            ),
            other => panic!("expected QueryFailed, got {other:?}"),
        }
        assert_eq!(
            count(&driver, &handle, &t).await,
            0,
            "the rejected insert must not write anything"
        );

        assert_eq!(
            driver
                .execute(&handle, &format!("INSERT INTO {t} ([v]) VALUES (10)"))
                .await
                .expect("insert with a generated identity"),
            1
        );
        let generated = scalar(&driver, &handle, &format!("SELECT [id] FROM {t}"))
            .await
            .expect("read the generated identity");
        assert_eq!(cell_i64(&generated), Some(1), "IDENTITY(1,1) starts at 1");

        // Path A: `SET IDENTITY_INSERT` as its own statement (its own RPC) — the
        // option is session-scoped, so it must persist for the next statement.
        let separate = match driver
            .execute(&handle, &format!("SET IDENTITY_INSERT {t} ON"))
            .await
        {
            Ok(_) => driver
                .execute(&handle, &format!("INSERT INTO {t} ([id], [v]) VALUES (7, 2)"))
                .await
                .map(|_| true)
                .unwrap_or_else(|e| {
                    eprintln!("ℹ️  separate SET IDENTITY_INSERT ON did not apply: {e}");
                    false
                }),
            Err(e) => {
                eprintln!("ℹ️  separate SET IDENTITY_INSERT ON failed: {e}");
                false
            }
        };
        if separate {
            eprintln!("ℹ️  IDENTITY_INSERT path: separate statements (the session-scoped SET persists)");
            let _ = driver
                .execute(&handle, &format!("SET IDENTITY_INSERT {t} OFF"))
                .await;
        } else {
            // Path B: the SET and the INSERT in one batch.
            let batch = format!(
                "SET IDENTITY_INSERT {t} ON; INSERT INTO {t} ([id], [v]) VALUES (7, 2); \
                 SET IDENTITY_INSERT {t} OFF"
            );
            assert_eq!(
                driver
                    .execute(&handle, &batch)
                    .await
                    .expect("IDENTITY_INSERT must work inside a single batch"),
                1
            );
            eprintln!("ℹ️  IDENTITY_INSERT path: single batch (a separate SET does not persist)");
        }
        assert_eq!(
            count(&driver, &handle, &format!("{t} WHERE [id] = 7")).await,
            1,
            "the explicit identity value must have landed"
        );

        // A plain insert after an explicit identity value continues from the seed.
        assert_eq!(
            driver
                .execute(&handle, &format!("INSERT INTO {t} ([v]) VALUES (11)"))
                .await
                .expect("insert after IDENTITY_INSERT"),
            1
        );
        assert_eq!(count(&driver, &handle, &t).await, 3);
    })
    .await;
}

// ---------------------------------------------------------------------------
// 3. Transactions
// ---------------------------------------------------------------------------

/// Transaction control **as its own statement** works on the session.
///
/// The driver has no transaction `DriverCommand` (`admin_commands.rs` and
/// `driver-api::execute_standard_sql_command` only carry
/// `query`/`execute`/`query_stream` plus admin/schema commands), so the SQL
/// editor's `BEGIN TRAN`/`COMMIT`/`ROLLBACK` reach the server through
/// `driver.execute`. `needs_own_batch` therefore routes transaction control (and
/// other session-scoped statements) through a real batch instead of
/// `sp_executesql`, whose module boundary used to raise error **266**
/// (`Transaction count after EXECUTE indicates a mismatching number of BEGIN and
/// COMMIT statements`) while leaving the transaction open on the session.
///
/// Regression: this test used to pin that defect. It now asserts the fixed
/// contract — the statements succeed *and* actually control the session
/// transaction. The self-contained batch form is covered separately by
/// `transaction_batch_form_rollback_and_commit`.
#[tokio::test]
async fn transaction_control_statements_work_on_their_own() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "transaction control statement probe") {
        return;
    }

    let table = cfg.scratch("txs");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    with_cleanup(&cfg, "transaction control probe", cleanup, probes, move |driver, handle| async move {
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE {t} ([id] INT IDENTITY(1,1) NOT NULL, [v] INT NOT NULL)"),
            )
            .await
            .expect("CREATE TABLE");

        let begin = driver.execute(&handle, "BEGIN TRANSACTION").await;
        assert!(
            begin.is_ok(),
            "BEGIN TRANSACTION must not be wrapped in sp_executesql (error 266): {begin:?}"
        );

        // The transaction is open on the session, so later statements join it.
        let trans = scalar(&driver, &handle, "SELECT @@TRANCOUNT AS [n]").await.unwrap();
        assert_eq!(
            cell_i64(&trans),
            Some(1),
            "BEGIN TRANSACTION must leave the session inside an open transaction, got @@TRANCOUNT = {trans:?}"
        );

        assert_eq!(
            driver
                .execute(&handle, &format!("INSERT INTO {t} ([v]) VALUES (1), (2)"))
                .await
                .expect("insert inside the open transaction"),
            2
        );
        assert_eq!(count(&driver, &handle, &t).await, 2);

        let rollback = driver.execute(&handle, "ROLLBACK TRANSACTION").await;
        assert!(
            rollback.is_ok(),
            "ROLLBACK TRANSACTION must not be wrapped in sp_executesql: {rollback:?}"
        );
        assert_eq!(
            cell_i64(&scalar(&driver, &handle, "SELECT @@TRANCOUNT AS [n]").await.unwrap()),
            Some(0),
            "the rollback must close the session transaction"
        );
        assert_eq!(
            count(&driver, &handle, &t).await,
            0,
            "the rolled-back rows must be gone"
        );
    })
    .await;
}

#[tokio::test]
async fn driver_transaction_api_keeps_parameterized_writes_on_the_same_session() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "SQL Server driver transaction API") {
        return;
    }
    let table = cfg.scratch("driver_tx");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];

    with_cleanup(
        &cfg,
        "driver transaction API",
        cleanup,
        probes,
        move |driver, handle| async move {
            driver
                .execute(
                    &handle,
                    &format!("CREATE TABLE {t} ([id] INT NOT NULL PRIMARY KEY, [note] NVARCHAR(100) NOT NULL)"),
                )
                .await
                .expect("create transaction probe table");

            let tx = driver
                .begin_transaction(&handle)
                .await
                .expect("begin_transaction starts on this connection's TDS session");
            let affected = driver
                .execute_with_params(
                    &handle,
                    &format!("INSERT INTO {t} ([id], [note]) VALUES (@P1, @P2)"),
                    &[Value::Integer(1), Value::String("O'Brien".into())],
                )
                .await
                .expect("parameterized insert runs inside the open transaction");
            assert_eq!(affected, 1);
            assert_eq!(
                count(&driver, &handle, &t).await,
                1,
                "reads on the same handle see the uncommitted write"
            );
            driver.rollback(tx).await.expect("rollback the first write");
            assert_eq!(
                count(&driver, &handle, &t).await,
                0,
                "rollback must clear the parameterized write"
            );

            let tx = driver
                .begin_transaction(&handle)
                .await
                .expect("begin a second transaction on the same live session");
            driver
                .execute_with_params(
                    &handle,
                    &format!("INSERT INTO {t} ([id], [note]) VALUES (@P1, @P2)"),
                    &[Value::Integer(2), Value::String("committed".into())],
                )
                .await
                .expect("insert before commit");
            driver.commit(tx).await.expect("commit the second write");
            assert_eq!(
                count(&driver, &handle, &t).await,
                1,
                "commit must leave the row visible after the transaction"
            );
        },
    )
    .await;
}

#[tokio::test]
async fn read_snapshot_keeps_a_stable_view_on_its_tds_session() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "SQL Server snapshot isolation") {
        return;
    }
    let table = cfg.scratch("snapshot");
    let t = dbo(&table);
    let connect_config = cfg.default_config();
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];

    with_cleanup(
        &cfg,
        "snapshot isolation",
        cleanup,
        probes,
        move |driver, first| async move {
            let snapshot_state = scalar(
                &driver,
                &first,
                "SELECT snapshot_isolation_state FROM sys.databases WHERE name = DB_NAME()",
            )
            .await
            .expect("read ALLOW_SNAPSHOT_ISOLATION state");
            if cell_i64(&snapshot_state) != Some(1) {
                eprintln!("⏭  Skipping snapshot assertion: ALLOW_SNAPSHOT_ISOLATION is not ON");
                return;
            }

            driver
                .execute(
                    &first,
                    &format!("CREATE TABLE {t} ([id] INT NOT NULL PRIMARY KEY)"),
                )
                .await
                .expect("create snapshot probe table");
            driver
                .execute(&first, &format!("INSERT INTO {t} ([id]) VALUES (1)"))
                .await
                .expect("seed snapshot probe table");
            let second = driver
                .connect(&connect_config)
                .await
                .expect("open a second TDS session");

            let snapshot = driver
                .begin_read_snapshot(&first)
                .await
                .expect("begin SNAPSHOT transaction on the first TDS session");
            assert_eq!(count(&driver, &first, &t).await, 1);
            assert_eq!(
                driver
                    .execute(&second, &format!("INSERT INTO {t} ([id]) VALUES (2)"))
                    .await
                    .expect("commit a row from the second session"),
                1
            );
            assert_eq!(
                count(&driver, &first, &t).await,
                1,
                "the open snapshot keeps the view established by its first read"
            );
            driver
                .rollback(snapshot)
                .await
                .expect("end the snapshot transaction and restore isolation");
            assert_eq!(
                count(&driver, &first, &t).await,
                2,
                "after rollback, a new statement sees the second session's commit"
            );
            driver
                .disconnect(second)
                .await
                .expect("disconnect the second TDS session");
        },
    )
    .await;
}

/// The supported transaction form: one self-contained batch whose trancount is
/// balanced when the `sp_executesql` module ends.
#[tokio::test]
async fn transaction_batch_form_rollback_and_commit() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "transaction rollback/commit in a batch") {
        return;
    }

    let table = cfg.scratch("txb");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    with_cleanup(
        &cfg,
        "transactions (batch form)",
        cleanup,
        probes,
        move |driver, handle| async move {
            driver
                .execute(
                    &handle,
                    &format!(
                        "CREATE TABLE {t} ([id] INT IDENTITY(1,1) NOT NULL, [v] INT NOT NULL)"
                    ),
                )
                .await
                .expect("CREATE TABLE");

            // ROLLBACK: the rows must not survive the batch.
            let rolled = driver
                .execute(
                    &handle,
                    &format!(
                    "BEGIN TRANSACTION; INSERT INTO {t} ([v]) VALUES (1), (2); ROLLBACK TRANSACTION"
                ),
                )
                .await
                .unwrap_or_else(|e| panic!("a balanced BEGIN/ROLLBACK batch must succeed: {e}"));
            eprintln!("ℹ️  BEGIN/INSERT/ROLLBACK batch reported rowsAffected = {rolled}");
            assert_eq!(
                cell_i64(
                    &scalar(&driver, &handle, "SELECT @@TRANCOUNT AS [n]")
                        .await
                        .unwrap()
                ),
                Some(0),
                "a balanced batch must leave no transaction open"
            );
            assert_eq!(
                count(&driver, &handle, &t).await,
                0,
                "ROLLBACK must leave no row behind"
            );

            // COMMIT: the rows must survive.
            let committed = driver
                .execute(
                    &handle,
                    &format!(
                    "BEGIN TRANSACTION; INSERT INTO {t} ([v]) VALUES (4), (5); COMMIT TRANSACTION"
                ),
                )
                .await
                .unwrap_or_else(|e| panic!("a balanced BEGIN/COMMIT batch must succeed: {e}"));
            eprintln!("ℹ️  BEGIN/INSERT/COMMIT batch reported rowsAffected = {committed}");
            assert_eq!(
                cell_i64(
                    &scalar(&driver, &handle, "SELECT @@TRANCOUNT AS [n]")
                        .await
                        .unwrap()
                ),
                Some(0),
                "a balanced batch must leave no transaction open"
            );
            assert_eq!(
                count(&driver, &handle, &t).await,
                2,
                "COMMIT must persist the two rows"
            );

            // A batch that reads back its own uncommitted write sees it.
            let in_batch = scalar(
                &driver,
                &handle,
                &format!(
                    "BEGIN TRANSACTION; INSERT INTO {t} ([v]) VALUES (6); \
                 SELECT COUNT(*) FROM {t}; ROLLBACK TRANSACTION"
                ),
            )
            .await
            .unwrap_or_else(|e| panic!("BEGIN/INSERT/SELECT/ROLLBACK batch must succeed: {e}"));
            assert_eq!(
                cell_i64(&in_batch),
                Some(3),
                "the batch must see its own uncommitted row before the rollback"
            );
            assert_eq!(
                count(&driver, &handle, &t).await,
                2,
                "the rolled-back row must be gone and the committed rows must remain"
            );
        },
    )
    .await;
}

/// Two handles on the same driver are two sessions: neither inherits the
/// other's transaction, and one handle's rollback cannot touch the other's
/// committed rows.
///
/// The open transaction on each handle is created with a single-statement
/// `BEGIN TRANSACTION`, which the driver rejects with error 266 while still
/// opening the transaction (see
/// `transaction_control_statements_are_rejected_by_the_rpc_path`) — the
/// assertions below therefore treat the 266 as a documented defect and verify
/// the real session state (`@@TRANCOUNT`) instead of the driver's return value.
#[tokio::test]
async fn two_handles_do_not_share_a_transaction() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "two-handle transaction isolation") {
        return;
    }

    let (driver, first) = connect(&cfg).await;
    let config = cfg.default_config();
    let second = with_resume_retry("second connection", || driver.connect(&config))
        .await
        .expect("a second connection to the same instance");

    let table = cfg.scratch("iso");
    let t = dbo(&table);
    if let Err(error) = driver
        .execute(&first, &format!("CREATE TABLE {t} ([v] INT NOT NULL)"))
        .await
    {
        panic!("setup failed: {error}");
    }

    let outcome = std::panic::AssertUnwindSafe(async {
        // Transaction control as a single statement is rejected by the RPC
        // path (see `transaction_control_statements_are_rejected_by_the_rpc_path`):
        // the server answers error 266 *and* leaves the transaction open, which
        // is exactly the open transaction this test needs on each handle.
        let transact = |result: Result<u64, DriverError>, what: &str| match result {
            Ok(rows) => {
                eprintln!("ℹ️  {what}: succeeded ({rows} rows) — no trancount mismatch")
            }
            Err(DriverError::QueryFailed(message)) => {
                assert!(
                    message.contains("266"),
                    "{what}: expected the sp_executesql trancount error 266, got: {message}"
                );
                eprintln!("ℹ️  {what}: documented driver defect (error 266); the statement still took effect");
            }
            Err(other) => panic!("{what}: expected QueryFailed, got {other:?}"),
        };

        transact(
            driver.execute(&first, "BEGIN TRANSACTION").await,
            "first handle: BEGIN TRANSACTION",
        );
        assert_eq!(
            cell_i64(&scalar(&driver, &first, "SELECT @@TRANCOUNT AS [n]").await.unwrap()),
            Some(1),
            "the first handle must really be inside a transaction"
        );
        assert_eq!(
            driver
                .execute(&first, &format!("INSERT INTO {t} ([v]) VALUES (1)"))
                .await
                .expect("uncommitted insert on the first handle"),
            1
        );

        assert_eq!(
            cell_i64(&scalar(&driver, &second, "SELECT @@TRANCOUNT AS [n]").await.unwrap()),
            Some(0),
            "the second handle must not inherit the first handle's transaction"
        );
        let rcsi = scalar(
            &driver,
            &second,
            "SELECT is_read_committed_snapshot_on AS [rcsi] FROM sys.databases WHERE name = DB_NAME()",
        )
        .await
        .expect("RCSI probe");
        eprintln!("ℹ️  is_read_committed_snapshot_on = {rcsi:?}");

        // Isolation probes. The uncommitted row is physically present (a dirty
        // read sees it), it is locked by the first handle (READPAST skips it),
        // and the second handle is not inside that transaction (so it cannot
        // see the row under READ COMMITTED either). `LOCK_TIMEOUT` is set in
        // the same batch because a session-scoped SET issued as its own
        // statement is reverted when its `sp_executesql` module ends.
        let dirty = count(&driver, &second, &format!("{t} WITH (READUNCOMMITTED)")).await;
        assert_eq!(
            dirty, 1,
            "a dirty read must still see the physically present uncommitted row"
        );
        let skipped = scalar(
            &driver,
            &second,
            &format!("SET LOCK_TIMEOUT 3000; SELECT COUNT(*) AS [n] FROM {t} WITH (READPAST)"),
        )
        .await
        .expect("a READPAST probe must not block");
        assert_eq!(
            cell_i64(&skipped),
            Some(0),
            "READPAST must skip the row locked by the first handle's transaction, which proves \
             the second handle is not inside it"
        );
        let committed_view = scalar(
            &driver,
            &second,
            &format!("SET LOCK_TIMEOUT 3000; SELECT COUNT(*) AS [n] FROM {t}"),
        )
        .await;
        match &committed_view {
            Ok(value) => assert_eq!(
                cell_i64(value),
                Some(0),
                "the second handle must not see the first handle's uncommitted row"
            ),
            Err(error) => eprintln!(
                "ℹ️  READ COMMITTED did not complete within the lock timeout (this database has \
                 READ_COMMITTED_SNAPSHOT off, so readers block on writers): {error}"
            ),
        }

        transact(
            driver.execute(&first, "ROLLBACK TRANSACTION").await,
            "first handle: ROLLBACK TRANSACTION",
        );
        assert_eq!(
            cell_i64(&scalar(&driver, &first, "SELECT @@TRANCOUNT AS [n]").await.unwrap()),
            Some(0),
            "the first handle's rollback must have taken effect"
        );
        assert_eq!(
            count(&driver, &second, &t).await,
            0,
            "after the rollback the row must still be gone"
        );

        // The second handle's own transaction rolls back independently of the
        // first handle's committed data.
        assert_eq!(
            driver
                .execute(&first, &format!("INSERT INTO {t} ([v]) VALUES (3)"))
                .await
                .expect("autocommit insert on the first handle"),
            1
        );
        transact(
            driver.execute(&second, "BEGIN TRANSACTION").await,
            "second handle: BEGIN TRANSACTION",
        );
        assert_eq!(
            cell_i64(&scalar(&driver, &second, "SELECT @@TRANCOUNT AS [n]").await.unwrap()),
            Some(1),
            "the second handle's own transaction must be open"
        );
        assert_eq!(
            driver
                .execute(&second, &format!("INSERT INTO {t} ([v]) VALUES (4)"))
                .await
                .expect("insert inside the second handle's transaction"),
            1
        );
        transact(
            driver.execute(&second, "ROLLBACK TRANSACTION").await,
            "second handle: ROLLBACK TRANSACTION",
        );
        assert_eq!(
            count(&driver, &first, &t).await,
            1,
            "the second handle's rollback must not touch the first handle's committed row"
        );
    })
    .catch_unwind()
    .await;

    drop_quietly(&driver, &first, &rollback_if_open()).await;
    drop_quietly(&driver, &second, &rollback_if_open()).await;
    drop_quietly(&driver, &first, &format!("DROP TABLE IF EXISTS {t}")).await;
    let probe = scalar(&driver, &first, &object_probe(&t)).await;
    let _ = driver.disconnect(second).await;
    let _ = driver.disconnect(first).await;
    if let Err(payload) = outcome {
        std::panic::resume_unwind(payload);
    }
    assert_eq!(
        cell_i64(&probe.expect("cleanup probe failed")),
        Some(0),
        "two-handle test left {t} behind"
    );
}

/// Read-only guard: `ConnectionConfig.read_only: true` must not break reads.
/// This test **observes and reports** whether the driver rejects a write,
/// because the guard lives in the host, not in the driver — there is no
/// `read_only` reference anywhere under `packages/drivers/sqlserver/src/`
/// (the host enforces it in `src-tauri/src/sql_guard/safety.rs`
/// `check_sql(sql, read_only, safe_mode)`), so the driver-level expectation is
/// "writes still go through"; anything else is reported, not asserted.
#[tokio::test]
async fn read_only_config_is_not_enforced_by_the_driver() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "read_only config probe") {
        return;
    }

    let (driver, handle) = connect(&cfg).await;
    let table = cfg.scratch("ro");
    let t = dbo(&table);

    let mut read_only = cfg.default_config();
    read_only.read_only = true;
    let ro_handle = with_resume_retry("read-only connect", || driver.connect(&read_only))
        .await
        .expect("connecting with read_only = true must work");

    let outcome = std::panic::AssertUnwindSafe(async {
        driver
            .execute(&handle, &format!("CREATE TABLE {t} ([v] INT NOT NULL)"))
            .await
            .expect("scratch setup on the normal connection");
        driver
            .execute(&handle, &format!("INSERT INTO {t} ([v]) VALUES (1)"))
            .await
            .expect("scratch seed on the normal connection");

        let read = driver
            .query(&ro_handle, &format!("SELECT COUNT(*) AS [n] FROM {t}"))
            .await;
        match &read {
            Ok(result) => assert_eq!(
                cell_i64(&result.rows[0][0]),
                Some(1),
                "SELECT must keep working on a read_only config"
            ),
            Err(error) => panic!("SELECT on a read_only config must work, got: {error}"),
        }

        let write = driver
            .execute(&ro_handle, &format!("INSERT INTO {t} ([v]) VALUES (99)"))
            .await;
        match &write {
            Ok(rows) => eprintln!(
                "⚠️  FINDING (host-layer guard, not a driver bug): ConnectionConfig.read_only = \
                 true was accepted and a write succeeded ({rows} row). SqlServerDriver never \
                 reads `config.read_only`; the host enforces read-only in \
                 src-tauri/src/sql_guard/safety.rs::check_sql."
            ),
            Err(error) => eprintln!("ℹ️  a write was rejected under read_only = true: {error}"),
        }

        // The read-only connection must stay usable either way.
        assert!(
            driver.query(&ro_handle, "SELECT 1 AS [ok]").await.is_ok(),
            "the read-only connection must stay usable"
        );
    })
    .catch_unwind()
    .await;

    drop_quietly(&driver, &handle, &rollback_if_open()).await;
    drop_quietly(&driver, &handle, &format!("DROP TABLE IF EXISTS {t}")).await;
    let probe = scalar(&driver, &handle, &object_probe(&t)).await;
    let _ = driver.disconnect(ro_handle).await;
    let _ = driver.disconnect(handle).await;
    if let Err(payload) = outcome {
        std::panic::resume_unwind(payload);
    }
    assert_eq!(
        cell_i64(&probe.expect("cleanup probe failed")),
        Some(0),
        "read_only test left {t} behind"
    );
}

// ---------------------------------------------------------------------------
// 4. Batches, the command API and routing limits
// ---------------------------------------------------------------------------

/// A multi-statement `execute` batch runs to completion, and `query_multi`
/// returns one result per `;`-separated statement.
#[tokio::test]
async fn batch_execute_and_query_multi_report_per_statement() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "multi-statement batch behaviour") {
        return;
    }

    let table = cfg.scratch("bt");
    let t = dbo(&table);
    let cleanup = vec![rollback_if_open(), format!("DROP TABLE IF EXISTS {t}")];
    let probes = vec![object_probe(&t)];
    with_cleanup(&cfg, "batch behaviour", cleanup, probes, move |driver, handle| async move {
        let batch = format!(
            "CREATE TABLE {t} ([a] INT NOT NULL); \
             INSERT INTO {t} ([a]) VALUES (1), (2); \
             SELECT COUNT(*) AS [c] FROM {t}"
        );
        let affected = driver
            .execute(&handle, &batch)
            .await
            .expect("a CREATE/INSERT/SELECT batch must succeed");
        eprintln!("ℹ️  multi-statement execute reported rowsAffected = {affected}");
        // tiberius' `ExecuteResult::total()` sums every DONE row count of the
        // batch, including the row count a trailing SELECT reports — so this is
        // 2 inserted rows + 1 row from `SELECT COUNT(*)`, not a DML-only count.
        assert_eq!(
            affected, 3,
            "a batch reports the sum of every statement's row count (2 inserted + 1 selected)"
        );
        assert_eq!(
            count(&driver, &handle, &t).await,
            2,
            "the whole batch must have run, including the statements after the CREATE"
        );

        let multi = driver
            .query_multi(
                &handle,
                &format!(
                    "SELECT 1 AS [a]; INSERT INTO {t} ([a]) VALUES (3); SELECT COUNT(*) AS [c] FROM {t}"
                ),
                None,
            )
            .await
            .expect("query_multi over a three-statement batch");
        assert_eq!(multi.results.len(), 3, "one result per statement");
        assert_eq!(multi.results[0].columns.len(), 1, "SELECT 1 keeps its column");
        assert_eq!(cell_i64(&multi.results[0].rows[0][0]), Some(1));
        assert!(
            multi.results[1].rows.is_empty(),
            "the INSERT statement produces no rows of its own"
        );
        assert_eq!(
            cell_i64(&multi.results[2].rows[0][0]),
            Some(3),
            "the trailing SELECT must see the INSERT from the previous statement"
        );
    })
    .await;
}

/// Routing limits of the write path:
///
/// 1. Only the **leading** statement of the text is inspected for the
///    batch-only rule, so a batch whose batch-only DDL is not first goes to
///    `sp_executesql` and the server rejects the whole batch (error 156/111).
///    This part is a *server* rule as well — `CREATE SCHEMA` must be the first
///    statement of a real batch in SSMS too — so it is documented, not asserted
///    as a driver defect: the test only pins that nothing at all was executed.
/// 2. `query_multi` splits statements with the shared quote/comment-aware
///    scanner, so a `;` inside a string literal no longer cut the statement in
///    half (it used to fail with error 105, "Unclosed quotation mark").
#[tokio::test]
async fn batch_only_ddl_routing_limitations() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "batch-only DDL routing probes") {
        return;
    }

    let table = cfg.scratch("nb");
    let schema = cfg.scratch("nbs");
    let (t, s) = (dbo(&table), qi(&schema));
    let cleanup = vec![
        rollback_if_open(),
        format!("DROP TABLE IF EXISTS {t}"),
        format!("DROP SCHEMA IF EXISTS {s}"),
    ];
    let probes = vec![object_probe(&t), schema_probe(&schema)];
    with_cleanup(
        &cfg,
        "routing limitations",
        cleanup,
        probes,
        move |driver, handle| async move {
            // (1) batch-only DDL that is not the first statement of the batch
            let trailing = format!("CREATE TABLE {t} ([a] INT NOT NULL); CREATE SCHEMA {s}");
            match driver.execute(&handle, &trailing).await {
                Ok(rows) => {
                    eprintln!(
                    "ℹ️  a trailing CREATE SCHEMA unexpectedly succeeded ({rows} rows): the whole \
                     batch is no longer rejected"
                );
                    let _ = driver.execute(&handle, &format!("DROP SCHEMA {s}")).await;
                }
                Err(DriverError::QueryFailed(message)) => {
                    eprintln!(
                    "⚠️  FINDING (driver): batch-only DDL after the first statement is still sent \
                     through sp_executesql: {message}"
                );
                    assert!(
                        message.contains("156")
                            || message.contains("111")
                            || message
                                .to_ascii_uppercase()
                                .contains("MUST BE THE FIRST STATEMENT"),
                        "expected error 111/156 for the non-leading CREATE SCHEMA, got: {message}"
                    );
                }
                Err(other) => panic!("expected QueryFailed, got {other:?}"),
            }
            assert_eq!(
                count(
                    &driver,
                    &handle,
                    &format!("sys.schemas WHERE name = N'{}'", schema)
                )
                .await,
                0,
                "the rejected batch must not have created the schema"
            );

            // (2) a `;` inside a string literal must not create a new statement
            let split = driver
                .query_multi(&handle, "SELECT ';' AS [a]", None)
                .await
                .expect("query_multi must not split on a ';' inside a string literal");
            assert_eq!(
                split.results.len(),
                1,
                "a literal ';' must not create a new statement: {:?}",
                split.results.iter().map(|r| &r.sql).collect::<Vec<_>>()
            );
            assert_eq!(
                cell_string(&split.results[0].rows[0][0]).as_deref(),
                Some(";"),
                "the literal must survive query_multi"
            );
            let single = driver
                .query(&handle, "SELECT ';' AS [a]")
                .await
                .expect("the same statement is valid T-SQL through the single-statement path");
            assert_eq!(
                cell_string(&single.rows[0][0]).as_deref(),
                Some(";"),
                "the single-statement path proves the statement itself is valid"
            );
        },
    )
    .await;
}

/// The Driver Command API: `create_schema` must route `CREATE SCHEMA` through a
/// real batch, `query`/`execute` must reach the same engine, and the schema
/// target must qualify an unqualified statement (driver-api F7).
#[tokio::test]
async fn driver_command_api_schema_crud() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "Driver Command API schema CRUD") {
        return;
    }

    let schema = cfg.scratch("csch");
    let table = cfg.scratch("ct");
    let qualified = cfg.scratch("cq");
    let (s, t, q) = (qi(&schema), qi(&table), qi(&qualified));
    let schema_name = schema.clone();
    let st = format!("{s}.{t}");
    let sq = format!("{s}.{q}");
    let cleanup = vec![
        rollback_if_open(),
        format!("DROP TABLE IF EXISTS {st}"),
        format!("DROP TABLE IF EXISTS {sq}"),
        format!("DROP SCHEMA IF EXISTS {s}"),
    ];
    let probes = vec![schema_probe(&schema), object_probe(&st), object_probe(&sq)];
    with_cleanup(&cfg, "driver command api", cleanup, probes, move |driver, handle| async move {
        let created = driver
            .execute_command(&handle, "create_schema", json!({ "name": schema_name }))
            .await
            .expect("the create_schema command must route CREATE SCHEMA through a real batch");
        assert_eq!(created.data["ok"], json!(true));
        assert_eq!(
            count(
                &driver,
                &handle,
                &format!("sys.schemas WHERE name = N'{}'", schema_name)
            )
            .await,
            1,
            "the create_schema command must really create the schema"
        );

        // `execute` through the command API.
        let ddl = driver
            .execute_command(
                &handle,
                "execute",
                json!({ "sql": format!("CREATE TABLE {st} ([id] INT NOT NULL)") }),
            )
            .await
            .expect("the execute command must run DDL");
        assert_eq!(ddl.data["rowsAffected"], json!(0), "DDL reports zero rows");

        let inserted = driver
            .execute_command(
                &handle,
                "execute",
                json!({ "sql": format!("INSERT INTO {st} ([id]) VALUES (1), (2)") }),
            )
            .await
            .expect("the execute command must run DML");
        assert_eq!(inserted.data["rowsAffected"], json!(2));

        // `query` through the command API returns the serialised multi-result.
        let queried = driver
            .execute_command(
                &handle,
                "query",
                json!({ "sql": format!("SELECT COUNT(*) AS [c] FROM {st}") }),
            )
            .await
            .expect("the query command must run a SELECT");
        assert_eq!(
            queried.data["results"][0]["rows"][0][0],
            json!(2),
            "the query command must return the decoded rows: {}",
            queried.data
        );

        // The schema target must qualify an unqualified name (driver-api F7).
        let qualified_query = driver
            .execute_command(
                &handle,
                "query",
                json!({ "sql": format!("SELECT COUNT(*) AS [c] FROM {}", table), "schema": schema_name }),
            )
            .await
            .expect("a schema-targeted query must be rewritten to [schema].[table]");
        assert_eq!(
            qualified_query.data["results"][0]["rows"][0][0],
            json!(2),
            "the unqualified name must resolve inside the scratch schema: {}",
            qualified_query.data
        );
        let qualified_ddl = driver
            .execute_command(
                &handle,
                "execute",
                json!({ "sql": format!("CREATE TABLE {q} ([id] INT NOT NULL)"), "schema": schema_name }),
            )
            .await
            .expect("a schema-targeted CREATE TABLE must be rewritten to [schema].[table]");
        assert_eq!(qualified_ddl.data["rowsAffected"], json!(0));

        // There is no `drop_schema` Driver Command; the SQL path is the only one.
        let missing_command = driver
            .execute_command(&handle, "drop_schema", json!({ "name": schema_name }))
            .await
            .expect_err("no drop_schema command should exist yet");
        match missing_command {
            DriverError::Unsupported(message) => eprintln!(
                "ℹ️  FINDING (API surface): there is no `drop_schema` Driver Command, only the SQL \
                 path: {message}"
            ),
            other => panic!("expected Unsupported for drop_schema, got {other:?}"),
        }

        assert_eq!(
            driver.execute(&handle, &format!("DROP TABLE {st}")).await.unwrap(),
            0
        );
        assert_eq!(
            driver.execute(&handle, &format!("DROP TABLE {sq}")).await.unwrap(),
            0
        );
        assert_eq!(
            driver.execute(&handle, &format!("DROP SCHEMA {s}")).await.unwrap(),
            0
        );
        assert_eq!(
            count(
                &driver,
                &handle,
                &format!("sys.schemas WHERE name = N'{}'", schema_name)
            )
            .await,
            0
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// 5. Cleanup audit
// ---------------------------------------------------------------------------

/// Catalog audit of every `dz_test_*` object still present after the suite.
///
/// This is a **report**, not a kill switch: other agents run their own live
/// suites against the same database at the same time, so objects carrying this
/// process' pid marker can still belong to a test that is running right now
/// (`--test-threads=2`). The authoritative cleanup assertion is per test: every
/// test drops its own objects and [`with_cleanup`] then verifies through the
/// catalog that those exact objects are gone.
#[tokio::test]
async fn catalog_sweep_reports_leftover_scratch_objects() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let escaped_prefix = cfg
        .temp_prefix
        .replace('[', "[[]")
        .replace('%', "[%]")
        .replace('_', "[_]");
    // `sys.objects.type_desc` lives in the fixed catalog collation, so the
    // concatenation needs an explicit DATABASE_DEFAULT (otherwise Azure SQL
    // raises error 451, a collation conflict in the add operator).
    let sql = format!(
        "SELECT N'SCHEMA:' + s.name AS [obj] FROM sys.schemas s WHERE s.name LIKE N'{escaped_prefix}%' \
         UNION ALL \
         SELECT o.type_desc COLLATE DATABASE_DEFAULT + N':' + sch.name COLLATE DATABASE_DEFAULT \
                + N'.' + o.name COLLATE DATABASE_DEFAULT AS [obj] \
         FROM sys.objects o JOIN sys.schemas sch ON sch.schema_id = o.schema_id \
         WHERE o.name LIKE N'{escaped_prefix}%' \
         ORDER BY 1"
    );
    let rows = catalog_rows(&driver, &handle, &sql).await;
    let marker = pid_marker();
    let mut ours: Vec<String> = Vec::new();
    let mut others: Vec<String> = Vec::new();
    for row in &rows {
        let object = row.first().cloned().unwrap_or_default();
        if object.contains(&marker) {
            ours.push(object);
        } else {
            others.push(object);
        }
    }

    eprintln!(
        "ℹ️  scratch-object audit: {} object(s) carry this process' marker ({marker}); {} belong \
         to other processes",
        ours.len(),
        others.len()
    );
    for object in &ours {
        eprintln!("⚠️  in flight or leftover (this process): {object}");
    }
    for object in &others {
        eprintln!("ℹ️  left in place (another process/agent): {object}");
    }

    let _ = driver.disconnect(handle).await;
}
