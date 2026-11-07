//! Append, filter, and deduplication behaviour of `query_history`.
//!
//! Paging/search/delete coverage lives in the sibling `page_tests` module,
//! which sits one directory up because it exercises the global history dialog
//! contract rather than this table's storage rules.

use super::*;
use chrono::Utc;

use crate::store::history_db::fixtures::{make_test_result, sample_query, sample_query_for_config};
use crate::store::history_db::schema::LegacyWorkflowHistoryEntry;

#[test]
fn migrates_queries_json_once() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("history")).unwrap();
    let legacy = serde_json::json!([{
        "id": "test-id",
        "connectionId": "c1",
        "database": "app",
        "sql": "SELECT 1",
        "executedAt": chrono::Utc::now().to_rfc3339(),
        "executionTimeMs": 10,
        "rowsAffected": 1,
        "success": true
    }]);
    std::fs::write(
        dir.path().join("history/queries.json"),
        serde_json::to_string_pretty(&legacy).unwrap(),
    )
    .unwrap();

    let db = HistoryDb::open(dir.path()).unwrap();
    let loaded = db.get_query_history(10, None, None, None).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].sql, "SELECT 1");
    assert!(!dir.path().join("history/queries.json").exists());
    assert!(dir.path().join("history/queries.json.migrated").is_file());

    // Re-open must not duplicate.
    let db2 = HistoryDb::open(dir.path()).unwrap();
    assert_eq!(
        db2.get_query_history(10, None, None, None).unwrap().len(),
        1
    );
    let _ = db;
    let _ = db2;
}

#[test]
fn migrates_workflow_json_dir_once() {
    let dir = tempfile::tempdir().unwrap();
    let wf_dir = dir.path().join("workflow_history");
    std::fs::create_dir_all(&wf_dir).unwrap();
    let entry = LegacyWorkflowHistoryEntry {
        id: "h1".into(),
        workflow_id: "wf1".into(),
        workflow_name: "WF".into(),
        variables: serde_json::json!({}),
        result: make_test_result(true),
        created_at: Utc::now().to_rfc3339(),
    };
    std::fs::write(
        wf_dir.join("h1.json"),
        serde_json::to_string_pretty(&entry).unwrap(),
    )
    .unwrap();

    let db = HistoryDb::open(dir.path()).unwrap();
    let list = db.list_workflow_history(None).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].workflow_id, "wf1");
    assert!(!wf_dir.is_dir());
    assert!(dir.path().join("workflow_history.migrated").is_dir());
}

#[test]
fn query_history_connection_id_filter() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    db.add_query_history(sample_query_for_config("SELECT 1", "cfg-a"))
        .unwrap();
    db.add_query_history(sample_query_for_config("SELECT 2", "cfg-b"))
        .unwrap();
    db.add_query_history(sample_query_for_config("SELECT 3", "cfg-a"))
        .unwrap();

    let all = db.get_query_history(10, None, None, None).unwrap();
    assert_eq!(all.len(), 3);

    let a_only = db.get_query_history(10, Some("cfg-a"), None, None).unwrap();
    assert_eq!(a_only.len(), 2);
    assert!(a_only.iter().all(|e| e.connection_id == "cfg-a"));

    let b_only = db.get_query_history(10, Some("cfg-b"), None, None).unwrap();
    assert_eq!(b_only.len(), 1);
    assert_eq!(b_only[0].connection_id, "cfg-b");
}

#[test]
fn query_history_dedup_scoped_by_connection_id() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    db.add_query_history(sample_query_for_config("SELECT 1", "cfg-a"))
        .unwrap();
    db.add_query_history(sample_query_for_config("SELECT 1", "cfg-b"))
        .unwrap();
    let all = db.get_query_history(10, None, None, None).unwrap();
    assert_eq!(
        all.len(),
        2,
        "same SQL on different configs should not dedup"
    );
}

#[test]
fn query_history_database_filter() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();

    let mut app = sample_query("SELECT 1", 0);
    app.database = "app_db".into();
    db.add_query_history(app.clone()).unwrap();

    let mut analytics = sample_query("SELECT 2", 0);
    analytics.database = "analytics".into();
    analytics.schema = Some("public".into());
    db.add_query_history(analytics).unwrap();

    let legacy = sample_query("SELECT 3", 0);
    let legacy_db = legacy.database.clone();
    db.add_query_history(legacy).unwrap();

    let app_only = db
        .get_query_history(10, None, Some("app_db"), None)
        .unwrap();
    assert_eq!(app_only.len(), 1);
    assert_eq!(app_only[0].database, "app_db");

    // Legacy rows record an empty database string; filtering by "" finds them.
    let legacy_rows = db
        .get_query_history(10, None, Some(&legacy_db), None)
        .unwrap();
    assert_eq!(legacy_rows.len(), 1);

    let none = db
        .get_query_history(10, None, Some("missing"), None)
        .unwrap();
    assert!(none.is_empty());

    // Unfiltered still returns everything.
    assert_eq!(db.get_query_history(10, None, None, None).unwrap().len(), 3);
}

#[test]
fn query_history_schema_roundtrip_and_filter() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();

    let mut with_schema = sample_query("SELECT 1", 0);
    with_schema.database = "analytics".into();
    with_schema.schema = Some("sales".into());
    db.add_query_history(with_schema).unwrap();

    let mut without_schema = sample_query("SELECT 2", 0);
    without_schema.database = "analytics".into();
    without_schema.schema = None;
    db.add_query_history(without_schema).unwrap();

    let sales = db
        .get_query_history(10, None, Some("analytics"), Some("sales"))
        .unwrap();
    assert_eq!(sales.len(), 1);
    assert_eq!(sales[0].schema.as_deref(), Some("sales"));

    // NULL-safe: NULL schema rows only match when the filter asks for NULL
    // (empty string is the sentinel for "no schema").
    let null_schema = db
        .get_query_history(10, None, Some("analytics"), Some(""))
        .unwrap();
    assert_eq!(null_schema.len(), 1);

    let all = db
        .get_query_history(10, None, Some("analytics"), None)
        .unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn query_history_dedup_scoped_by_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();

    let mut a = sample_query("SELECT 1", 0);
    a.connection_id = "cfg1".into();
    a.database = "app_db".into();
    db.add_query_history(a).unwrap();

    let mut b = sample_query("SELECT 1", 0);
    b.connection_id = "cfg1".into();
    b.database = "other_db".into();
    db.add_query_history(b).unwrap();

    let all = db.get_query_history(10, None, None, None).unwrap();
    assert_eq!(
        all.len(),
        2,
        "same SQL on different databases of one config should not dedup"
    );
}

#[test]
fn query_history_dedup_updates_latest() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    let mut e1 = sample_query("SELECT 1", 0);
    db.add_query_history(e1.clone()).unwrap();
    e1.execution_time_ms = 99;
    db.add_query_history(e1).unwrap();
    let history = db.get_query_history(10, None, None, None).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].execution_time_ms, 99);
}

#[test]
fn schema_version_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    drop(db);
    let db2 = HistoryDb::open(dir.path()).unwrap();
    db2.add_query_history(sample_query("SELECT 1", 0)).unwrap();
    let loaded = db2.get_query_history(10, None, None, None).unwrap();
    assert_eq!(loaded.len(), 1);
}
