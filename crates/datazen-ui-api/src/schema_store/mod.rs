mod actions;
mod helpers;
mod namespace;
mod state;
mod types;

#[cfg(test)]
mod action_tests;
#[cfg(test)]
mod tests;

pub use actions::{
    bind_metadata_identity, cache_path_items, empty_snapshot_field, load_databases_decision,
    merge_namespace, namespace_tree_of, record_session_error, register_path_aliases,
    remove_connection, remove_relation, reset_store, schema_of_relation, set_active_connection,
    set_current_database, set_current_schema, set_loaded_tables, set_selected, toggle_expand,
};
pub use helpers::{
    compute_is_multi_database, is_schema_grouping_schema, is_system_database_name,
    known_table_names, latest_identity, parse_path_hierarchy_database_entry,
    resolve_preferred_database, resolve_visible_databases,
};
pub use namespace::{
    collect_table_leaf_names, merge_namespace_path, omit_table_leaf, path_key,
};
pub use state::{
    apply_patch, commit_connection_patch, connection_field, create_empty_connection_schema,
    patch_connection_schema,
};
pub use types::{
    DatabaseTypeMeta, LoadDatabasesDecision, LoadForConnectionOptions, NamespaceMergeKind,
    PathDatabaseEntry, SchemaPatch, SchemaSessionState, SchemaStoreSnapshot, SchemaTableInfo,
    SetLoadedTablesOptions, SCHEMA_STORE_GAP, SCHEMA_STORE_SURFACE,
};
