//! React `windows/connection/navigator/utils.ts` — the formal port (T582).
//!
//! The flat-row half (the depth ladder, the multi-database decision, schema
//! grouping, the namespace flattener) lived in
//! `build_flat_rows/deps/utils.rs` from T570 until this task promoted it; the
//! SQL quoting, the error extractor and the drop-database fallback joined it
//! with T578's menus. Everything is here now and every reader re-points,
//! behaviour unchanged.
//!
//! What this module owns, in React file order: the depth ladder's two
//! constants; `shouldUseMultiDatabaseTree` (multi-DB unless the connection
//! pinned a *logical* database — Kiwi's domain in `connection.database` must
//! not lock the sidebar); `depthPadding`; `groupBySchema`;
//! `resolveDropDatabaseFallback`; `extractErrorMessage`; identifier quoting
//! (`escapeIdent` + `quoteRelationName`); the namespace-tree trio
//! (`namespaceTreeContains`, `namespaceLeafContext`, `flattenNamespaceTree`);
//! the stable row key ([`get_unified_row_key`], re-exported from the T572
//! types port where the union lives); and the drag ghost.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;

use super::build_flat_rows::deps::sql_namespace::{path_key, SqlNamespace};
use super::build_flat_rows::deps::tree_rules::{effective_expanded, searched_row_levels};
use super::build_flat_rows::inputs::{TableInfo, TableType};
use super::types::NavigatorRow;

/// React `getUnifiedRowKey` — lives with the row union it reads (T572's
/// `types.ts` port); re-exported here because `utils.ts` is where React
/// exports it from.
pub use super::types::row_key as get_unified_row_key;

/// React `CONNECTION_CHILD_DEPTH` — the depth every child of a connection is
/// painted at — databases, namespace branches, and their loading
/// placeholders alike.
///
/// A connection is emitted at depth 1 beneath its section/group header, or at
/// depth 0 when a search suppresses that header. Its children do not move with
/// it: they always start here, so a search removes exactly one level of parent
/// between a connection and its databases.
///
/// It lives here rather than in the builder because `flattenNamespaceTree`
/// builds rows of the same ladder and would otherwise have to import from a
/// module that already imports it.
pub const CONNECTION_CHILD_DEPTH: usize = 2;

/// React `DATABASE_CHILD_DEPTH` — the depth a database's children are painted
/// at: its schema row, and the placeholders shown while it loads.
///
/// Derived rather than declared, so the ladder has a single root: move
/// `CONNECTION_CHILD_DEPTH` and every level below it follows. A database with
/// no schemas puts its categories here instead of one level deeper.
pub const DATABASE_CHILD_DEPTH: usize = CONNECTION_CHILD_DEPTH + 1;

/// React `shouldUseMultiDatabaseTree`: multi-DB tree when the driver supports
/// it, unless the connection pinned a *logical* database. Kiwi stores its
/// domain in `connection.database` (`databaseFieldType: 'domain'`) and that
/// must not lock the sidebar out of the multi-database tree.
pub fn should_use_multi_database_tree(
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

/// React `depthPadding(depth)` — `` `${0.375 + depth * 1}rem` ``. The number
/// is the port (the rem unit is the stylesheet's business); the base is the
/// chevron gutter, the step one indent rung.
pub fn depth_padding(depth: usize) -> f32 {
    0.375 + depth as f32
}

/// React `groupBySchema` — group items by their schema, seeding empty groups
/// for `extra_schema_names` (PG lists schemas that hold no tables).
///
/// `None` is React's `null`: no item carries a schema and no extras were
/// named, so there is no schema ladder to draw at all.
pub fn group_by_schema(
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

/// React `resolveDropDatabaseFallback` — pick a safe database to switch to
/// before dropping `dropping`: `postgres` first (the maintenance database is
/// always there on PG), then the configured database, then whatever else
/// remains. `None` when `dropping` was the last one.
pub fn resolve_drop_database_fallback(
    databases: &[String],
    dropping: &str,
    configured_db: Option<&str>,
) -> Option<String> {
    if databases.iter().any(|d| d == "postgres") && dropping != "postgres" {
        return Some("postgres".to_owned());
    }
    let configured = configured_db.map(str::trim).filter(|c| !c.is_empty());
    if let Some(configured) = configured {
        if configured != dropping && databases.iter().any(|d| d == configured) {
            return Some(configured.to_owned());
        }
    }
    databases.iter().find(|d| *d != dropping).cloned()
}

/// React `extractErrorMessage` — the message to show for a failed command:
/// a non-empty string as-is, an error's `.message`, an object's `message`
/// field, else the localized fallback. The command failures arrive here as
/// JSON values (the driver IPC shape), so the object branch reads
/// `message` off `serde_json::Value` exactly as React reads it off `unknown`.
pub fn extract_error_message(error: &Value, fallback: &str) -> String {
    match error {
        Value::String(s) if !s.trim().is_empty() => s.clone(),
        Value::Object(obj) => match obj.get("message") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
            _ => fallback.to_owned(),
        },
        _ => fallback.to_owned(),
    }
}

/// React `escapeIdent` (lib/databaseTypes.ts) — quote with the driver's
/// `quoteChar` (default `"`), doubling the quote char inside the name.
/// `` ` `` and `[` have their own closing rules; a driver with no quoting
/// (Redis) gets the name as-is. Removal stage: shard 2's registry task
/// (T450) ports `DB_REGISTRY` formally; the quote char arrives from the
/// caller's meta snapshot until then.
pub fn escape_ident(name: &str, quote_char: &str) -> String {
    match quote_char {
        "`" => format!("`{}`", name.replace('`', "``")),
        "\"" => format!("\"{}\"", name.replace('"', "\"\"")),
        "[" => format!("[{}]", name.replace(']', "]]")),
        // No quoting (e.g. Redis).
        _ => name.to_owned(),
    }
}

/// React `quoteRelationName` — `schema.name` with both halves quoted by the
/// driver's identifier quoter (the escape char arrives from the caller's
/// registry snapshot; `"` is PostgreSQL's and the default).
pub fn quote_relation_name(
    name: &str,
    schema: Option<&str>,
    quote_char: &str,
) -> String {
    match schema {
        Some(schema) => format!(
            "{}.{}",
            escape_ident(schema, quote_char),
            escape_ident(name, quote_char)
        ),
        None => escape_ident(name, quote_char),
    }
}

/// React `namespaceTreeContains` — any key on any depth matches the query.
pub fn namespace_tree_contains(tree: &SqlNamespace, query: &str) -> bool {
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

/// React `namespaceLeafContext` — map namespace leaf segments to the driver
/// fetch path + SQL schema a table panel needs.
///
/// The last segment is the table; the first parent segment resolves through
/// `pathAliases` (Superset-style ids), the rest join with `/` to form the
/// fetch path; the schema is only meaningful at depth 2+ (the segment that
/// names the schema inside the path).
pub struct NamespaceLeafContext {
    pub table_name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
}

pub fn namespace_leaf_context(
    segments: &[String],
    path_aliases: &HashMap<String, String>,
) -> NamespaceLeafContext {
    if segments.is_empty() {
        return NamespaceLeafContext {
            table_name: String::new(),
            schema: None,
            database: None,
        };
    }
    let table_name = segments[segments.len() - 1].clone();
    let parent_segments = &segments[..segments.len() - 1];
    if parent_segments.is_empty() {
        return NamespaceLeafContext { table_name, schema: None, database: None };
    }
    let root_id = path_aliases
        .get(&parent_segments[0])
        .cloned()
        .unwrap_or_else(|| parent_segments[0].clone());
    let fetch_path = if parent_segments.len() == 1 {
        root_id
    } else {
        let mut path = root_id;
        for segment in &parent_segments[1..] {
            path.push('/');
            path.push_str(segment);
        }
        path
    };
    let schema = if parent_segments.len() >= 2 {
        Some(parent_segments[parent_segments.len() - 1].clone())
    } else {
        None
    };
    NamespaceLeafContext {
        table_name,
        schema,
        database: Some(fetch_path),
    }
}

/// React `flattenNamespaceTree` — the path-hierarchy driver's row ladder.
///
/// Branches become expandable namespace-node rows keyed by their path; leaves
/// take their kind from the tables metadata (`table` when the map has no say).
/// A query keeps a branch whose own name misses but whose subtree matches, and
/// drops a leaf that misses; an expanded branch with no children yet shows the
/// path-ensure spinner unless the path is already loaded or a search is on.
#[allow(clippy::too_many_arguments)]
pub fn flatten_namespace_tree(
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

// ---------------------------------------------------------------------------
// The drag ghost
// ---------------------------------------------------------------------------

/// React `createDragGhost`'s element id — the handle `removeDragGhost` looks
/// up when the drag began in an earlier render (or another window).
pub const DRAG_GHOST_ID: &str = "datazen-drag-ghost";

/// React `createDragGhost(label)`'s DOM node, as a spec: the host mounts it
/// as a detached overlay (fixed, off-screen, pointer-events none) and hands
/// it to its drag-image API with the 16×14 offset T574's
/// `ShowDragGhost` carries. The WebKit bug the React doc names — a
/// transformed virtualized container snapshots the wrong coordinates — is
/// exactly why the spec is fixed-positioned off-screen rather than inline.
#[derive(Clone, Debug, PartialEq)]
pub struct DragGhostSpec {
    pub id: &'static str,
    pub label: String,
    /// `padding: 4px 10px; border-radius: 6px` — the pill.
    pub padding: (f32, f32),
    pub radius: f32,
    pub font_size: f32,
}

/// React `createDragGhost` — build the ghost spec (and forget any previous
/// one: the React function removes the active ghost before creating a new
/// node, which the drag machine expresses as `ShowDragGhost` after
/// `HideDragGhost` when a drag follows a drag).
pub fn create_drag_ghost(label: &str) -> DragGhostSpec {
    DragGhostSpec {
        id: DRAG_GHOST_ID,
        label: label.to_owned(),
        padding: (4.0, 10.0),
        radius: 6.0,
        font_size: 12.0,
    }
}
