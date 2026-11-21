use std::collections::{BTreeMap, BTreeSet};

use crate::SqlNamespace;

use super::helpers::{
    compute_is_multi_database, is_schema_grouping_schema, parse_path_hierarchy_database_entry,
    resolve_preferred_database, resolve_visible_databases,
};
use super::namespace::{
    merge_namespace_path, omit_table_leaf, path_key,
};
use super::state::{commit_connection_patch, create_empty_connection_schema};
use super::types::{
    DatabaseTypeMeta, LoadDatabasesDecision, LoadForConnectionOptions, NamespaceMergeKind,
    PathDatabaseEntry, SchemaPatch, SchemaSessionState, SchemaStoreSnapshot, SchemaTableInfo,
    SetLoadedTablesOptions,
};

fn blank(id: &str) -> bool {
    id.is_empty()
}

fn existing<'a>(store: &'a SchemaStoreSnapshot, id: &str) -> SchemaSessionState {
    store.schemas.get(id).cloned().unwrap_or_else(create_empty_connection_schema)
}

pub fn set_active_connection(store: &SchemaStoreSnapshot, id: Option<String>) -> SchemaStoreSnapshot {
    SchemaStoreSnapshot { active_db_session_id: id, ..store.clone() }
}

pub fn remove_connection(store: &SchemaStoreSnapshot, id: &str) -> SchemaStoreSnapshot {
    let mut schemas = store.schemas.clone();
    schemas.remove(id);
    SchemaStoreSnapshot {
        active_db_session_id: if store.active_db_session_id.as_deref() == Some(id) {
            None
        } else {
            store.active_db_session_id.clone()
        },
        schemas,
    }
}

pub fn record_session_error(store: &SchemaStoreSnapshot, id: &str, message: &str) -> SchemaStoreSnapshot {
    if blank(id) {
        return store.clone();
    }
    commit_connection_patch(
        store,
        id,
        SchemaPatch { error: Some(Some(message.to_owned())), ..Default::default() },
        false,
    )
}

pub fn set_current_schema(
    store: &SchemaStoreSnapshot,
    schema: Option<&str>,
    id: &str,
) -> SchemaStoreSnapshot {
    if blank(id) {
        return store.clone();
    }
    let normalized = schema.map(str::trim).filter(|value| !value.is_empty());
    commit_connection_patch(
        store,
        id,
        SchemaPatch { current_schema: Some(normalized.map(str::to_owned)), ..Default::default() },
        false,
    )
}

pub fn set_current_database(
    store: &SchemaStoreSnapshot,
    database: Option<&str>,
    id: &str,
) -> SchemaStoreSnapshot {
    if blank(id) {
        return store.clone();
    }
    let normalized = database.map(str::trim).filter(|value| !value.is_empty());
    let Some(normalized) = normalized else {
        return store.clone();
    };
    let session = store.schemas.get(id);
    let databases = session.map(|entry| entry.databases.as_slice()).unwrap_or(&[]);
    if !databases.is_empty() && !databases.iter().any(|db| db == normalized) {
        return store.clone();
    }
    commit_connection_patch(
        store,
        id,
        SchemaPatch { current_database: Some(Some(normalized.to_owned())), ..Default::default() },
        false,
    )
}

pub fn merge_namespace(
    store: &SchemaStoreSnapshot,
    segments: &[String],
    kind: NamespaceMergeKind,
    names: &[String],
    id: &str,
) -> SchemaStoreSnapshot {
    let schema = existing(store, id);
    let mut loaded_paths = schema.loaded_paths.clone();
    loaded_paths.insert(path_key(segments));
    commit_connection_patch(
        store,
        id,
        SchemaPatch {
            namespace_tree: Some(merge_namespace_path(
                &schema.namespace_tree,
                segments,
                kind,
                names,
                false,
            )),
            loaded_paths: Some(loaded_paths),
            ..Default::default()
        },
        false,
    )
}

pub fn cache_path_items(
    store: &SchemaStoreSnapshot,
    fetch_path: &str,
    items: Vec<SchemaTableInfo>,
    id: &str,
) -> SchemaStoreSnapshot {
    if fetch_path.is_empty() {
        return store.clone();
    }
    let schema = existing(store, id);
    let mut path_items = schema.path_items.clone();
    path_items.insert(fetch_path.to_owned(), items);
    commit_connection_patch(
        store,
        id,
        SchemaPatch { path_items: Some(path_items), ..Default::default() },
        false,
    )
}

pub fn register_path_aliases(
    store: &SchemaStoreSnapshot,
    entries: &[PathDatabaseEntry],
    id: &str,
) -> SchemaStoreSnapshot {
    let schema = existing(store, id);
    let mut aliases = schema.path_aliases.clone();
    let mut names = Vec::new();
    for entry in entries {
        aliases.insert(entry.name.clone(), entry.id.clone());
        names.push(entry.name.clone());
    }
    let patched = commit_connection_patch(
        store,
        id,
        SchemaPatch {
            path_aliases: Some(aliases),
            namespace_owned_by_plugin: Some(true),
            ..Default::default()
        },
        false,
    );
    merge_namespace(&patched, &[], NamespaceMergeKind::Branch, &names, id)
}

pub fn schema_of_relation(
    store: &SchemaStoreSnapshot,
    name: &str,
    id: &str,
) -> Option<String> {
    let schema = store.schemas.get(id)?;
    let target = name.trim();
    if target.is_empty() {
        return None;
    }
    let mut owners = BTreeSet::new();
    let mut consider = |item: &SchemaTableInfo| {
        if item.name == target {
            owners.insert(item.schema.clone());
        }
    };
    for item in schema.tables.iter().chain(schema.views.iter()) {
        consider(item);
    }
    for items in schema.path_items.values() {
        for item in items {
            consider(item);
        }
    }
    if owners.len() == 1 {
        owners.into_iter().next().flatten()
    } else {
        None
    }
}

pub fn set_loaded_tables(
    store: &SchemaStoreSnapshot,
    database: &str,
    all: &[SchemaTableInfo],
    id: &str,
    options: Option<&SetLoadedTablesOptions>,
    meta_plugin_owned: bool,
) -> SchemaStoreSnapshot {
    let schema = existing(store, id);
    let mut table_catalogs = schema.table_catalogs.clone();
    table_catalogs.insert(database.to_owned(), all.to_vec());
    let pin = options.map_or(true, |options| options.pin_current_database);
    let option_schemas: &[String] = options.map(|options| options.schema_names.as_slice()).unwrap_or(&[]);
    if !pin && schema.current_database.as_deref() != Some(database) {
        return commit_connection_patch(
            store,
            id,
            SchemaPatch { table_catalogs: Some(table_catalogs), ..Default::default() },
            false,
        );
    }

    let real_items: Vec<SchemaTableInfo> =
        all.iter().filter(|item| !item.name.is_empty()).cloned().collect();
    let tables: Vec<SchemaTableInfo> = real_items
        .iter()
        .filter(|item| item.table_type != "view")
        .cloned()
        .collect();
    let views: Vec<SchemaTableInfo> = real_items
        .iter()
        .filter(|item| item.table_type == "view")
        .cloned()
        .collect();
    let mut next_tree = schema.namespace_tree.clone();
    let mut next_loaded = schema.loaded_paths.clone();

    if !schema.namespace_owned_by_plugin && !meta_plugin_owned {
        let has_grouping = option_schemas.iter().any(|name| is_schema_grouping_schema(Some(name)))
            || all.iter().any(|item| is_schema_grouping_schema(item.schema.as_deref()));
        if has_grouping {
            let mut by_schema: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for name in option_schemas {
                if is_schema_grouping_schema(Some(name)) {
                    by_schema.entry(name.clone()).or_default();
                }
            }
            for item in all {
                let Some(schema_name) = item.schema.as_deref() else {
                    continue;
                };
                if !is_schema_grouping_schema(Some(schema_name)) {
                    continue;
                }
                if item.name.is_empty() {
                    by_schema.entry(schema_name.to_owned()).or_default();
                    continue;
                }
                by_schema.entry(schema_name.to_owned()).or_default().push(item.name.clone());
            }
            next_loaded = schema.loaded_paths.clone();
            for (schema_name, names) in by_schema {
                let segments = if schema.is_multi_database {
                    vec![database.to_owned(), schema_name]
                } else {
                    vec![schema_name]
                };
                next_tree = merge_namespace_path(
                    &next_tree,
                    &segments,
                    NamespaceMergeKind::Tables,
                    &names,
                    true,
                );
                next_loaded.insert(path_key(&segments));
            }
        } else {
            let names: Vec<String> = real_items.iter().map(|item| item.name.clone()).collect();
            let segments = vec![database.to_owned()];
            next_tree = merge_namespace_path(
                &next_tree,
                &segments,
                NamespaceMergeKind::Tables,
                &names,
                true,
            );
            next_loaded.insert(path_key(&segments));
        }
    }

    let mut schema_names = option_schemas.to_vec();
    for item in all {
        if let Some(name) = &item.schema {
            if !schema_names.contains(name) {
                schema_names.push(name.clone());
            }
        }
    }
    let relation_columns = schema
        .relation_columns
        .iter()
        .filter(|(_, value)| value.relation.database != database)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    commit_connection_patch(
        store,
        id,
        SchemaPatch {
            tables: Some(tables),
            table_catalogs: Some(table_catalogs),
            views: Some(views),
            schema_names: Some(schema_names),
            column_inflight: Some(BTreeSet::new()),
            relation_columns: Some(relation_columns),
            current_database: if pin { Some(Some(database.to_owned())) } else { None },
            namespace_tree: Some(next_tree),
            loaded_paths: Some(next_loaded),
            ..Default::default()
        },
        false,
    )
}

pub fn remove_relation(store: &SchemaStoreSnapshot, name: &str, id: &str) -> SchemaStoreSnapshot {
    let schema = existing(store, id);
    let selected = if schema.selected_id.as_deref() == Some(&format!("table:{name}"))
        || schema.selected_id.as_deref() == Some(&format!("view:{name}"))
    {
        None
    } else {
        schema.selected_id.clone()
    };
    commit_connection_patch(
        store,
        id,
        SchemaPatch {
            tables: Some(schema.tables.into_iter().filter(|item| item.name != name).collect()),
            views: Some(schema.views.into_iter().filter(|item| item.name != name).collect()),
            namespace_tree: Some(omit_table_leaf(&schema.namespace_tree, name)),
            selected_id: Some(selected),
            ..Default::default()
        },
        false,
    )
}

pub fn bind_metadata_identity(
    store: &SchemaStoreSnapshot,
    id: &str,
    connection_id: &str,
    scope: Option<&str>,
) -> SchemaStoreSnapshot {
    if blank(id) || blank(connection_id) {
        return store.clone();
    }
    let schema = existing(store, id);
    if schema.connection_id == connection_id
        && schema.metadata_revision > 0
        && scope.is_none_or(|scope| schema.metadata_scope_key.as_deref() == Some(scope))
    {
        return store.clone();
    }
    commit_connection_patch(
        store,
        id,
        SchemaPatch {
            connection_id: Some(connection_id.to_owned()),
            metadata_scope_key: Some(scope.map(str::to_owned)),
            metadata_revision: Some(schema.metadata_revision + 1),
            relation_columns: Some(BTreeMap::new()),
            column_inflight: Some(BTreeSet::new()),
            ..Default::default()
        },
        false,
    )
}

pub fn toggle_expand(store: &SchemaStoreSnapshot, item_id: &str, id: &str) -> SchemaStoreSnapshot {
    let schema = existing(store, id);
    let mut expanded = schema.expanded.clone();
    if !expanded.remove(item_id) {
        expanded.insert(item_id.to_owned());
    }
    commit_connection_patch(
        store,
        id,
        SchemaPatch { expanded: Some(expanded), ..Default::default() },
        false,
    )
}

pub fn set_selected(
    store: &SchemaStoreSnapshot,
    item_id: Option<String>,
    id: &str,
) -> SchemaStoreSnapshot {
    commit_connection_patch(
        store,
        id,
        SchemaPatch { selected_id: Some(item_id), ..Default::default() },
        false,
    )
}

pub fn load_databases_decision(
    all_databases: &[String],
    options: &LoadForConnectionOptions,
    meta: DatabaseTypeMeta,
    previous_database: Option<&str>,
) -> LoadDatabasesDecision {
    let is_path_hierarchy = meta.schema_tree_mode_custom || meta.namespace_ensure_path_hierarchy;
    let uses_plugin_db_list = is_path_hierarchy
        && (meta.namespace_owned_by_plugin || meta.schema_tree_mode_custom)
        && !all_databases.is_empty();
    let configured = options
        .preferred_database
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());

    let (databases, preferred, locked) = if uses_plugin_db_list {
        let display_names: Vec<String> = all_databases
            .iter()
            .map(|entry| parse_path_hierarchy_database_entry(entry).name)
            .collect();
        match configured {
            Some(name) if all_databases.iter().any(|entry| entry == name) => {
                let display = parse_path_hierarchy_database_entry(name).name;
                (vec![display.clone()], Some(display), true)
            }
            _ => (
                display_names.clone(),
                resolve_preferred_database(&display_names, configured),
                false,
            ),
        }
    } else {
        let resolved = resolve_visible_databases(all_databases, options.preferred_database.as_deref());
        (resolved.databases, resolved.preferred, resolved.locked_to_configured)
    };

    let is_multi_database =
        !locked && compute_is_multi_database(meta.has_multi_database, databases.len());
    let current_database = previous_database
        .filter(|previous| databases.iter().any(|database| database == previous))
        .map(str::to_owned)
        .or_else(|| preferred.clone());
    let alias_entries = if uses_plugin_db_list {
        all_databases.iter().map(|entry| parse_path_hierarchy_database_entry(entry)).collect()
    } else {
        Vec::new()
    };

    LoadDatabasesDecision {
        databases,
        preferred,
        locked_to_configured: locked,
        is_multi_database,
        current_database,
        uses_plugin_db_list,
        is_path_hierarchy,
        alias_entries,
    }
}

pub fn reset_store() -> SchemaStoreSnapshot {
    SchemaStoreSnapshot::default()
}

pub fn empty_snapshot_field(store: &SchemaStoreSnapshot, id: &str) -> SchemaSessionState {
    store.schemas.get(id).cloned().unwrap_or_default()
}

pub fn namespace_tree_of(entry: &SchemaSessionState) -> &SqlNamespace {
    &entry.namespace_tree
}
