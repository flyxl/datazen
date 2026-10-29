//! Migration profile and resumable task persistence tests.

use super::super::*;
use super::fixtures::*;
use chrono::Utc;

#[tokio::test]
async fn sync_profiles_roundtrip_and_runtime_fields_are_not_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let now = Utc::now();
    let profile = crate::data_sync::SyncProfile {
        version: crate::data_sync::SyncProfile::CURRENT_VERSION,
        id: "sync-profile-1".into(),
        name: "Nightly sync".into(),
        source_connection_id: "source-config".into(),
        target_connection_id: "target-config".into(),
        source_database: Some("app".into()),
        target_database: Some("app".into()),
        source_schema: Some("public".into()),
        target_schema: Some("public".into()),
        tables: vec![crate::data_sync::TableMapping::auto("users")],
        options: crate::data_sync::SyncOptions::default(),
        created_at: now,
        updated_at: now,
    };
    store.save_sync_profile(profile.clone()).await.unwrap();
    assert_eq!(store.get_sync_profiles().await, vec![profile]);

    let persisted = tokio::fs::read_to_string(dir.path().join("sync_profiles.json"))
        .await
        .unwrap();
    assert!(!persisted.contains("dbSessionId"));
    assert!(!persisted.contains("planId"));
    assert!(!persisted.contains("credentials"));

    let reloaded = init_store_for_test(dir.path()).await;
    assert_eq!(reloaded.get_sync_profiles().await.len(), 1);
    reloaded
        .delete_sync_profile("sync-profile-1")
        .await
        .unwrap();
    assert!(reloaded.get_sync_profiles().await.is_empty());
}

#[tokio::test]
async fn sync_profiles_filter_invalid_records_on_load() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let valid = crate::data_sync::SyncProfile {
        version: crate::data_sync::SyncProfile::CURRENT_VERSION,
        id: "valid-profile".into(),
        name: "Valid sync".into(),
        source_connection_id: "source-config".into(),
        target_connection_id: "target-config".into(),
        source_database: None,
        target_database: None,
        source_schema: None,
        target_schema: None,
        tables: vec![crate::data_sync::TableMapping::auto("users")],
        options: crate::data_sync::SyncOptions::default(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let mut unknown = serde_json::to_value(&valid).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    let mut invalid = serde_json::to_value(&valid).unwrap();
    invalid["version"] = serde_json::json!(99);
    tokio::fs::write(
        dir.path().join("sync_profiles.json"),
        serde_json::to_vec(&serde_json::json!([
            valid,
            unknown,
            invalid,
            null,
            "malformed-profile-record"
        ]))
        .unwrap(),
    )
    .await
    .unwrap();

    let profiles = store.get_sync_profiles().await;
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, "valid-profile");

    let replacement = crate::data_sync::SyncProfile {
        id: "replacement-profile".into(),
        name: "Replacement sync".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        ..profiles[0].clone()
    };
    store.save_sync_profile(replacement.clone()).await.unwrap();
    assert_eq!(
        store
            .get_sync_profiles()
            .await
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>(),
        vec!["valid-profile", "replacement-profile"]
    );
    store.delete_sync_profile("valid-profile").await.unwrap();
    assert_eq!(store.get_sync_profiles().await, vec![replacement]);
}

#[tokio::test]
async fn sync_task_persistence_drops_runtime_ids_and_blocks_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let now = Utc::now();
    let task = SyncTask {
        id: "legacy-offset".into(),
        source_db_session_id: "stale-source-session".into(),
        target_db_session_id: "stale-target-session".into(),
        source_connection_id: "source-config".into(),
        target_connection_id: "target-config".into(),
        source_database: Some("source_db".into()),
        target_database: Some("target_db".into()),
        source_schema: Some("public".into()),
        target_schema: Some("public".into()),
        tables: vec!["users".into()],
        completed_tables: vec![],
        current_table: Some("users".into()),
        current_table_offset: 42,
        source_row_counts: [("users".to_string(), 100u64)].into_iter().collect(),
        strategy: "continue".into(),
        status: "paused".into(),
        error_message: None,
        created_at: now,
        updated_at: now,
        resume_state: "unknown".into(),
    };

    store.save_sync_task(task).await.unwrap();

    let persisted = tokio::fs::read_to_string(dir.path().join("sync_tasks.json"))
        .await
        .unwrap();
    assert!(!persisted.contains("sourceDbSessionId"));
    assert!(!persisted.contains("targetDbSessionId"));

    let loaded = store.get_sync_tasks().await;
    let loaded = &loaded[0];
    assert!(loaded.source_db_session_id.is_empty());
    assert!(loaded.target_db_session_id.is_empty());
    assert_eq!(loaded.current_table_offset, 0);
    assert_eq!(loaded.strategy, "unknown");
    assert_eq!(loaded.status, "interrupted");
    assert_eq!(loaded.resume_state, "unknown");
    assert!(loaded
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains("cannot be resumed safely")));
}

#[tokio::test]
async fn sync_task_legacy_json_loads_and_is_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let now = Utc::now();
    let legacy = serde_json::json!([{
        "id": "old-task",
        "sourceDbSessionId": "old-source-session",
        "targetDbSessionId": "old-target-session",
        "sourceConnectionId": "source-config",
        "targetConnectionId": "target-config",
        "tables": ["users"],
        "completedTables": [],
        "currentTable": "users",
        "currentTableOffset": 7,
        "sourceRowCounts": {"users": 10},
        "strategy": "continue",
        "status": "running",
        "errorMessage": null,
        "createdAt": now,
        "updatedAt": now
    }]);
    tokio::fs::write(
        dir.path().join("sync_tasks.json"),
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .await
    .unwrap();

    let loaded = store.get_sync_tasks().await;
    let loaded = &loaded[0];
    assert_eq!(loaded.id, "old-task");
    assert_eq!(loaded.source_connection_id, "source-config");
    assert_eq!(loaded.target_connection_id, "target-config");
    assert!(loaded.source_db_session_id.is_empty());
    assert!(loaded.target_db_session_id.is_empty());
    assert_eq!(loaded.current_table_offset, 0);
    assert_eq!(loaded.strategy, "unknown");
    assert_eq!(loaded.status, "interrupted");
    assert_eq!(loaded.resume_state, "unknown");

    let migrated = tokio::fs::read_to_string(dir.path().join("sync_tasks.json"))
        .await
        .unwrap();
    assert!(!migrated.contains("old-source-session"));
    assert!(!migrated.contains("old-target-session"));
    assert!(migrated.contains("\"resumeState\": \"unknown\""));
}
