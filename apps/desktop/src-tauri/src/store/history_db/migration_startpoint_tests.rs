//! Legacy-column helpers shared by the v2/v3 upgrade regression tests.
//!
//! These build raw SQLite files in a *pre-v4* physical shape so that opening
//! the store must run the guarded renames. They deliberately bypass the typed
//! API: the whole point is to hand the migration code a database it could not
//! have produced itself.

use super::*;
use chrono::Utc;
use rusqlite::Connection;
use std::path::Path;

/// Builds a raw SQLite file in the *pre-v4* physical shape (columns named
/// `config_id`, indexes on `config_id`) so that opening the store must run
/// the guarded RENAME migration without touching product code paths.
/// `with_schema_column` models a v3 library when true, a v2 one otherwise.
fn create_legacy_db(dir: &Path, with_schema_column: bool) -> Connection {
    let conn = Connection::open(dir.join("history.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_version (version INTEGER NOT NULL);
         INSERT INTO schema_version (version) VALUES (2);
         CREATE TABLE query_history (
             id TEXT PRIMARY KEY NOT NULL,
             config_id TEXT NOT NULL,
             database TEXT NOT NULL,
             sql TEXT NOT NULL,
             executed_at TEXT NOT NULL,
             execution_time_ms INTEGER NOT NULL,
             rows_affected INTEGER,
             success INTEGER NOT NULL,
             error_message TEXT
         );
         CREATE INDEX idx_query_history_config_id ON query_history(config_id);
         CREATE TABLE favorite_queries (
             id TEXT PRIMARY KEY NOT NULL,
             config_id TEXT NOT NULL,
             title TEXT NOT NULL,
             sql TEXT NOT NULL,
             created_at TEXT NOT NULL
         );
         CREATE INDEX idx_favorite_queries_config_id ON favorite_queries(config_id);",
    )
    .unwrap();
    if with_schema_column {
        conn.execute_batch(
            "ALTER TABLE query_history ADD COLUMN schema TEXT;
             CREATE INDEX idx_query_history_config_db ON query_history(config_id, database);
             INSERT INTO schema_version (version) VALUES (3);",
        )
        .unwrap();
    }
    conn
}

fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn legacy_v3_database_with_rows_migrates_to_connection_id_preserving_data() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = create_legacy_db(dir.path(), true);
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO query_history (id, config_id, database, sql, executed_at,
                 execution_time_ms, rows_affected, success, error_message, schema)
             VALUES ('h1', 'cfg-legacy', 'app', 'SELECT 1', ?1, 10, 1, 1, NULL, NULL),
                    ('h2', 'cfg-legacy', 'app', 'SELECT 2', ?1, 20, 0, 1, NULL, 'public')",
            [now.clone()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO favorite_queries (id, config_id, title, sql, created_at)
             VALUES ('f1', 'cfg-legacy', 'My Fav', 'SELECT 3', ?1)",
            [&now],
        )
        .unwrap();
    }

    let db = HistoryDb::open(dir.path()).unwrap();

    // Physical columns renamed on both tables.
    let db_conn = db.conn.lock().unwrap();
    let qh_cols = column_names(&db_conn, "query_history");
    assert!(qh_cols.contains(&"connection_id".to_string()));
    assert!(!qh_cols.contains(&"config_id".to_string()));
    let fq_cols = column_names(&db_conn, "favorite_queries");
    assert!(fq_cols.contains(&"connection_id".to_string()));
    assert!(!fq_cols.contains(&"config_id".to_string()));

    // New indexes exist; every legacy config_id index is gone.
    let indexes: Vec<String> = {
        let mut stmt = db_conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='index'
                 AND tbl_name IN ('query_history','favorite_queries')",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    assert!(indexes.contains(&"idx_query_history_connection_id".to_string()));
    assert!(indexes.contains(&"idx_query_history_connection_db".to_string()));
    assert!(indexes.contains(&"idx_favorite_queries_connection_id".to_string()));
    for stale in [
        "idx_query_history_config_id",
        "idx_query_history_config_db",
        "idx_favorite_queries_config_id",
    ] {
        assert!(
            !indexes.contains(&stale.to_string()),
            "legacy index {stale} must be dropped"
        );
    }

    // Schema version stamped to 4.
    let version: i32 = db_conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 4);

    // Data rows survive the rename and remain filterable by connection.
    drop(db_conn);
    let history = db
        .get_query_history(10, Some("cfg-legacy"), None, None)
        .unwrap();
    assert_eq!(history.len(), 2, "history rows must be preserved");
    assert!(history.iter().any(|e| e.sql == "SELECT 1"));
    assert!(history.iter().any(|e| e.sql == "SELECT 2"));

    // The favorite row is left intact for the file export to pick up.
    // Opening the database must not consume it.
    let favs = db.read_legacy_favorites().unwrap().expect("table exists");
    assert_eq!(favs.len(), 1, "favorite row must survive schema migration");
    assert_eq!(favs[0].title, "My Fav");
    assert_eq!(favs[0].sql, "SELECT 3");
    assert_eq!(favs[0].connection_id, "cfg-legacy");
}

#[test]
fn empty_v2_database_migrates_cleanly_through_the_full_ring() {
    // v2 starting point: pre-rename columns, no `schema` column yet, no
    // data rows. The v1→v2 guard must not fire (no connection_id column),
    // and the ring must land on v4 with final-state naming.
    let dir = tempfile::tempdir().unwrap();
    {
        let _conn = create_legacy_db(dir.path(), false);
    }

    let db = HistoryDb::open(dir.path()).unwrap();

    let db_conn = db.conn.lock().unwrap();
    let qh_cols = column_names(&db_conn, "query_history");
    assert!(qh_cols.contains(&"connection_id".to_string()));
    assert!(qh_cols.contains(&"schema".to_string()));
    assert!(!qh_cols.contains(&"config_id".to_string()));
    let version: i32 = db_conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 4);
    drop(db_conn);

    assert!(db
        .get_query_history(10, None, None, None)
        .unwrap()
        .is_empty());
    // A v2 database still carries the (now empty) table. It is left in
    // place: the export has nothing to write, so renaming it would be
    // churn on a schema the next reinstall will discard anyway.
    assert_eq!(
        db.read_legacy_favorites().unwrap().map(|r| r.len()),
        Some(0)
    );
}

#[test]
fn a_v1_database_without_a_favorites_table_still_reaches_v4() {
    // Regression guard for retiring the table: the v2 step no longer
    // creates it, so a v1 library upgrading for the first time has no
    // `favorite_queries` at all by the time the v4 step runs. Index DDL
    // for a table that was never created makes the whole batch fail, which
    // would abort startup for exactly the oldest users.
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = Connection::open(dir.path().join("history.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version (version) VALUES (1);
             CREATE TABLE query_history (
                 id TEXT PRIMARY KEY NOT NULL,
                 config_id TEXT NOT NULL,
                 database TEXT NOT NULL,
                 sql TEXT NOT NULL,
                 executed_at TEXT NOT NULL,
                 execution_time_ms INTEGER NOT NULL,
                 rows_affected INTEGER,
                 success INTEGER NOT NULL,
                 error_message TEXT
             );",
        )
        .unwrap();
    }

    let db = HistoryDb::open(dir.path()).unwrap();
    let db_conn = db.conn.lock().unwrap();
    let version: i32 = db_conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 4, "the full migration ring must still complete");
    assert!(column_names(&db_conn, "query_history").contains(&"connection_id".to_string()));
    drop(db_conn);
    assert!(db.read_legacy_favorites().unwrap().is_none());
}
