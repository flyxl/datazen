//! End-to-end SQLite table rebuild journeys with real catalog readback.

use std::path::PathBuf;

use datazen_driver_api::*;
use datazen_driver_sqlite::{SqliteDriver, SqliteMigrationCapabilities, SqliteMigrationRenderer};

fn config(path: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: format!("sqlite-rebuild-{}", uuid::Uuid::new_v4()),
        name: "SQLite rebuild journey".into(),
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

async fn fixture() -> (SqliteDriver, ConnectionHandle, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("datazen-sqlite-rebuild-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("target.db");
    std::fs::File::create(&path).unwrap();

    let driver = SqliteDriver::new();
    let handle = driver
        .connect(&config(path.to_str().unwrap()))
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE parents (id INTEGER PRIMARY KEY, label TEXT NOT NULL)",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "INSERT INTO parents (id, label) VALUES (7, 'parent')",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id INTEGER REFERENCES parents(id) ON UPDATE NO ACTION ON DELETE CASCADE,
                email TEXT NOT NULL DEFAULT 'unknown',
                score INTEGER DEFAULT 0,
                CONSTRAINT ck_users_score CHECK (score >= 0),
                UNIQUE (email)
            )",
        )
        .await
        .unwrap();
    driver
        .execute(&handle, "CREATE INDEX idx_users_score ON users(score)")
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "INSERT INTO users (id, parent_id, email, score) VALUES (11, 7, 'one@example.test', 4)",
        )
        .await
        .unwrap();
    (driver, handle, directory)
}

fn assert_integer(value: &Option<Value>, expected: i64) {
    assert!(
        matches!(value, Some(Value::Integer(actual)) if *actual == expected)
            || matches!(value, Some(Value::Float(actual)) if *actual == expected as f64),
        "expected integer {expected}, got {value:?}"
    );
}

async fn rebuild_plan(
    driver: &SqliteDriver,
    handle: &ConnectionHandle,
) -> (TableSchema, TableSchema, Vec<MigrationStatement>) {
    let current = driver
        .get_table_schema(handle, "users", "main", None)
        .await
        .unwrap();
    assert!(current.table_options.migration_blockers.is_empty());
    assert_eq!(current.primary_keys, vec!["id"]);
    assert!(current
        .columns
        .iter()
        .any(|column| column.is_auto_increment));
    assert_eq!(current.check_constraints.len(), 1);
    assert!(current.check_constraints[0]
        .expression
        .to_ascii_lowercase()
        .contains("score >= 0"));
    assert!(current.indexes.iter().any(|index| {
        index.is_unique && !index.is_primary && index.name.starts_with("sqlite_autoindex_")
    }));
    assert!(current
        .indexes
        .iter()
        .any(|index| index.name == "idx_users_score"));
    assert_eq!(current.foreign_keys.len(), 1);
    assert_eq!(
        current.foreign_keys[0].deferrability,
        ForeignKeyDeferrability::NotDeferrable
    );

    let mut desired = current.clone();
    desired
        .columns
        .iter_mut()
        .find(|column| column.name == "score")
        .unwrap()
        .data_type = "REAL".into();
    assert!(SqliteMigrationCapabilities.requires_table_rebuild(
        &MigrationOperation::AlterColumnType {
            table: "users".into(),
            column: "score".into(),
            from: "INTEGER".into(),
            to: "REAL".into(),
        }
    ));
    let statements = SqliteMigrationRenderer
        .render_table_rebuild("users", &desired, &current)
        .unwrap();
    (current, desired, statements)
}

#[tokio::test]
async fn rebuild_fails_closed_for_column_and_table_conflict_policies() {
    let (driver, handle, directory) = fixture().await;
    let cases = [
        (
            "conflict_column_unique_ignore",
            "CREATE TABLE conflict_column_unique_ignore (id INTEGER PRIMARY KEY, value TEXT UNIQUE ON CONFLICT IGNORE)",
            "IGNORE",
        ),
        (
            "conflict_column_not_null_fail",
            "CREATE TABLE conflict_column_not_null_fail (id INTEGER PRIMARY KEY, value TEXT NOT NULL ON CONFLICT FAIL)",
            "FAIL",
        ),
        (
            "conflict_column_primary_key_replace",
            "CREATE TABLE conflict_column_primary_key_replace (id INTEGER PRIMARY KEY ON CONFLICT REPLACE, value TEXT)",
            "REPLACE",
        ),
        (
            "conflict_table_unique_abort",
            "CREATE TABLE conflict_table_unique_abort (id INTEGER PRIMARY KEY, value TEXT, UNIQUE (value) ON CONFLICT ABORT)",
            "ABORT",
        ),
        (
            "conflict_table_primary_key_rollback",
            "CREATE TABLE conflict_table_primary_key_rollback (id INTEGER, value TEXT, PRIMARY KEY (id) ON CONFLICT ROLLBACK)",
            "ROLLBACK",
        ),
        (
            "conflict_table_check_ignore",
            "CREATE TABLE conflict_table_check_ignore (id INTEGER PRIMARY KEY, value TEXT, CHECK (value <> '') ON /* policy */ CONFLICT IGNORE)",
            "IGNORE",
        ),
    ];

    for (table, ddl, expected_action) in cases {
        driver.execute(&handle, ddl).await.unwrap();
        driver
            .execute(
                &handle,
                &format!("INSERT INTO {table} (id, value) VALUES (1, 'kept')"),
            )
            .await
            .unwrap();

        let catalog_before = driver
            .query(
                &handle,
                &format!(
                    "SELECT sql FROM main.sqlite_master WHERE type = 'table' AND name = '{table}'"
                ),
            )
            .await
            .unwrap();
        let rows_before = driver
            .query(
                &handle,
                &format!("SELECT id, value FROM {table} ORDER BY id"),
            )
            .await
            .unwrap();
        assert_eq!(rows_before.rows.len(), 1, "{table}");

        let current = driver
            .get_table_schema(&handle, table, "main", None)
            .await
            .unwrap();
        let conflict_blocker = current
            .table_options
            .migration_blockers
            .iter()
            .find(|blocker| blocker.contains("ON CONFLICT"))
            .unwrap_or_else(|| panic!("{table} did not report its conflict policy"));
        assert!(
            conflict_blocker.contains(expected_action),
            "{table}: {conflict_blocker}"
        );

        let mut desired = current.clone();
        desired.columns[0].data_type = "BLOB".into();
        let error = SqliteMigrationRenderer
            .render_table_rebuild(table, &desired, &current)
            .unwrap_err();
        assert!(error.contains("ON CONFLICT"), "{table}: {error}");
        assert!(error.contains(expected_action), "{table}: {error}");

        let catalog_after = driver
            .query(
                &handle,
                &format!(
                    "SELECT sql FROM main.sqlite_master WHERE type = 'table' AND name = '{table}'"
                ),
            )
            .await
            .unwrap();
        let rows_after = driver
            .query(
                &handle,
                &format!("SELECT id, value FROM {table} ORDER BY id"),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&catalog_after.rows).unwrap(),
            serde_json::to_value(&catalog_before.rows).unwrap(),
            "{table} catalog changed while preparing a blocked rebuild"
        );
        assert_eq!(
            serde_json::to_value(&rows_after.rows).unwrap(),
            serde_json::to_value(&rows_before.rows).unwrap(),
            "{table} rows changed while preparing a blocked rebuild"
        );
    }

    driver
        .execute(
            &handle,
            "CREATE TABLE conflict_words_in_default (id INTEGER PRIMARY KEY, message TEXT DEFAULT 'ON CONFLICT IGNORE')",
        )
        .await
        .unwrap();
    let safe_table = driver
        .get_table_schema(&handle, "conflict_words_in_default", "main", None)
        .await
        .unwrap();
    assert!(safe_table.table_options.migration_blockers.is_empty());
    let mut safe_target = safe_table.clone();
    safe_target.columns[1].data_type = "BLOB".into();
    assert!(!SqliteMigrationRenderer
        .render_table_rebuild("conflict_words_in_default", &safe_target, &safe_table)
        .unwrap()
        .is_empty());

    driver
        .execute(
            &handle,
            "INSERT INTO conflict_column_unique_ignore (id, value) VALUES (2, 'kept')",
        )
        .await
        .unwrap();
    let rows = driver
        .query(
            &handle,
            "SELECT id, value FROM conflict_column_unique_ignore ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(
        rows.rows.len(),
        1,
        "UNIQUE ON CONFLICT IGNORE must remain intact"
    );

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn test_tester_quoted_conflict_words_do_not_block_safe_rebuild() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(
            &handle,
            "CREATE TABLE quoted_conflict_words (id INTEGER PRIMARY KEY, \"ON\" TEXT DEFAULT 'CONFLICT IGNORE', \"CONFLICT\" TEXT)",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "INSERT INTO quoted_conflict_words (id, \"ON\", \"CONFLICT\") VALUES (7, 'kept', 'value')",
        )
        .await
        .unwrap();

    let current = driver
        .get_table_schema(&handle, "quoted_conflict_words", "main", None)
        .await
        .unwrap();
    assert!(current.table_options.migration_blockers.is_empty());
    let mut desired = current.clone();
    desired.columns[1].data_type = "BLOB".into();
    assert!(!SqliteMigrationRenderer
        .render_table_rebuild("quoted_conflict_words", &desired, &current)
        .unwrap()
        .is_empty());

    let rows = driver
        .query(
            &handle,
            "SELECT id, \"ON\", \"CONFLICT\" FROM quoted_conflict_words",
        )
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_integer(&rows.rows[0][0], 7);
    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn stale_reviewed_schema_is_rejected_before_rebuild_writes() {
    let (driver, handle, directory) = fixture().await;
    let reviewed = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    driver
        .execute(&handle, "ALTER TABLE users ADD COLUMN unexpected TEXT")
        .await
        .unwrap();

    let transaction = driver.begin_transaction(&handle).await.unwrap();
    let result = driver
        .validate_schema_migration_plan(
            &handle,
            SqlTarget::new(Some("main"), None),
            std::slice::from_ref(&reviewed),
        )
        .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("changed since the schema comparison"));
    driver.rollback(transaction).await.unwrap();

    let current = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    assert!(current
        .columns
        .iter()
        .any(|column| column.name == "unexpected"));
    let rows = driver
        .query(&handle, "SELECT id, email FROM users")
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_integer(&rows.rows[0][0], 11);
    assert!(matches!(&rows.rows[0][1], Some(Value::String(value)) if value == "one@example.test"));
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn reviewed_rebuild_preserves_rows_constraints_and_indexes_after_readback() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(
            &handle,
            "INSERT INTO users (id, parent_id, email, score) VALUES (100, 7, 'deleted-high-id@example.test', 8)",
        )
        .await
        .unwrap();
    driver
        .execute(&handle, "DELETE FROM users WHERE id = 100")
        .await
        .unwrap();
    let (current, _desired, statements) = rebuild_plan(&driver, &handle).await;
    let transaction = driver.begin_transaction(&handle).await.unwrap();
    for statement in &statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", statement.sql));
    }
    driver
        .validate_schema_migration(&handle, SqlTarget::new(Some("main"), None))
        .await
        .unwrap();
    driver.commit(transaction).await.unwrap();

    let rows = driver
        .query(
            &handle,
            "SELECT id, parent_id, email, score FROM users ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_integer(&rows.rows[0][0], 11);
    assert_integer(&rows.rows[0][1], 7);
    assert!(matches!(&rows.rows[0][2], Some(Value::String(value)) if value == "one@example.test"));
    let rebuilt = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    assert_eq!(
        rebuilt
            .columns
            .iter()
            .find(|column| column.name == "score")
            .map(|column| column.data_type.as_str()),
        Some("REAL")
    );
    assert_eq!(rebuilt.primary_keys, current.primary_keys);
    assert_eq!(rebuilt.check_constraints, current.check_constraints);
    assert_eq!(rebuilt.foreign_keys, current.foreign_keys);
    assert_eq!(
        rebuilt
            .columns
            .iter()
            .find(|column| column.name == "email")
            .and_then(|column| column.default_value.as_deref()),
        Some("'unknown'")
    );
    assert_eq!(
        rebuilt
            .columns
            .iter()
            .find(|column| column.name == "score")
            .and_then(|column| column.default_value.as_deref()),
        Some("0")
    );
    assert!(rebuilt
        .indexes
        .iter()
        .any(|index| index.name == "idx_users_score"));
    let sequence = driver
        .query(
            &handle,
            "SELECT seq FROM sqlite_sequence WHERE name = 'users'",
        )
        .await
        .unwrap();
    assert_eq!(sequence.rows.len(), 1);
    assert_integer(&sequence.rows[0][0], 100);
    driver
        .execute(&handle, "INSERT INTO users (parent_id) VALUES (7)")
        .await
        .unwrap();
    let next_id = driver
        .query(&handle, "SELECT id, email, score FROM users WHERE id = 101")
        .await
        .unwrap();
    assert_eq!(next_id.rows.len(), 1);
    assert_integer(&next_id.rows[0][0], 101);
    assert!(matches!(&next_id.rows[0][1], Some(Value::String(value)) if value == "unknown"));
    assert_integer(&next_id.rows[0][2], 0);
    assert!(driver
        .execute(
            &handle,
            "INSERT INTO users (parent_id, email, score) VALUES (7, 'one@example.test', 9)",
        )
        .await
        .is_err());
    assert!(driver
        .execute(&handle, "UPDATE users SET score = -1 WHERE id = 101")
        .await
        .is_err());

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn injected_failure_after_drop_rolls_back_data_and_catalog() {
    let (driver, handle, directory) = fixture().await;
    let (current, _desired, statements) = rebuild_plan(&driver, &handle).await;
    let transaction = driver.begin_transaction(&handle).await.unwrap();

    let mut old_table_was_dropped = false;
    let mut injected_failure = false;
    for statement in &statements {
        if statement.summary.starts_with("RENAME replacement table") {
            assert!(old_table_was_dropped);
            assert!(driver
                .execute(&handle, "ALTER TABLE missing_rebuild_table RENAME TO users")
                .await
                .is_err());
            injected_failure = true;
            break;
        }
        driver
            .execute(&handle, &statement.sql)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", statement.sql));
        if statement.summary.starts_with("DROP old table") {
            old_table_was_dropped = true;
        }
    }
    assert!(injected_failure);
    driver.rollback(transaction).await.unwrap();

    let after = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&after.columns).unwrap(),
        serde_json::to_value(&current.columns).unwrap()
    );
    assert_eq!(after.primary_keys, current.primary_keys);
    let rows = driver
        .query(&handle, "SELECT id, email, score FROM users")
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_integer(&rows.rows[0][0], 11);
    let replacement = driver
        .query(
            &handle,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = '__datazen_rebuild_users'",
        )
        .await
        .unwrap();
    assert!(replacement.rows.is_empty());
    let sequence_state = driver
        .query(
            &handle,
            "SELECT name FROM sqlite_temp_master WHERE name LIKE '__datazen_rebuild_sequence_%'",
        )
        .await
        .unwrap();
    assert!(sequence_state.rows.is_empty());

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn rebuild_preserves_sparse_implicit_rowids_after_readback() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(&handle, "CREATE TABLE rowid_items (value TEXT)")
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "INSERT INTO rowid_items (rowid, value) VALUES (20, 'first'), (42, 'second')",
        )
        .await
        .unwrap();
    let current = driver
        .get_table_schema(&handle, "rowid_items", "main", None)
        .await
        .unwrap();
    let mut desired = current.clone();
    desired.columns[0].data_type = "BLOB".into();
    let statements = SqliteMigrationRenderer
        .render_table_rebuild("rowid_items", &desired, &current)
        .unwrap();
    let transaction = driver.begin_transaction(&handle).await.unwrap();
    for statement in &statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", statement.sql));
    }
    driver.commit(transaction).await.unwrap();

    let rows = driver
        .query(
            &handle,
            "SELECT rowid, value FROM rowid_items ORDER BY rowid",
        )
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 2);
    assert_integer(&rows.rows[0][0], 20);
    assert_integer(&rows.rows[1][0], 42);

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn rebuild_restores_autoincrement_highwater_for_an_empty_table() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(
            &handle,
            "INSERT INTO users (id, parent_id, email, score) VALUES (100, 7, 'deleted-high-id@example.test', 8)",
        )
        .await
        .unwrap();
    driver.execute(&handle, "DELETE FROM users").await.unwrap();
    let sequence = driver
        .query(
            &handle,
            "SELECT seq FROM sqlite_sequence WHERE name = 'users'",
        )
        .await
        .unwrap();
    assert_eq!(sequence.rows.len(), 1);
    assert_integer(&sequence.rows[0][0], 100);

    let (current, _desired, statements) = rebuild_plan(&driver, &handle).await;
    let transaction = driver.begin_transaction(&handle).await.unwrap();
    for statement in &statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", statement.sql));
    }
    driver.commit(transaction).await.unwrap();

    let rebuilt = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    assert_eq!(rebuilt.primary_keys, current.primary_keys);
    driver
        .execute(&handle, "INSERT INTO users (parent_id) VALUES (7)")
        .await
        .unwrap();
    let next = driver.query(&handle, "SELECT id FROM users").await.unwrap();
    assert_eq!(next.rows.len(), 1);
    assert_integer(&next.rows[0][0], 101);

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn foreign_key_validation_failure_rolls_back_the_rebuild() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(&handle, "PRAGMA foreign_keys = OFF")
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "INSERT INTO users (parent_id, email, score) VALUES (999, 'orphan@example.test', 2)",
        )
        .await
        .unwrap();
    let (current, _desired, statements) = rebuild_plan(&driver, &handle).await;
    let transaction = driver.begin_transaction(&handle).await.unwrap();
    for statement in &statements {
        driver.execute(&handle, &statement.sql).await.unwrap();
    }
    let validation = driver
        .validate_schema_migration(&handle, SqlTarget::new(Some("main"), None))
        .await;
    assert!(validation.is_err());
    driver.rollback(transaction).await.unwrap();

    let after = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&after.columns).unwrap(),
        serde_json::to_value(&current.columns).unwrap()
    );
    let orphan = driver
        .query(
            &handle,
            "SELECT parent_id, email FROM users WHERE email = 'orphan@example.test'",
        )
        .await
        .unwrap();
    assert_eq!(orphan.rows.len(), 1);

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn rebuild_fails_closed_for_triggers_views_and_inbound_foreign_keys() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(
            &handle,
            "CREATE TABLE user_events (id INTEGER REFERENCES users(id), value TEXT)",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "CREATE TRIGGER users_after_insert AFTER INSERT ON users BEGIN INSERT INTO user_events(value) VALUES ('insert'); END",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "CREATE VIEW user_scores AS SELECT score FROM users",
        )
        .await
        .unwrap();
    let schema = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    let reason = SqliteMigrationRenderer
        .render_table_rebuild("users", &schema, &schema)
        .unwrap_err();
    assert!(reason.contains("Trigger `users_after_insert`"), "{reason}");
    assert!(reason.contains("View `user_scores`"), "{reason}");
    assert!(
        reason.contains("foreign key referencing `users`"),
        "{reason}"
    );

    let rows = driver
        .query(&handle, "SELECT id, score FROM users")
        .await
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_integer(&rows.rows[0][0], 11);

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn rebuild_fails_closed_for_unrepresented_table_and_index_features() {
    let (driver, handle, directory) = fixture().await;
    driver
        .execute(
            &handle,
            "CREATE INDEX idx_users_partial ON users(score) WHERE score > 0",
        )
        .await
        .unwrap();
    driver
        .execute(
            &handle,
            "CREATE INDEX idx_users_expression ON users(lower(email))",
        )
        .await
        .unwrap();
    let indexed = driver
        .get_table_schema(&handle, "users", "main", None)
        .await
        .unwrap();
    let index_error = SqliteMigrationRenderer
        .render_table_rebuild("users", &indexed, &indexed)
        .unwrap_err();
    assert!(
        index_error.contains("Partial index `idx_users_partial`"),
        "{index_error}"
    );
    assert!(
        index_error.contains("Expression index `idx_users_expression`"),
        "{index_error}"
    );

    for (table, ddl, expected) in [
        (
            "strict_values",
            "CREATE TABLE strict_values (id INTEGER PRIMARY KEY) STRICT",
            "STRICT table typing",
        ),
        (
            "without_rowid_values",
            "CREATE TABLE without_rowid_values (id TEXT PRIMARY KEY) WITHOUT ROWID",
            "WITHOUT ROWID",
        ),
        (
            "generated_values",
            "CREATE TABLE generated_values (id INTEGER, doubled INTEGER GENERATED ALWAYS AS (id * 2) STORED)",
            "Generated column",
        ),
        (
            "collated_values",
            "CREATE TABLE collated_values (name TEXT COLLATE NOCASE)",
            "Column collation",
        ),
    ] {
        driver.execute(&handle, ddl).await.unwrap();
        let schema = driver
            .get_table_schema(&handle, table, "main", None)
            .await
            .unwrap();
        let error = SqliteMigrationRenderer
            .render_table_rebuild(table, &schema, &schema)
            .unwrap_err();
        assert!(error.contains(expected), "{table}: {error}");
    }

    driver.disconnect(handle).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
