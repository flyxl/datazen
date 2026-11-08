//! v15 sync 双表的存储测试（W8 刀3e-2）。
//!
//! 覆盖面：档案三面（保存/列表/删除 + 全列往返 + 组织隔离）、任务三面
//! （保存/列表/删除 + 整档往返 + 组织隔离）、遗留归一化的读写两沿纪律
//! （写沿清除运行时会话与不安全检查点；读沿归一化应答但不回写行）。

use std::sync::Arc;

use chrono::{TimeZone, Utc};

use datazen::migration::data_sync::profile::SyncProfile;
use datazen::migration::data_sync::{StoredSyncTask, TableMapping};
use datazen::shared::id::OrganizationId;
use datazen::shared::scope::OrganizationScope;

use crate::storage::{AdminStore, ServerSyncProfiles, ServerSyncTasks};

const ORG: &str = "org-sync-assets-tests";
const OTHER_ORG: &str = "org-other";

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("admin store");
    store.migrate().expect("migrations");
    Arc::new(store)
}

fn scope(org: &str) -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(org))
}

fn epoch() -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(0, 0).single().expect("epoch")
}

fn profile(id: &str) -> SyncProfile {
    SyncProfile {
        version: SyncProfile::CURRENT_VERSION,
        id: id.to_owned(),
        name: format!("Nightly {id}"),
        source_connection_id: "conn-source".to_owned(),
        target_connection_id: "conn-target".to_owned(),
        source_database: Some("app".to_owned()),
        target_database: Some("app".to_owned()),
        source_schema: Some("public".to_owned()),
        target_schema: None,
        tables: vec![TableMapping::auto("users")],
        options: Default::default(),
        created_at: epoch(),
        updated_at: epoch(),
    }
}

fn task(id: &str) -> StoredSyncTask {
    StoredSyncTask {
        id: id.to_owned(),
        source_db_session_id: String::new(),
        target_db_session_id: String::new(),
        source_connection_id: "conn-source".to_owned(),
        target_connection_id: "conn-target".to_owned(),
        source_database: Some("app".to_owned()),
        target_database: Some("app".to_owned()),
        source_schema: None,
        target_schema: None,
        tables: vec!["users".to_owned()],
        completed_tables: vec!["users".to_owned()],
        current_table: None,
        current_table_offset: 0,
        source_row_counts: [("users".to_owned(), 10)].into_iter().collect(),
        strategy: "full".to_owned(),
        status: "completed".to_owned(),
        error_message: None,
        created_at: epoch(),
        updated_at: epoch(),
        resume_state: "unknown".to_owned(),
    }
}

#[tokio::test]
async fn profile_round_trip_is_lossless_and_org_scoped() {
    let store = store();
    let profiles = ServerSyncProfiles::new(store.clone());

    profiles
        .save(&scope(ORG), profile("p-1"))
        .await
        .expect("save");
    profiles
        .save(&scope(OTHER_ORG), profile("p-other"))
        .await
        .expect("save other org");

    let listed = profiles.list(&scope(ORG)).await.expect("list");
    assert_eq!(listed, vec![profile("p-1")], "full-column round trip");
    // 越组织读空（CM-05）。
    assert!(profiles
        .list(&scope("no-such-org"))
        .await
        .unwrap()
        .is_empty());

    profiles.delete(&scope(ORG), "p-1").await.expect("delete");
    assert!(profiles.list(&scope(ORG)).await.unwrap().is_empty());
    // 另一组织的行不受影响。
    assert_eq!(profiles.list(&scope(OTHER_ORG)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn task_round_trip_is_lossless_and_org_scoped() {
    let store = store();
    let tasks = ServerSyncTasks::new(store.clone());

    tasks.save(&scope(ORG), task("t-1")).await.expect("save");
    tasks
        .save(&scope(OTHER_ORG), task("t-other"))
        .await
        .expect("save other org");

    let listed = tasks.list(&scope(ORG)).await.expect("list");
    assert_eq!(listed, vec![task("t-1")], "whole-payload round trip");
    assert!(tasks.list(&scope("no-such-org")).await.unwrap().is_empty());

    tasks.delete(&scope(ORG), "t-1").await.expect("delete");
    assert!(tasks.list(&scope(ORG)).await.unwrap().is_empty());
    assert_eq!(tasks.list(&scope(OTHER_ORG)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn saving_normalizes_runtime_sessions_and_unsafe_checkpoints() {
    let store = store();
    let tasks = ServerSyncTasks::new(store.clone());

    let mut unsafe_task = task("t-unsafe");
    unsafe_task.source_db_session_id = "session-9".to_owned();
    unsafe_task.target_db_session_id = "session-8".to_owned();
    unsafe_task.current_table_offset = 42;
    unsafe_task.strategy = "continue".to_owned();
    unsafe_task.status = "running".to_owned();
    tasks
        .save(&scope(ORG), unsafe_task)
        .await
        .expect("save unsafe task");

    let listed = tasks.list(&scope(ORG)).await.expect("list");
    let stored = listed.first().expect("one row");
    // 写沿归一化落库：检查点清除、状态降级、错误信息在场。
    assert!(stored.source_db_session_id.is_empty());
    assert_eq!(stored.current_table_offset, 0);
    assert_eq!(stored.strategy, "unknown");
    assert_eq!(stored.status, "interrupted");
    assert!(stored
        .error_message
        .as_deref()
        .unwrap_or_default()
        .contains("cannot be resumed safely"));
}

#[tokio::test]
async fn reading_normalizes_the_answer_without_rewriting_the_row() {
    let store = store();
    let tasks = ServerSyncTasks::new(store.clone());
    tasks.save(&scope(ORG), task("t-read")).await.expect("save");

    // 直接在行里伪造一个不安全检查点（模拟旧版本写入的行——读沿必须
    // 归一化应答，但不得回写）。
    {
        let conn = store.lock_for_adapter().expect("lock");
        conn.execute(
            "UPDATE sync_tasks SET payload_json = ?1 WHERE organization_id = ?2 AND id = 't-read'",
            rusqlite::params![
                serde_json::to_string(&{
                    let mut forged = task("t-read");
                    forged.current_table_offset = 7;
                    forged
                })
                .unwrap(),
                ORG
            ],
        )
        .expect("forge row");
    }

    let listed = tasks.list(&scope(ORG)).await.expect("list");
    let answered = listed.first().expect("one row");
    // 应答已归一化……
    assert_eq!(answered.current_table_offset, 0);
    assert_eq!(answered.status, "interrupted");

    // ……而行保持原样（读沿不产生写放大）。
    let raw: String = {
        let conn = store.lock_for_adapter().expect("lock");
        conn.query_row(
            "SELECT payload_json FROM sync_tasks WHERE organization_id = ?1 AND id = 't-read'",
            rusqlite::params![ORG],
            |row| row.get(0),
        )
        .expect("read raw row")
    };
    let raw_task: StoredSyncTask = serde_json::from_str(&raw).unwrap();
    assert_eq!(raw_task.current_table_offset, 7);
}
