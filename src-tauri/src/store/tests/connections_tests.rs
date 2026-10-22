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
