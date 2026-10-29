//! The retired `favorite_queries` table: read it out, then archive it.
//!
//! These tests exist to protect a one-way door. Once the export has run, the
//! table is renamed rather than dropped, so the only thing that can go wrong is
//! losing rows or re-exporting forever — both of which are asserted here
//! directly rather than inferred from "the app started".

use super::*;
use chrono::{DateTime, Utc};
use rusqlite::params;

#[test]
fn a_fresh_install_never_creates_the_retired_favorites_table() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    // Nothing to export on a fresh install, and nothing to clean up later.
    assert!(db.read_legacy_favorites().unwrap().is_none());
}

#[test]
fn legacy_favorites_are_readable_then_archived_not_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    seed_legacy_favorites(&db, 3);

    let rows = db.read_legacy_favorites().unwrap().expect("table exists");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].title, "legacy 0");

    db.archive_legacy_favorites_table().unwrap();

    // Gone as a live table...
    assert!(db.read_legacy_favorites().unwrap().is_none());
    // ...but the rows are still in the database for rollback.
    let conn = db.conn.lock().unwrap();
    let archived: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {LEGACY_FAVORITES_ARCHIVE_TABLE}"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archived, 3);
}

#[test]
fn archiving_twice_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    seed_legacy_favorites(&db, 1);
    db.archive_legacy_favorites_table().unwrap();
    db.archive_legacy_favorites_table().unwrap();
    assert!(db.read_legacy_favorites().unwrap().is_none());
}

#[test]
fn a_row_with_a_broken_timestamp_does_not_strand_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    seed_legacy_favorites(&db, 2);
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE favorite_queries SET created_at = 'not-a-date' WHERE title = 'legacy 0'",
        [],
    )
    .unwrap();
    drop(conn);

    let rows = db.read_legacy_favorites().unwrap().expect("table exists");
    assert_eq!(rows.len(), 2, "one bad row must not abort the export");
    let broken = rows.iter().find(|r| r.title == "legacy 0").unwrap();
    assert_eq!(
        broken.created_at,
        DateTime::<Utc>::from(std::time::UNIX_EPOCH)
    );
}

/// Recreate the exact v4 table shape a pre-§2.6 install has on disk.
fn seed_legacy_favorites(db: &HistoryDb, count: usize) {
    let conn = db.conn.lock().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {LEGACY_FAVORITES_TABLE} (
             id TEXT PRIMARY KEY NOT NULL,
             connection_id TEXT NOT NULL,
             title TEXT NOT NULL,
             sql TEXT NOT NULL,
             created_at TEXT NOT NULL
         );"
    ))
    .unwrap();
    for i in 0..count {
        conn.execute(
            &format!(
                "INSERT INTO {LEGACY_FAVORITES_TABLE}
                     (id, connection_id, title, sql, created_at) VALUES (?1, ?2, ?3, ?4, ?5)"
            ),
            params![
                format!("fav{i}"),
                format!("cfg-{}", i % 2),
                format!("legacy {i}"),
                format!("SELECT {i}"),
                (Utc::now() - chrono::Duration::hours(i as i64)).to_rfc3339()
            ],
        )
        .unwrap();
    }
}
