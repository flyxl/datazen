//! `windows/connection/navigator/utils.ts` — the flat-row half of it, ported
//! verbatim for T570. Removal stage: T582 ports this file formally (its other
//! exports — the SQL quoting, the error extractor, the drag ghost — belong to
//! the navigator host and later tasks); when it lands, these move and the
//! builder re-points, behaviour unchanged.
//!
//! What lives here: the depth ladder's two constants, the multi-database
//! decision, schema grouping, and the namespace-tree flattener.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::super::inputs::{TableInfo, TableType};
use super::sql_namespace::{path_key, SqlNamespace};
use super::tree_rules::{effective_expanded, searched_row_levels};
use crate::views::navigator::build_flat_rows::NavigatorRow;

/// React `CONNECTION_CHILD_DEPTH` — the depth every child of a connection is
/// painted at, databases and namespace branches alike.
pub(crate) const CONNECTION_CHILD_DEPTH: usize = 2;

/// React `DATABASE_CHILD_DEPTH` — derived, so the ladder has one root.
pub(crate) const DATABASE_CHILD_DEPTH: usize = CONNECTION_CHILD_DEPTH + 1;

/// React `shouldUseMultiDatabaseTree`: multi-DB when the driver supports it,
/// unless the connection pinned a *logical* database. Kiwi stores its domain
/// in `connection.database` (`databaseFieldType: 'domain'`) and that must not
/// lock the sidebar out of the multi-database tree.
pub(crate) fn should_use_multi_database_tree(
    meta_has_multi_database: bool,
    meta_database_field_type: &str,
    initial_database: Option<&str>,
) -> bool {
    if !meta_has_multi_database {
        return false;
    }
    if meta_database_field_type == "domain" {
        return true;
    }
    !initial_database.is_some_and(|d| !d.trim().is_empty())
}

/// React `groupBySchema` — group items by their schema, seeding empty groups
/// for `extra_schema_names` (PG lists schemas that hold no tables).
///
/// `None` is React's `null`: no item carries a schema and no extras were
/// named, so there is no schema ladder to draw at all.
pub(crate) fn group_by_schema(
    items: &[TableInfo],
    extra_schema_names: &[String],
) -> Option<BTreeMap<String, Vec<TableInfo>>> {
    let has_any_schema = items
        .iter()
        .any(|item| item.schema.as_deref().is_some_and(|s| !s.is_empty()));
    if !has_any_schema && extra_schema_names.is_empty() {
        return None;
    }
    let mut map: BTreeMap<String, Vec<TableInfo>> = BTreeMap::new();
    for schema in extra_schema_names {
        map.entry(schema.clone()).or_default();
    }
    for item in items {
        if item.name.is_empty() {
            continue;
        }
        let key = item.schema.clone().unwrap_or_default();
        map.entry(key).or_default().push(item.clone());
    }
    Some(map)
}

/// React `namespaceTreeContains` — any key on any depth matches the query.
pub(crate) fn namespace_tree_contains(tree: &SqlNamespace, query: &str) -> bool {
    if tree.is_leaf() {
        return false;
    }
    for (key, child) in tree.entries() {
        if key.to_lowercase().contains(query) {
            return true;
        }
        if !child.is_leaf() && namespace_tree_contains(&child, query) {
            return true;
        }
    }
    false
}

/// React `flattenNamespaceTree` — the path-hierarchy driver's row ladder.
///
/// Branches become expandable namespace-node rows keyed by their path; leaves
/// take their kind from the tables metadata (`table` when the map has no say).
/// A query keeps a branch whose own name misses but whose subtree matches, and
/// drops a leaf that misses; an expanded branch with no children yet shows the
/// path-ensure spinner unless the path is already loaded or a search is on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn flatten_namespace_tree(
    tree: &SqlNamespace,
    connection_id: &str,
    db_session_id: &str,
    base_depth: usize,
    rows: &mut Vec<NavigatorRow>,
    expanded_dbs: &HashSet<String>,
    query: &str,
    table_type_map: &HashMap<String, TableType>,
    loaded_paths: &HashSet<String>,
    parent_segments: &[String],
) {
    if tree.is_leaf() {
        return;
    }
    let searching = !query.is_empty();
    let levels = |depth: usize| searched_row_levels(depth, CONNECTION_CHILD_DEPTH, searching);

    // React sorts the entries with `localeCompare`; a sorted map gives the
    // same order for the ASCII names trees are built from.
    for (name, child) in tree.entries() {
        if searching && !name.to_lowercase().contains(query) {
            if child.is_leaf() {
                continue;
            }
            if !namespace_tree_contains(&child, query) {
                continue;
            }
        }

        let mut segments = parent_segments.to_vec();
        segments.push(name.clone());
        let segments_ref: Vec<&str> = segments.iter().map(String::as_str).collect();
        let node_key = format!("{connection_id}::ns::{}", path_key(&segments_ref));

        if child.is_leaf() {
            rows.push(NavigatorRow::NamespaceNode {
                name: name.clone(),
                depth_pair: levels(base_depth),
                expanded: false,
                is_leaf: true,
                leaf_kind: Some(
                    table_type_map
                        .get(&name)
                        .cloned()
                        .unwrap_or(TableType::Table),
                ),
                segments: segments.clone(),
                key: node_key,
                connection_id: connection_id.to_owned(),
                db_session_id: db_session_id.to_owned(),
            });
        } else {
            let expanded = effective_expanded(expanded_dbs.contains(&node_key), searching);
            rows.push(NavigatorRow::NamespaceNode {
                name: name.clone(),
                depth_pair: levels(base_depth),
                expanded,
                is_leaf: false,
                leaf_kind: None,
                segments: segments.clone(),
                key: node_key,
                connection_id: connection_id.to_owned(),
                db_session_id: db_session_id.to_owned(),
            });
            if expanded {
                let child_len = child.len();
                let path_loaded = loaded_paths.contains(&path_key(&segments_ref));
                if child_len == 0 && !path_loaded && !searching {
                    // The spinner stands in for the path still being ensured,
                    // so it is owned by that path — not by where it sits.
                    rows.push(NavigatorRow::DbLoading {
                        depth_pair: levels(base_depth + 1),
                        owner_key: format!("ns:{}", path_key(&segments_ref)),
                    });
                } else {
                    flatten_namespace_tree(
                        &child,
                        connection_id,
                        db_session_id,
                        base_depth + 1,
                        rows,
                        expanded_dbs,
                        query,
                        table_type_map,
                        loaded_paths,
                        &segments,
                    );
                }
            }
        }
    }
}
