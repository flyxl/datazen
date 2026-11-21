use std::collections::{BTreeMap, BTreeSet};

use crate::{RelationColumn, RelationColumns, RelationRef, SqlNamespace};

use super::*;

fn table(name: &str, schema: Option<&str>, table_type: &str) -> SchemaTableInfo {
    SchemaTableInfo {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        table_type: table_type.to_owned(),
        row_count: None,
    }
}

fn snapshot(id: &str) -> SchemaStoreSnapshot {
    let mut schemas = BTreeMap::new();
    schemas.insert(id.to_owned(), SchemaSessionState::default());
    SchemaStoreSnapshot { schemas, active_db_session_id: Some(id.to_owned()) }
}

#[test]
fn active_connection_removal_and_error_recording() {
    let mut store = snapshot("s1");
    store = set_active_connection(&store, Some("s2".to_owned()));
    assert_eq!(store.active_db_session_id.as_deref(), Some("s2"));

    let after = remove_connection(&store, "s1");
    assert!(!after.schemas.contains_key("s1"));
    assert_eq!(after.active_db_session_id.as_deref(), Some("s2"));
    let after_active = remove_connection(&after, "s2");
    assert_eq!(after_active.active_db_session_id, None);

    let errored = record_session_error(&after, "s1", "failed");
    assert_eq!(errored.schemas.get("s1").and_then(|entry| entry.error.as_deref()), Some("failed"));
    assert_eq!(record_session_error(&errored, " ", "x"), errored);
}

#[test]
fn current_schema_and_database_normalize_and_validate() {
    let store = snapshot("s1");
    let set = set_current_schema(&store, Some("  public "), "s1");
    assert_eq!(set.schemas["s1"].current_schema.as_deref(), Some("public"));
    let cleared = set_current_schema(&set, Some("   "), "s1");
    assert_eq!(cleared.schemas["s1"].current_schema, None);

    let with_dbs = commit_connection_patch(
        &store,
        "s1",
        SchemaPatch {
            databases: Some(vec!["main".to_owned()]),
            ..Default::default()
        },
        false,
    );
    let adopted = set_current_database(&with_dbs, Some(" main "), "s1");
    assert_eq!(adopted.schemas["s1"].current_database.as_deref(), Some("main"));
    assert_eq!(
        set_current_database(&with_dbs, Some("other"), "s1").schemas["s1"].current_database,
        None
    );
    assert_eq!(set_current_database(&with_dbs, None, "s1"), with_dbs);
    assert_eq!(set_current_database(&store, Some("any"), "s1").schemas["s1"].current_database.as_deref(), Some("any"), "before the list loads a pointer is allowed");
}

#[test]
fn namespace_cache_and_alias_actions_merge_branch_names() {
    let store = snapshot("s1");
    let merged = merge_namespace(
        &store,
        &["db".to_owned()],
        NamespaceMergeKind::Tables,
        &["users".to_owned()],
        "s1",
    );
    assert!(merged.schemas["s1"].loaded_paths.contains("db"));
    assert_eq!(collect_table_leaf_names(&merged.schemas["s1"].namespace_tree), BTreeSet::from(["users".to_owned()]));

    let cached = cache_path_items(&store, "path", vec![table("a", None, "table")], "s1");
    assert_eq!(cached.schemas["s1"].path_items["path"].len(), 1);
    assert_eq!(cache_path_items(&store, "", vec![], "s1"), store);

    let entries = vec![PathDatabaseEntry { id: "id1".to_owned(), name: "db1".to_owned() }];
    let aliases = register_path_aliases(&store, &entries, "s1");
    assert_eq!(aliases.schemas["s1"].path_aliases["db1"], "id1");
    assert!(aliases.schemas["s1"].namespace_owned_by_plugin);
    assert!(aliases.schemas["s1"].loaded_paths.contains(""));
}

#[test]
fn relation_schema_is_unique_only_when_every_owner_agrees() {
    let mut store = snapshot("s1");
    let schemas = commit_connection_patch(
        &store,
        "s1",
        SchemaPatch {
            tables: Some(vec![table("users", Some("public"), "table")]),
            views: Some(vec![table("v", Some("sales"), "view")]),
            ..Default::default()
        },
        false,
    );
    store.schemas = schemas.schemas;
    assert_eq!(schema_of_relation(&store, "users", "s1").as_deref(), Some("public"));
    assert_eq!(schema_of_relation(&store, "v", "s1").as_deref(), Some("sales"));
    assert_eq!(schema_of_relation(&store, "missing", "s1"), None);
    assert_eq!(schema_of_relation(&store, " users ", "s1").as_deref(), Some("public"));
    assert_eq!(schema_of_relation(&store, "users", "other"), None);

    let conflicting = commit_connection_patch(
        &store,
        "s1",
        SchemaPatch {
            path_items: Some(BTreeMap::from([(
                "p".to_owned(),
                vec![table("users", Some("other"), "table")],
            )])),
            ..Default::default()
        },
        false,
    );
    assert_eq!(schema_of_relation(&conflicting, "users", "s1"), None, "two owners is ambiguous");
}

#[test]
fn loaded_tables_pin_catalog_and_invalidate_the_old_database() {
    let mut entry = SchemaSessionState::default();
    entry.relation_columns.insert(
        "old".to_owned(),
        RelationColumns {
            relation: RelationRef { database: "olddb".to_owned(), schema: None, name: "t".to_owned() },
            columns: vec![RelationColumn { name: "c".to_owned(), data_type: "int".to_owned() }],
        },
    );
    entry.relation_columns.insert(
        "keep".to_owned(),
        RelationColumns {
            relation: RelationRef { database: "db".to_owned(), schema: None, name: "t".to_owned() },
            columns: vec![RelationColumn { name: "c".to_owned(), data_type: "int".to_owned() }],
        },
    );
    let mut store = snapshot("s1");
    store.schemas.insert("s1".to_owned(), entry);
    let all = vec![
        table("users", None, "table"),
        table("v", Some("public"), "view"),
        table("", None, "table"),
    ];
    let loaded = set_loaded_tables(&store, "db", &all, "s1", None, false);
    let entry = &loaded.schemas["s1"];
    assert_eq!(entry.current_database.as_deref(), Some("db"));
    assert_eq!(entry.tables.len(), 1);
    assert_eq!(entry.views.len(), 1);
    assert_eq!(entry.table_catalogs["db"].len(), 3);
    assert!(entry.column_inflight.is_empty());
    assert!(entry.relation_columns.contains_key("old"));
    assert!(!entry.relation_columns.contains_key("keep"), "the replaced database's columns are dropped");
    assert_eq!(entry.schema_names, vec!["public".to_owned()]);
    assert!(entry.loaded_paths.contains("public"), "schema grouping namespaces the loaded paths");

    let no_pin = SetLoadedTablesOptions { pin_current_database: false, schema_names: Vec::new() };
    let background = set_loaded_tables(&loaded, "other", &all, "s1", Some(&no_pin), false);
    assert_eq!(background.schemas["s1"].current_database.as_deref(), Some("db"), "a background load must not steal the pointer");
}

#[test]
fn grouped_schemas_build_a_two_segment_namespace_path() {
    let store = snapshot("s1");
    let multi = commit_connection_patch(
        &store,
        "s1",
        SchemaPatch { is_multi_database: Some(true), ..Default::default() },
        false,
    );
    let options = SetLoadedTablesOptions {
        pin_current_database: true,
        schema_names: vec!["public".to_owned()],
    };
    let loaded = set_loaded_tables(
        &multi,
        "db",
        &[table("users", Some("public"), "table")],
        "s1",
        Some(&options),
        false,
    );
    assert!(loaded.schemas["s1"].loaded_paths.contains("db/public"));
    let leaves = collect_table_leaf_names(&loaded.schemas["s1"].namespace_tree);
    assert!(leaves.contains("users"));
}

#[test]
fn relation_removal_binding_and_selection_actions() {
    let mut store = snapshot("s1");
    let with_relation = commit_connection_patch(
        &store,
        "s1",
        SchemaPatch {
            tables: Some(vec![table("users", None, "table")]),
            namespace_tree: Some(merge_namespace_path(
                &SqlNamespace::Leaf,
                &[],
                NamespaceMergeKind::Tables,
                &["users".to_owned()],
                false,
            )),
            selected_id: Some(Some("table:users".to_owned())),
            ..Default::default()
        },
        false,
    );
    store.schemas = with_relation.schemas;
    let removed = remove_relation(&store, "users", "s1");
    assert!(removed.schemas["s1"].tables.is_empty());
    assert_eq!(removed.schemas["s1"].selected_id, None);
    assert!(collect_table_leaf_names(&removed.schemas["s1"].namespace_tree).is_empty());

    let bound = bind_metadata_identity(&removed, "s1", "conn1", Some("scope"));
    assert_eq!(bound.schemas["s1"].connection_id, "conn1");
    assert_eq!(bound.schemas["s1"].metadata_revision, 1);
    assert_eq!(bind_metadata_identity(&bound, "s1", "conn1", Some("scope")), bound, "the same config is a no-op");
    let rebound = bind_metadata_identity(&bound, "s1", "conn2", None);
    assert_eq!(rebound.schemas["s1"].metadata_revision, 2);
    assert_eq!(bind_metadata_identity(&bound, " ", "conn", None), bound);

    let expanded = toggle_expand(&store, "db", "s1");
    assert!(expanded.schemas["s1"].expanded.contains("db"));
    assert!(!toggle_expand(&expanded, "db", "s1").schemas["s1"].expanded.contains("db"));
    let selected = set_selected(&store, Some("db:main".to_owned()), "s1");
    assert_eq!(selected.schemas["s1"].selected_id.as_deref(), Some("db:main"));
    assert!(reset_store().schemas.is_empty());
}

#[test]
fn load_databases_decision_handles_locked_plugin_lists_and_previous_selection() {
    let meta = DatabaseTypeMeta {
        has_multi_database: true,
        schema_tree_mode_custom: true,
        namespace_owned_by_plugin: true,
        ..Default::default()
    };
    let options = LoadForConnectionOptions {
        preferred_database: Some("id1:one (backend)".to_owned()),
        activate: true,
        ..Default::default()
    };
    let all = vec!["id1:one (backend)".to_owned(), "id2:two (backend)".to_owned()];
    let decision = load_databases_decision(&all, &options, meta, None);
    assert!(decision.uses_plugin_db_list);
    assert!(decision.is_path_hierarchy);
    assert_eq!(decision.databases, vec!["one".to_owned()]);
    assert!(decision.locked_to_configured);
    assert!(!decision.is_multi_database, "a locked list is never multi-database");
    assert_eq!(decision.current_database.as_deref(), Some("one"));
    assert_eq!(decision.alias_entries.len(), 2);

    let regular = DatabaseTypeMeta { has_multi_database: true, ..Default::default() };
    let decision = load_databases_decision(
        &["information_schema".to_owned(), "shop".to_owned()],
        &LoadForConnectionOptions::default(),
        regular,
        Some("shop"),
    );
    assert!(!decision.uses_plugin_db_list);
    assert!(decision.is_multi_database);
    assert_eq!(decision.current_database.as_deref(), Some("shop"), "the user's selection survives a remount");
    let stale = load_databases_decision(
        &["shop".to_owned()],
        &LoadForConnectionOptions::default(),
        regular,
        Some("gone"),
    );
    assert_eq!(stale.current_database.as_deref(), Some("shop"));
}

#[test]
fn crate_root_actions_surface_is_reexported() {
    let _ = crate::set_active_connection;
    let _ = crate::remove_connection;
    let _ = crate::record_session_error;
    let _ = crate::set_current_schema;
    let _ = crate::set_current_database;
    let _ = crate::merge_namespace;
    let _ = crate::cache_path_items;
    let _ = crate::register_path_aliases;
    let _ = crate::schema_of_relation;
    let _ = crate::set_loaded_tables;
    let _ = crate::remove_relation;
    let _ = crate::bind_metadata_identity;
    let _ = crate::toggle_expand;
    let _ = crate::set_selected;
    let _ = crate::load_databases_decision;
    let _ = crate::reset_store;
}
