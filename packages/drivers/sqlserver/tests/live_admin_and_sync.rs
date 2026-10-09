//! Live SQL Server **admin-command surface** + **sync / keyset pagination**.
//!
//! Scope and safety (see also `tests/common/mod.rs`):
//! - every test skips (never fails) when the instance is not configured.
//!   Settings come from the process environment; a local env file is read
//!   only when you opt in with `TEST_SQLSERVER_ENV_FILE=/path/to/your/env/file`;
//! - writes happen only when `TEST_SQLSERVER_ALLOW_WRITE=1`, only on
//!   `cfg.scratch(..)` objects, and every scratch object is dropped in a guard
//!   that runs even when the test body panics;
//! - `DataZen`, `master` and every pre-existing object are read-only here;
//! - `CREATE DATABASE` / `DROP DATABASE` additionally require
//!   `TEST_SQLSERVER_ALLOW_CREATE_DATABASE=1`, a scratch name and a 3-minute
//!   bound — a timeout is reported as “not completed”, never a hang.
//!
//! Driver-specific tests live in this crate (`AGENTS.md`). Nothing outside
//! `tests/` is modified by this file.

mod common;

use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use common::{connect, drop_quietly, live_config, write_allowed, LiveConfig, Platform};
use datazen_driver_api::{
    required_access_level, CommandAccessLevel, ConnectionHandle, DatabaseDriver as _, DriverError,
    SqlTarget, Value,
};
use datazen_driver_sqlserver::SqlServerDriver;
use serde_json::{json, Value as JsonValue};

// ── shared helpers (no changes to tests/common/mod.rs) ─────────────

const DB_TIMEOUT: Duration = Duration::from_secs(180);

/// A scratch object name must be provably ours before it may be created or
/// dropped: prefixed, never the configured database, never `master`.
fn assert_scratch_name(cfg: &LiveConfig, name: &str) {
    assert!(
        cfg.temp_prefix.is_empty() || name.starts_with(&cfg.temp_prefix),
        "refusing to touch `{name}`: does not start with TEST_SQLSERVER_TEMP_PREFIX ({})",
        cfg.temp_prefix
    );
    assert!(
        !name.eq_ignore_ascii_case(&cfg.database),
        "refusing to touch the configured database `{name}`"
    );
    assert!(
        !name.eq_ignore_ascii_case("master"),
        "refusing to touch `master`"
    );
}

/// Never echo a credential, even inside a server error message.
fn redact(cfg: &LiveConfig, text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    if cfg.password.is_empty() {
        text.to_string()
    } else {
        text.replace(&cfg.password, "<redacted>")
    }
}

/// Run `body` on its own task and always clean up afterwards — including when
/// the body panics. `cleanup` statements run in order on the same connection.
async fn guarded_live<F, Fut, T>(cfg: &LiveConfig, cleanup: Vec<String>, body: F) -> T
where
    F: FnOnce(Arc<SqlServerDriver>, ConnectionHandle) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (driver, handle) = connect(cfg).await;
    let body_driver = Arc::clone(&driver);
    let body_handle = handle.clone();
    let joined = tokio::spawn(async move { body(body_driver, body_handle).await }).await;
    for stmt in &cleanup {
        drop_quietly(&driver, &handle, stmt).await;
    }
    let _ = driver.disconnect(handle).await;
    match joined {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// A scratch table lives in a scratch schema; both are dropped by the guard.
async fn create_scratch_table(
    driver: &SqlServerDriver,
    handle: &ConnectionHandle,
    schema: &str,
    table: &str,
    columns: &str,
) {
    driver
        .execute(handle, &format!("CREATE SCHEMA [{schema}]"))
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA [{schema}]: {e}"));
    driver
        .execute(handle, &format!("CREATE TABLE {table} ({columns})"))
        .await
        .unwrap_or_else(|e| panic!("CREATE TABLE {table}: {e}"));
}

/// `INSERT INTO t ([id],[name]) VALUES (1,N'row-1'),…` batch.
fn id_name_batch(table: &str, first_id: i64, count: i64) -> String {
    let mut sql = format!("INSERT INTO {table} ([id], [name]) VALUES ");
    for i in 0..count {
        if i > 0 {
            sql.push(',');
        }
        let id = first_id + i;
        sql.push_str(&format!("({id}, N'row-{id}')"));
    }
    sql
}

/// 12 tenants × 10 regions = 120 rows behind a composite primary key.
fn tenant_region_batch(table: &str, tenants: i64, regions: i64) -> String {
    let mut sql = format!("INSERT INTO {table} ([tenant], [region], [payload]) VALUES ");
    let mut first = true;
    for tenant in 1..=tenants {
        for region in 1..=regions {
            if !first {
                sql.push(',');
            }
            first = false;
            sql.push_str(&format!("({tenant}, {region}, N'p-{tenant}-{region}')"));
        }
    }
    sql
}

fn row_ids(result: &datazen_driver_api::QueryResult) -> Vec<i64> {
    result
        .rows
        .iter()
        .filter_map(|row| common::cell_i64(row.first().unwrap_or(&None)))
        .collect()
}

/// First cell of the first row, flattened to the driver's `Option<Value>` shape.
fn first_cell(result: &datazen_driver_api::QueryResult) -> Option<Value> {
    result
        .rows
        .first()
        .and_then(|row| row.first().cloned().flatten())
}

/// In these tests the connection is `stock`; this only papers over the
/// serverless-resume window for individual statements.
async fn query_resumable(
    driver: &SqlServerDriver,
    handle: &ConnectionHandle,
    sql: &str,
) -> Result<datazen_driver_api::QueryResult, DriverError> {
    common::with_resume_retry("live query", || driver.query(handle, sql)).await
}

// ── A1: command definitions ────────────────────────────────────────

/// Offline (no server needed): every advertised command must be a usable
/// contract — non-empty name/description and a JSON-schema-shaped
/// `parameters`/`properties` block with self-consistent `required`.
#[test]
fn command_definitions_are_well_formed_json_schemas() {
    let driver = SqlServerDriver::new();
    let defs = driver.command_definitions();
    assert!(!defs.is_empty(), "the driver advertises no commands at all");

    let mut problems: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for def in &defs {
        if def.id.trim().is_empty() {
            problems.push("<unnamed>: empty command id".to_string());
        }
        if !seen.insert(def.id.clone()) {
            problems.push(format!("{}: duplicate command id", def.id));
        }
        if def.name.trim().is_empty() {
            problems.push(format!("{}: empty name", def.id));
        }
        match def.description.as_deref() {
            Some(text) if !text.trim().is_empty() => {}
            _ => problems.push(format!("{}: missing/empty description", def.id)),
        }

        let schema = &def.input_schema;
        if schema.get("type").and_then(JsonValue::as_str) != Some("object") {
            problems.push(format!("{}: input_schema.type is not \"object\"", def.id));
        }
        let properties = schema.get("properties").and_then(JsonValue::as_object);
        match properties {
            Some(map) => {
                for (key, value) in map {
                    if !value.is_object() {
                        problems.push(format!(
                            "{}: property `{key}` is not a JSON-schema object",
                            def.id
                        ));
                    }
                }
            }
            None => problems.push(format!(
                "{}: input_schema has no `parameters`/`properties` object",
                def.id
            )),
        }
        if let Some(required) = schema.get("required") {
            match required.as_array() {
                Some(items) => {
                    for item in items {
                        let name = item.as_str().unwrap_or_default();
                        let declared = properties
                            .map(|map| map.contains_key(name))
                            .unwrap_or(false);
                        if name.is_empty() || !declared {
                            problems.push(format!(
                                "{}: required `{name}` is not declared in properties",
                                def.id
                            ));
                        }
                    }
                }
                None => problems.push(format!("{}: `required` is not an array", def.id)),
            }
        }
        for permission in &def.permissions {
            if permission.trim().is_empty() {
                problems.push(format!("{}: empty permission id", def.id));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "malformed command definitions: {problems:#?}"
    );

    for expected in [
        "query",
        "execute",
        "query_stream",
        "create_database",
        "create_schema",
        "create_user",
        "drop_database",
        "list_databases",
        "list_tables",
        "get_table_schema",
    ] {
        assert!(
            seen.contains(expected),
            "command `{expected}` disappeared from the surface: {seen:?}"
        );
    }

    // Risk classification is metadata the host gates on.
    let level = |id: &str| -> CommandAccessLevel {
        let def = defs
            .iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("missing definition for {id}"));
        required_access_level(def)
    };
    assert_eq!(level("query"), CommandAccessLevel::Read);
    assert_eq!(level("query_stream"), CommandAccessLevel::Read);
    assert_eq!(level("list_tables"), CommandAccessLevel::Read);
    assert_eq!(level("create_schema"), CommandAccessLevel::HighRisk);
    assert_eq!(level("create_database"), CommandAccessLevel::HighRisk);
    assert_eq!(level("drop_database"), CommandAccessLevel::HighRisk);
    eprintln!("ℹ️  {} command definitions, all schema-shaped", defs.len());
}

// ── A2: the live per-command matrix ────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disposition {
    Ok,
    Unsupported,
    Failed,
    NotCompleted,
}

impl Disposition {
    fn label(self) -> &'static str {
        match self {
            Disposition::Ok => "ok",
            Disposition::Unsupported => "Unsupported",
            Disposition::Failed => "failed",
            Disposition::NotCompleted => "not-completed",
        }
    }
}

async fn run_command(
    driver: &SqlServerDriver,
    handle: &ConnectionHandle,
    name: &str,
    input: JsonValue,
) -> (Disposition, String) {
    match driver.execute_command(handle, name, input).await {
        Ok(result) => (Disposition::Ok, result.data.to_string()),
        Err(DriverError::Unsupported(message)) => (Disposition::Unsupported, message),
        Err(error) => (Disposition::Failed, error.to_string()),
    }
}

/// Execute every advertised command against the live instance and print a
/// name → outcome matrix. Heavyweight create/drop-database paths are bounded
/// and gated.
#[tokio::test]
async fn admin_command_matrix_live() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "admin command matrix (scratch schema/table/user)") {
        return;
    }

    let schema = cfg.scratch("admin");
    let table = format!("[{schema}].[rows]");
    assert_scratch_name(&cfg, &schema);
    let username = cfg.scratch("usr");
    let user_password = format!("Dz-{}!", cfg.scratch("pw"));
    let database_name = cfg.scratch("db");
    assert_scratch_name(&cfg, &database_name);

    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP PROCEDURE IF EXISTS [{schema}].[p_probe]"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
        format!("DROP USER IF EXISTS [{username}]"),
        format!("DROP LOGIN [{username}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        let mut matrix: Vec<(String, String, String)> = Vec::new();

        // 1. query — a full result payload.
        let (d, payload) = run_command(
            &driver,
            &handle,
            "query",
            json!({ "sql": "SELECT 1 AS one" }),
        )
        .await;
        assert_eq!(d, Disposition::Ok, "query: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("query result must be JSON");
        assert!(
            value["results"].is_array() && !value["results"].as_array().unwrap().is_empty(),
            "query result must carry a `results` array: {payload}"
        );
        assert_eq!(value["results"][0]["rows"][0][0], json!(1));
        matrix.push((
            "query".into(),
            d.label().into(),
            "SELECT 1 → results[0].rows[0][0] == 1".into(),
        ));

        // 2. execute — rows-affected envelope.
        let (d, payload) = run_command(
            &driver,
            &handle,
            "execute",
            json!({ "sql": "SELECT 1 AS one" }),
        )
        .await;
        assert_eq!(d, Disposition::Ok, "execute: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("execute result must be JSON");
        assert!(
            value["rowsAffected"].is_number(),
            "execute must report rowsAffected: {payload}"
        );
        matrix.push((
            "execute".into(),
            d.label().into(),
            "rowsAffected present".into(),
        ));

        // 3. query_stream through execute_command — the host routes streaming
        //    through DriverCommandStream, so this path is expected to reject.
        let (d, detail) = run_command(
            &driver,
            &handle,
            "query_stream",
            json!({ "sql": "SELECT 1" }),
        )
        .await;
        assert_eq!(
            d,
            Disposition::Unsupported,
            "execute_command(query_stream) unexpectedly {d:?}: {detail}"
        );
        assert!(!detail.trim().is_empty(), "Unsupported must explain itself");
        matrix.push((
            "query_stream".into(),
            d.label().into(),
            format!("host-routed (execute_driver_command_stream); execute_command says: {detail}"),
        ));

        // 4. list_databases
        let (d, payload) = run_command(&driver, &handle, "list_databases", json!({})).await;
        assert_eq!(d, Disposition::Ok, "list_databases: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("list_databases JSON");
        let databases = value["databases"]
            .as_array()
            .unwrap_or_else(|| panic!("list_databases must return a `databases` array: {payload}"));
        assert!(
            databases.iter().any(|name| name
                .as_str()
                .map(|n| n.eq_ignore_ascii_case(&cfg.database))
                .unwrap_or(false)),
            "list_databases must contain {}: {payload}",
            cfg.database
        );
        matrix.push((
            "list_databases".into(),
            d.label().into(),
            format!("{} database(s), current database present", databases.len()),
        ));

        // 5. create_schema (command) then a table via `execute`.
        let (d, payload) =
            run_command(&driver, &handle, "create_schema", json!({ "name": schema })).await;
        assert_eq!(d, Disposition::Ok, "create_schema: {payload}");
        assert_eq!(
            serde_json::from_str::<JsonValue>(&payload).unwrap()["ok"],
            json!(true)
        );
        let probe = query_resumable(
            &driver,
            &handle,
            &format!("SELECT COUNT(*) FROM sys.schemas WHERE [name] = N'{schema}'"),
        )
        .await
        .expect("sys.schemas probe");
        assert_eq!(
            common::cell_i64(&first_cell(&probe)),
            Some(1),
            "create_schema must make the schema visible in sys.schemas"
        );
        matrix.push((
            "create_schema".into(),
            d.label().into(),
            "visible in sys.schemas".into(),
        ));

        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE {table} ([id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL)"
                ),
            )
            .await
            .unwrap_or_else(|e| panic!("CREATE TABLE failed: {}", redact(&cfg, e.to_string())));
        let inserted = driver
            .execute(&handle, &id_name_batch(&table, 1, 3))
            .await
            .expect("seed insert");
        assert_eq!(inserted, 3, "execute must report 3 inserted rows");

        // A real procedure so the schema-object commands have material.
        driver
            .execute(
                &handle,
                &format!("CREATE PROCEDURE [{schema}].[p_probe] AS SELECT 1 AS one"),
            )
            .await
            .expect("CREATE PROCEDURE");

        // 6. list_tables — concrete fields, not just “no error”.
        let (d, payload) = run_command(
            &driver,
            &handle,
            "list_tables",
            json!({ "database": cfg.database, "schema": schema }),
        )
        .await;
        assert_eq!(d, Disposition::Ok, "list_tables: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("list_tables JSON");
        let tables = value["tables"]
            .as_array()
            .unwrap_or_else(|| panic!("list_tables must return a `tables` array: {payload}"));
        for entry in tables {
            assert!(
                entry["name"].is_string(),
                "every table entry needs a name: {entry}"
            );
            assert!(
                entry["tableType"].is_string(),
                "every table entry needs a tableType: {entry}"
            );
        }
        let scratch_entry = tables
            .iter()
            .find(|entry| entry["name"] == json!("rows"))
            .unwrap_or_else(|| panic!("scratch table missing from list_tables: {payload}"));
        assert_eq!(
            scratch_entry["schema"],
            json!(schema),
            "list_tables must keep the real schema level: {scratch_entry}"
        );
        matrix.push((
            "list_tables".into(),
            d.label().into(),
            format!(
                "{} table(s); scratch table with schema `{schema}`",
                tables.len()
            ),
        ));

        // 7. get_table_schema — column/PK metadata.
        let (d, payload) = run_command(
            &driver,
            &handle,
            "get_table_schema",
            json!({ "table": "rows", "database": cfg.database, "schema": schema }),
        )
        .await;
        assert_eq!(d, Disposition::Ok, "get_table_schema: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("get_table_schema JSON");
        let ts = &value["schema"];
        assert_eq!(ts["tableName"], json!("rows"));
        let columns = ts["columns"]
            .as_array()
            .unwrap_or_else(|| panic!("schema.columns must be an array: {payload}"));
        let id_col = columns
            .iter()
            .find(|c| c["name"] == json!("id"))
            .unwrap_or_else(|| panic!("id column missing: {payload}"));
        assert_eq!(
            id_col["isPrimaryKey"],
            json!(true),
            "id must be the PK: {id_col}"
        );
        assert!(columns.iter().any(|c| c["name"] == json!("name")));
        assert_eq!(ts["primaryKeys"], json!(["id"]));
        matrix.push((
            "get_table_schema".into(),
            d.label().into(),
            format!("{} column(s), primaryKeys [id]", columns.len()),
        ));

        // 7b. list_privileges — shared driver-api command, driver supplies SQL.
        let (d, payload) = run_command(&driver, &handle, "list_privileges", json!({})).await;
        assert_eq!(d, Disposition::Ok, "list_privileges: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("list_privileges JSON");
        let grants = value["grants"]
            .as_array()
            .unwrap_or_else(|| panic!("list_privileges needs a `grants` array: {payload}"));
        for grant in grants {
            assert!(
                grant["grantee"].is_string(),
                "grant needs a grantee: {grant}"
            );
            assert!(
                grant["objectName"].is_string(),
                "grant needs an objectName: {grant}"
            );
            assert!(
                grant["privilege"].is_string(),
                "grant needs a privilege: {grant}"
            );
        }
        matrix.push((
            "list_privileges".into(),
            d.label().into(),
            format!("{} grant(s), well-formed fields", grants.len()),
        ));

        // 7c. list_objects
        let (d, payload) = run_command(
            &driver,
            &handle,
            "list_objects",
            json!({ "kind": "procedure" }),
        )
        .await;
        assert_eq!(d, Disposition::Ok, "list_objects: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("list_objects JSON");
        let objects = value["objects"]
            .as_array()
            .unwrap_or_else(|| panic!("list_objects needs an `objects` array: {payload}"));
        for object in objects {
            assert!(object["kind"].is_string(), "object needs a kind: {object}");
            assert!(object["name"].is_string(), "object needs a name: {object}");
        }
        assert!(
            objects
                .iter()
                .any(|object| object["name"] == json!("p_probe")),
            "the scratch procedure must be listed: {payload}"
        );
        matrix.push((
            "list_objects".into(),
            d.label().into(),
            format!("{} procedure(s), scratch procedure present", objects.len()),
        ));

        // 7d. get_object_ddl
        let (d, payload) = run_command(
            &driver,
            &handle,
            "get_object_ddl",
            json!({ "kind": "procedure", "name": "p_probe", "schema": schema }),
        )
        .await;
        assert_eq!(d, Disposition::Ok, "get_object_ddl: {payload}");
        let value: JsonValue = serde_json::from_str(&payload).expect("get_object_ddl JSON");
        let ddl = value["ddl"]
            .as_str()
            .unwrap_or_else(|| panic!("get_object_ddl needs a `ddl` string: {payload}"));
        assert!(
            ddl.to_ascii_uppercase().contains("PROCEDURE"),
            "the DDL must be the procedure definition: {ddl:?}"
        );
        matrix.push((
            "get_object_ddl".into(),
            d.label().into(),
            format!("{} chars of CREATE PROCEDURE text", ddl.len()),
        ));

        // 8. create_user — valid input; whatever the platform does must be a
        //    clean outcome, never a panic. The password never reaches stdout.
        let (d, detail) = run_command(
            &driver,
            &handle,
            "create_user",
            json!({ "username": username, "password": user_password }),
        )
        .await;
        assert_ne!(
            d,
            Disposition::Unsupported,
            "create_user with a valid input must not be Unsupported: {}",
            redact(&cfg, &detail)
        );
        matrix.push((
            "create_user".into(),
            d.label().into(),
            redact(&cfg, &detail).chars().take(160).collect(),
        ));

        // 9/10. create_database + drop_database — gated, scratch-named, bounded.
        let mut db_disposition = "skipped (allow_create_database or write gate off)".to_string();
        if cfg.allow_create_database {
            let outcome = tokio::time::timeout(
                DB_TIMEOUT,
                run_command(
                    &driver,
                    &handle,
                    "create_database",
                    json!({ "name": database_name }),
                ),
            )
            .await;
            match outcome {
                Ok((d, detail)) => {
                    db_disposition =
                        format!("create_database: {} — {}", d.label(), redact(&cfg, &detail));
                    matrix.push((
                        "create_database".into(),
                        d.label().into(),
                        redact(&cfg, &detail).chars().take(160).collect(),
                    ));
                }
                Err(_) => {
                    db_disposition = "create_database: not completed within 180s".into();
                    matrix.push((
                        "create_database".into(),
                        Disposition::NotCompleted.label().into(),
                        "no answer within 180s; connection abandoned".into(),
                    ));
                }
            }
        } else {
            matrix.push((
                "create_database".into(),
                "skipped".into(),
                "TEST_SQLSERVER_ALLOW_CREATE_DATABASE is not 1".into(),
            ));
        }
        eprintln!("ℹ️  {db_disposition}");

        if cfg.allow_create_database {
            let drop_outcome = tokio::time::timeout(
                DB_TIMEOUT,
                run_command(
                    &driver,
                    &handle,
                    "drop_database",
                    json!({ "name": database_name }),
                ),
            )
            .await;
            match drop_outcome {
                Ok((d, detail)) => matrix.push((
                    "drop_database".into(),
                    d.label().into(),
                    redact(&cfg, &detail).chars().take(160).collect(),
                )),
                Err(_) => matrix.push((
                    "drop_database".into(),
                    Disposition::NotCompleted.label().into(),
                    "no answer within 180s; connection abandoned".into(),
                )),
            }
        } else {
            matrix.push((
                "drop_database".into(),
                "skipped".into(),
                "TEST_SQLSERVER_ALLOW_CREATE_DATABASE is not 1".into(),
            ));
        }

        // Verification/escape hatch. Azure scopes `sys.databases` to the
        // current database (+ `master`), so a server-wide view needs a `master`
        // connection — and that is also where `DROP DATABASE` is legal.
        let mut master = cfg.default_config();
        master.database = Some("master".into());
        master.id = "sqlserver-live-master".into();
        match common::with_resume_retry("master connect", || driver.connect(&master)).await {
            Ok(master_handle) => {
                let count = format!(
                    "SELECT COUNT(*) FROM sys.databases WHERE [name] = N'{}'",
                    database_name.replace('\'', "''")
                );
                let still_there = query_resumable(&driver, &master_handle, &count)
                    .await
                    .map(|r| common::cell_i64(&first_cell(&r)))
                    .ok()
                    .flatten()
                    .unwrap_or(0);
                if still_there > 0 {
                    eprintln!(
                        "⚠️  scratch database `{database_name}` still exists after drop_database; \
                         dropping it from `master`"
                    );
                    let sql = format!("DROP DATABASE [{}]", database_name.replace(']', "]]"));
                    match driver.execute(&master_handle, &sql).await {
                        Ok(_) => eprintln!("✅ dropped `{database_name}` from master"),
                        Err(error) => eprintln!(
                            "❌ could not drop `{database_name}` from master: {}",
                            redact(&cfg, error.to_string())
                        ),
                    }
                } else {
                    eprintln!("ℹ️  no scratch database `{database_name}` present on the server");
                }
                let _ = driver.disconnect(master_handle).await;
            }
            Err(error) => eprintln!(
                "⚠️  could not open a `master` connection to verify scratch-database cleanup: {}",
                redact(&cfg, error.to_string())
            ),
        }

        eprintln!("\n── admin command matrix (live) ─────────────────────────────");
        eprintln!("{:<18} {:<14} {}", "command", "outcome", "evidence");
        for (name, outcome, note) in &matrix {
            eprintln!("{name:<18} {outcome:<14} {note}");
        }
        eprintln!("────────────────────────────────────────────────────────────\n");

        // The advertised surface must be fully accounted for; the surface may
        // grow while other work lands, so only completeness is asserted.
        let advertised: BTreeSet<String> = driver
            .command_definitions()
            .into_iter()
            .map(|d| d.id)
            .collect();
        let reported: BTreeSet<String> = matrix.iter().map(|(name, _, _)| name.clone()).collect();
        let uncovered: Vec<&String> = advertised.difference(&reported).collect();
        assert!(
            uncovered.is_empty(),
            "advertised commands with no matrix entry: {uncovered:?}"
        );
        eprintln!(
            "ℹ️  matrix covers {}/{} advertised command(s)",
            reported.len(),
            advertised.len()
        );
    })
    .await;
}

// ── A3: bogus names and bad inputs fail cleanly ────────────────────

#[tokio::test]
async fn unknown_and_malformed_commands_are_clean_errors() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    for bogus in [
        "definitely_not_a_command",
        "backup_database",
        "restore_database",
        "start_agent_job",
        "list_agent_jobs",
        "sqlcmd",
        "create_view",
        "",
    ] {
        match driver.execute_command(&handle, bogus, json!({})).await {
            Err(DriverError::Unsupported(message)) => {
                assert!(
                    !message.trim().is_empty(),
                    "bogus command `{bogus}` must explain itself"
                );
                eprintln!("ℹ️  `{bogus}` → Unsupported: {message}");
            }
            Ok(result) => panic!("bogus command `{bogus}` succeeded: {}", result.data),
            Err(other) => panic!(
                "bogus command `{bogus}` must be a clean Unsupported, got {}",
                redact(&cfg, other.to_string())
            ),
        }
    }

    // Valid command names with missing required input must be InvalidConfig,
    // never a panic and never an empty message.
    for (command, input, field) in [
        ("create_schema", json!({}), "name"),
        ("create_database", json!({}), "name"),
        ("drop_database", json!({}), "name"),
        ("create_user", json!({}), "username"),
        ("list_tables", json!({}), "database"),
        (
            "get_table_schema",
            json!({ "database": cfg.database }),
            "table",
        ),
        ("list_objects", json!({}), "kind"),
        ("get_object_ddl", json!({}), "kind"),
    ] {
        match driver.execute_command(&handle, command, input).await {
            Err(DriverError::InvalidConfig(message)) => {
                assert!(
                    message.contains(field),
                    "`{command}` must name the missing field `{field}`: {message}"
                );
                eprintln!("ℹ️  `{command}` without `{field}` → InvalidConfig: {message}");
            }
            other => {
                panic!("`{command}` with missing `{field}` must be InvalidConfig, got {other:?}")
            }
        }
    }

    // An unknown object kind must be a clean validation error too.
    match driver
        .execute_command(&handle, "list_objects", json!({ "kind": "wormhole" }))
        .await
    {
        Err(DriverError::InvalidConfig(message)) => {
            assert!(
                message.contains("wormhole"),
                "message must name the kind: {message}"
            );
            eprintln!("ℹ️  `list_objects(kind=wormhole)` → InvalidConfig: {message}");
        }
        other => panic!("an unknown kind must be InvalidConfig, got {other:?}"),
    }

    let _ = driver.disconnect(handle).await;
}

// ── A4: platform limits are clean, not hangs or panics ─────────────

#[tokio::test]
async fn azure_platform_limits_are_clean_errors() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let version = common::scalar(&driver, &handle, "SELECT @@VERSION")
        .await
        .expect("@@VERSION");
    let version = common::cell_string(&version).unwrap_or_default();
    eprintln!(
        "ℹ️  platform={} engine={}",
        cfg.platform.label(),
        version.lines().next().unwrap_or("(no version line)")
    );

    let databases = driver
        .query(&handle, "SELECT [name] FROM sys.databases ORDER BY [name]")
        .await
        .expect("sys.databases must be readable");
    let names: Vec<String> = databases
        .rows
        .iter()
        .filter_map(|row| common::cell_string(&row[0]))
        .collect();
    eprintln!("ℹ️  sys.databases → {names:?}");
    assert!(
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&cfg.database)),
        "the current database must be visible in sys.databases: {names:?}"
    );

    if cfg.platform != Platform::AzureSqlDatabase {
        eprintln!(
            "⏭  {} is not Azure SQL Database; BACKUP/Agent probes are not platform limits here",
            cfg.platform.label()
        );
        let _ = driver.disconnect(handle).await;
        return;
    }

    assert!(
        names.iter().all(|name| !name.eq_ignore_ascii_case("msdb")),
        "Azure SQL Database must not expose instance-scoped catalogs: {names:?}"
    );
    assert!(
        !cfg.platform.supports_backup_restore(),
        "Azure SQL Database has no BACKUP/RESTORE"
    );

    // BACKUP is rejected by the platform. The statement is *read-only* here:
    // Azure SQL Database refuses it outright, so no backup file is produced.
    let backup = driver
        .query(
            &handle,
            &format!(
                "BACKUP DATABASE [{}] TO DISK = N'{}'",
                cfg.database,
                cfg.scratch("probe")
            ),
        )
        .await
        .expect_err("BACKUP DATABASE must be rejected on Azure SQL Database");
    let text = redact(&cfg, backup.to_string());
    assert!(
        text.to_ascii_lowercase().contains("backup")
            || text.contains("40510")
            || text.to_ascii_lowercase().contains("not supported"),
        "the BACKUP rejection must explain itself: {text}"
    );
    eprintln!("ℹ️  BACKUP DATABASE rejected: {text}");

    let restore = driver
        .query(
            &handle,
            "RESTORE DATABASE [dz_test_probe_restore] FROM DISK = N'dz_test_probe_none.bak'",
        )
        .await
        .expect_err("RESTORE must be rejected on Azure SQL Database");
    let text = redact(&cfg, restore.to_string());
    assert!(
        !text.trim().is_empty(),
        "RESTORE rejection must carry a message"
    );
    eprintln!("ℹ️  RESTORE rejected: {text}");

    // No SQL Agent: msdb is not reachable from a user database.
    let agent = driver
        .query(&handle, "SELECT COUNT(*) FROM msdb.dbo.sysjobs")
        .await
        .expect_err("msdb/SQL Agent must not be reachable on Azure SQL Database");
    let text = redact(&cfg, agent.to_string());
    assert!(
        !text.trim().is_empty(),
        "the Agent probe must carry a message"
    );
    eprintln!("ℹ️  SQL Agent probe rejected: {text}");

    // …and the command surface degrades with a clear Unsupported, not a panic.
    for name in [
        "backup_database",
        "restore_database",
        "list_agent_jobs",
        "start_agent_job",
    ] {
        match driver.execute_command(&handle, name, json!({})).await {
            Err(DriverError::Unsupported(message)) => {
                assert!(
                    !message.trim().is_empty(),
                    "`{name}` must explain the refusal"
                );
                eprintln!("ℹ️  {name} → Unsupported: {}", redact(&cfg, &message));
            }
            other => panic!("`{name}` must be a clean Unsupported, got {other:?}"),
        }
    }

    let _ = driver.disconnect(handle).await;
}

// ── B1: the driver's pagination clause ─────────────────────────────

/// The recent regression was a hardcoded `LIMIT`, which T-SQL rejects for
/// every table read. The driver must own `OFFSET … FETCH NEXT … ROWS ONLY`.
#[test]
fn pagination_clause_is_offset_fetch_and_never_limit() {
    let driver = SqlServerDriver::new();
    assert!(driver.supports_offset(), "T-SQL has OFFSET … FETCH");

    let syntax = driver.pagination_syntax(37, 259);
    assert_eq!(
        syntax.clause, "OFFSET 259 ROWS FETCH NEXT 37 ROWS ONLY",
        "unexpected pagination clause"
    );
    assert!(
        syntax.requires_order_by,
        "OFFSET … FETCH requires ORDER BY on T-SQL"
    );
    assert_eq!(syntax.order_by_fallback, Some("(SELECT NULL)"));

    for (limit, offset) in [(1u64, 0u64), (25, 0), (37, 259), (u64::MAX, u64::MAX)] {
        let clause = driver.pagination_syntax(limit, offset).clause;
        assert!(!clause.trim().is_empty(), "clause must never be empty");
        let upper = clause.to_ascii_uppercase();
        assert!(
            upper.contains("OFFSET")
                && upper.contains("ROWS FETCH NEXT")
                && upper.contains("ROWS ONLY"),
            "clause must be OFFSET/FETCH: {clause}"
        );
        assert!(
            !upper.contains("LIMIT"),
            "SQL Server has no LIMIT; got {clause}"
        );
        let sql = format!("SELECT [id] FROM [t] ORDER BY [id] ASC {clause}");
        assert!(
            !sql.to_ascii_uppercase().contains("LIMIT"),
            "generated pagination SQL must never contain LIMIT: {sql}"
        );
    }
}

// ── B2: paging a real 250-row table ────────────────────────────────

#[tokio::test]
async fn paged_read_covers_250_rows_exactly_once() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "paged read over a 250-row scratch table") {
        return;
    }
    let schema = cfg.scratch("page");
    let table = format!("[{schema}].[rows]");
    assert_scratch_name(&cfg, &schema);
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        for batch in 0..5i64 {
            let inserted = driver
                .execute(&handle, &id_name_batch(&table, batch * 50 + 1, 50))
                .await
                .unwrap_or_else(|e| {
                    panic!("batch {batch} insert: {}", redact(&cfg, e.to_string()))
                });
            assert_eq!(inserted, 50, "batch {batch} must insert 50 rows");
        }
        let total = common::scalar(&driver, &handle, &format!("SELECT COUNT(*) FROM {table}"))
            .await
            .expect("count");
        assert_eq!(common::cell_i64(&total), Some(250));

        let page_size = 37u64;
        let mut seen: Vec<i64> = Vec::new();
        let mut page_sqls: Vec<String> = Vec::new();
        for page in 0..8u64 {
            let clause = driver.pagination_syntax(page_size, page * page_size).clause;
            let sql = format!("SELECT [id], [name] FROM {table} ORDER BY [id] ASC {clause}");
            assert!(
                !sql.to_ascii_uppercase().contains("LIMIT"),
                "page SQL must not use LIMIT: {sql}"
            );
            assert!(sql.contains("OFFSET") && sql.contains("FETCH NEXT"));
            page_sqls.push(sql.clone());
            let result = query_resumable(&driver, &handle, &sql)
                .await
                .unwrap_or_else(|e| {
                    panic!("page {page}: {}\nSQL: {sql}", redact(&cfg, e.to_string()))
                });
            assert_eq!(result.columns.len(), 2, "page {page} column metadata");
            let ids = row_ids(&result);
            // 250 = 6 full pages of 37 + a 28-row tail; the page past the tail
            // is empty (documented "short final page" behaviour).
            if page < 6 {
                assert_eq!(ids.len(), 37, "page {page} must be full");
            } else if page == 6 {
                assert_eq!(ids.len(), 28, "page 6 must be the 28-row tail");
            } else {
                assert!(
                    ids.is_empty(),
                    "the page past the end must be empty, got {ids:?}"
                );
            }
            for (row, id) in result.rows.iter().zip(ids.iter()) {
                let name = common::cell_string(&row[1]).expect("name decodes as text");
                assert_eq!(name, format!("row-{id}"), "row {id} name mismatch");
            }
            seen.extend(ids);
        }

        let expected: Vec<i64> = (1..=250).collect();
        assert_eq!(
            seen, expected,
            "pages must return 1..=250 in order, each row exactly once"
        );
        assert_eq!(seen.len(), 250, "no row may be duplicated or dropped");

        // The last non-empty page is short exactly because 250 = 6×37 + 28.
        let short = &page_sqls[6];
        let last = query_resumable(&driver, &handle, short)
            .await
            .expect("last page");
        assert_eq!(
            row_ids(&last).len(),
            28,
            "page 6 (offset 222) must be the short tail: {short}"
        );
        eprintln!("ℹ️  paged 250 rows in 37-row pages; tail page has 28 rows");
    })
    .await;
}

// ── B3: keyset (seek) pagination contract ──────────────────────────

/// Replica of the host's `build_keyset_select_sql` SQL shape
/// (`src-tauri/src/data_sync/keyset.rs`). The host builder lives in the
/// `datazen` binary crate and cannot be linked from a driver test
/// (`Cargo.toml` is off-limits), so the *string contract* is pinned here and
/// then executed live against the real server.
///
/// `none` renders no `WHERE`, `placeholder` renders `($1, $2, …)` exactly like
/// `postgres_placeholder` (which is what the host uses for non-MySQL families),
/// `literal` renders the bound values inline.
#[derive(Clone, Copy)]
enum KeyStyle {
    Placeholder,
    Literal,
}

fn keyset_select_sql(
    qualified: &str,
    columns: &[&str],
    pk: &[&str],
    after_key: Option<&[i64]>,
    clause: &str,
    style: KeyStyle,
) -> String {
    let select_cols = columns
        .iter()
        .map(|c| format!("[{c}]"))
        .collect::<Vec<_>>()
        .join(", ");
    let order_cols = pk
        .iter()
        .map(|c| format!("[{c}] ASC"))
        .collect::<Vec<_>>()
        .join(", ");
    let where_clause = match after_key {
        Some(key) => {
            let idents = pk
                .iter()
                .map(|c| format!("[{c}]"))
                .collect::<Vec<_>>()
                .join(", ");
            let values = match style {
                KeyStyle::Placeholder => (1..=key.len())
                    .map(|i| format!("${i}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                KeyStyle::Literal => key
                    .iter()
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            };
            format!(" WHERE ({idents}) > ({values})")
        }
        None => String::new(),
    };
    format!("SELECT {select_cols} FROM {qualified}{where_clause} ORDER BY {order_cols} {clause}")
}

#[tokio::test]
async fn keyset_pagination_over_a_single_pk_table_is_exact() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "keyset paging over a scratch table") {
        return;
    }
    let schema = cfg.scratch("keyset");
    let table = format!("[{schema}].[rows]");
    let qualified = format!("[{schema}].[rows]");
    assert_scratch_name(&cfg, &schema);
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        for batch in 0..5i64 {
            driver
                .execute(&handle, &id_name_batch(&table, batch * 50 + 1, 50))
                .await
                .expect("seed batch");
        }

        // Byte-level contract: what the host emits for SQL Server family.
        let page_size = 25u64;
        let clause = driver.pagination_syntax(page_size, 0).clause;
        assert_eq!(clause, "OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY");
        let next_page_sql = keyset_select_sql(
            &qualified,
            &["id", "name"],
            &["id"],
            Some(&[25]),
            &clause,
            KeyStyle::Placeholder,
        );
        assert_eq!(
            next_page_sql,
            format!(
                "SELECT [id], [name] FROM [{schema}].[rows] WHERE ([id]) > ($1) \
                 ORDER BY [id] ASC OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY"
            ),
            "host keyset SQL contract drifted"
        );
        assert!(!next_page_sql.to_ascii_uppercase().contains("LIMIT"));
        eprintln!("ℹ️  host keyset SQL for SQL Server: {next_page_sql}");

        // Walk the whole table with literal keys (the driver cannot bind
        // params — see `bound_parameters_are_dropped_for_sql_server`).
        let mut seen: Vec<i64> = Vec::new();
        let mut after: Option<i64> = None;
        for page in 0..12 {
            let key = after.map(|k| [k]);
            let sql = keyset_select_sql(
                &qualified,
                &["id", "name"],
                &["id"],
                key.as_ref().map(|k| k.as_slice()),
                &clause,
                KeyStyle::Literal,
            );
            let result = query_resumable(&driver, &handle, &sql)
                .await
                .unwrap_or_else(|e| panic!("keyset page {page}: {}", redact(&cfg, e.to_string())));
            let ids = row_ids(&result);
            if ids.is_empty() {
                break;
            }
            assert!(
                ids.windows(2).all(|w| w[0] < w[1]),
                "keyset pages must be strictly ascending: {ids:?}"
            );
            if let Some(previous) = after {
                assert!(
                    ids[0] > previous,
                    "page {page} must start after the previous key {previous}"
                );
            }
            after = ids.last().copied();
            seen.extend(ids);
        }
        let expected: Vec<i64> = (1..=250).collect();
        assert_eq!(
            seen, expected,
            "keyset paging must return each of the 250 rows exactly once"
        );
        eprintln!("ℹ️  keyset walk returned {} rows exactly once", seen.len());
    })
    .await;
}

// ── B4: composite-PK keyset predicate (host Finding 1) ─────────────

/// The host builds `WHERE (pk1, pk2) > ($1, $2)` for composite keys. T-SQL has
/// no row-value comparison, so this is expected to be rejected — an error is a
/// defect in the *host* predicate shape, never silently wrong data.
#[tokio::test]
async fn composite_pk_keyset_row_value_predicate_against_tsql() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "composite-PK keyset probe") {
        return;
    }
    let schema = cfg.scratch("ck");
    let table = format!("[{schema}].[shards]");
    assert_scratch_name(&cfg, &schema);
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[tenant] INT NOT NULL, [region] INT NOT NULL, [payload] NVARCHAR(50) NULL, \
             CONSTRAINT [pk_shards] PRIMARY KEY ([tenant], [region])",
        )
        .await;
        let inserted = driver
            .execute(&handle, &tenant_region_batch(&table, 12, 10))
            .await
            .expect("seed composite rows");
        assert_eq!(inserted, 120);

        let clause = driver.pagination_syntax(10, 0).clause;
        let qualified = format!("[{schema}].[shards]");

        // First page has no WHERE and must work.
        let first = keyset_select_sql(
            &qualified,
            &["tenant", "region", "payload"],
            &["tenant", "region"],
            None,
            &clause,
            KeyStyle::Placeholder,
        );
        let page1 = query_resumable(&driver, &handle, &first)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "first keyset page must work: {}",
                    redact(&cfg, e.to_string())
                )
            });
        assert_eq!(page1.rows.len(), 10, "first page must be full: {first}");

        // Continuation page, exactly as the host builds it.
        let continuation = keyset_select_sql(
            &qualified,
            &["tenant", "region", "payload"],
            &["tenant", "region"],
            Some(&[1, 3]),
            &clause,
            KeyStyle::Placeholder,
        );
        eprintln!("ℹ️  host composite-keyset SQL: {continuation}");
        let attempt = query_resumable(&driver, &handle, &continuation).await;
        match attempt {
            Ok(result) => {
                // If T-SQL ever accepts this, the continuation must be correct.
                let rows: Vec<(i64, i64)> = result
                    .rows
                    .iter()
                    .map(|r| {
                        (
                            common::cell_i64(&r[0]).expect("tenant"),
                            common::cell_i64(&r[1]).expect("region"),
                        )
                    })
                    .collect();
                assert!(
                    rows.iter().all(|(t, r)| (*t, *r) > (1, 3)),
                    "row-value comparison must be a strict tuple compare: {rows:?}"
                );
                eprintln!(
                    "⚠️  T-SQL accepted the row-value predicate ({continuation}) — host shape is \
                     fine on this engine version: {rows:?}"
                );
            }
            Err(error) => {
                let text = redact(&cfg, error.to_string());
                assert!(
                    !text.trim().is_empty(),
                    "a rejected predicate must carry a server message"
                );
                eprintln!("❌ host composite-keyset SQL rejected: {text}");
                eprintln!("   SQL: {continuation}");
            }
        }

        // The shape that *does* work on T-SQL, used to prove the data set is
        // otherwise fully pageable: expanded lexicographic predicate.
        let mut seen: Vec<(i64, i64)> = Vec::new();
        let mut after: Option<(i64, i64)> = None;
        for page in 0..14 {
            let sql = match after {
                None => format!(
                    "SELECT [tenant], [region], [payload] FROM {qualified} \
                     ORDER BY [tenant] ASC, [region] ASC {clause}"
                ),
                Some((t, r)) => format!(
                    "SELECT [tenant], [region], [payload] FROM {qualified} \
                     WHERE ([tenant] > {t} OR ([tenant] = {t} AND [region] > {r})) \
                     ORDER BY [tenant] ASC, [region] ASC {clause}"
                ),
            };
            let result = query_resumable(&driver, &handle, &sql)
                .await
                .unwrap_or_else(|e| {
                    panic!("rewritten page {page}: {}", redact(&cfg, e.to_string()))
                });
            let rows: Vec<(i64, i64)> = result
                .rows
                .iter()
                .map(|row| {
                    (
                        common::cell_i64(&row[0]).expect("tenant"),
                        common::cell_i64(&row[1]).expect("region"),
                    )
                })
                .collect();
            if rows.is_empty() {
                break;
            }
            after = rows.last().copied();
            seen.extend(rows);
        }
        let expected: Vec<(i64, i64)> = (1..=12)
            .flat_map(|t| (1..=10).map(move |r| (t, r)))
            .collect();
        assert_eq!(
            seen, expected,
            "the lexicographic rewrite must page all 120 rows exactly once"
        );
        eprintln!(
            "ℹ️  lexicographic rewrite paged {} composite rows exactly once",
            seen.len()
        );
        let _ = page1;
    })
    .await;
}

// ── B5: bound parameters (host Finding 2) ──────────────────────────

/// `SqlServerDriver::query_with_params` ignores `params` and delegates to
/// `query`. That is worse than “no binding”:
///
/// * `$n` is a **T-SQL money literal**, not a parameter, so `WHERE pk > $1`
///   silently compares against the literal `1` — *wrong data, no error*;
/// * `@Pn` is a variable name and fails with “Must declare the scalar
///   variable”;
/// * a parameter-free statement runs and the bound value is simply ignored.
///
/// Everything here runs against a private scratch table so the counts cannot
/// be perturbed by concurrent work on the shared instance.
#[tokio::test]
async fn bound_parameters_are_dropped_for_sql_server() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "bound-parameter probe on a scratch table") {
        return;
    }
    let schema = cfg.scratch("params");
    let table = format!("[{schema}].[rows]");
    assert_scratch_name(&cfg, &schema);
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        create_scratch_table(&driver, &handle, &schema, &table, "[id] INT NOT NULL PRIMARY KEY")
            .await;
        driver
            .execute(
                &handle,
                &format!("INSERT INTO {table} ([id]) VALUES (1), (2), (3)"),
            )
            .await
            .expect("seed");

        // 1. `$1` is not a placeholder in T-SQL — it is the money constant 1.
        let literal = driver
            .query_with_params(&handle, "SELECT $1 AS v, $2 AS w", &[Value::Integer(99)])
            .await
            .expect("T-SQL accepts `$1` as a money literal");
        let v: f64 = common::cell(&literal.rows[0][0])
            .parse()
            .expect("the first column decodes as a number");
        let w: f64 = common::cell(&literal.rows[0][1])
            .parse()
            .expect("the second column decodes as a number");
        assert_eq!(
            (v, w),
            (1.0, 2.0),
            "`$1`/`$2` resolve to the literals 1 and 2, never to the bound 99"
        );
        eprintln!("⚠️  `SELECT $1, $2` with bound 99 → v={v}, w={w}");

        // 2. The exact shape the host keyset source emits, with a bound key of
        //    -5: the bound value is ignored and the literal drives the filter.
        let filtered = format!("SELECT [id] FROM {table} WHERE [id] > $1");
        let via_params = driver
            .query_with_params(&handle, &filtered, &[Value::Integer(-5)])
            .await
            .expect("`$1` runs as a literal, not a syntax error");
        let via_plain = driver
            .query(&handle, &filtered)
            .await
            .expect("the same SQL without params");
        let ids = row_ids(&via_params);
        assert_eq!(
            ids,
            vec![2, 3],
            "`WHERE [id] > $1` must be `> 1` (the literal), not `> -5` (the bound value,              which would return 1,2,3) — SILENTLY WRONG DATA"
        );
        assert_eq!(
            ids,
            row_ids(&via_plain),
            "the bound value changed nothing"
        );

        // 3. `@Pn` is a variable in T-SQL, so this one fails loudly.
        let named = driver
            .query_with_params(&handle, "SELECT @P1 AS v", &[Value::Integer(7)])
            .await
            .expect_err("an undeclared T-SQL variable must fail");
        let named_text = redact(&cfg, named.to_string());
        assert!(
            named_text.to_ascii_lowercase().contains("declare")
                || named_text.contains("@P1"),
            "the `@P1` failure must mention the variable: {named_text}"
        );
        eprintln!("ℹ️  `SELECT @P1` with a bound value → {named_text}");

        // 4. Params passed to a parameter-free statement are ignored outright.
        let unfiltered = driver
            .query_with_params(
                &handle,
                &format!("SELECT COUNT(*) FROM {table}"),
                &[Value::Integer(99)],
            )
            .await
            .expect("a parameter-free statement still runs");
        assert_eq!(
            common::cell_i64(&first_cell(&unfiltered)),
            Some(3),
            "the bound value had no effect on the count"
        );
        eprintln!("⚠️  query_with_params with a bound value returned COUNT(*)=3 (all rows)");

        // 5. …and through the explicit-target wrapper the keyset source uses.
        let at = driver
            .query_with_params_at(
                &handle,
                &filtered,
                &[Value::Integer(-5)],
                SqlTarget::new(Some(&cfg.database), Some(&schema)),
            )
            .await
            .expect("query_with_params_at behaves identically");
        assert_eq!(
            row_ids(&at),
            vec![2, 3],
            "query_with_params_at must not apply the bound value either"
        );

        // 6. The T-SQL parameter form the driver would have to support: an
        //    explicit declaration is accepted by the server.
        let declared = driver
            .query(&handle, "DECLARE @p INT = 7; SELECT @p AS v")
            .await
            .expect("a declared T-SQL variable works");
        assert_eq!(
            common::cell_i64(&declared.rows[0][0]),
            Some(7),
            "T-SQL parameters need a declaration the driver never emits"
        );
    })
    .await;
}

// ── B6: degenerate pagination arguments ────────────────────────────

/// There is no driver-side “empty pagination clause” builder to reject: the
/// clause is generated from integers and is never empty. The host *does*
/// reject an empty clause (`data_sync/keyset.rs`), but that crate is not
/// linkable from here. What the driver must do is produce a clause T-SQL
/// answers cleanly instead of hanging or panicking — including `limit = 0`.
#[tokio::test]
async fn degenerate_pagination_limits_fail_cleanly() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "degenerate pagination probe") {
        return;
    }
    let schema = cfg.scratch("deg");
    let table = format!("[{schema}].[rows]");
    assert_scratch_name(&cfg, &schema);
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY",
        )
        .await;
        driver
            .execute(
                &handle,
                &format!("INSERT INTO {table} ([id]) VALUES (1), (2), (3)"),
            )
            .await
            .expect("seed");

        let zero = driver.pagination_syntax(0, 0);
        assert_eq!(zero.clause, "OFFSET 0 ROWS FETCH NEXT 0 ROWS ONLY");
        assert!(!zero.clause.trim().is_empty());
        let sql = format!("SELECT [id] FROM {table} ORDER BY [id] ASC {}", zero.clause);
        match query_resumable(&driver, &handle, &sql).await {
            Ok(result) => {
                assert!(
                    result.rows.is_empty(),
                    "FETCH NEXT 0 must not return rows: {:?}",
                    row_ids(&result)
                );
                eprintln!("ℹ️  `FETCH NEXT 0 ROWS ONLY` was accepted and returned no rows");
            }
            Err(error) => {
                let text = redact(&cfg, error.to_string());
                eprintln!("ℹ️  `FETCH NEXT 0 ROWS ONLY` rejected cleanly: {text}");
                assert!(!text.trim().is_empty());
                assert!(
                    text.to_ascii_uppercase().contains("FETCH")
                        || text.to_ascii_uppercase().contains("GREATER")
                        || text.to_ascii_uppercase().contains("SYNTAX")
                        || text.to_ascii_uppercase().contains("NEXT"),
                    "the rejection must mention the FETCH clause: {text}"
                );
            }
        }

        // The connection must survive the rejected statement.
        let alive = common::scalar(&driver, &handle, "SELECT 1")
            .await
            .expect("connection must survive a rejected pagination clause");
        assert_eq!(common::cell_i64(&alive), Some(1));

        // A sane clause still works.
        let sane = driver.pagination_syntax(2, 0).clause;
        let rows = query_resumable(
            &driver,
            &handle,
            &format!("SELECT [id] FROM {table} ORDER BY [id] ASC {sane}"),
        )
        .await
        .expect("sane page");
        assert_eq!(row_ids(&rows), vec![1, 2]);
    })
    .await;
}

// ── A5: schema-object commands (routines / triggers / views / privileges) ──

/// `list_objects` / `get_object_ddl` / `list_privileges` are shared driver-api
/// commands: the driver supplies its dialect and dispatch. This pins the fixed
/// state end-to-end — the shipped SQL Server SQL must run (an unquoted
/// `AS schema` alias is a T-SQL syntax error, 156) and the commands must be
/// advertised *and* executable.
#[tokio::test]
async fn schema_object_commands_are_wired_and_their_sql_runs_live() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    // 1. Shipped dialect SQL must run against the live instance.
    let mut candidates: Vec<(String, String)> = vec![(
        "list_privileges".to_string(),
        datazen_driver_api::list_privileges_sql("sqlserver")
            .expect("driver-api has SQL Server privilege SQL"),
    )];
    for kind in [
        datazen_driver_api::ObjectKind::View,
        datazen_driver_api::ObjectKind::Procedure,
        datazen_driver_api::ObjectKind::Function,
        datazen_driver_api::ObjectKind::Trigger,
        datazen_driver_api::ObjectKind::Sequence,
        datazen_driver_api::ObjectKind::Type,
    ] {
        let sql = datazen_driver_api::list_objects_sql("sqlserver", kind)
            .unwrap_or_else(|| panic!("driver-api lists {kind:?} for sqlserver"));
        assert!(
            !sql.contains("AS schema"),
            "`AS schema` is invalid T-SQL (SCHEMA is reserved): {sql}"
        );
        candidates.push((format!("list_objects({kind:?})"), sql));
    }

    let mut rejections: Vec<(String, String)> = Vec::new();
    for (label, sql) in &candidates {
        match query_resumable(&driver, &handle, sql).await {
            Ok(result) => {
                let columns: Vec<String> = result.columns.iter().map(|c| c.name.clone()).collect();
                assert!(
                    columns.iter().any(
                        |c| c.eq_ignore_ascii_case("name") || c.eq_ignore_ascii_case("grantee")
                    ),
                    "{label} must project a stable key column: {columns:?}"
                );
                eprintln!(
                    "✅ {label} ran live: {} row(s), columns {columns:?}",
                    result.rows.len()
                );
            }
            Err(error) => {
                let text = redact(&cfg, error.to_string());
                eprintln!("❌ {label} rejected: {text}");
                rejections.push((label.clone(), text));
            }
        }
    }

    // Every advertised schema-object SQL must run. The trigger listing used to
    // join `sys.schemas` on `sys.triggers.schema_id`, a column that does not
    // exist (error 207); it now reaches the schema through `sys.objects`.
    assert!(
        rejections.is_empty(),
        "every advertised schema-object SQL must run live: {rejections:#?}"
    );

    // 2. Advertised ⇒ executable, with well-formed JSON.
    let advertised: BTreeSet<String> = driver
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    for command in ["list_privileges", "list_objects", "get_object_ddl"] {
        assert!(
            advertised.contains(command),
            "`{command}` must be advertised by the SQL Server driver: {advertised:?}"
        );
    }

    let privileges = driver
        .execute_command(&handle, "list_privileges", json!({}))
        .await
        .expect("list_privileges must execute");
    let grants = privileges.data["grants"]
        .as_array()
        .unwrap_or_else(|| panic!("grants must be an array: {}", privileges.data));
    eprintln!("✅ list_privileges command → {} grant(s)", grants.len());

    let objects = driver
        .execute_command(&handle, "list_objects", json!({ "kind": "view" }))
        .await
        .expect("list_objects must execute");
    let listed = objects.data["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("objects must be an array: {}", objects.data));
    eprintln!("✅ list_objects(view) command → {} object(s)", listed.len());

    // A missing object must fail closed rather than returning an empty or
    // ambiguous definition that the caller could mistake for usable DDL.
    let error = driver
        .execute_command(
            &handle,
            "get_object_ddl",
            json!({ "kind": "procedure", "name": "dz_definitely_absent", "schema": "dbo" }),
        )
        .await
        .expect_err("get_object_ddl must reject a missing object");
    assert!(
        error
            .to_string()
            .contains("Object was not found or its DDL is unavailable"),
        "get_object_ddl should report a missing definition clearly: {error}"
    );
    eprintln!("✅ get_object_ddl(missing) rejected safely: {error}");

    let _ = driver.disconnect(handle).await;
}

// ── B7: the sync adapter itself (live metadata → IR → executable DDL) ──

/// `sync_adapter.rs` is a pure type-mapping adapter (there is no compare/apply
/// API in `driver-api`), so the end-to-end check available from this crate is:
/// live column metadata → `SyncSourceAdapter` → `IRTable` → `SyncTargetAdapter`
/// rendering → a `CREATE TABLE` the server accepts, whose re-read metadata
/// matches the IR.
#[tokio::test]
async fn sync_adapter_maps_live_metadata_and_renders_executable_ddl() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "sync-adapter round trip on a scratch table") {
        return;
    }
    use datazen_driver_api::{IRType, SyncSourceAdapter, SyncTargetAdapter};
    use datazen_driver_sqlserver::SqlServerSyncAdapter;

    let schema = cfg.scratch("sync");
    let source = format!("[{schema}].[source]");
    let target = format!("[{schema}].[copy]");
    assert_scratch_name(&cfg, &schema);
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {target}"),
        format!("DROP TABLE IF EXISTS {source}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    let cfg_outer = cfg.clone();
    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        let cfg = cfg_outer;
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &source,
            "[id] INT IDENTITY(1,1) NOT NULL, [name] NVARCHAR(100) NULL, \
             [price] DECIMAL(10,2) NULL, [active] BIT NOT NULL, \
             [created] DATETIME2 NULL, [uid] UNIQUEIDENTIFIER NULL, \
             CONSTRAINT [pk_source] PRIMARY KEY ([id])",
        )
        .await;

        let live = driver
            .get_table_schema(&handle, "source", &cfg.database, Some(&schema))
            .await
            .expect("live table metadata");
        assert_eq!(live.primary_keys, vec!["id".to_string()]);
        assert!(
            live.columns
                .iter()
                .any(|c| c.name == "id" && c.is_auto_increment),
            "the live IDENTITY column must be flagged: {:?}",
            live.columns
        );

        let adapter = SqlServerSyncAdapter;

        // (a) The adapter's own mapping is correct when it is handed a *full*
        //     type string — that is what the trait asks for.
        let bare = |name: &str, data_type: &str| datazen_driver_api::ColumnSchema {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        };
        assert_eq!(
            adapter
                .column_to_ir(&bare("name", "nvarchar"), Some("nvarchar(100)"))
                .ir_type,
            IRType::Varchar { length: Some(100) }
        );
        assert_eq!(
            adapter
                .column_to_ir(&bare("price", "decimal"), Some("decimal(10,2)"))
                .ir_type,
            IRType::Decimal {
                precision: 10,
                scale: 2
            }
        );

        // (b) The LIVE metadata alone reports `DATA_TYPE` without length or
        //     precision, so the adapter must supply the catalog query the host
        //     uses to recover them (the PostgreSQL adapter does the same).
        assert!(
            live.columns
                .iter()
                .find(|c| c.name == "name")
                .map(|c| c.data_type.eq_ignore_ascii_case("nvarchar"))
                .unwrap_or(false),
            "expected a bare `nvarchar` from get_table_schema: {:?}",
            live.columns
                .iter()
                .map(|c| (c.name.as_str(), c.data_type.as_str()))
                .collect::<Vec<_>>()
        );

        let full_sql = adapter
            .full_column_types_query(&format!("{schema}.source"))
            .expect("the SQL Server adapter must reconstruct declared column types");
        let full_rows = query_resumable(&driver, &handle, &full_sql)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "full column type query failed: {}",
                    redact(&cfg, e.to_string())
                )
            });
        let full_types: std::collections::HashMap<String, String> = full_rows
            .rows
            .iter()
            .filter_map(|row| {
                let name = common::cell_string(row.first()?)?;
                let full = common::cell_string(row.get(1)?)?;
                Some((name, full))
            })
            .collect();
        assert_eq!(
            full_types.get("name").map(String::as_str),
            Some("nvarchar(100)"),
            "declared nvarchar length must survive: {full_types:?}"
        );
        assert_eq!(
            full_types.get("price").map(String::as_str),
            Some("decimal(10,2)"),
            "declared decimal precision/scale must survive: {full_types:?}"
        );
        assert_eq!(
            full_types.get("created").map(String::as_str),
            Some("datetime2(7)"),
            "the default scale is part of the declared type: {full_types:?}"
        );

        let ir = adapter.table_to_ir(&live, Some(&full_types));
        assert_eq!(ir.name, "source");
        assert_eq!(ir.primary_keys, vec!["id".to_string()]);
        let ir_type = |name: &str| -> IRType {
            ir.columns
                .iter()
                .find(|c| c.name == name)
                .unwrap_or_else(|| panic!("column {name} missing from the IR: {ir:?}"))
                .ir_type
                .clone()
        };
        assert_eq!(ir_type("id"), IRType::Int32);
        assert_eq!(ir_type("active"), IRType::Bool);
        assert_eq!(
            ir_type("created"),
            IRType::Timestamp {
                with_timezone: false
            }
        );
        assert_eq!(ir_type("uid"), IRType::Uuid);
        // Length and precision are preserved end to end.
        assert_eq!(
            ir_type("name"),
            IRType::Varchar { length: Some(100) },
            "nvarchar(100) must keep its length"
        );
        assert_eq!(
            ir_type("price"),
            IRType::Decimal {
                precision: 10,
                scale: 2
            },
            "decimal(10,2) must keep its precision and scale"
        );

        // Target side: render a CREATE TABLE and let the server validate it.
        let mut ddl = format!("CREATE TABLE {target} (");
        for (index, column) in ir.columns.iter().enumerate() {
            if index > 0 {
                ddl.push(',');
            }
            ddl.push_str(&format!(
                "{} {}",
                adapter.quote_ident(&column.name),
                adapter.ir_type_to_native(&column.ir_type)
            ));
            if column.is_auto_increment {
                if let Some(keyword) = adapter.auto_increment_keyword() {
                    ddl.push(' ');
                    ddl.push_str(keyword);
                }
            }
            if !column.nullable {
                ddl.push_str(" NOT NULL");
            }
        }
        let pks = ir
            .primary_keys
            .iter()
            .map(|name| adapter.quote_ident(name))
            .collect::<Vec<_>>()
            .join(", ");
        ddl.push_str(&format!(", CONSTRAINT [pk_copy] PRIMARY KEY ({pks}))"));
        assert!(
            ddl.contains("NVARCHAR(100)") && ddl.contains("DECIMAL(10,2)"),
            "the rendered DDL must keep the declared length and precision: {ddl}"
        );
        assert!(
            !ddl.contains("NVARCHAR(MAX)") && !ddl.contains("DECIMAL(38,18)"),
            "the target must not be silently widened: {ddl}"
        );
        driver.execute(&handle, &ddl).await.unwrap_or_else(|e| {
            panic!("rendered DDL must execute: {}", redact(&cfg, e.to_string()))
        });

        // Re-read the copy: the types must have round-tripped.
        let copied = driver
            .get_table_schema(&handle, "copy", &cfg.database, Some(&schema))
            .await
            .expect("copy metadata");
        assert_eq!(copied.primary_keys, vec!["id".to_string()]);
        for column in &ir.columns {
            let round_tripped = copied
                .columns
                .iter()
                .find(|c| c.name == column.name)
                .unwrap_or_else(|| panic!("{} missing from the copy", column.name));
            let native = adapter.ir_type_to_native(&column.ir_type).to_lowercase();
            let actual = round_tripped.data_type.to_lowercase();
            assert!(
                actual.starts_with(native.split('(').next().unwrap_or(&native)),
                "{} must round-trip as {native}, got {actual}",
                column.name
            );
        }

        // Literal rendering is the other half of the target adapter contract.
        assert_eq!(
            adapter.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "1"
        );
        assert_eq!(
            adapter.format_literal(&Some(Value::Bool(false)), &IRType::Bool),
            "0"
        );
        assert_eq!(
            adapter.format_literal(
                &Some(Value::String("O'Brien".into())),
                &IRType::Varchar { length: None }
            ),
            "N'O''Brien'"
        );
        assert_eq!(
            adapter.format_literal(&Some(Value::Integer(42)), &IRType::Int32),
            "42"
        );
        eprintln!(
            "ℹ️  sync adapter mapped {} live columns and its DDL executed",
            ir.columns.len()
        );
    })
    .await;
}

// ── Audit: no scratch object may survive the suite ─────────────────

/// Post-run audit, `#[ignore]`d so it never races with the in-flight tests.
/// Run it explicitly once the suite has finished:
/// `cargo test -p datazen-driver-sqlserver --test live_admin_and_sync -- --ignored --exact scratch_objects_are_reclaimed`
#[tokio::test]
#[ignore = "post-run audit; run explicitly with --ignored after the suite"]
async fn scratch_objects_are_reclaimed() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let tables = driver
        .query(
            &handle,
            &format!(
                "SELECT s.name + '.' + o.name FROM sys.objects o \
                 JOIN sys.schemas s ON s.schema_id = o.schema_id \
                 WHERE (s.name LIKE '{}%' OR o.name LIKE '{}%') AND o.type IN ('U','V') \
                 ORDER BY 1",
                cfg.temp_prefix, cfg.temp_prefix
            ),
        )
        .await
        .expect("scratch-table audit query");
    let leftovers: Vec<String> = tables
        .rows
        .iter()
        .filter_map(|row| common::cell_string(&row[0]))
        .collect();
    eprintln!("ℹ️  leftover scratch tables/views: {leftovers:?}");

    let schemas = driver
        .query(
            &handle,
            &format!(
                "SELECT [name] FROM sys.schemas WHERE [name] LIKE '{}%' ORDER BY [name]",
                cfg.temp_prefix
            ),
        )
        .await
        .expect("scratch-schema audit query");
    let leftover_schemas: Vec<String> = schemas
        .rows
        .iter()
        .filter_map(|row| common::cell_string(&row[0]))
        .collect();
    eprintln!("ℹ️  leftover scratch schemas: {leftover_schemas:?}");

    assert!(
        leftovers.is_empty() && leftover_schemas.is_empty(),
        "scratch objects survived the suite: tables={leftovers:?} schemas={leftover_schemas:?}"
    );

    let _ = driver.disconnect(handle).await;
}

// ── A6: server-status queries behind the command surface ───────────

/// The task's assumed “server status command” list (version, session list,
/// principals, schema objects) does not exist as driver *commands*. The SQL
/// behind each one is checked live here so the report can distinguish “the
/// driver cannot do it” from “the driver does not expose it”.
#[tokio::test]
async fn server_status_surface_is_reachable_via_sql_and_test_connection() {
    let Some(cfg) = live_config() else { return };

    // Server version — the driver's own status API.
    let probe_driver = SqlServerDriver::new();
    let probe_config = cfg.default_config();
    let info = common::with_resume_retry("test_connection", || {
        probe_driver.test_connection(&probe_config)
    })
    .await
    .expect("test_connection must succeed");
    assert!(
        !info.server_version.trim().is_empty(),
        "server_version must be populated"
    );
    assert!(
        !info.server_type.trim().is_empty(),
        "server_type must be populated"
    );
    assert!(
        !info.server_version.contains(&cfg.password),
        "server info must never echo the password"
    );
    eprintln!(
        "ℹ️  test_connection → type={:?} version={:?}",
        info.server_type,
        info.server_version.lines().next().unwrap_or("")
    );

    let (driver, handle) = connect(&cfg).await;

    // Session / process list.
    match query_resumable(
        &driver,
        &handle,
        "SELECT COUNT(*) FROM sys.dm_exec_sessions",
    )
    .await
    {
        Ok(result) => {
            let sessions = common::cell_i64(&first_cell(&result)).unwrap_or(0);
            assert!(sessions >= 1, "at least this session must be visible");
            eprintln!("✅ sys.dm_exec_sessions → {sessions} session(s)");
        }
        Err(error) => {
            let text = redact(&cfg, error.to_string());
            assert!(
                text.to_ascii_lowercase().contains("permission"),
                "the session list must fail with a permission error if it fails: {text}"
            );
            eprintln!("⚠️  sys.dm_exec_sessions denied: {text}");
        }
    }

    // User / principal listing.
    let principals = query_resumable(
        &driver,
        &handle,
        "SELECT COUNT(*) FROM sys.database_principals WHERE [type] IN ('S','U','G','R')",
    )
    .await
    .expect("principal listing must be readable");
    let principal_count = common::cell_i64(&first_cell(&principals)).unwrap_or(0);
    assert!(principal_count >= 1, "at least dbo must be listed");
    eprintln!("✅ sys.database_principals → {principal_count} principal(s)");

    // Privilege listing (the SQL driver-api ships for `list_privileges`).
    let privileges_sql =
        datazen_driver_api::list_privileges_sql("sqlserver").expect("privilege SQL");
    let grants = query_resumable(&driver, &handle, &privileges_sql)
        .await
        .expect("privilege listing must be readable");
    assert_eq!(grants.columns.len(), 4, "grantee/schema/name/privilege");
    eprintln!("✅ privilege listing → {} grant(s)", grants.rows.len());

    // Schema-object listing (routines/triggers/sequences/types).
    let objects = query_resumable(
        &driver,
        &handle,
        "SELECT COUNT(*) FROM sys.objects WHERE [type] IN ('P','FN','IF','TR','SO','U')",
    )
    .await
    .expect("schema object listing must be readable");
    let object_count = common::cell_i64(&first_cell(&objects)).unwrap_or(0);
    eprintln!("✅ sys.objects → {object_count} object(s)");

    // …and every one of these is reachable through `query`, never through an
    // admin command: none of the six status commands exist in the surface.
    let advertised: BTreeSet<String> = driver
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    // …while the schema-object commands *are* part of the surface now.
    for wired in ["list_privileges", "list_objects", "get_object_ddl"] {
        assert!(
            advertised.contains(wired),
            "`{wired}` must be advertised: {advertised:?}"
        );
    }
    for missing in [
        "server_version",
        "server_status",
        "list_sessions",
        "process_list",
        "list_users",
    ] {
        assert!(
            !advertised.contains(missing),
            "`{missing}` is now advertised — update this test to execute it"
        );
    }

    let _ = driver.disconnect(handle).await;
}
