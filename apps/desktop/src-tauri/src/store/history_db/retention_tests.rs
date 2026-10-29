//! Retention: `purge` across both history tables.
//!
//! These two are the only tests that touch `query_history` and
//! `workflow_history` in the same case, which is the point — a scope that
//! silently purges only one of the two is a bug no single-table test can see.

use super::*;
use chrono::{Duration, Utc};

use super::fixtures::{make_test_result, sample_query};

#[test]
fn purge_retains_recent_rows_only() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();

    db.add_query_history(sample_query("old", 40)).unwrap();
    db.add_query_history(sample_query("recent", 1)).unwrap();
    db.record_workflow(
        "w-old",
        "wf",
        "WF",
        &serde_json::json!({}),
        &make_test_result(true),
        &(Utc::now() - Duration::days(40)).to_rfc3339(),
    )
    .unwrap();
    db.record_workflow(
        "w-new",
        "wf",
        "WF",
        &serde_json::json!({}),
        &make_test_result(true),
        &(Utc::now() - Duration::days(1)).to_rfc3339(),
    )
    .unwrap();

    let deleted = db.purge(HistoryScope::All, Some(30)).unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(db.get_query_history(10, None, None, None).unwrap().len(), 1);
    assert_eq!(
        db.get_query_history(10, None, None, None).unwrap()[0].sql,
        "recent"
    );
    assert_eq!(db.list_workflow_history(None).unwrap().len(), 1);
    assert_eq!(db.list_workflow_history(None).unwrap()[0].id, "w-new");
}

#[test]
fn purge_clear_all_empties_scope() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    db.add_query_history(sample_query("q", 0)).unwrap();
    db.record_workflow(
        "w1",
        "wf",
        "WF",
        &serde_json::json!({}),
        &make_test_result(true),
        &Utc::now().to_rfc3339(),
    )
    .unwrap();

    assert_eq!(db.purge(HistoryScope::Query, None).unwrap(), 1);
    assert!(db
        .get_query_history(10, None, None, None)
        .unwrap()
        .is_empty());
    assert_eq!(db.list_workflow_history(None).unwrap().len(), 1);

    assert_eq!(db.purge(HistoryScope::Workflow, None).unwrap(), 1);
    assert!(db.list_workflow_history(None).unwrap().is_empty());
}
