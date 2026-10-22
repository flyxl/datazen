//! The store facade's per-domain CRUD, exercised through `DesktopRepository` rather than
//! through the SQLite layer directly (that is `history_db`'s own suite).
//!
//! What is asserted here is the facade's behaviour: history dedup keys on the
//! most recent row, a favorite survives a restart because its SQL is a file.

use super::super::*;
use super::fixtures::*;
use chrono::Utc;

#[tokio::test]
async fn query_history_dedup_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    store
        .add_query_history(sample_history_entry("SELECT 1"))
        .await
        .unwrap();
    store
        .add_query_history(sample_history_entry("SELECT 2"))
        .await
        .unwrap();
    assert_eq!(store.get_query_history(10, None, None, None).await.len(), 2);

    // Dedup only applies when SQL matches the most recent entry.
    let mut dup = sample_history_entry("SELECT 2");
    dup.execution_time_ms = 99;
    store.add_query_history(dup).await.unwrap();
    let history = store.get_query_history(10, None, None, None).await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].execution_time_ms, 99);

    store.clear_query_history().await.unwrap();
    assert!(store
        .get_query_history(10, None, None, None)
        .await
        .is_empty());
}

#[tokio::test]
async fn favorite_queries_crud() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    // The id is a ULID minted by the store, so the delete below has to use the
    // id it got back — which is exactly what the command layer now does.
    let fav = store
        .add_favorite_query(NewFavorite {
            connection_id: "cfg-1".into(),
            title: "Users".into(),
            sql: "SELECT * FROM users".into(),
            database: None,
            keyword: None,
        })
        .await
        .unwrap();
    let all = store.get_favorite_queries(None).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, fav.id);
    assert_eq!(all[0].title, "Users");
    assert_eq!(
        store
            .get_favorite_queries(Some("cfg-1".into()))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(store
        .get_favorite_queries(Some("cfg-2".into()))
        .await
        .unwrap()
        .is_empty());

    // The SQL is a file on disk, so a restart must find it again.
    let reopened = init_store_for_test(dir.path()).await;
    let persisted = reopened.get_favorite_queries(None).await.unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].sql, "SELECT * FROM users\n");

    store.delete_favorite_query(&fav.id).await.unwrap();
    assert!(store.get_favorite_queries(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn sync_tasks_crud() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let now = Utc::now();
    let task = SyncTask {
        id: "t1".into(),
        source_db_session_id: "s".into(),
        target_db_session_id: "t".into(),
        source_connection_id: "sc".into(),
        target_connection_id: "tc".into(),
        source_database: Some("app".into()),
        target_database: Some("app".into()),
        source_schema: None,
        target_schema: None,
        tables: vec!["users".into()],
        completed_tables: vec![],
        current_table: None,
        current_table_offset: 0,
        source_row_counts: Default::default(),
        strategy: "full".into(),
        status: "running".into(),
        error_message: None,
        created_at: now,
        updated_at: now,
        resume_state: "unknown".into(),
    };
    store.save_sync_task(task.clone()).await.unwrap();
    assert_eq!(store.get_sync_tasks().await.len(), 1);

    let mut updated = task;
    updated.status = "completed".into();
    store.save_sync_task(updated).await.unwrap();
    assert_eq!(store.get_sync_tasks().await[0].status, "completed");

    store.delete_sync_task("t1").await.unwrap();
    assert!(store.get_sync_tasks().await.is_empty());
}
