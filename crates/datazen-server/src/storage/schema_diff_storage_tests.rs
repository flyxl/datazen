//! W8 刀3b 存储件钉子：`ServerSchemaDiffProfiles`（v12 `schema_diff_profiles`
//! 表）与 `ServerMigrationRuns`（v12 `migration_run_history` 表）。
//!
//! 钉的语义：组织隔离（CM-05：越界读空/写不落）、upsert 语义（save 同 id
//! 覆写、run 的 start/finish 两段落账）、截断保留上限、分页与过滤、
//! camelCase wire 词汇往返（共享层 `SchemaDiffProfile` /
//! `MigrationRunRecord`）。

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use datazen::history::{MigrationRunFilter, MigrationRunRecord};
use datazen::migration::schema_diff::profile::SchemaDiffProfile;
use datazen::migration::schema_diff::SchemaDiffProfileRepository;
use datazen::shared::scope::OrganizationScope;

use super::migration_runs::ServerMigrationRuns;
use super::schema_diff_profiles::{connection_exists, ServerSchemaDiffProfiles};
use super::AdminStore;

fn migrated_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn scope(organization: &str) -> OrganizationScope {
    OrganizationScope {
        organization_id: datazen::shared::id::OrganizationId::new(organization),
    }
}

fn profile(id: &str, name: &str) -> SchemaDiffProfile {
    SchemaDiffProfile {
        version: SchemaDiffProfile::CURRENT_VERSION,
        id: id.into(),
        name: name.into(),
        source_connection_id: "conn-source".into(),
        target_connection_id: "conn-target".into(),
        source_database: "app_dev".into(),
        target_database: "app_prod".into(),
        source_schema: Some("public".into()),
        target_schema: None,
        target_only_tables: vec!["audit".into()],
        tables: vec!["users".into(), "orders".into()],
        source_objects: vec![],
        target_objects: vec![datazen_driver_api::DatabaseObject {
            kind: "view".into(),
            name: "v_summary".into(),
            schema: Some("public".into()),
            target_schema: None,
            signature: None,
            target_name: None,
        }],
        allow_destructive: true,
        include_indexes: false,
        require_rollback: true,
        type_overrides: vec![],
        created_at: Utc.with_ymd_and_hms(2026, 11, 7, 1, 2, 3).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2026, 11, 7, 1, 2, 3).unwrap(),
    }
}

#[tokio::test]
async fn profiles_save_list_delete_roundtrip_and_org_isolation() {
    let store = migrated_store();
    let repository = ServerSchemaDiffProfiles::new(store.clone());
    let org_a = scope("org-a");
    let org_b = scope("org-b");

    assert!(repository.list(&org_a).await.unwrap().is_empty());

    repository
        .save(&org_a, profile("p1", "dev → prod"))
        .await
        .unwrap();
    repository
        .save(&org_a, profile("p2", "analytics"))
        .await
        .unwrap();
    // 同 id 覆写（upsert）。
    let mut replacement = profile("p2", "analytics v2");
    replacement.tables = vec!["events".into()];
    repository.save(&org_a, replacement).await.unwrap();

    let listed = repository.list(&org_a).await.unwrap();
    assert_eq!(listed.len(), 2);
    let p2 = listed.iter().find(|p| p.id == "p2").unwrap();
    assert_eq!(p2.name, "analytics v2");
    assert_eq!(p2.tables, vec!["events".to_string()]);
    // camelCase 词汇往返：bool/Option/集合列无损。
    assert!(p2.allow_destructive && p2.require_rollback && !p2.include_indexes);
    assert_eq!(p2.source_schema.as_deref(), Some("public"));
    assert_eq!(p2.target_objects.len(), 1);
    assert_eq!(p2.target_objects[0].kind, "view");

    // 组织隔离：org-b 看不见 org-a 的行，删除也只落自己边界。
    assert!(repository.list(&org_b).await.unwrap().is_empty());
    repository.delete(&org_b, "p1").await.unwrap();
    assert_eq!(repository.list(&org_a).await.unwrap().len(), 2);

    repository.delete(&org_a, "p1").await.unwrap();
    assert_eq!(repository.list(&org_a).await.unwrap().len(), 1);
}

#[tokio::test]
async fn connection_exists_probes_the_org_scoped_catalog() {
    let store = migrated_store();
    {
        let conn = store.lock_for_adapter().unwrap();
        conn.execute(
            "INSERT INTO connection_catalog (organization_id, connection_id, name, config_json, created_at, updated_at) \
             VALUES ('org-a', 'conn-source', 'source', '{}', '2026-11-07T00:00:00Z', '2026-11-07T00:00:00Z')",
            [],
        )
        .unwrap();
    }
    assert!(connection_exists(&store, &scope("org-a"), "conn-source")
        .await
        .unwrap());
    assert!(!connection_exists(&store, &scope("org-a"), "conn-missing")
        .await
        .unwrap());
    assert!(!connection_exists(&store, &scope("org-b"), "conn-source")
        .await
        .unwrap());
}

fn run(id: &str, operation: &str, status: &str) -> MigrationRunRecord {
    MigrationRunRecord {
        id: id.into(),
        operation: operation.into(),
        status: status.into(),
        outcome: if status == "completed" {
            "success".into()
        } else {
            "pending".into()
        },
        phase: "execute".into(),
        profile_id: Some("p1".into()),
        profile_revision: Some("3".into()),
        source_connection_id: Some("c1".into()),
        target_connection_id: Some("c2".into()),
        started_at: "2026-11-07T00:00:00Z".into(),
        finished_at: None,
        selected_count: 10,
        committed_count: 0,
        failed_count: 0,
        conflict_count: 0,
        cancelled: false,
        rollback_outcome: "notRequired".into(),
        error_summary: None,
    }
}

#[tokio::test]
async fn migration_runs_upsert_start_then_finish_and_isolate_by_org() {
    let store = migrated_store();
    let runs = ServerMigrationRuns::new(store.clone());

    let mut record = run("r1", "schemaDiff", "running");
    runs.save("org-a", record.clone()).await.unwrap();

    // finish 段：同 id 覆写终态。
    record.status = "completed".into();
    record.outcome = "success".into();
    record.phase = "finished".into();
    record.finished_at = Some("2026-11-07T00:01:00Z".into());
    record.committed_count = 10;
    runs.save("org-a", record).await.unwrap();

    // 组织隔离。
    runs.save("org-b", run("r2", "schemaDiff", "running"))
        .await
        .unwrap();

    let page = runs
        .list("org-a", MigrationRunFilter::default(), 0, 50)
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, "r1");
    assert_eq!(page.items[0].status, "completed");
    assert_eq!(page.items[0].committed_count, 10);

    // 过滤与分页。
    let filtered = runs
        .list(
            "org-a",
            MigrationRunFilter {
                operation: Some("dataSync".into()),
                ..Default::default()
            },
            0,
            50,
        )
        .await
        .unwrap();
    assert_eq!(filtered.total, 0);
    let by_connection = runs
        .list(
            "org-b",
            MigrationRunFilter {
                connection_id: Some("c2".into()),
                ..Default::default()
            },
            0,
            50,
        )
        .await
        .unwrap();
    assert_eq!(by_connection.total, 1);
}

#[tokio::test]
async fn migration_runs_truncate_beyond_the_retention_cap() {
    let store = migrated_store();
    let runs = ServerMigrationRuns::new(store);
    for index in 0..(super::migration_runs::MAX_MIGRATION_RUN_HISTORY + 10) {
        let mut record = run(&format!("r{index}"), "schemaDiff", "completed");
        record.started_at = format!("2026-11-07T{:02}:{:02}:00Z", index / 60, index % 60);
        runs.save("org-a", record).await.unwrap();
    }
    let page = runs
        .list("org-a", MigrationRunFilter::default(), 0, 1000)
        .await
        .unwrap();
    assert_eq!(
        page.total as usize,
        super::migration_runs::MAX_MIGRATION_RUN_HISTORY,
        "超出保留上限的旧行被截断"
    );
}
