//! The connection list as the user sees it: CRUD, grouping, and the merge
//! rule that a connection's own `group` field is the source of truth when no
//! groups file exists.
//!
//! Saving is last-write-wins per id; the concurrent cases assert that two saves
//! racing on the same store both return `Ok` rather than one losing the file.

use super::fixtures::*;
use datazen_driver_api::ConnectionConfig;

#[tokio::test]
async fn connection_options_persist_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let mut conn = sample_connection_with_ssh();
    let mut opts = serde_json::Map::new();
    opts.insert("topology".into(), serde_json::json!("cluster"));
    conn.options = Some(opts);
    store.save_connection(conn).await.unwrap();

    let loaded = store.get_connections().await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].options.as_ref().unwrap()["topology"],
        serde_json::json!("cluster")
    );

    let store2 = init_store_for_test(dir.path()).await;
    let reloaded = store2.get_connections().await;
    assert_eq!(
        reloaded[0].options.as_ref().unwrap()["topology"],
        serde_json::json!("cluster")
    );
}

#[tokio::test]
async fn connection_crud_and_groups() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let mut conn = sample_connection_with_ssh();
    conn.id = "c1".into();
    conn.group = Some("prod".into());
    store.save_connection(conn.clone()).await.unwrap();
    assert_eq!(store.get_connections().await.len(), 1);
    assert_eq!(store.get_connection("c1").await.unwrap().name, "SSH Test");
    assert!(store.get_connection("missing").await.is_none());

    conn.name = "Updated".into();
    store.save_connection(conn).await.unwrap();
    assert_eq!(store.get_connection("c1").await.unwrap().name, "Updated");

    store.save_groups(vec!["dev".into()]).await.unwrap();
    let groups = store.get_groups().await;
    assert!(groups.contains(&"dev".to_string()));
    assert!(groups.contains(&"prod".to_string()));

    store.delete_connection("c1").await.unwrap();
    assert!(store.get_connections().await.is_empty());
}

#[tokio::test]
async fn groups_merge_connection_groups_without_groups_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let mut conn = sample_connection_with_ssh();
    conn.id = "g1".into();
    conn.group = Some("from-conn".into());
    store.save_connection(conn).await.unwrap();
    let groups = store.get_groups().await;
    assert_eq!(groups, vec!["from-conn".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_save_connection_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(init_store_for_test(dir.path()).await);

    let mut handles = Vec::new();
    for i in 0..16u64 {
        let store = std::sync::Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let mut conn = sample_connection_with_ssh();
            conn.id = format!("conn-concurrent-{i}");
            conn.name = format!("Concurrent {i}");
            store.save_connection(conn).await.unwrap();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    let connections = store.get_connections().await;
    assert_eq!(connections.len(), 16);
    let content = tokio::fs::read_to_string(dir.path().join("connections.json"))
        .await
        .unwrap();
    let parsed: Vec<ConnectionConfig> = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed.len(), 16);
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!entries.iter().any(|n| n.contains(".tmp")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_save_groups_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(init_store_for_test(dir.path()).await);

    let mut handles = Vec::new();
    for i in 0..16u64 {
        let store = std::sync::Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let groups: Vec<String> = (0..4).map(|j| format!("g{i}-{j}")).collect();
            store.save_groups(groups).await.unwrap();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    let groups = store.get_groups().await;
    assert_eq!(groups.len(), 4);
    let content = tokio::fs::read_to_string(dir.path().join("groups.json"))
        .await
        .unwrap();
    let parsed: Vec<String> = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed.len(), 4);
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!entries.iter().any(|n| n.contains(".tmp")));
}

/// Deleting a connection must remove its AI notes, memories (including FTS
/// entries), and audit rows. The AI cleanup is one AppDb transaction invoked
/// by `DesktopRepository::delete_connection`.
#[tokio::test]
async fn deleting_connection_purges_ai_notes_memories_and_audit() {
    use crate::platform::LOCAL_ORGANIZATION_ID;
    use crate::store::ai_memory::{AiConnectionNoteRecord, AiMemoryAuditRecord, AiMemoryRecord};

    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let app_db = store.app_db();
    let ts = "2026-11-05T00:00:00Z";
    let owner = "owner-delete";

    app_db
        .upsert_ai_connection_note(&AiConnectionNoteRecord {
            connection_id: "c-delete".into(),
            organization_id: LOCAL_ORGANIZATION_ID.into(),
            owner_principal_id: owner.into(),
            content: "note".into(),
            updated_at: ts.into(),
        })
        .unwrap();
    app_db
        .upsert_ai_memory(&AiMemoryRecord {
            id: "m-delete".into(),
            organization_id: LOCAL_ORGANIZATION_ID.into(),
            owner_principal_id: owner.into(),
            scope_type: "connection".into(),
            scope_id: "c-delete".into(),
            connection_id: Some("c-delete".into()),
            database_name: None,
            schema_name: None,
            table_name: None,
            column_name: None,
            kind: "preference".into(),
            subject: "delete me".into(),
            content: "deletetoken".into(),
            structured_json: None,
            source: "explicit".into(),
            source_ref: None,
            confidence: 0.5,
            sensitivity: "normal".into(),
            status: "active".into(),
            pinned: false,
            use_count: 0,
            last_used_at: None,
            expires_at: None,
            content_hash: "hash-delete".into(),
            created_at: ts.into(),
            updated_at: ts.into(),
        })
        .unwrap();
    app_db
        .append_ai_memory_audit(&AiMemoryAuditRecord {
            id: "a-delete".into(),
            memory_id: "m-delete".into(),
            action: "create".into(),
            actor_principal_id: owner.into(),
            before_hash: None,
            after_hash: Some("hash-delete".into()),
            detail_json: None,
            created_at: ts.into(),
        })
        .unwrap();

    let mut conn = sample_connection_with_ssh();
    conn.id = "c-delete".into();
    store.save_connection(conn).await.unwrap();
    store.delete_connection("c-delete").await.unwrap();

    assert!(store.get_connection("c-delete").await.is_none());
    assert!(app_db
        .get_ai_connection_note(LOCAL_ORGANIZATION_ID, owner, "c-delete")
        .unwrap()
        .is_none());
    assert!(app_db
        .get_ai_memory(LOCAL_ORGANIZATION_ID, owner, "m-delete")
        .unwrap()
        .is_none());
    assert!(app_db.list_ai_memory_audit("m-delete").unwrap().is_empty());
    assert!(app_db
        .search_ai_memories(LOCAL_ORGANIZATION_ID, owner, "deletetoken", 10)
        .unwrap()
        .is_empty());
}

/// Purge-first delete: a failed AI cleanup must surface as an error and leave
/// both the catalog connection and its AI data (including audit) intact.
#[tokio::test]
async fn delete_connection_cleanup_failure_is_recoverable_and_keeps_ai_data() {
    use crate::platform::LOCAL_ORGANIZATION_ID;
    use crate::store::ai_memory::{AiConnectionNoteRecord, AiMemoryAuditRecord, AiMemoryRecord};

    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let app_db = store.app_db();
    let ts = "2026-11-05T00:00:00Z";
    let owner = "owner-fail";

    app_db
        .upsert_ai_connection_note(&AiConnectionNoteRecord {
            connection_id: "c-fail".into(),
            organization_id: LOCAL_ORGANIZATION_ID.into(),
            owner_principal_id: owner.into(),
            content: "keep me".into(),
            updated_at: ts.into(),
        })
        .unwrap();
    app_db
        .upsert_ai_memory(&AiMemoryRecord {
            id: "m-fail".into(),
            organization_id: LOCAL_ORGANIZATION_ID.into(),
            owner_principal_id: owner.into(),
            scope_type: "connection".into(),
            scope_id: "c-fail".into(),
            connection_id: Some("c-fail".into()),
            database_name: None,
            schema_name: None,
            table_name: None,
            column_name: None,
            kind: "preference".into(),
            subject: "keep me".into(),
            content: "failtoken".into(),
            structured_json: None,
            source: "explicit".into(),
            source_ref: None,
            confidence: 0.5,
            sensitivity: "normal".into(),
            status: "active".into(),
            pinned: false,
            use_count: 0,
            last_used_at: None,
            expires_at: None,
            content_hash: "hash-fail".into(),
            created_at: ts.into(),
            updated_at: ts.into(),
        })
        .unwrap();
    app_db
        .append_ai_memory_audit(&AiMemoryAuditRecord {
            id: "a-fail".into(),
            memory_id: "m-fail".into(),
            action: "create".into(),
            actor_principal_id: owner.into(),
            before_hash: None,
            after_hash: Some("hash-fail".into()),
            detail_json: None,
            created_at: ts.into(),
        })
        .unwrap();
    app_db
        .with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER fail_ai_note_delete
                 BEFORE DELETE ON ai_connection_notes
                 WHEN old.connection_id = 'c-fail'
                 BEGIN
                   SELECT RAISE(ABORT, 'forced AI cleanup failure');
                 END;",
            )?;
            Ok(())
        })
        .unwrap();

    let mut conn = sample_connection_with_ssh();
    conn.id = "c-fail".into();
    store.save_connection(conn).await.unwrap();

    let error = store.delete_connection("c-fail").await.unwrap_err();
    assert!(error.to_string().contains("AI cleanup failed"), "{error}");
    assert!(store.get_connection("c-fail").await.is_some());
    let on_disk = tokio::fs::read_to_string(dir.path().join("connections.json"))
        .await
        .unwrap();
    assert!(
        on_disk.contains("c-fail"),
        "failed cleanup must not persist the catalog delete"
    );
    assert_eq!(
        app_db
            .get_ai_connection_note(LOCAL_ORGANIZATION_ID, owner, "c-fail")
            .unwrap()
            .unwrap()
            .content,
        "keep me"
    );
    assert!(app_db
        .get_ai_memory(LOCAL_ORGANIZATION_ID, owner, "m-fail")
        .unwrap()
        .is_some());
    assert_eq!(
        app_db
            .search_ai_memories(LOCAL_ORGANIZATION_ID, owner, "failtoken", 10)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(app_db.list_ai_memory_audit("m-fail").unwrap().len(), 1);

    app_db
        .with_conn(|conn| {
            conn.execute("DROP TRIGGER fail_ai_note_delete", [])?;
            Ok(())
        })
        .unwrap();
    store.delete_connection("c-fail").await.unwrap();
    assert!(store.get_connection("c-fail").await.is_none());
    let on_disk = tokio::fs::read_to_string(dir.path().join("connections.json"))
        .await
        .unwrap();
    assert!(!on_disk.contains("c-fail"));
    assert!(app_db
        .get_ai_connection_note(LOCAL_ORGANIZATION_ID, owner, "c-fail")
        .unwrap()
        .is_none());
    assert!(app_db
        .get_ai_memory(LOCAL_ORGANIZATION_ID, owner, "m-fail")
        .unwrap()
        .is_none());
    assert!(app_db
        .search_ai_memories(LOCAL_ORGANIZATION_ID, owner, "failtoken", 10)
        .unwrap()
        .is_empty());
    assert!(app_db.list_ai_memory_audit("m-fail").unwrap().is_empty());
}
