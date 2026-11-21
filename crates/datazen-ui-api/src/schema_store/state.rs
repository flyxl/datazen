use std::collections::BTreeMap;

use crate::SqlNamespace;

use super::types::{SchemaPatch, SchemaSessionState, SchemaStoreSnapshot};

pub const EMPTY_NAMESPACE: SqlNamespace = SqlNamespace::Leaf;

pub const SCHEMA_STORE_STATE_SURFACE: [&str; 4] = [
    "ConnectionSchemaState",
    "EMPTY_NAMESPACE",
    "createEmptyConnectionSchema",
    "patchConnectionSchema",
];

pub fn create_empty_connection_schema() -> SchemaSessionState {
    let mut state = SchemaSessionState::default();
    state.namespace_tree = EMPTY_NAMESPACE;
    state
}

pub fn apply_patch(state: &mut SchemaSessionState, patch: SchemaPatch) {
    if let Some(value) = patch.connection_id { state.connection_id = value; }
    if let Some(value) = patch.metadata_revision { state.metadata_revision = value; }
    if let Some(value) = patch.metadata_scope_key { state.metadata_scope_key = value; }
    if let Some(value) = patch.current_database { state.current_database = value; }
    if let Some(value) = patch.current_schema { state.current_schema = value; }
    if let Some(value) = patch.databases { state.databases = value; }
    if let Some(value) = patch.database_type { state.database_type = value; }
    if let Some(value) = patch.is_multi_database { state.is_multi_database = value; }
    if let Some(value) = patch.tables { state.tables = value; }
    if let Some(value) = patch.table_catalogs { state.table_catalogs = value; }
    if let Some(value) = patch.views { state.views = value; }
    if let Some(value) = patch.schema_names { state.schema_names = value; }
    if let Some(value) = patch.relation_columns { state.relation_columns = value; }
    if let Some(value) = patch.namespace_tree { state.namespace_tree = value; }
    if let Some(value) = patch.loaded_paths { state.loaded_paths = value; }
    if let Some(value) = patch.path_items { state.path_items = value; }
    if let Some(value) = patch.path_aliases { state.path_aliases = value; }
    if let Some(value) = patch.namespace_owned_by_plugin { state.namespace_owned_by_plugin = value; }
    if let Some(value) = patch.schema_epoch { state.schema_epoch = value; }
    if let Some(value) = patch.expanded { state.expanded = value; }
    if let Some(value) = patch.selected_id { state.selected_id = value; }
    if let Some(value) = patch.loading { state.loading = value; }
    if let Some(value) = patch.ensuring_count { state.ensuring_count = value; }
    if let Some(value) = patch.error { state.error = value; }
    if let Some(value) = patch.column_inflight { state.column_inflight = value; }
}

pub fn patch_connection_schema(
    schemas: &BTreeMap<String, SchemaSessionState>,
    db_session_id: &str,
    patch: SchemaPatch,
) -> BTreeMap<String, SchemaSessionState> {
    let mut next = schemas.clone();
    let mut state = next.get(db_session_id).cloned().unwrap_or_default();
    apply_patch(&mut state, patch);
    next.insert(db_session_id.to_owned(), state);
    next
}

pub fn commit_connection_patch(
    store: &SchemaStoreSnapshot,
    db_session_id: &str,
    patch: SchemaPatch,
    activate: bool,
) -> SchemaStoreSnapshot {
    if db_session_id.trim().is_empty() {
        return store.clone();
    }
    SchemaStoreSnapshot {
        schemas: patch_connection_schema(&store.schemas, db_session_id, patch),
        active_db_session_id: if activate {
            Some(db_session_id.to_owned())
        } else {
            store.active_db_session_id.clone()
        },
    }
}

pub fn connection_field<'a>(
    store: &'a SchemaStoreSnapshot,
    db_session_id: &str,
) -> Option<&'a SchemaSessionState> {
    store.schemas.get(db_session_id)
}
