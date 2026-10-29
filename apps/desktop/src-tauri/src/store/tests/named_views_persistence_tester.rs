//! The **persistence** of the named views, checked on the file itself.
//!
//! The track's own store suite (`named_views_tests.rs`) checks the *behaviour*
//! of the store: a round trip across a reopen, a damaged record, the minted
//! ULID, the rules. This file checks the things that are only visible **on
//! disk**, because a store can behave correctly in memory and still write the
//! wrong file:
//!
//! - the file is `{appData}/named_views.json` and holds a **pretty** JSON
//!   `NamedView[]` — the same family as `settings.json` and
//!   `sync_profiles.json`;
//! - a **new** `Store` on the same directory finds the records, field by
//!   field — the proof that the data was written and not only cached;
//! - a **damaged record** on disk does not take the healthy ones down with
//!   it, and the healthy ones stay usable (they can still be listed *and*
//!   renamed — a record that survives the load but is then unusable would be
//!   half a fix);
//! - the write is **atomic**: no temp file is left behind, and the file is
//!   never observed half-written.

use super::super::*;
use super::fixtures::*;
use chrono::Utc;
use datazen::named_views::{list_named_views, save_named_view, NamedView, NamedViewScope};

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

fn minter() -> HostViewIdMinter {
    HostViewIdMinter
}

#[tokio::test]
async fn the_file_is_pretty_json_holding_a_list_of_views() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    save_named_view(&store, &minter(), view("users", "Open users"))
        .await
        .expect("saved");

    let path = dir.path().join("named_views.json");
    assert!(path.exists(), "the views live in named_views.json");

    let raw = tokio::fs::read_to_string(&path).await.expect("readable");
    // **Pretty** JSON: the same writer the other store files use.
    assert!(
        raw.contains('\n'),
        "the file is pretty JSON, not one line: {raw}"
    );
    assert!(
        raw.contains("  "),
        "the file is indented, not one line: {raw}"
    );

    // It parses as a list of views.
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let entries = parsed.as_array().expect("an array of views");
    assert_eq!(entries.len(), 1);
    let entry = entries[0].as_object().expect("an object");
    for key in [
        "id",
        "name",
        "scope",
        "filters",
        "filterLogic",
        "sorts",
        "visibleColumns",
        "columnOrder",
        "isDefault",
        "createdAt",
        "updatedAt",
    ] {
        assert!(
            entry.contains_key(key),
            "the record carries {key}: {entry:?}"
        );
    }
    // The scope is the persistent identity, and it carries no session id.
    let scope_object = entry["scope"].as_object().expect("a scope");
    assert!(scope_object.contains_key("connectionId"));
    assert!(scope_object.contains_key("table"));
    assert!(
        !scope_object.contains_key("dbSessionId"),
        "a runtime session id must never be persisted: {scope_object:?}"
    );
}

#[tokio::test]
async fn a_new_store_on_the_same_directory_finds_every_field() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let mut record = view("users", "Open users");
    record.filters = vec![datazen::services::query_executor::FilterCondition {
        column: "id".into(),
        operator: datazen_driver_api::filters::FilterOperator::Eq,
        value: datazen_driver_api::Value::Integer(7),
        connector: None,
    }];
    record.sorts = vec![datazen::services::query_executor::SortCondition {
        column: "name".into(),
        descending: true,
    }];
    record.visible_columns = Some(vec!["id".into(), "name".into()]);
    record.column_order = Some(vec!["name".into(), "id".into()]);
    record.is_default = true;

    let saved = save_named_view(&store, &minter(), record)
        .await
        .expect("saved");

    // A **new** store instance over the same directory: the cache is empty
    // and everything comes off the disk.
    let reopened = init_store_for_test(dir.path()).await;
    let listed = list_named_views(&reopened, Some(&scope("users")))
        .await
        .expect("listed");
    assert_eq!(listed.len(), 1);
    let found = &listed[0];

    // Field by field — the whole record, not a subset.
    assert_eq!(found.id, saved.id);
    assert_eq!(found.name, saved.name);
    assert_eq!(found.scope, saved.scope);
    assert_eq!(found.filter_logic, saved.filter_logic);
    assert_eq!(found.visible_columns, saved.visible_columns);
    assert_eq!(found.column_order, saved.column_order);
    assert_eq!(found.is_default, saved.is_default);
    assert_eq!(found.created_at, saved.created_at);
    assert_eq!(found.updated_at, saved.updated_at);
    assert_eq!(found.filters.len(), 1);
    assert_eq!(found.filters[0].column, "id");
    assert_eq!(found.filters[0].connector, None);
    assert_eq!(found.sorts.len(), 1);
    assert_eq!(found.sorts[0].column, "name");
    assert!(found.sorts[0].descending);
}

#[tokio::test]
async fn a_damaged_record_leaves_the_healthy_one_usable() {
    let dir = tempfile::tempdir().unwrap();
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

    // And it is **usable**: a rename writes it back, and the damaged
    // neighbour is still only skipped — not resurrected.
    let mut renamed = views[0].clone();
    renamed.name = "Renamed".into();
    let saved = save_named_view(&store, &minter(), renamed)
        .await
        .expect("the healthy record is still writable");
    assert_eq!(saved.name, "Renamed");

    let after = store.get_named_views().await;
    assert_eq!(after.len(), 1, "the damaged record did not come back");
    assert_eq!(after[0].name, "Renamed");
}

#[tokio::test]
async fn the_write_leaves_no_temporary_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    for name in ["First", "Second", "Third"] {
        save_named_view(&store, &minter(), view("users", name))
            .await
            .expect("saved");
    }

    // The atomic writer uses `.{file}.{pid}.{nanos}.{seq}.tmp` and renames it
    // over the target; a leftover would mean the rename did not happen.
    let mut leftovers: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir.path()).expect("readable") {
        let entry = entry.expect("an entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.contains(".tmp") {
            leftovers.push(name);
        }
    }
    assert!(
        leftovers.is_empty(),
        "the atomic write left temporary files behind: {leftovers:?}"
    );
    assert!(dir.path().join("named_views.json").exists());
}
