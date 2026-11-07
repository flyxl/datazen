//! End-to-end for the new-row INSERT builder at the driver boundary.
//!
//! The host never re-renders an insert: it calls
//! `DatabaseDriver::build_insert_statement`, then sends exactly `sql` +
//! `parameters` to the engine, and finally requires `affected == 1`. So the
//! statement this builder produces *is* the write. These tests execute that
//! output against a real SQLite engine and read the table back, which is the
//! only way to assert "the database ended up in this state" — a mock can only
//! echo the SQL it was handed.
//!
//! The host-side half of the journey (plan → fingerprint → execute → response)
//! lives in `src-tauri/src/commands/data/tests.rs`, because the plan types are
//! host-internal.

use datazen_driver_api::*;
use datazen_driver_sqlite::SqliteDriver;

fn config(path: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: format!("tester-insert-{}", uuid::Uuid::new_v4()),
        name: "tester insert".into(),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some(path.into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 1,
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

#[tokio::test]
async fn an_unwritten_column_reaches_the_database_as_its_column_default() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&config(":memory:")).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE users (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 name TEXT NOT NULL, \
                 note TEXT DEFAULT 'dflt')",
        )
        .await
        .unwrap();

    // `id` and `note` are absent from the write list: Unset, i.e. the database
    // decides. If the builder "completed" the row with NULLs, `id` would not
    // auto-increment and `note` would not take its default.
    let statement = driver
        .build_insert_statement("users", &[("name", Some(Value::String("Ada".into())))])
        .unwrap();
    assert_eq!(statement.sql, "INSERT INTO \"users\" (\"name\") VALUES (?)");
    assert_eq!(statement.parameters.len(), 1);
    assert_eq!(
        driver
            .execute_with_params(&handle, &statement.sql, &statement.parameters)
            .await
            .unwrap(),
        1,
        "the host requires affected == 1 for every write"
    );

    let rows = driver
        .query(&handle, "SELECT id, name, note FROM users")
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert!(
        matches!(rows.rows[0][0], Some(Value::Integer(1))),
        "auto-increment id must be assigned by the engine, got {:?}",
        rows.rows[0][0]
    );
    assert!(matches!(
        &rows.rows[0][1],
        Some(Value::String(name)) if name == "Ada"
    ));
    assert!(
        matches!(
            &rows.rows[0][2],
            Some(Value::String(note)) if note == "dflt"
        ),
        "an Unset column must keep the column default, got {:?}",
        rows.rows[0][2]
    );
}

#[tokio::test]
async fn null_unwritten_and_empty_string_are_three_distinct_stored_states() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&config(":memory:")).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE notes (id TEXT PRIMARY KEY, note TEXT DEFAULT 'dflt')",
        )
        .await
        .unwrap();

    // Three intents that must not collapse into each other.
    let writes: Vec<(String, Vec<(&str, Option<Value>)>)> = vec![
        ("null".to_string(), vec![("note", None)]),
        (
            "empty".to_string(),
            vec![("note", Some(Value::String(String::new())))],
        ),
        ("unset".to_string(), vec![]),
    ];
    for (id, columns) in writes {
        let mut values: Vec<(&str, Option<Value>)> = vec![("id", Some(Value::String(id)))];
        values.extend(columns);
        let statement = driver.build_insert_statement("notes", &values).unwrap();
        assert_eq!(
            driver
                .execute_with_params(&handle, &statement.sql, &statement.parameters)
                .await
                .unwrap(),
            1
        );
    }

    // Assert the *stored* state, not the decoded one. `COALESCE` keeps the
    // assertion independent of the read path: this driver currently decodes a
    // SQL NULL in a TEXT cell as `Some("")`, which is a pre-existing defect of
    // `SqliteDriver::decode_rows`, not of the INSERT builder.
    let rows = driver
        .query(
            &handle,
            "SELECT id, COALESCE(note, '<null>'), typeof(note) FROM notes ORDER BY id",
        )
        .await
        .unwrap();
    let stored: Vec<(String, String, String)> = rows
        .rows
        .iter()
        .map(|row| {
            let text = |cell: &Option<Value>| match cell {
                Some(Value::String(value)) => value.clone(),
                other => panic!("expected text cell, got {other:?}"),
            };
            (text(&row[0]), text(&row[1]), text(&row[2]))
        })
        .collect();
    assert_eq!(
        stored,
        vec![
            ("empty".to_string(), String::new(), "text".to_string()),
            ("null".to_string(), "<null>".to_string(), "null".to_string()),
            ("unset".to_string(), "dflt".to_string(), "text".to_string()),
        ],
        "NULL, empty string and DEFAULT must stay three different stored states"
    );
}

#[tokio::test]
async fn an_all_unset_row_renders_default_values_that_sqlite_accepts() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&config(":memory:")).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE defaults_only (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 note TEXT DEFAULT 'dflt')",
        )
        .await
        .unwrap();

    // An all-Unset draft is legal (contract §3.5 rule 2) and must not be
    // dropped as "no change". SQLite accepts the generic spelling.
    let statement = driver.build_insert_statement("defaults_only", &[]).unwrap();
    assert_eq!(
        statement.sql,
        "INSERT INTO \"defaults_only\" DEFAULT VALUES"
    );
    assert!(statement.parameters.is_empty());
    assert_eq!(
        driver
            .execute_with_params(&handle, &statement.sql, &statement.parameters)
            .await
            .unwrap(),
        1
    );

    let rows = driver
        .query(&handle, "SELECT id, note FROM defaults_only")
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert!(matches!(rows.rows[0][0], Some(Value::Integer(1))));
    assert!(matches!(
        &rows.rows[0][1],
        Some(Value::String(note)) if note == "dflt"
    ));
}

#[tokio::test]
async fn sqlite_cannot_backfill_the_new_primary_key() {
    let driver = SqliteDriver::new();
    // No RETURNING seam: the host must take the degradation path and refresh
    // the page rather than guessing which row it just created. If SQLite ever
    // gains the seam, this test is the reminder that the host side must change
    // with it (the host currently never asks).
    assert_eq!(driver.insert_returning_clause(&["id"]), None);
}

/// "INSERT succeeded, a later statement failed" must leave nothing behind.
///
/// The host runs inserts → updates → deletes inside one transaction and rolls
/// back on the first error. `MockDriver` cannot fail a single statement by
/// index, so the ordering half of that claim is proven here against a real
/// engine: the insert really lands, the update really fails, and the row is
/// gone after the rollback.
#[tokio::test]
async fn a_failed_statement_after_a_successful_insert_rolls_the_insert_back() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&config(":memory:")).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE guard (id INTEGER PRIMARY KEY, value INTEGER CHECK (value > 0))",
        )
        .await
        .unwrap();
    driver
        .execute(&handle, "INSERT INTO guard (id, value) VALUES (1, 5)")
        .await
        .unwrap();

    let tx = driver.begin_transaction(&handle).await.unwrap();

    let insert = driver
        .build_insert_statement("users", &[("name", Some(Value::String("Ada".into())))])
        .unwrap();
    assert_eq!(
        driver
            .execute_with_params(&handle, &insert.sql, &insert.parameters)
            .await
            .unwrap(),
        1
    );

    // A real constraint failure on the second statement of the batch.
    let update = driver
        .build_update_statement(
            "guard",
            &[("value", Some(Value::Integer(-1)))],
            &[("id", Some(Value::Integer(1)))],
        )
        .unwrap();
    let failure = driver
        .execute_with_params(&handle, &update.sql, &update.parameters)
        .await;
    assert!(
        failure.is_err(),
        "the CHECK constraint must reject the update"
    );

    driver.rollback(tx).await.unwrap();

    let rows = driver
        .query(&handle, "SELECT COUNT(*) FROM users")
        .await
        .unwrap();
    assert!(
        matches!(rows.rows[0][0], Some(Value::Integer(0))),
        "the inserted row must not survive the rolled-back batch, got {:?}",
        rows.rows[0][0]
    );
}
