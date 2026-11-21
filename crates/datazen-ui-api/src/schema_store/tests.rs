use std::collections::{BTreeMap, BTreeSet};

use crate::SqlNamespace;

use super::*;

fn table(name: &str, schema: Option<&str>, table_type: &str) -> SchemaTableInfo {
    SchemaTableInfo {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        table_type: table_type.to_owned(),
        row_count: None,
    }
}

#[test]
fn constants_and_gap_are_pinned() {
    assert_eq!(SCHEMA_STORE_GAP.len(), 7);
    assert_eq!(SCHEMA_STORE_SURFACE.len(), 8);
    assert!(SCHEMA_STORE_SURFACE.contains(&"bindMetadataIdentity"));
    assert!(SCHEMA_STORE_GAP.contains(&"schemaColumnBatcher.ensure asynchronous read"));
    assert_eq!(crate::SCHEMA_STORE_GAP.len(), 7);
}

#[test]
fn multi_database_needs_the_capability_and_more_than_one_database() {
    assert!(compute_is_multi_database(true, 2));
    assert!(!compute_is_multi_database(true, 1));
    assert!(!compute_is_multi_database(false, 5));
}

#[test]
fn preferred_database_skips_system_databases() {
    let databases = vec!["information_schema".to_owned(), "shop".to_owned(), "mysql".to_owned()];
    assert_eq!(resolve_preferred_database(&databases, None).as_deref(), Some("shop"));
    assert_eq!(
        resolve_preferred_database(&databases, Some("mysql")).as_deref(),
        Some("mysql"),
        "a configured database wins even when it is a system one"
    );
    assert_eq!(resolve_preferred_database(&[], None), None);
    let only_system = vec!["postgres".to_owned()];
    assert_eq!(resolve_preferred_database(&only_system, None).as_deref(), Some("postgres"));
    assert!(is_system_database_name("Information_Schema"));
    assert!(!is_system_database_name("shop"));
}

#[test]
fn visible_databases_lock_to_a_configured_member() {
    let all = vec!["a".to_owned(), "b".to_owned()];
    let locked = resolve_visible_databases(&all, Some(" b "));
    assert_eq!(locked.databases, vec!["b".to_owned()]);
    assert_eq!(locked.preferred.as_deref(), Some("b"));
    assert!(locked.locked_to_configured);

    let unlocked = resolve_visible_databases(&all, Some("missing"));
    assert_eq!(unlocked.databases, all);
    assert!(!unlocked.locked_to_configured);
    assert_eq!(unlocked.preferred.as_deref(), Some("a"));
}

#[test]
fn path_hierarchy_entries_strip_their_backend_suffix() {
    assert_eq!(
        parse_path_hierarchy_database_entry("id1:customers (mysql)"),
        PathDatabaseEntry { id: "id1".to_owned(), name: "customers".to_owned() }
    );
    assert_eq!(
        parse_path_hierarchy_database_entry("id1:customers"),
        PathDatabaseEntry { id: "id1".to_owned(), name: "customers".to_owned() }
    );
    assert_eq!(
        parse_path_hierarchy_database_entry("plain"),
        PathDatabaseEntry { id: "plain".to_owned(), name: "plain".to_owned() }
    );
}

#[test]
fn schema_grouping_excludes_the_two_catalog_pseudo_schemas() {
    assert!(is_schema_grouping_schema(Some("public")));
    assert!(!is_schema_grouping_schema(Some("CATALOG")));
    assert!(!is_schema_grouping_schema(Some("SCHEMA")));
    assert!(!is_schema_grouping_schema(None));
}

#[test]
fn namespace_paths_merge_replace_and_omit_leaves() {
    assert_eq!(path_key(&["db".to_owned(), "public".to_owned()]), "db/public");
    let empty = SqlNamespace::Leaf;
    let merged = merge_namespace_path(
        &empty,
        &["db".to_owned()],
        NamespaceMergeKind::Tables,
        &["users".to_owned(), "orders".to_owned()],
        false,
    );
    assert_eq!(collect_table_leaf_names(&merged), BTreeSet::from(["users".to_owned(), "orders".to_owned()]));

    let replaced = merge_namespace_path(
        &merged,
        &["db".to_owned()],
        NamespaceMergeKind::Tables,
        &["fresh".to_owned()],
        true,
    );
    assert_eq!(collect_table_leaf_names(&replaced), BTreeSet::from(["fresh".to_owned()]));

    let branch = merge_namespace_path(
        &empty,
        &[],
        NamespaceMergeKind::Branch,
        &["db".to_owned()],
        false,
    );
    assert!(matches!(branch, SqlNamespace::Branch(_)));
    let without = omit_table_leaf(&merged, "users");
    assert_eq!(collect_table_leaf_names(&without), BTreeSet::from(["orders".to_owned()]));
}

#[test]
fn empty_state_patch_and_commit_follow_the_id_rules() {
    let empty = create_empty_connection_schema();
    assert_eq!(empty.metadata_revision, 0);
    assert!(empty.databases.is_empty());
    assert!(empty.current_database.is_none());

    let mut schemas = BTreeMap::new();
    schemas.insert("s1".to_owned(), empty.clone());
    let patched = patch_connection_schema(
        &schemas,
        "s1",
        SchemaPatch { loading: Some(true), current_database: Some(Some("db".to_owned())), ..Default::default() },
    );
    assert!(patched["s1"].loading);
    assert_eq!(patched["s1"].current_database.as_deref(), Some("db"));
    assert!(patched["s1"].selected_id.is_none());

    let store = SchemaStoreSnapshot { schemas, active_db_session_id: Some("s0".to_owned()) };
    let committed = commit_connection_patch(
        &store,
        "s1",
        SchemaPatch { loading: Some(true), ..Default::default() },
        false,
    );
    assert_eq!(committed.active_db_session_id.as_deref(), Some("s0"));
    let activated = commit_connection_patch(&store, "s1", SchemaPatch::default(), true);
    assert_eq!(activated.active_db_session_id.as_deref(), Some("s1"));
    assert_eq!(commit_connection_patch(&store, " ", SchemaPatch::default(), true), store);
}

#[test]
fn known_table_names_combines_tree_tables_and_path_items() {
    let tree = merge_namespace_path(
        &SqlNamespace::Leaf,
        &[],
        NamespaceMergeKind::Tables,
        &["from_tree".to_owned()],
        false,
    );
    let tables = vec![table("from_tables", None, "table")];
    let views = vec![table("from_view", None, "view")];
    let mut path_items = BTreeMap::new();
    path_items.insert(
        "path".to_owned(),
        vec![
            table("catalog/a", Some("CATALOG"), "table"),
            table("bucket/nested/target", None, "table"),
        ],
    );
    let names = known_table_names(&tree, &tables, &views, &path_items);
    assert!(names.contains("from_tree"));
    assert!(names.contains("from_tables"));
    assert!(names.contains("from_view"));
    assert!(names.contains("target"));
    assert!(!names.contains("catalog/a"));
}

#[test]
fn identity_carries_connection_and_revision() {
    let entry = SchemaSessionState {
        connection_id: "conn".to_owned(),
        metadata_revision: 3,
        ..Default::default()
    };
    let identity = latest_identity(&entry);
    assert_eq!(identity.connection_id, "conn");
    assert_eq!(identity.metadata_revision, 3);
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::compute_is_multi_database;
    let _ = crate::resolve_preferred_database;
    let _ = crate::resolve_visible_databases;
    let _ = crate::parse_path_hierarchy_database_entry;
    let _ = crate::known_table_names;
    let _ = crate::is_schema_grouping_schema;
    let _ = crate::path_key;
    let _ = crate::merge_namespace_path;
    let _ = crate::omit_table_leaf;
    let _ = crate::collect_table_leaf_names;
    let _ = crate::create_empty_connection_schema;
    let _ = crate::patch_connection_schema;
    let _ = crate::commit_connection_patch;
    let _ = crate::apply_patch;
    let _: Option<crate::SchemaSessionState> = None;
    let _: Option<crate::SchemaPatch> = None;
    let _: Option<crate::SchemaStoreSnapshot> = None;
    let _: Option<crate::LoadDatabasesDecision> = None;
}
