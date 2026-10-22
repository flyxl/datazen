//! The named views of the desktop store.
//!
//! What is asserted here is the **store's** half: the file survives a
//! restart, a damaged record does not hide the healthy ones, and the ids the
//! store mints are ULIDs. The rules (the name uniqueness, the one default per
//! scope, the idempotent delete) live in the shared layer and are exercised
//! through it — the store is only the repository they run against.

use super::super::*;
use super::fixtures::*;
use chrono::Utc;
use datazen_feature_core::services::named_views::{
    delete_named_view, list_named_views, save_named_view, NamedView, NamedViewScope,
};
use datazen_feature_core::services::query_executor::FilterCondition;

fn scope(table: &str) -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-1".into(),
        database: Some("app".into()),
        schema: None,
        table: table.into(),
    }
}

fn view(table: &str, name: &str) -> NamedView {
    NamedView {
        id: String::new(),
        name: name.into(),
        scope: scope(table),
        filters: Vec::new(),
        filter_logic: "and".into(),
        sorts: Vec::new(),
        visible_columns: None,
        column_order: None,
        is_default: false,
        created_at: Utc::now(),
        updated_at: None,
    }
}

/// The store's id minter, as the command layer uses it.
fn minter() -> HostViewIdMinter {
    HostViewIdMinter
}

#[tokio::test]
async fn named_view_round_trip_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let mut record = view("users", "Open users");
    record.filters = vec![FilterCondition {
        column: "id".into(),
        operator: datazen_driver_api::filters::FilterOperator::Eq,
        value: datazen_driver_api::Value::Integer(7),
        connector: None,
    }];
    record.visible_columns = Some(vec!["id".into(), "name".into()]);
    let saved = save_named_view(&store, &minter(), record)
        .await
        .expect("saved");

    // Reopening the same directory has to find the record again: that is what
    // proves it was written to disk and not only cached.
    let reopened = init_store_for_test(dir.path()).await;
    let listed = list_named_views(&reopened, Some(&scope("users")))
        .await
        .expect("listed");

    assert_eq!(listed.len(), 1);
    let found = &listed[0];
    assert_eq!(found.id, saved.id);
    assert_eq!(found.name, "Open users");
    assert_eq!(found.scope, saved.scope);
    assert_eq!(found.filter_logic, "and");
    assert_eq!(found.visible_columns, saved.visible_columns);
    assert_eq!(found.column_order, saved.column_order);
    assert!(!found.is_default);
    assert_eq!(found.filters.len(), 1);
    assert_eq!(found.filters[0].column, "id");
    assert_eq!(found.created_at, saved.created_at);
}

#[tokio::test]
async fn save_with_empty_id_mints_ulid_and_created_at() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let saved = save_named_view(&store, &minter(), view("users", "Users"))
        .await
        .expect("saved");

    assert!(
        crate::store::favorites::ulid::Ulid::is_valid(&saved.id),
        "the store mints a ULID, got {:?}",
        saved.id
    );
    assert!(saved.created_at <= Utc::now());
    assert!(saved.updated_at.is_some());
}

#[tokio::test]
async fn duplicate_name_in_same_scope_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    save_named_view(&store, &minter(), view("users", "Users"))
        .await
        .expect("saved");

    let error = save_named_view(&store, &minter(), view("users", "users"))
        .await
        .expect_err("the name is taken");
    assert!(
        error.to_string().starts_with("grid.view.nameExists: "),
        "got: {error}"
    );
}

#[tokio::test]
async fn same_name_in_different_scope_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    save_named_view(&store, &minter(), view("users", "Open"))
        .await
        .expect("saved");

    // Another table of the same connection.
    save_named_view(&store, &minter(), view("orders", "Open"))
        .await
        .expect("another table may carry the same name");

    // Another connection's table.
    let mut other = view("users", "Open");
    other.scope.connection_id = "conn-2".into();
    save_named_view(&store, &minter(), other)
        .await
        .expect("another connection may carry the same name");

    assert_eq!(store.get_named_views().await.len(), 3);
}

#[tokio::test]
async fn rename_keeps_id_and_updates_updated_at() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let saved = save_named_view(&store, &minter(), view("users", "Users"))
        .await
        .expect("saved");

    let mut renamed = saved.clone();
    renamed.name = "Open users".into();
    renamed.updated_at = None;
    let updated = save_named_view(&store, &minter(), renamed)
        .await
        .expect("renamed");

    assert_eq!(updated.id, saved.id, "a rename keeps the identity");
    assert_eq!(updated.created_at, saved.created_at);
    assert!(updated.updated_at.is_some());

    let listed = list_named_views(&store, None).await.expect("listed");
    assert_eq!(listed.len(), 1, "a rename does not add a record");
    assert_eq!(listed[0].name, "Open users");
}

#[tokio::test]
async fn delete_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let saved = save_named_view(&store, &minter(), view("users", "Users"))
        .await
        .expect("saved");

    let remaining = delete_named_view(&store, &saved.id).await.expect("deleted");
    assert!(remaining.is_empty());

    // Deleting it again changes nothing and is not an error.
    let remaining = delete_named_view(&store, &saved.id)
        .await
        .expect("deleted again");
    assert!(remaining.is_empty());
    assert!(store.get_named_views().await.is_empty());
}

#[tokio::test]
async fn only_one_default_per_scope() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let mut first = view("users", "First");
    first.is_default = true;
    save_named_view(&store, &minter(), first)
        .await
        .expect("saved");

    let mut second = view("users", "Second");
    second.is_default = true;
    save_named_view(&store, &minter(), second)
        .await
        .expect("saved");

    let listed = list_named_views(&store, Some(&scope("users")))
        .await
        .expect("listed");
    assert_eq!(
        listed.iter().filter(|entry| entry.is_default).count(),
        1,
        "exactly one default per scope"
    );
    assert!(listed[0].is_default, "the default comes first");
}

#[tokio::test]
async fn corrupt_record_does_not_hide_healthy_records() {
    let dir = tempfile::tempdir().unwrap();

    // A file with one damaged record and one healthy one. The damaged record
    // must not take the healthy one down with it.
    let content = r#"[
      {"id":"broken","name":42,"scope":{"connectionId":"conn-1","table":"users"}},
      {
        "id":"good",
        "name":"Users",
        "scope":{"connectionId":"conn-1","database":"app","schema":null,"table":"users"},
        "filters":[],
        "filterLogic":"and",
        "sorts":[],
        "visibleColumns":null,
        "columnOrder":null,
        "isDefault":false,
        "createdAt":"2026-01-01T00:00:00Z",
        "updatedAt":null
      }
    ]"#;
    tokio::fs::write(dir.path().join("named_views.json"), content)
        .await
        .expect("written");

    let store = init_store_for_test(dir.path()).await;
    let views = store.get_named_views().await;

    assert_eq!(views.len(), 1, "the healthy record is still there");
    assert_eq!(views[0].id, "good");
    assert_eq!(views[0].name, "Users");
}

#[tokio::test]
async fn view_with_invalid_filter_logic_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let mut record = view("users", "Users");
    record.filter_logic = "xor".into();
    let error = save_named_view(&store, &minter(), record)
        .await
        .expect_err("xor is not a logic");

    assert!(
        error.to_string().starts_with("grid.view.invalid: "),
        "got: {error}"
    );
    assert!(store.get_named_views().await.is_empty());
}
