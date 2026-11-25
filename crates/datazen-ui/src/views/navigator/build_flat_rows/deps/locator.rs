//! `lib/connectionLocator.ts` — the search half, ported verbatim for T570.
//! Removal stage: shard 2's T403 ports this file formally (it also owns the
//! grouping helpers the navigator host uses); when it lands, this copy moves
//! out and the builder re-points.
//!
//! What the builder needs: the section keys, the scoped expand key, and
//! [`rank_connections`] — the one deterministic ordering contract behind the
//! navigator's search. A candidate's rank is `field_offset + 0|1|2` (exact /
//! prefix / substring), so a connection named for the query outranks one that
//! merely hosts a matching table. The tiebreak ladder after rank: pinned
//! first, most recently connected first, then group, name, id.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::super::inputs::{
    ConnectionSnapshot, DatabaseObject, SchemaStateSnapshot, TableInfo, TableType,
    UsageStateSnapshot,
};

/// React `PINNED_GROUP_KEY`.
pub const PINNED_GROUP_KEY: &str = "__pinned__";
/// React `RECENT_GROUP_KEY`.
pub const RECENT_GROUP_KEY: &str = "__recent__";

/// React `connectionExpandKey` — scoped so the same connection expands
/// independently per navigator section.
pub fn connection_expand_key(section_group: &str, connection_id: &str) -> String {
    format!("{section_group}::{connection_id}")
}

/// React `ConnectionMatchReason` (order matters only for the tiebreak sort).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchReason {
    Name,
    Host,
    Database,
    DatabaseType,
    Schema,
    Table,
    View,
    Object,
    Namespace,
    Path,
}

impl MatchReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Host => "host",
            Self::Database => "database",
            Self::DatabaseType => "database-type",
            Self::Schema => "schema",
            Self::Table => "table",
            Self::View => "view",
            Self::Object => "object",
            Self::Namespace => "namespace",
            Self::Path => "path",
        }
    }
}

/// React `ConnectionMatch` — lower rank is stronger and sorts first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionMatch {
    pub rank: i64,
    pub reason: MatchReason,
    pub context: String,
}

/// React `ConnectionLocatorResult`.
#[derive(Clone, Debug)]
pub struct ConnectionLocatorResult {
    pub connection: ConnectionSnapshot,
    pub match_payload: Option<ConnectionMatch>,
}

struct Candidate {
    rank: i64,
    reason: MatchReason,
    context: String,
}

fn normalize(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().to_lowercase()
}

fn match_candidate(
    value: Option<&str>,
    query: &str,
    reason: MatchReason,
    context: Option<&str>,
    field_offset: i64,
) -> Option<Candidate> {
    let normalized_value = normalize(value);
    if normalized_value.is_empty() || !normalized_value.contains(query) {
        return None;
    }
    let rank = if normalized_value == query {
        field_offset
    } else if normalized_value.starts_with(query) {
        field_offset + 1
    } else {
        field_offset + 2
    };
    Some(Candidate {
        rank,
        reason,
        context: context.unwrap_or(value.unwrap_or_default()).to_owned(),
    })
}

fn object_reason(is_table: bool, table_type: Option<&TableType>) -> MatchReason {
    if is_table {
        match table_type {
            Some(TableType::View) | Some(TableType::MaterializedView) => MatchReason::View,
            _ => MatchReason::Table,
        }
    } else {
        MatchReason::Object
    }
}

fn object_context(schema: Option<&str>, name: &str) -> String {
    match schema {
        Some(schema) if !schema.is_empty() => format!("{schema}.{name}"),
        _ => name.to_owned(),
    }
}

fn table_context(item: &TableInfo) -> String {
    object_context(item.schema.as_deref(), &item.name)
}

fn db_object_context(item: &DatabaseObject) -> String {
    object_context(item.schema.as_deref(), &item.name)
}

fn collect_namespace_candidates(
    tree: &super::sql_namespace::SqlNamespace,
    query: &str,
    parent_segments: &[String],
    candidates: &mut Vec<Candidate>,
) {
    if tree.is_leaf() {
        return;
    }
    for (name, child) in tree.entries() {
        let mut segments = parent_segments.to_vec();
        segments.push(name.clone());
        let context = segments.join("/");
        let reason = if child.is_leaf() {
            MatchReason::Table
        } else {
            MatchReason::Namespace
        };
        if let Some(candidate) = match_candidate(Some(&name), query, reason, Some(&context), 60) {
            candidates.push(candidate);
        }
        if !child.is_leaf() {
            collect_namespace_candidates(&child, query, &segments, candidates);
        }
    }
}

fn connection_candidates(
    connection: &ConnectionSnapshot,
    query: &str,
    usage: &UsageStateSnapshot,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let mut namespace_candidates = Vec::new();
    let mut push =
        |value: Option<&str>, reason: MatchReason, context: Option<&str>, offset: i64| {
            if let Some(candidate) = match_candidate(value, query, reason, context, offset) {
                candidates.push(candidate);
            }
        };

    push(
        Some(connection.name.as_str()),
        MatchReason::Name,
        Some(&connection.name),
        0,
    );
    push(
        connection.host.as_deref(),
        MatchReason::Host,
        connection.host.as_deref(),
        10,
    );
    push(
        connection.database.as_deref(),
        MatchReason::Database,
        connection.database.as_deref(),
        20,
    );
    push(
        Some(connection.database_type.as_str()),
        MatchReason::DatabaseType,
        Some(&connection.database_type),
        30,
    );
    push(
        connection.driver_label.as_deref(),
        MatchReason::DatabaseType,
        connection.driver_label.as_deref(),
        30,
    );

    let Some(active) = usage.active_connections.get(&connection.id) else {
        return candidates;
    };
    if active.status != "connected" {
        return candidates;
    }
    let Some(db_session_id) = active.db_session_id.as_deref() else {
        return candidates;
    };

    if let Some(schema_data) = usage.schemas.get(db_session_id) {
        for database in &schema_data.databases {
            push(Some(database), MatchReason::Database, Some(database), 20);
        }
        for schema in &schema_data.schema_names {
            push(Some(schema), MatchReason::Schema, Some(schema), 40);
        }
        for item in schema_data.tables.iter().chain(schema_data.views.iter()) {
            let reason = object_reason(true, Some(&item.table_type));
            push(Some(&item.name), reason, Some(&table_context(item)), 50);
            if let Some(schema) = item.schema.as_deref() {
                push(Some(schema), MatchReason::Schema, Some(schema), 40);
            }
        }
        // Collected into the outer vec (the `push` closure holds `candidates`
        // for the rest of the function); merged after the closure is done.
        collect_namespace_candidates(
            &schema_data.namespace_tree,
            query,
            &[],
            &mut namespace_candidates,
        );
        for items in schema_data.path_items.values() {
            for item in items {
                push(
                    Some(&item.name),
                    MatchReason::Path,
                    Some(&table_context(item)),
                    70,
                );
                if let Some(schema) = item.schema.as_deref() {
                    push(Some(schema), MatchReason::Path, Some(schema), 70);
                }
            }
        }
    }

    let table_prefix = format!("{db_session_id}::");
    for (key, items) in &usage.db_tables_map {
        if !key.starts_with(&table_prefix) {
            continue;
        }
        let database = &key[table_prefix.len()..];
        push(Some(database), MatchReason::Database, Some(database), 20);
        for item in items {
            let reason = object_reason(true, Some(&item.table_type));
            push(Some(&item.name), reason, Some(&table_context(item)), 50);
            if let Some(schema) = item.schema.as_deref() {
                push(Some(schema), MatchReason::Schema, Some(schema), 40);
            }
        }
    }

    let object_prefix = format!("{}::", connection.id);
    for (key, items) in &usage.db_objects_map {
        if !key.starts_with(&object_prefix) {
            continue;
        }
        // React strips one trailing `::<kind>` via regex — exactly one, so a
        // kind name repeated in the key itself is left standing.
        let mut context_prefix = key[object_prefix.len()..].to_string();
        for kind in [
            "tables",
            "views",
            "functions",
            "procedures",
            "triggers",
            "sequences",
            "types",
        ] {
            let suffix = format!("::{kind}");
            if let Some(stripped) = context_prefix.strip_suffix(&suffix) {
                context_prefix = stripped.to_owned();
                break;
            }
        }
        for item in items {
            let context = if context_prefix.is_empty() {
                db_object_context(item)
            } else {
                format!("{context_prefix}.{}", db_object_context(item))
            };
            let reason = object_reason(false, None);
            push(Some(&item.name), reason, Some(&context), 50);
            if let Some(schema) = item.schema.as_deref() {
                push(Some(schema), MatchReason::Schema, Some(schema), 40);
            }
        }
    }

    // Namespace candidates were collected beside the closure-held `push`, so
    // they merge here; `best_candidate` takes the min, order is not observable.
    let mut namespace = std::mem::take(&mut namespace_candidates);
    candidates.append(&mut namespace);

    candidates
}

fn best_candidate(candidates: Vec<Candidate>) -> Option<ConnectionMatch> {
    candidates
        .into_iter()
        .min_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| a.reason.as_str().cmp(b.reason.as_str()))
                .then_with(|| a.context.cmp(&b.context))
        })
        .map(|candidate| ConnectionMatch {
            rank: candidate.rank,
            reason: candidate.reason,
            context: candidate.context,
        })
}

/// React `timestamp` — `Date.parse` of an ISO string; unparseable is absent.
fn timestamp(value: Option<&str>) -> Option<i64> {
    let value = value?;
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc).timestamp_millis())
}

/// React `compareStrings` with `{ sensitivity: 'base' }` — case-insensitive;
/// ASCII names (everything the tree shows) order the same as byte order on
/// the lowercased strings.
fn compare_strings_lower(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    a.unwrap_or_default()
        .to_lowercase()
        .cmp(&b.unwrap_or_default().to_lowercase())
}

fn compare_results(a: &ConnectionLocatorResult, b: &ConnectionLocatorResult) -> std::cmp::Ordering {
    let a_rank = a.match_payload.as_ref().map_or(i64::MAX, |m| m.rank);
    let b_rank = b.match_payload.as_ref().map_or(i64::MAX, |m| m.rank);
    if a_rank != b_rank {
        return a_rank.cmp(&b_rank);
    }
    let a_pinned = a.connection.pinned == Some(true);
    let b_pinned = b.connection.pinned == Some(true);
    if a_pinned != b_pinned {
        return b_pinned.cmp(&a_pinned); // pinned first
    }
    let a_time = timestamp(a.connection.last_connected_at.as_deref());
    let b_time = timestamp(b.connection.last_connected_at.as_deref());
    if a_time != b_time {
        // Most recent first; never-connected sorts last.
        return match (a_time, b_time) {
            (None, _) => std::cmp::Ordering::Greater,
            (_, None) => std::cmp::Ordering::Less,
            (Some(a), Some(b)) => b.cmp(&a),
        };
    }
    let group_order =
        compare_strings_lower(a.connection.group.as_deref(), b.connection.group.as_deref());
    if group_order != std::cmp::Ordering::Equal {
        return group_order;
    }
    let name_order = compare_strings_lower(Some(&a.connection.name), Some(&b.connection.name));
    if name_order != std::cmp::Ordering::Equal {
        return name_order;
    }
    a.connection.id.cmp(&b.connection.id)
}

/// React `rankConnections`: search and rank connection configs with one
/// deterministic ordering. A non-empty query returns only matching
/// connections; an empty query returns every connection, deduplicated by id
/// in first-seen order, each with a null match.
pub fn rank_connections(
    connections: &[ConnectionSnapshot],
    query: &str,
    usage: &UsageStateSnapshot,
) -> Vec<ConnectionLocatorResult> {
    let normalized_query = normalize(Some(query));
    let mut unique: HashMap<String, ()> = HashMap::new();
    let mut results = Vec::new();
    for connection in connections {
        if unique.contains_key(&connection.id) {
            continue;
        }
        unique.insert(connection.id.clone(), ());
        let match_payload = if normalized_query.is_empty() {
            None
        } else {
            best_candidate(connection_candidates(connection, &normalized_query, usage))
        };
        results.push(ConnectionLocatorResult {
            connection: connection.clone(),
            match_payload,
        });
    }
    results.retain(|result| normalized_query.is_empty() || result.match_payload.is_some());
    results.sort_by(compare_results);
    results
}
