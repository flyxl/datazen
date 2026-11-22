use std::cmp::Ordering;

use crate::connection_types::{ConnectionConfig, TableInfo, TableType};
use crate::build_editor_schema::{is_leaf, SqlNamespace};

use super::types::{
    get, ActiveConnectionSlice, ActiveStatus, Candidate, ConnectionLocatorResult,
    ConnectionLocatorUsageState, ConnectionMatch, ConnectionMatchReason, ConnectionSection,
    ExpandKey, SchemaSlice, PINNED_GROUP_KEY, RECENT_CONNECTION_LIMIT, RECENT_GROUP_KEY,
};

pub fn normalize(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_lowercase()
}

fn match_candidate(
    value: Option<&str>,
    query: &str,
    reason: ConnectionMatchReason,
    context: &str,
    field_offset: i32,
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
    Some(Candidate { rank, reason, context: context.to_owned() })
}

fn push(
    out: &mut Vec<Candidate>,
    value: Option<&str>,
    query: &str,
    reason: ConnectionMatchReason,
    context: &str,
    offset: i32,
) {
    if let Some(candidate) = match_candidate(value, query, reason, context, offset) {
        out.push(candidate);
    }
}

fn table_reason(item: &TableInfo) -> ConnectionMatchReason {
    match item.table_type {
        TableType::View | TableType::MaterializedView => ConnectionMatchReason::View,
        TableType::Table | TableType::SystemTable => ConnectionMatchReason::Table,
    }
}

fn object_context(schema: Option<&str>, name: &str) -> String {
    match schema {
        Some(schema) if !schema.is_empty() => format!("{schema}.{name}"),
        _ => name.to_owned(),
    }
}

fn collect_namespace_candidates(
    tree: &SqlNamespace,
    query: &str,
    parent_segments: &[String],
    out: &mut Vec<Candidate>,
) {
    if is_leaf(tree) {
        return;
    }
    let SqlNamespace::Branch(branch) = tree else {
        return;
    };
    for (name, child) in branch {
        let mut segments = parent_segments.to_vec();
        segments.push(name.clone());
        let context = segments.join("/");
        let reason = if is_leaf(child) {
            ConnectionMatchReason::Table
        } else {
            ConnectionMatchReason::Namespace
        };
        if let Some(candidate) = match_candidate(Some(name), query, reason, &context, 60) {
            out.push(candidate);
        }
        if !is_leaf(child) {
            collect_namespace_candidates(child, query, &segments, out);
        }
    }
}

fn context_prefix(key: &str) -> &str {
    for suffix in ["tables", "views", "functions", "procedures", "triggers", "sequences", "types"] {
        let needle = format!("::{suffix}");
        if let Some(stripped) = key.strip_suffix(&needle) {
            return stripped;
        }
    }
    key
}

fn connection_candidates(
    connection: &ConnectionConfig,
    query: &str,
    usage_state: &ConnectionLocatorUsageState,
    db_label: &dyn Fn(&str) -> Option<String>,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    push(&mut candidates, Some(&connection.name), query, ConnectionMatchReason::Name, &connection.name, 0);
    if let Some(host) = connection.host.as_deref() {
        push(&mut candidates, Some(host), query, ConnectionMatchReason::Host, host, 10);
    }
    if let Some(database) = connection.database.as_deref() {
        push(&mut candidates, Some(database), query, ConnectionMatchReason::Database, database, 20);
    }
    let database_type = connection.database_type.as_str();
    push(&mut candidates, Some(database_type), query, ConnectionMatchReason::DatabaseType, database_type, 30);
    let label = db_label(database_type);
    if let Some(label) = label.as_deref() {
        push(&mut candidates, Some(label), query, ConnectionMatchReason::DatabaseType, label, 30);
    }

    let Some(active) = usage_state.active(&connection.id) else {
        return candidates;
    };
    if !active_is_connected(active) || active.db_session_id.is_empty() {
        return candidates;
    }

    if let Some(schema) = usage_state.schema(&active.db_session_id) {
        collect_schema_candidates(schema, query, &mut candidates);
    }

    let table_prefix = format!("{}::", active.db_session_id);
    for (key, items) in &usage_state.db_tables_map {
        if !key.starts_with(&table_prefix) {
            continue;
        }
        let database = &key[table_prefix.len()..];
        push(&mut candidates, Some(database), query, ConnectionMatchReason::Database, database, 20);
        for item in items {
            let reason = table_reason(item);
            let context = object_context(item.schema.as_deref(), &item.name);
            push(&mut candidates, Some(&item.name), query, reason, &context, 50);
            if let Some(item_schema) = item.schema.as_deref() {
                push(&mut candidates, Some(item_schema), query, ConnectionMatchReason::Schema, item_schema, 40);
            }
        }
    }

    let object_prefix = format!("{}::", connection.id);
    for (key, items) in &usage_state.db_objects_map {
        if !key.starts_with(&object_prefix) {
            continue;
        }
        let prefix = context_prefix(&key[object_prefix.len()..]);
        for item in items {
            let base = object_context(item.schema.as_deref(), &item.name);
            let context = if prefix.is_empty() { base } else { format!("{prefix}.{base}") };
            push(&mut candidates, Some(&item.name), query, ConnectionMatchReason::Object, &context, 50);
            if let Some(item_schema) = item.schema.as_deref() {
                push(&mut candidates, Some(item_schema), query, ConnectionMatchReason::Schema, item_schema, 40);
            }
        }
    }

    candidates
}

fn active_is_connected(active: &ActiveConnectionSlice) -> bool {
    active.status == ActiveStatus::Connected
}

fn collect_schema_candidates(schema: &SchemaSlice, query: &str, out: &mut Vec<Candidate>) {
    for database in &schema.databases {
        push(out, Some(database), query, ConnectionMatchReason::Database, database, 20);
    }
    for name in &schema.schema_names {
        push(out, Some(name), query, ConnectionMatchReason::Schema, name, 40);
    }
    for item in schema.tables.iter().chain(schema.views.iter()) {
        let reason = table_reason(item);
        let context = object_context(item.schema.as_deref(), &item.name);
        push(out, Some(&item.name), query, reason, &context, 50);
        if let Some(item_schema) = item.schema.as_deref() {
            push(out, Some(item_schema), query, ConnectionMatchReason::Schema, item_schema, 40);
        }
    }
    collect_namespace_candidates(&schema.namespace_tree, query, &[], out);
    for (_, items) in &schema.path_items {
        for item in items {
            let context = object_context(item.schema.as_deref(), &item.name);
            push(out, Some(&item.name), query, ConnectionMatchReason::Path, &context, 70);
            if let Some(item_schema) = item.schema.as_deref() {
                push(out, Some(item_schema), query, ConnectionMatchReason::Path, item_schema, 70);
            }
        }
    }
}

fn best_candidate(candidates: &[Candidate]) -> Option<ConnectionMatch> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.reason.as_str().cmp(b.reason.as_str()))
            .then_with(|| a.context.cmp(&b.context))
    });
    sorted.into_iter().next()
}

fn parse_digits(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = year - i64::from(month <= 2);
    let era = if shifted_year >= 0 { shifted_year } else { shifted_year - 399 } / 400;
    let year_of_era = shifted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

fn timestamp(value: Option<&str>) -> Option<i64> {
    let text = value?;
    if !text.is_ascii() || text.len() < 10 {
        return None;
    }
    let year = parse_digits(&text[0..4])?;
    if text.as_bytes()[4] != b'-' {
        return None;
    }
    let month = parse_digits(&text[5..7])?;
    if text.as_bytes()[7] != b'-' {
        return None;
    }
    let day = parse_digits(&text[8..10])?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut hour = 0i64;
    let mut minute = 0i64;
    let mut second = 0i64;
    let mut millis = 0i64;
    let mut rest = &text[10..];
    if rest.starts_with('T') || rest.starts_with(' ') {
        rest = &rest[1..];
        if rest.len() < 5 {
            return None;
        }
        hour = parse_digits(&rest[0..2])?;
        if rest.as_bytes()[2] != b':' {
            return None;
        }
        minute = parse_digits(&rest[3..5])?;
        rest = &rest[5..];
        if rest.starts_with(':') {
            if rest.len() < 3 {
                return None;
            }
            second = parse_digits(&rest[1..3])?;
            rest = &rest[3..];
            if rest.starts_with('.') {
                let digits = rest[1..].chars().take_while(|ch| ch.is_ascii_digit()).count();
                if digits == 0 {
                    return None;
                }
                let fraction = &rest[1..1 + digits];
                let mut value = 0i64;
                for (index, ch) in fraction.chars().enumerate() {
                    if index < 3 {
                        value = value * 10 + i64::from(ch as u8 - b'0');
                    }
                }
                for _ in 0..3_usize.saturating_sub(digits) {
                    value *= 10;
                }
                millis = value;
                rest = &rest[1 + digits..];
            }
        }
        if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=59).contains(&second) {
            return None;
        }
        let epoch = days_from_civil(year, month, day) * 86_400_000
            + (hour * 3600 + minute * 60 + second) * 1000
            + millis;
        if rest.starts_with('Z') {
            let remaining = &rest[1..];
            return if remaining.is_empty() { Some(epoch) } else { None };
        }
        if rest.len() >= 6 && (rest.starts_with('+') || rest.starts_with('-')) {
            if rest.as_bytes()[3] != b':' {
                return None;
            }
            let offset_hour = parse_digits(&rest[1..3])?;
            let offset_minute = parse_digits(&rest[4..6])?;
            let remaining = &rest[6..];
            if !remaining.is_empty() || offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            return Some(epoch - sign * (offset_hour * 60 + offset_minute) * 60 * 1000);
        }
        return if rest.is_empty() { Some(epoch) } else { None };
    }
    if !rest.is_empty() {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400_000)
}

fn compare_strings(a: Option<&str>, b: Option<&str>) -> Ordering {
    a.unwrap_or("").to_lowercase().cmp(&b.unwrap_or("").to_lowercase())
}

fn compare_results(a: &ConnectionLocatorResult, b: &ConnectionLocatorResult) -> Ordering {
    let a_rank = a.match_value.as_ref().map_or(i32::MAX, |value| value.rank);
    let b_rank = b.match_value.as_ref().map_or(i32::MAX, |value| value.rank);
    match a_rank.cmp(&b_rank) {
        Ordering::Equal => (),
        ordering => return ordering,
    }
    let a_pinned = i32::from(a.connection.pinned == Some(true));
    let b_pinned = i32::from(b.connection.pinned == Some(true));
    match b_pinned.cmp(&a_pinned) {
        Ordering::Equal => (),
        ordering => return ordering,
    }
    let a_time = timestamp(a.connection.last_connected_at.as_deref());
    let b_time = timestamp(b.connection.last_connected_at.as_deref());
    if a_time != b_time {
        return match (a_time, b_time) {
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
            (Some(a_time), Some(b_time)) => b_time.cmp(&a_time),
        };
    }
    match compare_strings(a.connection.group.as_deref(), b.connection.group.as_deref()) {
        Ordering::Equal => (),
        ordering => return ordering,
    }
    match compare_strings(Some(a.connection.name.as_str()), Some(b.connection.name.as_str())) {
        Ordering::Equal => (),
        ordering => return ordering,
    }
    a.connection.id.cmp(&b.connection.id)
}

pub fn rank_connections(
    connections: &[ConnectionConfig],
    query: &str,
    usage_state: &ConnectionLocatorUsageState,
    db_label: &dyn Fn(&str) -> Option<String>,
) -> Vec<ConnectionLocatorResult> {
    let normalized_query = normalize(Some(query));
    let mut unique = Vec::new();
    for connection in connections {
        if !unique.iter().any(|existing: &ConnectionConfig| existing.id == connection.id) {
            unique.push(connection.clone());
        }
    }
    let mut results: Vec<ConnectionLocatorResult> = unique
        .into_iter()
        .map(|connection| {
            let match_value = if normalized_query.is_empty() {
                None
            } else {
                best_candidate(&connection_candidates(&connection, &normalized_query, usage_state, db_label))
            };
            ConnectionLocatorResult { connection, match_value }
        })
        .filter(|result| normalized_query.is_empty() || result.match_value.is_some())
        .collect();
    results.sort_by(compare_results);
    results
}

pub fn order_connections_for_display(connections: &[ConnectionConfig]) -> Vec<ConnectionConfig> {
    let pinned: Vec<ConnectionConfig> = connections
        .iter()
        .filter(|connection| connection.pinned == Some(true))
        .cloned()
        .collect();
    if pinned.is_empty() {
        return connections.to_vec();
    }
    let mut ordered = pinned;
    ordered.extend(
        connections
            .iter()
            .filter(|connection| connection.pinned != Some(true))
            .cloned(),
    );
    ordered
}

pub fn group_connections_with_recent_sections(
    connections: &[ConnectionConfig],
    groups: &[String],
) -> Vec<ConnectionSection> {
    let mut grouped: Vec<(String, Vec<ConnectionConfig>)> =
        groups.iter().map(|group| (group.clone(), Vec::new())).collect();
    for connection in connections {
        let group = connection.group.clone().unwrap_or_default();
        if let Some((_, items)) = grouped.iter_mut().find(|(key, _)| key == &group) {
            items.push(connection.clone());
        } else {
            grouped.push((group, vec![connection.clone()]));
        }
    }

    let pinned: Vec<ConnectionConfig> = connections
        .iter()
        .filter(|connection| connection.pinned == Some(true))
        .cloned()
        .collect();
    let mut recent: Vec<ConnectionConfig> = connections
        .iter()
        .filter(|connection| timestamp(connection.last_connected_at.as_deref()).is_some())
        .cloned()
        .collect();
    recent.sort_by(|a, b| {
        let a_time = timestamp(a.last_connected_at.as_deref()).unwrap_or(0);
        let b_time = timestamp(b.last_connected_at.as_deref()).unwrap_or(0);
        b_time
            .cmp(&a_time)
            .then_with(|| compare_strings(Some(a.name.as_str()), Some(b.name.as_str())))
            .then_with(|| a.id.cmp(&b.id))
    });
    recent.truncate(RECENT_CONNECTION_LIMIT);

    if pinned.is_empty() && recent.is_empty() {
        return grouped
            .into_iter()
            .map(|(group, items)| ConnectionSection {
                group,
                connections: order_connections_for_display(&items),
            })
            .collect();
    }

    let mut sections = Vec::new();
    if !pinned.is_empty() {
        sections.push(ConnectionSection {
            group: PINNED_GROUP_KEY.to_owned(),
            connections: order_connections_for_display(&pinned),
        });
    }
    if !recent.is_empty() {
        sections.push(ConnectionSection {
            group: RECENT_GROUP_KEY.to_owned(),
            connections: recent,
        });
    }
    for group in groups {
        let items = get(&grouped, group).cloned().unwrap_or_default();
        sections.push(ConnectionSection {
            group: group.clone(),
            connections: order_connections_for_display(&items),
        });
    }
    for (group, items) in grouped {
        if groups.iter().any(|name| name == &group) {
            continue;
        }
        if !items.is_empty() {
            sections.push(ConnectionSection {
                group,
                connections: order_connections_for_display(&items),
            });
        }
    }
    sections
}

pub fn connection_expand_key(section_group: &str, connection_id: &str) -> String {
    format!("{section_group}::{connection_id}")
}

pub fn parse_connection_expand_key(key: &str) -> ExpandKey {
    match key.find("::") {
        None => ExpandKey {
            section_group: String::new(),
            connection_id: key.to_owned(),
        },
        Some(separator) => ExpandKey {
            section_group: key[..separator].to_owned(),
            connection_id: key[separator + 2..].to_owned(),
        },
    }
}
