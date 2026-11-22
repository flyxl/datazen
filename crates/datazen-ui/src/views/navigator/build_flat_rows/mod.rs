//! `windows/connection/navigator/buildFlatRows.ts` — the navigator tree's row
//! builder, ported for T570. Schemas and five expand sets in, the flat row
//! list the virtualized tree paints out.
//!
//! ## Shape of the port
//!
//! The builder is pure: no store subscription, no commands, no React. The
//! host (a later task) owns the stores and calls [`build_navigator_flat_rows`]
//! on every change. Rows carry their paint depth *and* their announced level,
//! because a search suppresses headers and those two disagree below the
//! connection — the shared `searchedRowLevels` rule is the one spelling.
//!
//! The libraries the React file imports are shard 2's to port formally; their
//! flat-row-relevant halves live verbatim in [`deps`], one file per React
//! source, each with its removal stage noted.
//!
//! ## The ladder
//!
//! section/group at depth 0 → connection at 1 (0 when a search suppresses the
//! header) → databases/namespace roots at [`CONNECTION_CHILD_DEPTH`] →
//! schemas at [`DATABASE_CHILD_DEPTH`] → categories at schema+1 (or database
//! depth when the connection has no schema ladder) → tables/objects last.
//!
//! ## What each placeholder stands for
//!
//! Every `db-loading` names the row it stands in for (`conn:`, `session:`,
//! `db:`, `ns:` owner keys) and every `db-error` replaces a silent empty
//! list — a failed read must not read as "no tables".

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::i18n::t;

pub(crate) mod deps;
pub(crate) mod inputs;
pub(crate) mod row_model;

use deps::categories::get_categories_for_driver;
use deps::groups::format_group_label;
use deps::locator::{connection_expand_key, rank_connections, PINNED_GROUP_KEY, RECENT_GROUP_KEY};
use deps::object_filter::{
    filter_table_items, get_object_filter, matches_table_name_filter, should_show_database,
    should_show_schema, ObjectFilterPrefs,
};
use deps::tree_rules::{effective_expanded, searched_row_levels};
use deps::utils::{
    flatten_namespace_tree, group_by_schema, should_use_multi_database_tree,
    CONNECTION_CHILD_DEPTH, DATABASE_CHILD_DEPTH,
};
use inputs::{
    ActiveEntrySnapshot, ConnectionSnapshot, DatabaseObject, DriverMeta, GroupedSection,
    SchemaStateSnapshot, TableInfo, TableType, UsageStateSnapshot,
};
use row_model::{row_key, DepthPair, NavigatorRow};

/// React `BuildNavigatorFlatRowsParams`.
#[derive(Clone, Default)]
pub(crate) struct BuildFlatRowsParams {
    pub grouped: Vec<GroupedSection>,
    pub expanded_groups: HashSet<String>,
    pub expanded_connections: HashSet<String>,
    pub expanded_dbs: HashSet<String>,
    pub expanded_schemas: HashSet<String>,
    pub expanded_cats: HashSet<String>,
    pub active_connections: HashMap<String, ActiveEntrySnapshot>,
    pub active_connection_id: Option<String>,
    pub schemas: HashMap<String, SchemaStateSnapshot>,
    pub db_tables_map: BTreeMap<String, Vec<TableInfo>>,
    /// Per-`${dbSessionId}::${dbName}` load failures, for the error row.
    pub db_errors: BTreeMap<String, String>,
    pub db_objects_map: BTreeMap<String, Vec<DatabaseObject>>,
    pub loading_dbs: HashSet<String>,
    /// Per-session set of databases with an open backend resource.
    pub open_dbs: HashMap<String, HashSet<String>>,
    pub query: String,
    /// The caller's driver registry (`DB_REGISTRY`), injected so the builder
    /// needs no registry port; missing entries read as drivers with no
    /// special tree shape.
    pub db_registry: HashMap<String, DriverMeta>,
    /// Overrides the derived usage state; the host passes its own caches when
    /// they differ from the row inputs (React's optional `usageState`).
    pub usage_state: Option<UsageStateSnapshot>,
}

/// React `isDatabaseOpen` — open means the user expanded it in the tree, or
/// the driver still holds a per-database resource a "close database
/// connection" could release (PostgreSQL caches one pool per foreign
/// database). Deliberately does NOT consult the effective expand state: a
/// global object search force-expands matching databases, which must not
/// read as "open".
fn is_database_open(
    expanded_dbs: &HashSet<String>,
    open_dbs: &HashMap<String, HashSet<String>>,
    db_session_id: &str,
    db_name: &str,
    conn_id: &str,
) -> bool {
    if expanded_dbs.contains(&format!("{conn_id}::{db_name}")) {
        return true;
    }
    open_dbs
        .get(db_session_id)
        .is_some_and(|set| set.contains(db_name))
}

/// React `buildNavigatorFlatRows`.
pub(crate) fn build_navigator_flat_rows(params: &BuildFlatRowsParams) -> Vec<NavigatorRow> {
    let mut rows: Vec<NavigatorRow> = Vec::new();
    let query = params.query.as_str();
    let searching = !query.is_empty();
    let query_lower = query.to_lowercase();

    // "Is this branch open" and "how deep does it announce" are two halves of
    // the same fact; `levels` and `is_open` are the one spelling.
    let levels = |depth: usize| -> DepthPair {
        searched_row_levels(depth, CONNECTION_CHILD_DEPTH, searching)
    };
    let is_open = |own: bool| -> bool { effective_expanded(own, searching) };

    // Categories under one owner (a database, or a schema within it).
    let add_categories =
        |rows: &mut Vec<NavigatorRow>,
         all_items: &[TableInfo],
         connection_id: &str,
         db_session_id: &str,
         db_name: &str,
         schema_name: Option<&str>,
         base_depth: usize,
         db_type: &str,
         object_filter: &ObjectFilterPrefs,
         db_objects_map: &BTreeMap<String, Vec<DatabaseObject>>| {
            let filtered_items = filter_table_items(all_items, object_filter);
            let real_items: Vec<&TableInfo> = filtered_items
                .iter()
                .filter(|i| !i.name.is_empty())
                .collect();
            let tbl_items: Vec<TableInfo> = real_items
                .iter()
                .filter(|i| matches!(i.table_type, TableType::Table | TableType::SystemTable))
                .map(|i| (*i).clone())
                .collect();
            let view_items: Vec<TableInfo> = real_items
                .iter()
                .filter(|i| matches!(i.table_type, TableType::View | TableType::MaterializedView))
                .map(|i| (*i).clone())
                .collect();

            let meta = params.db_registry.get(db_type);
            for cat in get_categories_for_driver(meta) {
                let cat_key = match schema_name {
                    Some(schema) => format!("{connection_id}::{db_name}::{schema}::{}", cat.id),
                    None => format!("{connection_id}::{db_name}::{}", cat.id),
                };
                let is_expanded = is_open(params.expanded_cats.contains(&cat_key));
                let mut category_objects: Vec<DatabaseObject> = Vec::new();

                let count = if cat.id == "tables" {
                    items_matching(&tbl_items, &query_lower).len()
                } else if cat.id == "views" {
                    items_matching(&view_items, &query_lower).len()
                } else {
                    // Driver commands list a whole database's objects; when the
                    // navigator groups by schema, scope the shared result to this
                    // category's owner so a leaf never appears under the wrong
                    // parent. Objects with no schema metadata stay: some drivers
                    // do not expose it.
                    let loaded = db_objects_map.get(&cat_key).cloned().unwrap_or_default();
                    category_objects = match schema_name {
                        Some(schema) => loaded
                            .into_iter()
                            .filter(|object| object.schema.as_deref().is_none_or(|s| s == schema))
                            .collect(),
                        None => loaded,
                    };
                    objects_matching(&category_objects, &query_lower).len()
                };

                if searching && count == 0 {
                    continue;
                }

                let pair = levels(base_depth);
                rows.push(NavigatorRow::Category {
                    depth_pair: pair,
                    key: cat_key,
                    db_name: db_name.to_owned(),
                    cat_id: cat.id,
                    cat_label_key: cat.label_key,
                    count,
                    expanded: is_expanded,
                });

                if is_expanded {
                    let mut items: Vec<TableInfo> = Vec::new();
                    let mut objs: Vec<DatabaseObject> = Vec::new();
                    if cat.id == "tables" {
                        items = tbl_items.clone();
                    } else if cat.id == "views" {
                        items = view_items.clone();
                    } else {
                        objs = category_objects;
                    }
                    // React's truthy check: `hideSystemSchemas` must be `true`, a
                    // name filter must be present (its emptiness `matches_table_name_filter`
                    // already treats as "no opinion").
                    if object_filter.hide_system_schemas == Some(true)
                        || object_filter.table_name_include.is_some()
                        || object_filter.table_name_exclude.is_some()
                    {
                        objs.retain(|o| matches_table_name_filter(&o.name, object_filter));
                    }
                    if searching {
                        items.retain(|i| i.name.to_lowercase().contains(&query_lower));
                        objs.retain(|o| o.name.to_lowercase().contains(&query_lower));
                    }

                    if !items.is_empty() {
                        let pair = levels(base_depth + 1);
                        for item in items {
                            rows.push(NavigatorRow::Table {
                                depth_pair: pair,
                                item,
                                cat_id: cat.id,
                                is_selected: false,
                                connection_id: connection_id.to_owned(),
                                db_session_id: db_session_id.to_owned(),
                                db_name: db_name.to_owned(),
                            });
                        }
                    } else if !objs.is_empty() {
                        let pair = levels(base_depth + 1);
                        for obj in objs {
                            // The owner tuple travels with the row, exactly as the
                            // `table` variant's does: two connections can each hold
                            // an object with the same name.
                            rows.push(NavigatorRow::Object {
                                depth_pair: pair,
                                obj,
                                cat_id: cat.id,
                                connection_id: connection_id.to_owned(),
                                db_name: db_name.to_owned(),
                                schema_name: schema_name.map(str::to_owned),
                            });
                        }
                    }
                }
            }
        };

    // Unique connections across sections, then the locator's ranking.
    let mut unique_connections: Vec<ConnectionSnapshot> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for section in &params.grouped {
        for connection in &section.connections {
            if seen.insert(connection.id.clone()) {
                unique_connections.push(connection.clone());
            }
        }
    }
    let usage_state = params
        .usage_state
        .clone()
        .unwrap_or_else(|| UsageStateSnapshot {
            active_connections: params.active_connections.clone(),
            schemas: params.schemas.clone(),
            db_tables_map: params.db_tables_map.clone(),
            db_objects_map: params.db_objects_map.clone(),
        });
    let locator_results = rank_connections(&unique_connections, query, &usage_state);
    let matches_by_id: HashMap<String, deps::locator::ConnectionMatch> = locator_results
        .iter()
        .filter_map(|result| {
            result
                .match_payload
                .clone()
                .map(|m| (result.connection.id.clone(), m))
        })
        .collect();
    // A search flattens the sections into one unnamed one, in rank order.
    let sections: Vec<GroupedSection> = if searching {
        vec![GroupedSection {
            group: String::new(),
            connections: locator_results
                .iter()
                .map(|r| r.connection.clone())
                .collect(),
        }]
    } else {
        params.grouped.clone()
    };

    if sections.is_empty() {
        rows.push(NavigatorRow::NoConnections { depth_pair: (0, 0) });
        return rows;
    }

    for section in &sections {
        let group_name = section.group.as_str();
        let filtered_conns = &section.connections;
        if searching && filtered_conns.is_empty() {
            continue;
        }

        let is_pinned_section = group_name == PINNED_GROUP_KEY;
        let is_recent_section = group_name == RECENT_GROUP_KEY;
        let expanded = is_open(params.expanded_groups.contains(group_name));
        let display_name = if is_pinned_section {
            t("main.ctx.pinConnection").to_owned()
        } else if is_recent_section {
            t("context.recent").to_owned()
        } else if group_name.is_empty() {
            t("main.ungrouped").to_owned()
        } else {
            format_group_label(group_name)
        };

        if is_pinned_section || is_recent_section {
            rows.push(NavigatorRow::Section {
                depth_pair: (0, 0),
                section: if is_pinned_section {
                    "pinned"
                } else {
                    "recent"
                },
                display_name,
                count: filtered_conns.len(),
                expanded,
            });
        } else if !searching {
            rows.push(NavigatorRow::Group {
                depth_pair: (0, 0),
                group_name: group_name.to_owned(),
                display_name,
                count: filtered_conns.len(),
                expanded,
            });
        }

        if !expanded {
            continue;
        }
        if filtered_conns.is_empty() {
            let pair = levels(1);
            rows.push(NavigatorRow::EmptyGroup {
                depth_pair: pair,
                group_name: group_name.to_owned(),
            });
            continue;
        }

        for conn in filtered_conns {
            let object_filter = get_object_filter(conn);
            let entry = params.active_connections.get(&conn.id);
            let status = entry
                .map(|e| e.status.as_str())
                .unwrap_or("idle")
                .to_owned();
            let is_connected = status == "connected";
            let is_connecting = status == "connecting";
            let own_expanded = params
                .expanded_connections
                .contains(&connection_expand_key(group_name, &conn.id));
            let is_expanded = is_open(own_expanded);

            rows.push(NavigatorRow::Connection {
                depth_pair: levels(if searching {
                    0
                } else {
                    CONNECTION_CHILD_DEPTH - 1
                }),
                id: conn.id.clone(),
                name: conn.name.clone(),
                engine: conn.database_type.clone(),
                section_group: group_name.to_owned(),
                is_selected: params.active_connection_id.as_deref() == Some(conn.id.as_str()),
                status: status.clone(),
                expanded: is_open(is_expanded && (is_connected || is_connecting)),
                match_payload: matches_by_id.get(&conn.id).cloned(),
            });

            if (!is_connected && !is_connecting) || (!is_expanded && !searching) {
                continue;
            }

            if is_connecting {
                // Still dialling: this spinner *is* the connection row.
                let pair = levels(CONNECTION_CHILD_DEPTH);
                rows.push(NavigatorRow::DbLoading {
                    depth_pair: pair,
                    owner_key: format!("conn:{}", conn.id),
                });
                continue;
            }

            let db_session_id = entry
                .and_then(|e| e.db_session_id.clone())
                .unwrap_or_default();
            let Some(schema_data) = params.schemas.get(&db_session_id) else {
                // The session exists but its schema has not arrived.
                let pair = levels(CONNECTION_CHILD_DEPTH);
                rows.push(NavigatorRow::DbLoading {
                    depth_pair: pair,
                    owner_key: format!("session:{db_session_id}"),
                });
                continue;
            };

            let meta = params
                .db_registry
                .get(&conn.database_type)
                .cloned()
                .unwrap_or_default();

            if meta.schema_tree_mode.as_deref() == Some("custom")
                || meta.namespace_ensure.as_deref() == Some("path-hierarchy")
            {
                let tree = &schema_data.namespace_tree;
                let tree_empty = tree.is_leaf() || tree.len() == 0;
                if tree_empty {
                    if !searching && (schema_data.loading || schema_data.ensuring_count > 0) {
                        let pair = levels(CONNECTION_CHILD_DEPTH);
                        rows.push(NavigatorRow::DbLoading {
                            depth_pair: pair,
                            owner_key: format!("session:{db_session_id}"),
                        });
                    }
                    continue;
                }
                let mut type_map: HashMap<String, TableType> = HashMap::new();
                for tbl in &schema_data.tables {
                    type_map.insert(tbl.name.clone(), tbl.table_type);
                }
                flatten_namespace_tree(
                    tree,
                    &conn.id,
                    &db_session_id,
                    CONNECTION_CHILD_DEPTH,
                    &mut rows,
                    &params.expanded_dbs,
                    query,
                    &type_map,
                    &schema_data.loaded_paths,
                    &[],
                );
                continue;
            }

            if meta.is_key_value {
                let dbs = &schema_data.databases;
                if schema_data.loading && dbs.is_empty() {
                    // The key-value db list itself is still loading.
                    let pair = levels(CONNECTION_CHILD_DEPTH);
                    rows.push(NavigatorRow::DbLoading {
                        depth_pair: pair,
                        owner_key: format!("session:{db_session_id}"),
                    });
                } else {
                    let filtered_dbs: Vec<&String> = dbs
                        .iter()
                        .filter(|d| !searching || d.to_lowercase().contains(&query_lower))
                        .collect();
                    for db_name in filtered_dbs {
                        let pair = levels(CONNECTION_CHILD_DEPTH);
                        rows.push(NavigatorRow::KvDb {
                            depth_pair: pair,
                            connection_id: conn.id.clone(),
                            db_session_id: db_session_id.clone(),
                            db_name: db_name.clone(),
                            is_selected: false,
                            db_counts_command: meta.db_counts_command.clone(),
                        });
                    }
                }
                continue;
            }

            let is_multi_db = should_use_multi_database_tree(
                meta.has_multi_database,
                &meta.database_field_type,
                conn.database.as_deref(),
            );

            if is_multi_db {
                let mut dbs: Vec<String> = if searching {
                    schema_data
                        .databases
                        .iter()
                        .filter(|d| {
                            if d.to_lowercase().contains(&query_lower) {
                                return true;
                            }
                            let tbl_key = format!("{db_session_id}::{d}");
                            params.db_tables_map.get(&tbl_key).is_some_and(|tbls| {
                                tbls.iter()
                                    .any(|tbl| tbl.name.to_lowercase().contains(&query_lower))
                            })
                        })
                        .cloned()
                        .collect()
                } else {
                    schema_data.databases.clone()
                };
                dbs.retain(|d| should_show_database(d, &object_filter));

                for db_name in dbs {
                    let db_key = format!("{}::{}", conn.id, db_name);
                    let table_key = format!("{db_session_id}::{db_name}");
                    let is_db_expanded = is_open(params.expanded_dbs.contains(&db_key));
                    let is_loading = params.loading_dbs.contains(&table_key);

                    let pair = levels(CONNECTION_CHILD_DEPTH);
                    rows.push(NavigatorRow::Db {
                        depth_pair: pair,
                        connection_id: conn.id.clone(),
                        db_session_id: db_session_id.clone(),
                        db_name: db_name.clone(),
                        expanded: is_db_expanded,
                        loading: is_loading,
                        is_open: is_database_open(
                            &params.expanded_dbs,
                            &params.open_dbs,
                            &db_session_id,
                            &db_name,
                            &conn.id,
                        ),
                    });

                    if !is_db_expanded {
                        continue;
                    }
                    if is_loading {
                        let pair = levels(DATABASE_CHILD_DEPTH);
                        rows.push(NavigatorRow::DbLoading {
                            depth_pair: pair,
                            owner_key: format!("db:{db_key}"),
                        });
                        continue;
                    }

                    let all_items = params
                        .db_tables_map
                        .get(&table_key)
                        .cloned()
                        .unwrap_or_default();
                    // No tables AND a load failure is a failed read, not an
                    // empty one — the error replaces the silent empty list.
                    let load_error = params.db_errors.get(&table_key);
                    if all_items.is_empty() {
                        if let Some(message) = load_error {
                            let pair = levels(DATABASE_CHILD_DEPTH);
                            rows.push(NavigatorRow::DbError {
                                depth_pair: pair,
                                owner_key: format!("db:{db_key}"),
                                message: message.clone(),
                            });
                            continue;
                        }
                    }
                    let db_schema_names: Vec<String> = {
                        let mut seen_schemas: Vec<String> = Vec::new();
                        for item in &all_items {
                            if let Some(schema) = item.schema.as_deref() {
                                if !schema.is_empty() && !seen_schemas.iter().any(|s| s == schema) {
                                    seen_schemas.push(schema.to_owned());
                                }
                            }
                        }
                        seen_schemas
                    };
                    let mut schema_groups = group_by_schema(&all_items, &db_schema_names);
                    if let Some(groups) = &schema_groups {
                        if groups.len() == 1 && groups.contains_key(&db_name) {
                            schema_groups = None;
                        }
                    }

                    match schema_groups {
                        Some(schema_groups) => {
                            let preferred = params
                                .db_registry
                                .get(&conn.database_type)
                                .and_then(|m| m.default_schema.clone());
                            let mut sorted_schemas: Vec<String> =
                                schema_groups.keys().cloned().collect();
                            sorted_schemas.sort_by(|a, b| {
                                if let Some(preferred) = &preferred {
                                    if a == preferred {
                                        return std::cmp::Ordering::Less;
                                    }
                                    if b == preferred {
                                        return std::cmp::Ordering::Greater;
                                    }
                                }
                                a.to_lowercase().cmp(&b.to_lowercase())
                            });

                            for schema_name in sorted_schemas {
                                if !should_show_schema(&schema_name, &object_filter) {
                                    continue;
                                }
                                let schema_key = format!("{db_key}::{schema_name}");
                                let schema_items =
                                    schema_groups.get(&schema_name).cloned().unwrap_or_default();
                                let schema_expanded =
                                    is_open(params.expanded_schemas.contains(&schema_key));

                                let pair = levels(DATABASE_CHILD_DEPTH);
                                rows.push(NavigatorRow::Schema {
                                    depth_pair: pair,
                                    connection_id: conn.id.clone(),
                                    db_name: db_name.clone(),
                                    schema_name: schema_name.clone(),
                                    expanded: schema_expanded,
                                });

                                if schema_expanded {
                                    add_categories(
                                        &mut rows,
                                        &schema_items,
                                        &conn.id,
                                        &db_session_id,
                                        &db_name,
                                        Some(&schema_name),
                                        DATABASE_CHILD_DEPTH + 1,
                                        &conn.database_type,
                                        &object_filter,
                                        &params.db_objects_map,
                                    );
                                }
                            }
                        }
                        None => {
                            add_categories(
                                &mut rows,
                                &all_items,
                                &conn.id,
                                &db_session_id,
                                &db_name,
                                None,
                                DATABASE_CHILD_DEPTH,
                                &conn.database_type,
                                &object_filter,
                                &params.db_objects_map,
                            );
                        }
                    }
                }
            } else {
                // Single-database connection: the tree shows one database,
                // named by the session's current database or the config's.
                let db_name = schema_data
                    .current_database
                    .clone()
                    .or_else(|| conn.database.clone())
                    .unwrap_or_default();
                if db_name.is_empty() {
                    continue;
                }

                let db_key = format!("{}::{}", conn.id, db_name);
                // Deliberately the raw expand state, not `is_open`: the
                // single-database branch only paints the db row and its
                // children when the user actually expanded it (a search still
                // reaches the children through the connection's force-expand).
                let is_db_expanded = params.expanded_dbs.contains(&db_key);
                let db_loading = schema_data.loading && schema_data.tables.is_empty();

                let pair = levels(CONNECTION_CHILD_DEPTH);
                rows.push(NavigatorRow::Db {
                    depth_pair: pair,
                    connection_id: conn.id.clone(),
                    db_session_id: db_session_id.clone(),
                    db_name: db_name.clone(),
                    expanded: is_db_expanded,
                    loading: db_loading,
                    is_open: is_database_open(
                        &params.expanded_dbs,
                        &params.open_dbs,
                        &db_session_id,
                        &db_name,
                        &conn.id,
                    ),
                });

                if !is_db_expanded {
                    continue;
                }
                if db_loading {
                    let pair = levels(DATABASE_CHILD_DEPTH);
                    rows.push(NavigatorRow::DbLoading {
                        depth_pair: pair,
                        owner_key: format!("db:{db_key}"),
                    });
                    continue;
                }

                let mut all_items = schema_data.tables.clone();
                all_items.extend(schema_data.views.iter().cloned());
                // Same failed-read distinction, reading the session-level
                // error a single-database connection records.
                if all_items.is_empty() {
                    if let Some(message) = schema_data.error.clone() {
                        let pair = levels(DATABASE_CHILD_DEPTH);
                        rows.push(NavigatorRow::DbError {
                            depth_pair: pair,
                            owner_key: format!("db:{db_key}"),
                            message,
                        });
                        continue;
                    }
                }
                let schema_groups = group_by_schema(&all_items, &schema_data.schema_names);

                match schema_groups {
                    Some(schema_groups) => {
                        let preferred = params
                            .db_registry
                            .get(&conn.database_type)
                            .and_then(|m| m.default_schema.clone());
                        let mut sorted_schemas: Vec<String> =
                            schema_groups.keys().cloned().collect();
                        sorted_schemas.sort_by(|a, b| {
                            if let Some(preferred) = &preferred {
                                if a == preferred {
                                    return std::cmp::Ordering::Less;
                                }
                                if b == preferred {
                                    return std::cmp::Ordering::Greater;
                                }
                            }
                            a.to_lowercase().cmp(&b.to_lowercase())
                        });

                        for schema_name in sorted_schemas {
                            if !should_show_schema(&schema_name, &object_filter) {
                                continue;
                            }
                            let schema_key = format!("{db_key}::{schema_name}");
                            let schema_items =
                                schema_groups.get(&schema_name).cloned().unwrap_or_default();
                            let schema_expanded =
                                is_open(params.expanded_schemas.contains(&schema_key));

                            let pair = levels(DATABASE_CHILD_DEPTH);
                            rows.push(NavigatorRow::Schema {
                                depth_pair: pair,
                                connection_id: conn.id.clone(),
                                db_name: db_name.clone(),
                                schema_name: schema_name.clone(),
                                expanded: schema_expanded,
                            });

                            if schema_expanded {
                                add_categories(
                                    &mut rows,
                                    &schema_items,
                                    &conn.id,
                                    &db_session_id,
                                    &db_name,
                                    Some(&schema_name),
                                    DATABASE_CHILD_DEPTH + 1,
                                    &conn.database_type,
                                    &object_filter,
                                    &params.db_objects_map,
                                );
                            }
                        }
                    }
                    None => {
                        add_categories(
                            &mut rows,
                            &all_items,
                            &conn.id,
                            &db_session_id,
                            &db_name,
                            None,
                            DATABASE_CHILD_DEPTH,
                            &conn.database_type,
                            &object_filter,
                            &params.db_objects_map,
                        );
                    }
                }
            }
        }
    }

    rows
}

/// Count (or pass through) the table items a query keeps.
fn items_matching(items: &[TableInfo], query_lower: &str) -> Vec<TableInfo> {
    if query_lower.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|i| i.name.to_lowercase().contains(query_lower))
        .cloned()
        .collect()
}

fn objects_matching(objects: &[DatabaseObject], query_lower: &str) -> Vec<DatabaseObject> {
    if query_lower.is_empty() {
        return objects.to_vec();
    }
    objects
        .iter()
        .filter(|o| o.name.to_lowercase().contains(query_lower))
        .cloned()
        .collect()
}
