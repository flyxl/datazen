use std::collections::BTreeMap;

use crate::SqlNamespace;

use super::*;

#[test]
fn surface_names_the_four_state_exports() {
    assert_eq!(SCHEMA_STORE_STATE_SURFACE.len(), 4);
    assert_eq!(
        SCHEMA_STORE_STATE_SURFACE,
        [
            "ConnectionSchemaState",
            "EMPTY_NAMESPACE",
            "createEmptyConnectionSchema",
            "patchConnectionSchema",
        ]
    );
    assert_eq!(crate::SCHEMA_STORE_STATE_SURFACE.len(), 4);
}

#[test]
fn empty_namespace_is_the_leaf_tree_used_by_default() {
    assert_eq!(EMPTY_NAMESPACE, SqlNamespace::Leaf);
    assert_eq!(EMPTY_NAMESPACE, SqlNamespace::default());
    let entry = create_empty_connection_schema();
    assert_eq!(entry.namespace_tree, EMPTY_NAMESPACE);
}

#[test]
fn an_empty_entry_has_exactly_the_documented_zero_values() {
    let entry = create_empty_connection_schema();
    assert_eq!(entry.connection_id, "");
    assert_eq!(entry.metadata_revision, 0);
    assert_eq!(entry.metadata_scope_key, None);
    assert_eq!(entry.current_database, None);
    assert_eq!(entry.current_schema, None);
    assert!(entry.databases.is_empty());
    assert_eq!(entry.database_type, None);
    assert!(!entry.is_multi_database);
    assert!(entry.tables.is_empty());
    assert!(entry.table_catalogs.is_empty());
    assert!(entry.views.is_empty());
    assert!(entry.schema_names.is_empty());
    assert!(entry.relation_columns.is_empty());
    assert!(entry.loaded_paths.is_empty());
    assert!(entry.path_items.is_empty());
    assert!(entry.path_aliases.is_empty());
    assert!(!entry.namespace_owned_by_plugin);
    assert_eq!(entry.schema_epoch, 0);
    assert!(entry.expanded.is_empty());
    assert_eq!(entry.selected_id, None);
    assert!(!entry.loading);
    assert_eq!(entry.ensuring_count, 0);
    assert_eq!(entry.error, None);
    assert!(entry.column_inflight.is_empty());
}

#[test]
fn patching_a_missing_session_starts_from_the_empty_entry() {
    let schemas = BTreeMap::new();
    let patched = patch_connection_schema(
        &schemas,
        "s1",
        SchemaPatch { loading: Some(true), current_database: Some(Some("db".to_owned())), ..Default::default() },
    );
    assert!(schemas.is_empty(), "the input map is not mutated");
    assert!(patched["s1"].loading);
    assert_eq!(patched["s1"].current_database.as_deref(), Some("db"));
    assert_eq!(patched["s1"].namespace_tree, EMPTY_NAMESPACE);
    assert!(patched["s1"].metadata_scope_key.is_none());
}

#[test]
fn patching_an_existing_session_keeps_every_unmentioned_field() {
    let original = create_empty_connection_schema();
    let mut schemas = BTreeMap::new();
    schemas.insert("s1".to_owned(), original.clone());
    let with_connection = patch_connection_schema(
        &schemas,
        "s1",
        SchemaPatch {
            connection_id: Some("conn".to_owned()),
            metadata_revision: Some(2),
            schema_epoch: Some(4),
            ..Default::default()
        },
    );
    let patched = patch_connection_schema(
        &with_connection,
        "s1",
        SchemaPatch { error: Some(Some("boom".to_owned())), ..Default::default() },
    );
    let entry = &patched["s1"];
    assert_eq!(entry.connection_id, "conn");
    assert_eq!(entry.metadata_revision, 2);
    assert_eq!(entry.schema_epoch, 4);
    assert_eq!(entry.error.as_deref(), Some("boom"));
    assert_eq!(entry.namespace_tree, EMPTY_NAMESPACE);
    assert_eq!(with_connection["s1"].error, None, "the previous snapshot is untouched");
}

#[test]
fn nullable_patch_fields_clear_and_leave_independently() {
    let mut entry = create_empty_connection_schema();
    entry.current_database = Some("db".to_owned());
    entry.selected_id = Some("table:t".to_owned());
    let mut schemas = BTreeMap::new();
    schemas.insert("s1".to_owned(), entry);
    let cleared = patch_connection_schema(
        &schemas,
        "s1",
        SchemaPatch { current_database: Some(None), ..Default::default() },
    );
    assert_eq!(cleared["s1"].current_database, None);
    assert_eq!(cleared["s1"].selected_id.as_deref(), Some("table:t"));
    let untouched = patch_connection_schema(&schemas, "s1", SchemaPatch::default());
    assert_eq!(untouched["s1"].current_database.as_deref(), Some("db"));
}

#[test]
fn crate_root_state_surface_is_reexported() {
    let _ = crate::create_empty_connection_schema;
    let _ = crate::patch_connection_schema;
    let _ = crate::apply_patch;
    let _ = crate::commit_connection_patch;
    let _ = crate::connection_field;
    assert_eq!(crate::EMPTY_NAMESPACE, SqlNamespace::Leaf);
    let _: crate::SchemaSessionState = Default::default();
    let _: crate::SchemaPatch = Default::default();
}
