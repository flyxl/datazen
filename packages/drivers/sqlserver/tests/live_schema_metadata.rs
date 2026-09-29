//! Live **schema / metadata** coverage for the SQL Server driver.
//!
//! Every probe runs against the Azure SQL Database configured in
//! `packages/drivers/sqlserver/.env.test` (see `tests/common/mod.rs`). Tests
//! that only read skip when no instance is configured; tests that create
//! objects additionally require `TEST_SQLSERVER_ALLOW_WRITE=1`.
//!
//! What is pinned here:
//! - `get_tables` / `get_table_schema` / `get_columns` / `get_all_columns`
//!   agree with each other on a scratch table with a diverse column set;
//! - identifier edge cases (spaces, `]`, dots, reserved words, leading digits,
//!   unicode, embedded quotes) resolve through the driver's *queries*, not by
//!   hand-built reads;
//! - missing objects fail loudly instead of degrading to an empty column set;
//! - `CREATE VIEW / PROCEDURE / FUNCTION` go through the driver's own-batch
//!   path and are then visible to the catalog surfaces;
//! - SQL Server paging never emits `LIMIT`.
//!
//! Cleanup is panic-proof: the body of each scratch test runs in a child task
//! so a failing assertion cannot skip the `DROP …` teardown.
//!
//! Run from the repo root:
//!   cargo test -p datazen-driver-sqlserver --test live_schema_metadata -- --nocapture --test-threads=2

mod common;

use std::sync::Arc;

use common::{
    cell_i64, cell_string, connect, drop_quietly, live_config, write_allowed, LiveConfig,
};
use datazen_driver_api::{
    ColumnSchema, CommandResult, ConnectionHandle, DatabaseDriver as _, DriverError, QueryResult,
    TableInfo, TableSchema, TableType,
};
use datazen_driver_sqlserver::SqlServerDriver;
use serde_json::json;

// ░░░ small helpers ░░░

/// Bracket-quote an identifier, doubling any embedded `]` (T-SQL rule).
fn bracket(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// `[schema].[table]` with both parts bracket-quoted.
fn qualified(schema: &str, table: &str) -> String {
    format!("{}.{}", bracket(schema), bracket(table))
}

/// `driver.execute` with the failing operation named in the panic.
async fn exec(driver: &SqlServerDriver, handle: &ConnectionHandle, what: &str, sql: &str) {
    if let Err(error) = driver.execute(handle, sql).await {
        panic!("{what} failed: {error}\nSQL: {sql}");
    }
}

/// `driver.query` with the SQL echoed on failure.
async fn query(driver: &SqlServerDriver, handle: &ConnectionHandle, sql: &str) -> QueryResult {
    driver
        .query(handle, sql)
        .await
        .unwrap_or_else(|error| panic!("query failed: {error}\nSQL: {sql}"))
}

/// `driver.execute_command` with the failing command named in the panic.
async fn command(
    driver: &SqlServerDriver,
    handle: &ConnectionHandle,
    name: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    driver.execute_command(handle, name, input).await
}

fn columns_of(schema: &TableSchema) -> Vec<String> {
    schema.columns.iter().map(|c| c.name.clone()).collect()
}

fn find_column<'a>(schema: &'a TableSchema, name: &str) -> &'a ColumnSchema {
    schema
        .columns
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("column {name:?} missing from {:?}", columns_of(schema)))
}

/// Comparable projection of the metadata a driver reports for one column.
type ColumnKey = (String, String, bool, Option<String>, bool, bool);

fn column_key(c: &ColumnSchema) -> ColumnKey {
    (
        c.name.clone(),
        c.data_type.clone(),
        c.nullable,
        c.default_value.clone(),
        c.is_primary_key,
        c.is_auto_increment,
    )
}

fn observed_schema(result: &Result<TableSchema, DriverError>) -> String {
    match result {
        Ok(schema) => format!(
            "Ok({} columns: {:?}, pk: {:?})",
            schema.columns.len(),
            columns_of(schema),
            schema.primary_keys
        ),
        Err(error) => format!("Err({error})"),
    }
}

fn scalar_text(result: &Result<Option<datazen_driver_api::Value>, DriverError>) -> String {
    match result {
        Ok(value) => common::cell(value),
        Err(error) => format!("Err({error})"),
    }
}

// ░░░ panic-proof scratch fixture ░░░

/// Live connection plus the teardown statements for the scratch objects a test
/// creates.
///
/// [`Fixture::run`] executes the body in a child task: an assertion panic is
/// captured there, the teardown always runs, and the panic is re-raised
/// afterwards. Teardown statements run in the order they were tracked, so
/// track `DROP TABLE` before `DROP SCHEMA`.
struct Fixture {
    driver: Arc<SqlServerDriver>,
    handle: ConnectionHandle,
    teardown: Vec<String>,
}

impl Fixture {
    /// Connect for a test that is allowed to write. Nothing is created yet, so
    /// a failure here cannot leak an object.
    async fn start(cfg: &LiveConfig) -> Self {
        let (driver, handle) = connect(cfg).await;
        Self {
            driver,
            handle,
            teardown: Vec::new(),
        }
    }

    fn track(&mut self, sql: impl Into<String>) {
        self.teardown.push(sql.into());
    }

    async fn run<T, F, Fut>(self, body: F) -> T
    where
        F: FnOnce(Arc<SqlServerDriver>, ConnectionHandle) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let Fixture {
            driver,
            handle,
            teardown,
        } = self;
        let child_driver = Arc::clone(&driver);
        let child_handle = handle.clone();
        let body_task = tokio::spawn(async move { body(child_driver, child_handle).await });
        let outcome = body_task.await;

        for sql in &teardown {
            drop_quietly(&driver, &handle, sql).await;
        }
        let _ = driver.disconnect(handle).await;

        match outcome {
            Ok(value) => value,
            Err(join) if join.is_panic() => std::panic::resume_unwind(join.into_panic()),
            Err(join) => panic!("scratch test body task failed: {join}"),
        }
    }
}

#[tokio::test]
async fn composite_key_and_secondary_constraints_preserve_catalog_state() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "composite SQL Server schema metadata") {
        return;
    }

    let schema = cfg.scratch("meta_keys");
    let parent = qualified(&schema, "parent");
    let child = qualified(&schema, "child");
    let db = cfg.database.clone();
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {child}"));
    fixture.track(format!("DROP TABLE IF EXISTS {parent}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            exec(
                &driver,
                &handle,
                "create metadata scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&schema)),
            )
            .await;
            exec(
                &driver,
                &handle,
                "create composite-key parent",
                &format!(
                    "CREATE TABLE {parent} (\
                     [a] INT NOT NULL, [b] INT NOT NULL, \
                     CONSTRAINT [pk_meta_parent] PRIMARY KEY ([b], [a]))"
                ),
            )
            .await;
            exec(
                &driver,
                &handle,
                "create composite-key child and metadata",
                &format!(
                    "CREATE TABLE {child} (\
                     [parent_a] INT NOT NULL, [parent_b] INT NOT NULL, \
                     [local_a] INT NOT NULL, [local_b] INT NOT NULL, \
                     CONSTRAINT [pk_meta_child] PRIMARY KEY ([local_b], [local_a]), \
                     CONSTRAINT [fk_meta_child_parent] FOREIGN KEY ([parent_b], [parent_a]) \
                       REFERENCES {parent} ([b], [a]) ON DELETE CASCADE ON UPDATE NO ACTION, \
                     CONSTRAINT [ck_meta_child_positive] CHECK ([local_a] >= 0))"
                ),
            )
            .await;
            exec(
                &driver,
                &handle,
                "create ordered secondary index",
                &format!(
                    "CREATE NONCLUSTERED INDEX [ix_meta_child_parent] ON {child} ([parent_b], [parent_a])"
                ),
            )
            .await;

            let table_schema = driver
                .get_table_schema(&handle, "child", &db, Some(&schema))
                .await
                .expect("composite metadata should be representable");
            assert_eq!(table_schema.primary_keys, vec!["local_b", "local_a"]);
            assert_eq!(
                columns_of(&table_schema),
                vec!["parent_a", "parent_b", "local_a", "local_b"]
            );

            let all = driver
                .get_all_columns(&handle, &db, Some(&schema))
                .await
                .expect("get_all_columns should preserve the composite key order");
            assert_eq!(all["child"].1, table_schema.primary_keys);

            let index = table_schema
                .indexes
                .iter()
                .find(|index| index.name == "ix_meta_child_parent")
                .expect("secondary index should be present");
            assert_eq!(index.columns, vec!["parent_b", "parent_a"]);
            assert!(!index.is_unique && !index.is_primary);

            assert_eq!(table_schema.foreign_keys.len(), 1);
            let foreign_key = &table_schema.foreign_keys[0];
            assert_eq!(foreign_key.name, "fk_meta_child_parent");
            assert_eq!(foreign_key.columns, vec!["parent_b", "parent_a"]);
            assert_eq!(foreign_key.referenced_table, format!("{}.{}", bracket(&schema), bracket("parent")));
            assert_eq!(foreign_key.referenced_columns, vec!["b", "a"]);
            assert_eq!(foreign_key.on_update, "NO ACTION");
            assert_eq!(foreign_key.on_delete, "CASCADE");

            assert_eq!(table_schema.check_constraints.len(), 1);
            let check = &table_schema.check_constraints[0];
            assert_eq!(check.name, "ck_meta_child_positive");
            assert!(check.expression.contains("local_a"));
            assert!(check.expression.contains('0'));
        })
        .await;
}

// ═══════════════════════ 1. diverse scratch table ═══════════════════════

#[tokio::test]
async fn scratch_table_metadata_round_trip() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "scratch table metadata round trip") {
        return;
    }

    let schema = cfg.scratch("meta");
    let table = "accounts";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            let driver = driver.as_ref();
            exec(
                driver,
                &handle,
                "create scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&schema)),
            )
            .await;
            exec(
                driver,
                &handle,
                "create scratch table",
                &format!(
                    "CREATE TABLE {target} (\n\
                     [id] INT IDENTITY(1,1) NOT NULL CONSTRAINT [pk_accounts] PRIMARY KEY,\n\
                     [note] NVARCHAR(100) NOT NULL CONSTRAINT [df_accounts_note] DEFAULT (N'none'),\n\
                     [amount] DECIMAL(18,4) NULL,\n\
                     [created_at] DATETIME2(3) NOT NULL CONSTRAINT [df_accounts_created] DEFAULT (SYSUTCDATETIME()),\n\
                     [payload] VARBINARY(16) NULL,\n\
                     [token] UNIQUEIDENTIFIER NULL,\n\
                     [big] BIGINT NULL,\n\
                     [ratio] FLOAT NULL\n\
                     )"
                ),
            )
            .await;
            exec(
                driver,
                &handle,
                "seed rows",
                &format!(
                    "INSERT INTO {target} ([note],[amount],[payload],[token],[big],[ratio]) VALUES \
                     (N'first', 1.5000, 0x0102, '6F9619FF-8B86-D011-B42D-00C04FC964FF', 9007199254740993, 1.5), \
                     (N'第二 🚀', -2.2500, 0x00FF, NULL, -1, NULL), \
                     (N'third', NULL, NULL, NULL, NULL, 3.25)"
                ),
            )
            .await;

            let seeded = query(driver, &handle, &format!("SELECT COUNT(*) AS n FROM {target}")).await;
            assert_eq!(
                cell_i64(&seeded.rows[0][0]),
                Some(3),
                "the fixture must really hold 3 rows"
            );

            // ── get_tables ──
            let listed = driver
                .get_tables(&handle, &db, Some(&schema))
                .await
                .unwrap_or_else(|e| panic!("get_tables({schema:?}) failed: {e}"));
            let found = listed
                .iter()
                .find(|t| t.name == table)
                .unwrap_or_else(|| panic!("get_tables must list {table:?} in {schema:?}; got {listed:?}"));
            assert_eq!(
                found.schema.as_deref(),
                Some(schema.as_str()),
                "TableInfo::schema must name the owning schema"
            );
            assert!(
                matches!(found.table_type, TableType::Table),
                "expected a plain table, got {:?}",
                found.table_type
            );
            match found.row_count {
                Some(count) => assert_eq!(count, 3, "TableInfo::row_count must match the seeded rows"),
                None => eprintln!(
                    "ℹ️  get_tables: TableInfo::row_count is null — the SQL Server driver never populates it"
                ),
            }

            let everything = driver
                .get_tables(&handle, &db, None)
                .await
                .expect("unfiltered get_tables must succeed");
            assert!(
                everything
                    .iter()
                    .any(|t| t.name == table && t.schema.as_deref() == Some(schema.as_str())),
                "unfiltered get_tables must include {schema}.{table}"
            );

            let missing_schema = driver
                .get_tables(&handle, &db, Some("dz_no_such_schema_xyz"))
                .await
                .expect("listing a nonexistent schema must succeed with an empty list");
            assert!(
                missing_schema.is_empty(),
                "a nonexistent schema must yield no tables, got {missing_schema:?}"
            );

            // ── get_table_schema ──
            let table_schema = driver
                .get_table_schema(&handle, table, &db, Some(&schema))
                .await
                .unwrap_or_else(|e| panic!("get_table_schema failed: {e}"));
            assert_eq!(table_schema.table_name, table);
            assert_eq!(
                columns_of(&table_schema),
                vec![
                    "id", "note", "amount", "created_at", "payload", "token", "big", "ratio"
                ],
                "column names must follow ORDINAL_POSITION"
            );
            assert_eq!(
                table_schema.primary_keys,
                vec!["id".to_string()],
                "the identity PK must be reported"
            );

            let id = find_column(&table_schema, "id");
            assert_eq!(id.data_type, "int");
            assert!(!id.nullable, "an IDENTITY PK column is NOT NULL");
            assert!(id.is_primary_key, "id must be flagged as the primary key");
            assert!(id.is_auto_increment, "IDENTITY(1,1) must be flagged");
            assert!(id.default_value.is_none(), "an IDENTITY column has no DEFAULT");

            let note = find_column(&table_schema, "note");
            assert_eq!(note.data_type, "nvarchar(100)");
            assert!(!note.nullable);
            assert!(
                note.default_value
                    .as_deref()
                    .is_some_and(|d| d.to_ascii_lowercase().contains("none")),
                "the DEFAULT (N'none') constraint must be surfaced, got {:?}",
                note.default_value
            );

            let amount = find_column(&table_schema, "amount");
            assert_eq!(amount.data_type, "decimal(18,4)");
            assert!(amount.nullable);

            let created = find_column(&table_schema, "created_at");
            assert_eq!(created.data_type, "datetime2(3)");
            assert!(!created.nullable);
            assert!(
                created
                    .default_value
                    .as_deref()
                    .is_some_and(|d| d.to_ascii_lowercase().contains("sysutcdatetime")),
                "the SYSUTCDATETIME() default must be surfaced, got {:?}",
                created.default_value
            );

            assert_eq!(find_column(&table_schema, "payload").data_type, "varbinary(16)");
            assert_eq!(find_column(&table_schema, "token").data_type, "uniqueidentifier");
            assert_eq!(find_column(&table_schema, "big").data_type, "bigint");
            assert_eq!(find_column(&table_schema, "ratio").data_type, "float(53)");
            assert!(find_column(&table_schema, "payload").nullable);
            assert!(find_column(&table_schema, "token").nullable);

            // ── get_columns (trait default) agrees ──
            let (columns, primary_keys) = driver
                .get_columns(&handle, table, &db, Some(&schema))
                .await
                .expect("get_columns must succeed");
            assert_eq!(primary_keys, vec!["id".to_string()]);
            assert_eq!(columns.len(), table_schema.columns.len());
            for (index, (from_columns, from_schema)) in
                columns.iter().zip(table_schema.columns.iter()).enumerate()
            {
                assert_eq!(
                    column_key(from_columns),
                    column_key(from_schema),
                    "get_columns and get_table_schema disagree at position {index}"
                );
            }

            // ── get_all_columns ──
            // Full column-for-column agreement is pinned by
            // `get_all_columns_agrees_with_get_table_schema`; here we only
            // require the table to be keyed at all.
            let all = driver
                .get_all_columns(&handle, &db, Some(&schema))
                .await
                .expect("get_all_columns must succeed");
            let (batch_columns, batch_pks) = all.get(table).unwrap_or_else(|| {
                panic!(
                    "get_all_columns must key {table:?}; keys were {:?}",
                    all.keys().collect::<Vec<_>>()
                )
            });
            eprintln!(
                "ℹ️  get_all_columns({schema:?}) reported {table:?} with {} of {} columns (pk {batch_pks:?})",
                batch_columns.len(),
                table_schema.columns.len()
            );
            // `id` is the first column, so the batch payload still carries the PK.
            assert_eq!(
                batch_pks,
                &vec!["id".to_string()],
                "the identity PK must be reported by get_all_columns"
            );

            // ── explicit cleanup + proof that nothing is left behind ──
            exec(driver, &handle, "drop scratch table", &format!("DROP TABLE IF EXISTS {target}")).await;
            exec(
                driver,
                &handle,
                "drop scratch schema",
                &format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)),
            )
            .await;
            let after = driver
                .get_tables(&handle, &db, Some(&schema))
                .await
                .expect("get_tables after cleanup");
            assert!(
                after.is_empty(),
                "the scratch table must be gone after cleanup, still listed: {after:?}"
            );
        })
        .await;
}

// ═══════════════════════ 2. get_all_columns completeness ═══════════════════════

/// Regression probe: `get_all_columns` is the batch metadata read the host
/// schema cache and the ER diagram use, so it must report **every** column of a
/// table (and its primary key), matching `get_table_schema`.
#[tokio::test]
async fn get_all_columns_agrees_with_get_table_schema() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "get_all_columns agreement") {
        return;
    }

    let schema = cfg.scratch("allcols");
    // The PK is deliberately *not* the first column, so a batch read that keeps
    // only the first row also loses the primary key.
    let table = "three_cols";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            let driver = driver.as_ref();
            exec(
                driver,
                &handle,
                "create scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&schema)),
            )
            .await;
            exec(
                driver,
                &handle,
                "create three-column table",
                &format!(
                    "CREATE TABLE {target} ([a] NVARCHAR(10) NULL, [id] INT NOT NULL PRIMARY KEY, [c] DECIMAL(9,2) NULL)"
                ),
            )
            .await;

            let table_schema = driver
                .get_table_schema(&handle, table, &db, Some(&schema))
                .await
                .expect("get_table_schema must succeed");
            assert_eq!(columns_of(&table_schema), vec!["a", "id", "c"]);
            assert_eq!(table_schema.primary_keys, vec!["id".to_string()]);

            // Control: the catalog really holds one row per column, so a short
            // answer from `get_all_columns` cannot be blamed on the query.
            let control = query(
                driver,
                &handle,
                &format!(
                    "SELECT c.name AS column_name FROM sys.columns c \
                     JOIN sys.objects o ON c.object_id = o.object_id \
                     JOIN sys.schemas s ON o.schema_id = s.schema_id \
                     WHERE o.type IN ('U','V') AND s.name = '{}' AND o.name = '{}' \
                     ORDER BY s.name, o.name, c.column_id",
                    schema.replace('\'', "''"),
                    table.replace('\'', "''")
                ),
            )
            .await;
            assert_eq!(
                control.rows.len(),
                3,
                "the catalog must return one row per column"
            );

            let all = driver
                .get_all_columns(&handle, &db, Some(&schema))
                .await
                .expect("get_all_columns must succeed");
            let (batch, batch_pks) = all.get(table).unwrap_or_else(|| {
                panic!(
                    "get_all_columns must key {table:?}; keys were {:?}",
                    all.keys().collect::<Vec<_>>()
                )
            });
            eprintln!(
                "ℹ️  get_all_columns reported {:?} (pk {batch_pks:?}) for a table with {:?} (pk {:?})",
                batch.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
                columns_of(&table_schema),
                table_schema.primary_keys
            );

            let mut problems: Vec<String> = Vec::new();
            if batch.len() != table_schema.columns.len() {
                problems.push(format!(
                    "reported {} of {} columns: {:?}",
                    batch.len(),
                    table_schema.columns.len(),
                    batch.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
                ));
            }
            if batch_pks != &table_schema.primary_keys {
                problems.push(format!(
                    "primary key list {batch_pks:?} but get_table_schema says {:?}",
                    table_schema.primary_keys
                ));
            }
            let mut expected: Vec<ColumnKey> = table_schema.columns.iter().map(column_key).collect();
            let mut actual: Vec<ColumnKey> = batch.iter().map(column_key).collect();
            expected.sort();
            actual.sort();
            if actual != expected {
                problems.push(format!(
                    "column metadata differs: get_all_columns {actual:?} vs get_table_schema {expected:?}"
                ));
            }
            assert!(
                problems.is_empty(),
                "get_all_columns must agree with get_table_schema:\n  - {}",
                problems.join("\n  - ")
            );
        })
        .await;
}

// ═══════════════════════ 3. identifier edge cases ═══════════════════════

#[tokio::test]
async fn identifier_edge_cases_resolve_through_the_driver() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "identifier edge cases") {
        return;
    }

    let schema = cfg.scratch("edge");
    // A dot, spaces, and a reserved word in the relation name itself.
    let table = "weird.name with space";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            let driver = driver.as_ref();
            exec(
                driver,
                &handle,
                "create scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&schema)),
            )
            .await;
            // `[has]]bracket]` declares the column named `has]bracket`.
            exec(
                driver,
                &handle,
                "create edge-case table",
                &format!(
                    "CREATE TABLE {target} (\n\
                     [col with space] INT NULL,\n\
                     [has]]bracket] NVARCHAR(20) NULL,\n\
                     [a.b] INT NULL,\n\
                     [order] INT NULL,\n\
                     [select] NVARCHAR(20) NULL,\n\
                     [1starts] INT NULL,\n\
                     [normal] INT NULL\n\
                     )"
                ),
            )
            .await;
            exec(
                driver,
                &handle,
                "seed edge-case row",
                &format!(
                    "INSERT INTO {target} ([col with space],[has]]bracket],[a.b],[order],[select],[1starts],[normal]) \
                     VALUES (1, N'oops', 2, 3, N'中文🚀', 4, 5)"
                ),
            )
            .await;

            let expected = vec![
                "col with space",
                "has]bracket",
                "a.b",
                "order",
                "select",
                "1starts",
                "normal",
            ];

            // Round-trip the *data* too, so the dialect path is exercised.
            let row = query(
                driver,
                &handle,
                &format!("SELECT [select], [1starts] FROM {target}"),
            )
            .await;
            assert_eq!(row.rows.len(), 1);
            assert_eq!(cell_string(&row.rows[0][0]).as_deref(), Some("中文🚀"));

            let table_schema = driver
                .get_table_schema(&handle, table, &db, Some(&schema))
                .await
                .unwrap_or_else(|e| panic!("get_table_schema on edge-case names failed: {e}"));
            assert_eq!(columns_of(&table_schema), expected);

            let listed = driver
                .get_tables(&handle, &db, Some(&schema))
                .await
                .expect("get_tables must succeed");
            assert!(
                listed.iter().any(|t| t.name == table),
                "get_tables must list {table:?}, got {listed:?}"
            );

            let all = driver
                .get_all_columns(&handle, &db, Some(&schema))
                .await
                .expect("get_all_columns must succeed");
            let (batch, batch_pks) = all
                .get(table)
                .unwrap_or_else(|| panic!("get_all_columns keys: {:?}", all.keys().collect::<Vec<_>>()));
            eprintln!(
                "ℹ️  get_all_columns(edge cases) -> {:?} (pk {batch_pks:?})",
                batch.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
            );

            // ── same names through the Command API ──
            let result = command(
                driver,
                &handle,
                "list_tables",
                json!({ "database": db, "schema": schema }),
            )
            .await
            .expect("list_tables command must succeed");
            let tables: Vec<TableInfo> =
                serde_json::from_value(result.data["tables"].clone()).expect("tables payload");
            assert!(
                tables.iter().any(|t| t.name == table),
                "list_tables must report {table:?}, got {tables:?}"
            );

            let result = command(
                driver,
                &handle,
                "get_table_schema",
                json!({ "table": table, "database": db, "schema": schema }),
            )
            .await
            .expect("get_table_schema command must succeed");
            let command_schema: TableSchema =
                serde_json::from_value(result.data["schema"].clone()).expect("schema payload");
            assert_eq!(columns_of(&command_schema), expected);
        })
        .await;
}

// ═══════════════════════ 3. unicode identifiers ═══════════════════════

#[tokio::test]
async fn unicode_identifiers_resolve_through_the_driver() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "unicode identifiers") {
        return;
    }

    let ascii_schema = cfg.scratch("uni");
    let unicode_table = "中文表";
    let unicode_schema = "模式";
    let ascii_table = "plain_table";
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!(
        "DROP TABLE IF EXISTS {}",
        qualified(&ascii_schema, unicode_table)
    ));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&ascii_schema)));
    fixture.track(format!(
        "DROP TABLE IF EXISTS {}",
        qualified(unicode_schema, ascii_table)
    ));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(unicode_schema)));

    let db = cfg.database.clone();
    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            let driver = driver.as_ref();
            exec(
                driver,
                &handle,
                "create ascii scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&ascii_schema)),
            )
            .await;
            exec(
                driver,
                &handle,
                "create unicode-named table",
                &format!(
                    "CREATE TABLE {} ([中文列] INT NOT NULL, [plain] INT NULL)",
                    qualified(&ascii_schema, unicode_table)
                ),
            )
            .await;
            exec(
                driver,
                &handle,
                "create unicode scratch schema",
                &format!("CREATE SCHEMA {}", bracket(unicode_schema)),
            )
            .await;
            exec(
                driver,
                &handle,
                "create table in unicode schema",
                &format!(
                    "CREATE TABLE {} ([a] INT NULL)",
                    qualified(unicode_schema, ascii_table)
                ),
            )
            .await;

            // Control 1: without a name literal the catalog reports the real
            // Unicode names, so the objects demonstrably exist.
            let unfiltered = driver
                .get_tables(&handle, &db, None)
                .await
                .expect("unfiltered get_tables must succeed");
            let unicode_table_visible = unfiltered
                .iter()
                .any(|t| t.name == unicode_table && t.schema.as_deref() == Some(ascii_schema.as_str()));
            let unicode_schema_visible = unfiltered
                .iter()
                .any(|t| t.schema.as_deref() == Some(unicode_schema));
            eprintln!("ℹ️  control: unfiltered catalog sees unicode table = {unicode_table_visible}, unicode schema = {unicode_schema_visible}");

            // Control 2: the literal typing the driver's own SQL relies on.
            // A bare `'…'` literal is varchar and is converted with the database
            // collation code page; `N'…'` stays Unicode.
            for (label, literal) in [
                ("varchar", format!("'{unicode_table}'")),
                ("nvarchar", format!("N'{unicode_table}'")),
            ] {
                let sql = format!(
                    "SELECT COUNT(*) AS n FROM INFORMATION_SCHEMA.COLUMNS \
                     WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = {literal}",
                    ascii_schema.replace('\'', "''")
                );
                let probe = driver.query(&handle, &sql).await;
                eprintln!(
                    "ℹ️  control: TABLE_NAME = {label} literal -> {}",
                    scalar_text(&probe.map(|r| r.rows.first().and_then(|row| row.first().cloned()).flatten()))
                );
            }

            // Requirement: a unicode table name resolves through the driver.
            let table_schema = driver
                .get_table_schema(&handle, unicode_table, &db, Some(&ascii_schema))
                .await;
            eprintln!(
                "ℹ️  unicode table name via get_table_schema -> {}",
                observed_schema(&table_schema)
            );
            let table_ok = matches!(&table_schema, Ok(s) if columns_of(s) == vec!["中文列", "plain"]);

            // Requirement: a schema filter with a unicode schema resolves.
            let filtered = driver
                .get_tables(&handle, &db, Some(unicode_schema))
                .await;
            let filtered_len = filtered.as_ref().map(|t| t.len()).unwrap_or(0);
            eprintln!(
                "ℹ️  unicode schema filter via get_tables -> {:?} ({} rows)",
                filtered.as_ref().map(|t| t.iter().map(|x| x.name.clone()).collect::<Vec<_>>()),
                filtered_len
            );
            let schema_ok = matches!(&filtered, Ok(t) if t.iter().any(|x| x.name == ascii_table));

            // Non-literal read paths still work: the unicode *table* is keyed by
            // the catalog's own name, not by a literal comparison.
            let all = driver.get_all_columns(&handle, &db, Some(&ascii_schema)).await;
            eprintln!(
                "ℹ️  unicode table via get_all_columns -> keys {:?}",
                all.as_ref().map(|m| m.keys().cloned().collect::<Vec<_>>())
            );

            assert!(
                table_ok,
                "get_table_schema must resolve the unicode table name {unicode_table:?}; got {}",
                observed_schema(&table_schema)
            );
            assert!(
                schema_ok,
                "get_tables must honour a unicode schema filter {unicode_schema:?}; got {filtered:?}"
            );
        })
        .await;
}

// ═══════════════════════ 4. quoted identifiers ═══════════════════════

#[tokio::test]
async fn embedded_quotes_in_schema_and_table_names_are_escaped() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "quoted identifiers") {
        return;
    }

    // A schema name containing a single quote and a table name containing both
    // a quote and a closing bracket: the driver builds `name = '…'` literals,
    // so `'` must be doubled.
    let schema = format!("{}'s", cfg.scratch("quote"));
    let table = "it's a ]table";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(
            move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
                let driver = driver.as_ref();
                exec(
                    driver,
                    &handle,
                    "create quoted scratch schema",
                    &format!("CREATE SCHEMA {}", bracket(&schema)),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create quoted scratch table",
                    &format!("CREATE TABLE {target} ([ok] INT NULL, [n] NVARCHAR(10) NULL)"),
                )
                .await;

                let listed = driver
                    .get_tables(&handle, &db, Some(&schema))
                    .await
                    .unwrap_or_else(|e| {
                        panic!("get_tables for a quote-bearing schema failed: {e}")
                    });
                assert!(
                    listed.iter().any(|t| t.name == table),
                    "the schema filter must match {schema:?}; got {listed:?}"
                );

                let table_schema = driver
                    .get_table_schema(&handle, table, &db, Some(&schema))
                    .await
                    .unwrap_or_else(|e| panic!("get_table_schema for quoted names failed: {e}"));
                assert_eq!(
                    columns_of(&table_schema),
                    vec!["ok", "n"],
                    "quoted identifiers must resolve to their columns"
                );

                let all = driver
                    .get_all_columns(&handle, &db, Some(&schema))
                    .await
                    .expect("get_all_columns must succeed");
                assert!(
                    all.contains_key(table),
                    "get_all_columns must key {table:?}; keys were {:?}",
                    all.keys().collect::<Vec<_>>()
                );

                let result = command(
                    driver,
                    &handle,
                    "list_tables",
                    json!({ "database": db, "schema": schema }),
                )
                .await
                .expect("list_tables command must succeed");
                let tables: Vec<TableInfo> =
                    serde_json::from_value(result.data["tables"].clone()).expect("tables payload");
                assert!(
                    tables.iter().any(|t| t.name == table),
                    "list_tables must escape the schema literal; got {tables:?}"
                );
            },
        )
        .await;
}

// ═══════════════════════ 5. error paths ═══════════════════════

#[tokio::test]
async fn missing_schema_and_table_fail_loudly() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let missing_table = cfg.scratch("absent");
    let missing_schema = format!("{}_schema", cfg.scratch("absent"));
    let db = cfg.database.clone();

    let by_table = driver
        .get_table_schema(&handle, &missing_table, &db, Some("dbo"))
        .await;
    eprintln!(
        "ℹ️  get_table_schema(missing table, schema dbo) -> {}",
        observed_schema(&by_table)
    );

    let by_schema = driver
        .get_table_schema(&handle, &missing_table, &db, Some(&missing_schema))
        .await;
    eprintln!(
        "ℹ️  get_table_schema(missing table, missing schema) -> {}",
        observed_schema(&by_schema)
    );

    let listing = driver.get_tables(&handle, &db, Some(&missing_schema)).await;
    eprintln!(
        "ℹ️  get_tables(missing schema) -> {:?}",
        listing.as_ref().map(|t| t.len()).map_err(|e| e.to_string())
    );

    let no_schema = driver
        .get_table_schema(&handle, &missing_table, &db, None)
        .await;
    eprintln!(
        "ℹ️  get_table_schema(schema = None) -> {}",
        observed_schema(&no_schema)
    );

    // A schema-aware driver must reject a single-table read without a schema.
    match &no_schema {
        Err(DriverError::InvalidConfig(message)) => {
            assert!(
                message.contains("schema"),
                "the InvalidConfig message should mention the schema requirement, got: {message}"
            );
        }
        other => panic!("expected InvalidConfig for a schema-less table read, got {other:?}"),
    }

    // Listing a nonexistent schema is not an error: it simply has no tables.
    let listing = listing.expect("listing a missing schema must succeed");
    assert!(
        listing.is_empty(),
        "a nonexistent schema must yield no tables, got {listing:?}"
    );

    // A missing relation must fail instead of degrading to a column-less table
    // (the `Ok(TableSchema { columns: [] })` answer callers then cache).
    let Err(by_table_error) = by_table else {
        panic!("a missing table must not report success, got {by_table:?}");
    };
    assert!(
        matches!(by_table_error, DriverError::QueryFailed(_)),
        "expected DriverError::QueryFailed for a missing table, got {by_table_error:?}"
    );

    let Err(by_schema_error) = by_schema else {
        panic!("a missing schema must not report success, got {by_schema:?}");
    };
    assert!(
        matches!(by_schema_error, DriverError::QueryFailed(_)),
        "expected DriverError::QueryFailed for a missing schema, got {by_schema_error:?}"
    );

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn empty_table_returns_columns_with_zero_rows() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "empty table read") {
        return;
    }

    let schema = cfg.scratch("empty");
    let table = "no_rows";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(
            move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
                let driver = driver.as_ref();
                exec(
                    driver,
                    &handle,
                    "create scratch schema",
                    &format!("CREATE SCHEMA {}", bracket(&schema)),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create empty table",
                    &format!("CREATE TABLE {target} ([a] INT NULL, [b] NVARCHAR(10) NULL)"),
                )
                .await;

                let result = query(driver, &handle, &format!("SELECT * FROM {target}")).await;
                assert_eq!(
                    result
                        .columns
                        .iter()
                        .map(|c| c.name.clone())
                        .collect::<Vec<_>>(),
                    vec!["a", "b"],
                    "an empty table must still describe its columns"
                );
                assert!(
                    result.rows.is_empty(),
                    "an empty table must return zero rows"
                );

                let table_schema = driver
                    .get_table_schema(&handle, table, &db, Some(&schema))
                    .await
                    .expect("get_table_schema on an empty table must succeed");
                assert_eq!(columns_of(&table_schema), vec!["a", "b"]);
                assert!(table_schema.primary_keys.is_empty());

                // The Command API path must behave the same way.
                let result = command(
                    driver,
                    &handle,
                    "query",
                    json!({ "sql": format!("SELECT * FROM {target}") }),
                )
                .await
                .expect("query command must succeed");
                let statement = &result.data["results"][0];
                assert_eq!(
                    statement["columns"].as_array().map(|c| c.len()),
                    Some(2),
                    "the command payload must describe both columns: {statement}"
                );
                assert!(
                    statement["rows"].as_array().is_some_and(|r| r.is_empty()),
                    "the command payload must carry zero rows: {statement}"
                );
            },
        )
        .await;
}

// ═══════════════════════ 6. pagination ═══════════════════════

#[tokio::test]
async fn pagination_never_emits_the_limit_keyword() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "pagination probes") {
        return;
    }

    let schema = cfg.scratch("page");
    let table = "paged";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            let driver = driver.as_ref();
            exec(
                driver,
                &handle,
                "create scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&schema)),
            )
            .await;
            exec(
                driver,
                &handle,
                "create paged table",
                &format!("CREATE TABLE {target} ([id] INT NOT NULL PRIMARY KEY, [label] NVARCHAR(20) NULL)"),
            )
            .await;
            exec(
                driver,
                &handle,
                "seed paged table",
                &format!(
                    "INSERT INTO {target} ([id],[label]) VALUES (1,N'a'),(2,N'b'),(3,N'c'),(4,N'd'),(5,N'e')"
                ),
            )
            .await;

            let syntax = driver.pagination_syntax(2, 0);
            let clause_upper = syntax.clause.to_ascii_uppercase();
            assert!(
                !clause_upper.contains("LIMIT"),
                "SQL Server has no LIMIT; the driver emitted {:?}",
                syntax.clause
            );
            assert_eq!(syntax.clause, "OFFSET 0 ROWS FETCH NEXT 2 ROWS ONLY");
            assert!(clause_upper.contains("OFFSET") && clause_upper.contains("FETCH NEXT"));
            assert!(syntax.requires_order_by, "OFFSET/FETCH requires ORDER BY");
            assert_eq!(syntax.order_by_fallback.as_deref(), Some("(SELECT NULL)"));

            let page1 = query(
                driver,
                &handle,
                &format!("SELECT [id] FROM {target} ORDER BY [id] ASC {}", syntax.clause),
            )
            .await;
            assert_eq!(
                page1
                    .rows
                    .iter()
                    .map(|r| cell_i64(&r[0]))
                    .collect::<Vec<_>>(),
                vec![Some(1), Some(2)],
                "the driver clause must page without LIMIT"
            );

            let page2_clause = driver.pagination_syntax(2, 2).clause;
            let page2 = query(
                driver,
                &handle,
                &format!("SELECT [id] FROM {target} ORDER BY [id] ASC {page2_clause}"),
            )
            .await;
            assert_eq!(
                page2
                    .rows
                    .iter()
                    .map(|r| cell_i64(&r[0]))
                    .collect::<Vec<_>>(),
                vec![Some(3), Some(4)]
            );

            // Negative control: the host's old MySQL-style suffix really is
            // invalid T-SQL, so the pages above could not have contained it.
            let legacy = driver
                .query(
                    &handle,
                    &format!("SELECT [id] FROM {target} ORDER BY [id] ASC LIMIT 2 OFFSET 0"),
                )
                .await
                .expect_err("T-SQL must reject a LIMIT suffix");
            match legacy {
                DriverError::QueryFailed(message) => assert!(
                    message.to_ascii_uppercase().contains("LIMIT"),
                    "expected a LIMIT syntax error, got: {message}"
                ),
                other => panic!("expected QueryFailed, got {other:?}"),
            }

            // The host path: the `query` command with a result cap must cap with
            // TOP/OFFSET-FETCH, never LIMIT.
            let result = command(
                driver,
                &handle,
                "query",
                json!({
                    "sql": format!("SELECT [id] FROM {target} ORDER BY [id] ASC"),
                    "limit": 2
                }),
            )
            .await
            .expect("capped query command must succeed");
            assert_eq!(
                result.data["results"][0]["rows"].as_array().map(|r| r.len()),
                Some(2),
                "the result cap must be applied with T-SQL syntax: {}",
                result.data
            );
        })
        .await;
}

// ═══════════════════════ 7. views / procedures / functions ═══════════════════════

#[tokio::test]
async fn view_procedure_and_function_create_and_surface_in_the_catalog() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "programmable object creation") {
        return;
    }

    let schema = cfg.scratch("prog");
    let base = "base";
    let view = "v_base";
    let proc = "p_count";
    let func = "f_inc";
    let base_target = qualified(&schema, base);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP VIEW IF EXISTS {}", qualified(&schema, view)));
    fixture.track(format!(
        "DROP PROCEDURE IF EXISTS {}",
        qualified(&schema, proc)
    ));
    fixture.track(format!(
        "DROP FUNCTION IF EXISTS {}",
        qualified(&schema, func)
    ));
    fixture.track(format!("DROP TABLE IF EXISTS {base_target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
            let driver = driver.as_ref();
            exec(
                driver,
                &handle,
                "create scratch schema",
                &format!("CREATE SCHEMA {}", bracket(&schema)),
            )
            .await;
            exec(
                driver,
                &handle,
                "create base table",
                &format!("CREATE TABLE {base_target} ([id] INT NOT NULL PRIMARY KEY, [label] NVARCHAR(20) NULL)"),
            )
            .await;
            exec(
                driver,
                &handle,
                "seed base table",
                &format!("INSERT INTO {base_target} ([id],[label]) VALUES (1,N'one'),(2,N'two')"),
            )
            .await;

            // These three statements are only legal as the first statement of a
            // batch; the driver must route them through `simple_query`.
            exec(
                driver,
                &handle,
                "create view",
                &format!(
                    "CREATE VIEW {} AS SELECT [id], [label] FROM {base_target}",
                    qualified(&schema, view)
                ),
            )
            .await;
            exec(
                driver,
                &handle,
                "create procedure",
                &format!(
                    "CREATE PROCEDURE {} AS BEGIN SET NOCOUNT ON; SELECT COUNT(*) AS [n] FROM {base_target}; END",
                    qualified(&schema, proc)
                ),
            )
            .await;
            exec(
                driver,
                &handle,
                "create function",
                &format!(
                    "CREATE FUNCTION {}(@x INT) RETURNS INT AS BEGIN RETURN @x + 1; END",
                    qualified(&schema, func)
                ),
            )
            .await;

            // The objects are callable.
            let proc_rows = query(driver, &handle, &format!("EXEC {}", qualified(&schema, proc))).await;
            assert_eq!(proc_rows.rows.len(), 1, "the procedure must return its count");
            assert_eq!(cell_i64(&proc_rows.rows[0][0]), Some(2));

            let func_value = query(
                driver,
                &handle,
                &format!("SELECT {}(41) AS v", qualified(&schema, func)),
            )
            .await;
            assert_eq!(cell_i64(&func_value.rows[0][0]), Some(42));

            // The view is a relation in the catalog and carries real columns.
            let listed = driver
                .get_tables(&handle, &db, Some(&schema))
                .await
                .expect("get_tables must succeed");
            let view_info = listed
                .iter()
                .find(|t| t.name == view)
                .unwrap_or_else(|| panic!("get_tables must list the view; got {listed:?}"));
            assert!(
                matches!(view_info.table_type, TableType::View),
                "expected TableType::View, got {:?}",
                view_info.table_type
            );
            assert_eq!(view_info.schema.as_deref(), Some(schema.as_str()));

            let view_schema = driver
                .get_table_schema(&handle, view, &db, Some(&schema))
                .await
                .expect("get_table_schema on a view must succeed");
            assert_eq!(columns_of(&view_schema), vec!["id", "label"]);

            // The view must be readable with the same column shape.
            let view_rows = query(
                driver,
                &handle,
                &format!("SELECT * FROM {}", qualified(&schema, view)),
            )
            .await;
            assert_eq!(
                view_rows
                    .columns
                    .iter()
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>(),
                vec!["id", "label"]
            );
            assert_eq!(view_rows.rows.len(), 2);

            // DDL text for each programmable object is retrievable and usable.
            for (object, needle) in [
                (proc, "PROCEDURE"),
                (func, "FUNCTION"),
                (view, "VIEW"),
            ] {
                let ddl = driver
                    .query(
                        &handle,
                        &format!(
                            "SELECT OBJECT_DEFINITION(OBJECT_ID('{}.{}')) AS ddl",
                            schema.replace('\'', "''"),
                            object.replace('\'', "''")
                        ),
                    )
                    .await
                    .unwrap_or_else(|e| panic!("OBJECT_DEFINITION({object}) failed: {e}"));
                let text = cell_string(&ddl.rows[0][0]).unwrap_or_default();
                assert!(
                    text.to_ascii_uppercase().contains(needle),
                    "DDL for {object} must contain {needle}, got {text:?}"
                );
            }
        })
        .await;
}

// ═══════════════════════ 8. schema-object command surface ═══════════════════════

#[tokio::test]
async fn object_listing_and_ddl_commands_surface_views_procedures_and_functions() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "schema object commands") {
        return;
    }

    let schema = cfg.scratch("obj");
    let base = "base";
    let view = "v_obj";
    let proc = "p_obj";
    let func = "f_obj";
    let base_target = qualified(&schema, base);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP VIEW IF EXISTS {}", qualified(&schema, view)));
    fixture.track(format!(
        "DROP PROCEDURE IF EXISTS {}",
        qualified(&schema, proc)
    ));
    fixture.track(format!(
        "DROP FUNCTION IF EXISTS {}",
        qualified(&schema, func)
    ));
    fixture.track(format!("DROP TABLE IF EXISTS {base_target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    fixture
        .run(
            move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
                let driver = driver.as_ref();
                exec(
                    driver,
                    &handle,
                    "create scratch schema",
                    &format!("CREATE SCHEMA {}", bracket(&schema)),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create base table",
                    &format!("CREATE TABLE {base_target} ([id] INT NOT NULL PRIMARY KEY)"),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create view",
                    &format!(
                        "CREATE VIEW {} AS SELECT [id] FROM {base_target}",
                        qualified(&schema, view)
                    ),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create procedure",
                    &format!(
                        "CREATE PROCEDURE {} AS BEGIN SET NOCOUNT ON; SELECT 1 AS [n]; END",
                        qualified(&schema, proc)
                    ),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create function",
                    &format!(
                        "CREATE FUNCTION {}(@x INT) RETURNS INT AS BEGIN RETURN @x; END",
                        qualified(&schema, func)
                    ),
                )
                .await;

                // ── evidence: what the driver's own command catalog advertises ──
                let ids: Vec<String> = driver
                    .command_definitions()
                    .iter()
                    .map(|d| d.id.clone())
                    .collect();
                eprintln!("ℹ️  driver command ids: {ids:?}");

                let mut problems: Vec<String> = Vec::new();
                for required in ["list_objects", "get_object_ddl"] {
                    if !ids.iter().any(|id| id == required) {
                        problems.push(format!("command_definitions is missing `{required}`"));
                    }
                }

                // ── evidence: what the commands actually return ──
                for kind in ["view", "procedure", "function"] {
                    let result =
                        command(driver, &handle, "list_objects", json!({ "kind": kind })).await;
                    eprintln!(
                        "ℹ️  list_objects(kind={kind}) -> {}",
                        match &result {
                            Ok(r) => r.data.to_string(),
                            Err(e) => format!("Err({e})"),
                        }
                    );
                    match result {
                        Ok(r) => {
                            let objects = r.data["objects"].as_array().cloned().unwrap_or_default();
                            let found = objects.iter().any(|o| {
                                o["name"].as_str().is_some_and(|name| {
                                    (kind == "view" && name == view)
                                        || (kind == "procedure" && name == proc)
                                        || (kind == "function" && name == func)
                                })
                            });
                            if !found {
                                problems.push(format!(
                                    "list_objects(kind={kind}) did not report our {kind}"
                                ));
                            }
                        }
                        Err(e) => problems.push(format!("list_objects(kind={kind}) failed: {e}")),
                    }
                }

                for (kind, name, needle) in [
                    ("view", view, "VIEW"),
                    ("procedure", proc, "PROCEDURE"),
                    ("function", func, "FUNCTION"),
                ] {
                    let result = command(
                        driver,
                        &handle,
                        "get_object_ddl",
                        json!({ "kind": kind, "name": name, "schema": schema }),
                    )
                    .await;
                    eprintln!(
                        "ℹ️  get_object_ddl(kind={kind}, name={name}) -> {}",
                        match &result {
                            Ok(r) => r.data.to_string(),
                            Err(e) => format!("Err({e})"),
                        }
                    );
                    match result {
                        Ok(r) => {
                            let ddl = r.data["ddl"].as_str().unwrap_or_default();
                            if !ddl.to_ascii_uppercase().contains(needle) {
                                problems.push(format!(
                                    "get_object_ddl(kind={kind}) returned no usable DDL: {ddl:?}"
                                ));
                            }
                        }
                        Err(e) => problems.push(format!("get_object_ddl(kind={kind}) failed: {e}")),
                    }
                }

                // ── positive control: the catalog SQL recipe works when run raw, so
                //    the gap (if any) is the command wiring, not the catalog access.
                let catalogue = query(
                    driver,
                    &handle,
                    "SELECT s.name AS [schema], o.name AS [name] \
                 FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
                 WHERE o.type IN ('P','PC','FN','IF','TF') ORDER BY 1, 2",
                )
                .await;
                let routine_names: Vec<String> = catalogue
                    .rows
                    .iter()
                    .filter(|row| cell_string(&row[0]).as_deref() == Some(schema.as_str()))
                    .filter_map(|row| cell_string(&row[1]))
                    .collect();
                eprintln!("ℹ️  raw catalog recipe found routines in {schema:?}: {routine_names:?}");
                for expected in [proc, func] {
                    assert!(
                        routine_names.iter().any(|name| name == expected),
                        "the raw catalog recipe must find {expected:?}; got {routine_names:?}"
                    );
                }
                let view_catalogue = query(
                    driver,
                    &handle,
                    "SELECT s.name AS [schema], v.name AS [name] \
                 FROM sys.views v JOIN sys.schemas s ON s.schema_id = v.schema_id ORDER BY 1, 2",
                )
                .await;
                assert!(
                    view_catalogue.rows.iter().any(|row| {
                        cell_string(&row[0]).as_deref() == Some(schema.as_str())
                            && cell_string(&row[1]).as_deref() == Some(view)
                    }),
                    "the raw catalog recipe must find the view"
                );

                assert!(
                    problems.is_empty(),
                    "the schema-object command surface is incomplete:\n  - {}",
                    problems.join("\n  - ")
                );
            },
        )
        .await;
}

// ═══════════════════════ 9. cross-schema same-named tables ═══════════════════════

#[tokio::test]
async fn get_all_columns_is_keyed_by_bare_table_name_across_schemas() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "same-named tables in two schemas") {
        return;
    }

    let schema_a = format!("{}_a", cfg.scratch("dup"));
    let schema_b = format!("{}_b", cfg.scratch("dup"));
    let table = "dup";
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!(
        "DROP TABLE IF EXISTS {}",
        qualified(&schema_a, table)
    ));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema_a)));
    fixture.track(format!(
        "DROP TABLE IF EXISTS {}",
        qualified(&schema_b, table)
    ));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema_b)));

    let db = cfg.database.clone();
    fixture
        .run(
            move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
                let driver = driver.as_ref();
                for (schema, column) in [(&schema_a, "a_only"), (&schema_b, "b_only")] {
                    exec(
                        driver,
                        &handle,
                        "create scratch schema",
                        &format!("CREATE SCHEMA {}", bracket(schema)),
                    )
                    .await;
                    exec(
                        driver,
                        &handle,
                        "create same-named table",
                        &format!(
                            "CREATE TABLE {} ([{column}] INT NULL)",
                            qualified(schema, table)
                        ),
                    )
                    .await;
                }

                // A schema-narrowed read is unambiguous.
                let narrowed = driver
                    .get_all_columns(&handle, &db, Some(&schema_b))
                    .await
                    .expect("schema-narrowed get_all_columns must succeed");
                let (columns_b, _) = narrowed
                    .get(table)
                    .unwrap_or_else(|| panic!("get_all_columns must key {table:?}"));
                assert_eq!(
                    columns_b.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
                    vec!["b_only"],
                    "the schema filter must select only {schema_b}"
                );

                // An unfiltered read has one slot per bare table name, so only one
                // of the two schemas can be represented.
                let mixed = driver
                    .get_all_columns(&handle, &db, None)
                    .await
                    .expect("unfiltered get_all_columns must succeed");
                let (columns, _) = mixed
                    .get(table)
                    .unwrap_or_else(|| panic!("get_all_columns must key {table:?}"));
                let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
                eprintln!(
                    "⚠️  get_all_columns(unfiltered) reported {table:?} -> {names:?}; \
                 the same-named table in the other schema is not representable"
                );
                assert_eq!(
                    names.len(),
                    1,
                    "the payload has one slot per bare table name"
                );
                assert!(
                    names == vec!["a_only".to_string()] || names == vec!["b_only".to_string()],
                    "the surviving definition must come from exactly one schema, got {names:?}"
                );
            },
        )
        .await;
}

// ═══════════════════════ 10. database dimension ═══════════════════════

#[tokio::test]
async fn catalog_reads_honour_the_database_dimension() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;
    let db = cfg.database.clone();

    let databases = driver
        .get_databases(&handle)
        .await
        .expect("get_databases must succeed");
    eprintln!("ℹ️  get_databases -> {databases:?}");
    assert!(
        databases.iter().any(|name| name.eq_ignore_ascii_case(&db)),
        "the connected database must be listed"
    );
    if !cfg.platform.lists_all_databases() {
        assert!(
            databases.len() <= 2,
            "{} scopes sys.databases to the current database (+ master), got {databases:?}",
            cfg.platform.label()
        );
    }

    // An explicitly qualified read of the connected database resolves.
    let self_read = driver
        .get_tables(&handle, &db, None)
        .await
        .expect("get_tables with an explicit database must succeed");
    eprintln!(
        "ℹ️  get_tables(database={db:?}) -> {} relations",
        self_read.len()
    );
    assert!(
        self_read.iter().all(|t| t.schema.is_some()),
        "every listed relation must carry a schema"
    );

    // A blank database means "the session's current catalog".
    let blank = driver.get_tables(&handle, "", Some("dbo")).await;
    eprintln!(
        "ℹ️  get_tables(database=\"\", schema=dbo) -> {:?}",
        blank.as_ref().map(|t| t.len()).map_err(|e| e.to_string())
    );
    assert!(
        blank.is_ok(),
        "a blank database must fall back to the current catalog"
    );

    // A database that does not exist must fail, not hang or silently degrade.
    let missing = driver
        .get_tables(&handle, "dz_no_such_database_xyz", None)
        .await;
    eprintln!(
        "ℹ️  get_tables(missing database) -> {:?}",
        match &missing {
            Ok(t) => format!("Ok({} relations)", t.len()),
            Err(e) => format!("Err({e})"),
        }
    );
    assert!(
        matches!(missing, Err(DriverError::QueryFailed(_))),
        "a nonexistent database must produce QueryFailed, got {missing:?}"
    );

    // Platform probe (informational): a cross-database read of `master`.
    match driver.get_tables(&handle, "master", None).await {
        Ok(rows) => {
            eprintln!(
                "ℹ️  cross-database read of `master` -> {} relations",
                rows.len()
            );
            assert!(
                rows.iter().all(|t| t.schema.is_some()),
                "master relations must carry a schema"
            );
        }
        Err(error) => eprintln!("⚠️  cross-database read of `master` failed: {error}"),
    }
    match driver
        .query(&handle, "SELECT COUNT(*) AS n FROM [master].sys.databases")
        .await
    {
        Ok(result) => eprintln!(
            "ℹ️  [master].sys.databases count -> {}",
            common::cell(&result.rows[0][0])
        ),
        Err(error) => eprintln!("⚠️  [master].sys.databases probe failed: {error}"),
    }

    let _ = driver.disconnect(handle).await;
}

// ═══════════════════════ 11. cleanup is idempotent ═══════════════════════

#[tokio::test]
async fn scratch_cleanup_is_idempotent_and_leaves_no_residue() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "cleanup idempotence") {
        return;
    }

    let schema = cfg.scratch("clean");
    let table = "t";
    let target = qualified(&schema, table);
    let mut fixture = Fixture::start(&cfg).await;
    fixture.track(format!("DROP TABLE IF EXISTS {target}"));
    fixture.track(format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)));

    let db = cfg.database.clone();
    fixture
        .run(
            move |driver: Arc<SqlServerDriver>, handle: ConnectionHandle| async move {
                let driver = driver.as_ref();
                exec(
                    driver,
                    &handle,
                    "create scratch schema",
                    &format!("CREATE SCHEMA {}", bracket(&schema)),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "create scratch table",
                    &format!("CREATE TABLE {target} ([id] INT NULL)"),
                )
                .await;

                exec(
                    driver,
                    &handle,
                    "drop table",
                    &format!("DROP TABLE IF EXISTS {target}"),
                )
                .await;
                exec(
                    driver,
                    &handle,
                    "drop schema",
                    &format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)),
                )
                .await;

                // The same statements again must be harmless (they are what the
                // guard runs, so they must never fail or panic after a real drop).
                drop_quietly(driver, &handle, &format!("DROP TABLE IF EXISTS {target}")).await;
                drop_quietly(
                    driver,
                    &handle,
                    &format!("DROP SCHEMA IF EXISTS {}", bracket(&schema)),
                )
                .await;

                let listed = driver
                    .get_tables(&handle, &db, Some(&schema))
                    .await
                    .expect("get_tables after cleanup");
                assert!(
                    listed.is_empty(),
                    "no relation may survive cleanup: {listed:?}"
                );

                let surviving = query(
                    driver,
                    &handle,
                    &format!(
                        "SELECT COUNT(*) AS n FROM sys.schemas WHERE name = '{}'",
                        schema.replace('\'', "''")
                    ),
                )
                .await;
                assert_eq!(
                    cell_i64(&surviving.rows[0][0]),
                    Some(0),
                    "the scratch schema must be gone"
                );
            },
        )
        .await;
}
