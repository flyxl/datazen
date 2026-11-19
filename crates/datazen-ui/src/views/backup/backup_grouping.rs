//! T026-S1: connection grouping and search ranking for the Backup window.
//!
//! Mirrors `groupConnections` (`stores/connectionStore.ts`) and
//! `rankConnections` (`lib/connectionLocator.ts`, with the empty usage
//! state the Backup window passes), plus `formatGroupLabel`
//! (`lib/connectionGroups.ts`). See `backup_window_state` for the
//! S1/S2 boundary and the documented deviations.

// The production consumer is the Backup window (T026-S2); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::i18n::t;

use super::backup_db_meta::db_label;
use super::backup_window_state::BackupConnection;

/// One `groupConnections` result row: a group key and its connections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConnectionGroup {
    pub group: String,
    pub connections: Vec<BackupConnection>,
}

/// The `rankConnections` match reason (`connectionLocator.ts:6-17`),
/// restricted to the reasons that occur with empty usage state (see
/// the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MatchReason {
    Name,
    Host,
    Database,
    DatabaseType,
}

impl MatchReason {
    /// The reason string `bestCandidate` sorts on.
    fn as_str(self) -> &'static str {
        match self {
            MatchReason::Name => "name",
            MatchReason::Host => "host",
            MatchReason::Database => "database",
            MatchReason::DatabaseType => "database-type",
        }
    }
}

/// One `rankConnections` match (`ConnectionMatch`, `connectionLocator.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectionMatch {
    pub rank: u32,
    pub reason: MatchReason,
    pub context: String,
}

/// React `formatGroupLabel` (`lib/connectionGroups.ts`): the three
/// preset keys localize, any other key is returned as-is.
pub(crate) fn format_group_label(key: &str) -> String {
    match key {
        "preset:production" => t("newConn.groupProd").to_owned(),
        "preset:development" => t("newConn.groupDev").to_owned(),
        "preset:testing" => t("newConn.groupTest").to_owned(),
        _ => key.to_owned(),
    }
}

/// React's group-header label: `''` is `t('main.ungrouped')`, anything
/// else goes through [`format_group_label`].
pub(crate) fn group_display_label(group: &str) -> String {
    if group.is_empty() {
        t("main.ungrouped").to_owned()
    } else {
        format_group_label(group)
    }
}

/// React's initial `expandedGroups`: every declared group, the
/// ungrouped key, and every connection's group — everything expanded.
pub(crate) fn initial_expanded_groups(
    groups: &[String],
    connections: &[BackupConnection],
) -> BTreeSet<String> {
    let mut expanded = BTreeSet::new();
    for group in groups {
        expanded.insert(group.clone());
    }
    expanded.insert(String::new());
    for conn in connections {
        expanded.insert(conn.group.clone().unwrap_or_default());
    }
    expanded
}

/// `normalize` (`connectionLocator.ts:60-62`): trim + lowercase.
fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// `matchCandidate` (`connectionLocator.ts:64-77`): a case-insensitive
/// substring hit, ranked exact (offset) / prefix (offset + 1) /
/// contains (offset + 2). The context is the raw value, not the
/// normalized one.
fn match_candidate(
    value: Option<&str>,
    query: &str,
    reason: MatchReason,
    offset: u32,
) -> Option<ConnectionMatch> {
    let normalized = normalize(value.unwrap_or(""));
    if normalized.is_empty() || !normalized.contains(query) {
        return None;
    }
    let rank = if normalized == query {
        offset
    } else if normalized.starts_with(query) {
        offset + 1
    } else {
        offset + 2
    };
    Some(ConnectionMatch {
        rank,
        reason,
        context: value.unwrap_or("").to_owned(),
    })
}

/// `connectionCandidates` (`connectionLocator.ts:119-144`) with the
/// empty usage state the Backup window passes: only the connection's
/// own fields are scored — name, host, database, database type, and
/// the type label.
fn connection_candidates(conn: &BackupConnection, query: &str) -> Vec<ConnectionMatch> {
    let mut out = Vec::new();
    let label = db_label(&conn.database_type);
    for candidate in [
        match_candidate(Some(&conn.name), query, MatchReason::Name, 0),
        match_candidate(conn.host.as_deref(), query, MatchReason::Host, 10),
        match_candidate(conn.database.as_deref(), query, MatchReason::Database, 20),
        match_candidate(
            Some(&conn.database_type),
            query,
            MatchReason::DatabaseType,
            30,
        ),
        match_candidate(Some(&label), query, MatchReason::DatabaseType, 30),
    ] {
        if let Some(candidate) = candidate {
            out.push(candidate);
        }
    }
    out
}

/// `bestCandidate` (`connectionLocator.ts:195-205`): the lowest rank
/// wins; ties break on reason, then on context (both compared
/// case-insensitively, mirroring `localeCompare` with base
/// sensitivity — see the module docs).
pub(crate) fn connection_match(conn: &BackupConnection, query: &str) -> Option<ConnectionMatch> {
    let normalized = normalize(query);
    if normalized.is_empty() {
        return None;
    }
    let mut candidates = connection_candidates(conn, &normalized);
    candidates.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.reason.as_str().cmp(b.reason.as_str()))
            .then_with(|| a.context.to_lowercase().cmp(&b.context.to_lowercase()))
    });
    candidates.into_iter().next()
}

/// `compareStrings` (`connectionLocator.ts:214-216`): `localeCompare`
/// with base sensitivity, approximated by lowercased byte order.
fn compare_base(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// `timestamp` (`connectionLocator.ts:207-211`): `Date.parse`, with an
/// invalid date as `None`. Only ISO-8601 shapes are accepted here —
/// see the module docs.
fn parse_iso_timestamp(value: &str) -> Option<i64> {
    fn digits(text: &str) -> Option<i64> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse::<i64>().ok()
    }
    fn days_in_month(year: i64, month: i64) -> i64 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        }
    }
    fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (month + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    let bytes = value.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let year = digits(value.get(0..4)?)?;
    if value.get(4..5)? != "-" {
        return None;
    }
    let month = digits(value.get(5..7)?)?;
    if value.get(7..8)? != "-" {
        return None;
    }
    let day = digits(value.get(8..10)?)?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    let mut seconds = days_from_civil(year, month, day) * 86_400;
    if bytes.len() == 10 {
        return Some(seconds);
    }
    if value.get(10..11)? != "T" || bytes.len() < 19 {
        return None;
    }
    let hour = digits(value.get(11..13)?)?;
    if value.get(13..14)? != ":" {
        return None;
    }
    let minute = digits(value.get(14..16)?)?;
    if value.get(16..17)? != ":" {
        return None;
    }
    let second = digits(value.get(17..19)?)?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    seconds += hour * 3_600 + minute * 60 + second;
    let mut rest = value.get(19..)?;
    if let Some(frac) = rest.strip_prefix('.') {
        let end = frac.find(|c: char| !c.is_ascii_digit())?;
        if end == 0 {
            return None;
        }
        rest = frac.get(end..)?;
    }
    match rest {
        "Z" => Some(seconds),
        _ => {
            let sign = match rest.get(0..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let off_hour = digits(rest.get(1..3)?)?;
            if rest.get(3..4)? != ":" {
                return None;
            }
            let off_minute = digits(rest.get(4..6)?)?;
            if off_hour > 23 || off_minute > 59 || rest.len() != 6 {
                return None;
            }
            Some(seconds - sign * (off_hour * 3_600 + off_minute * 60))
        }
    }
}

/// `compareResults` (`connectionLocator.ts:219-242`): match rank
/// (`null` sorts last), pinned first, `lastConnectedAt` descending
/// with nulls last, then group, name, and id.
fn compare_ranked(
    a: &(BackupConnection, Option<ConnectionMatch>),
    b: &(BackupConnection, Option<ConnectionMatch>),
) -> Ordering {
    let a_rank = a.1.as_ref().map_or(u32::MAX, |m| m.rank);
    let b_rank = b.1.as_ref().map_or(u32::MAX, |m| m.rank);
    if a_rank != b_rank {
        return a_rank.cmp(&b_rank);
    }
    if a.0.pinned != b.0.pinned {
        return b.0.pinned.cmp(&a.0.pinned);
    }
    let a_time =
        a.0.last_connected_at
            .as_deref()
            .and_then(parse_iso_timestamp);
    let b_time =
        b.0.last_connected_at
            .as_deref()
            .and_then(parse_iso_timestamp);
    if a_time != b_time {
        return match (a_time, b_time) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => b.cmp(&a),
            (None, None) => Ordering::Equal,
        };
    }
    let group = compare_base(
        a.0.group.as_deref().unwrap_or(""),
        b.0.group.as_deref().unwrap_or(""),
    );
    if group != Ordering::Equal {
        return group;
    }
    let name = compare_base(&a.0.name, &b.0.name);
    if name != Ordering::Equal {
        return name;
    }
    a.0.id.cmp(&b.0.id)
}

/// `rankConnections` (`connectionLocator.ts:244-262`): dedupe by id
/// (first wins), keep only matches for a non-empty query, and sort by
/// [`compare_ranked`]. An empty query returns every connection with a
/// null match, still sorted.
pub(crate) fn rank_connections(
    connections: &[BackupConnection],
    query: &str,
) -> Vec<BackupConnection> {
    let normalized = normalize(query);
    let mut unique: Vec<&BackupConnection> = Vec::new();
    for conn in connections {
        if !unique.iter().any(|seen| seen.id == conn.id) {
            unique.push(conn);
        }
    }
    let mut ranked: Vec<(BackupConnection, Option<ConnectionMatch>)> = unique
        .into_iter()
        .map(|conn| {
            let matched = if normalized.is_empty() {
                None
            } else {
                connection_match(conn, &normalized)
            };
            (conn.clone(), matched)
        })
        .filter(|(_, matched)| normalized.is_empty() || matched.is_some())
        .collect();
    ranked.sort_by(compare_ranked);
    ranked.into_iter().map(|(conn, _)| conn).collect()
}

/// `sortConnectionsInGroup` (`connectionStore.ts:30-33`): pinned
/// first, stable — the persisted order is kept otherwise.
fn sort_connections_in_group(connections: &mut [BackupConnection]) {
    connections.sort_by(|a, b| b.pinned.cmp(&a.pinned));
}

/// `groupConnections` (`connectionStore.ts:45-77`): group by the
/// `group` field; declared groups come first in `groups` order (even
/// when empty), then extra non-empty groups in encounter order, and
/// the ungrouped (`''`) group last when non-empty. While searching,
/// the ranked order is kept; otherwise each group is pinned-first.
pub(crate) fn group_connections(
    connections: &[BackupConnection],
    groups: &[String],
    search_query: &str,
) -> Vec<ConnectionGroup> {
    let searching = !search_query.trim().is_empty();
    let filtered: Vec<BackupConnection> = if searching {
        rank_connections(connections, search_query)
    } else {
        connections.to_vec()
    };
    // Insertion-ordered map: declared groups first, then groups met
    // while bucketing (React's `Map` has the same order).
    let mut map: Vec<(String, Vec<BackupConnection>)> = Vec::new();
    for group in groups {
        if !map.iter().any(|(key, _)| key == group) {
            map.push((group.clone(), Vec::new()));
        }
    }
    for conn in filtered {
        let key = conn.group.clone().unwrap_or_default();
        match map.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(conn),
            None => map.push((key, vec![conn])),
        }
    }
    if !searching {
        for (_, list) in map.iter_mut() {
            sort_connections_in_group(list);
        }
    }
    let mut result = Vec::new();
    let seen: BTreeSet<&str> = groups.iter().map(String::as_str).collect();
    for group in groups {
        let conns = map
            .iter()
            .find(|(key, _)| key == group)
            .map(|(_, list)| list.clone())
            .unwrap_or_default();
        result.push(ConnectionGroup {
            group: group.clone(),
            connections: conns,
        });
    }
    for (key, conns) in &map {
        if !key.is_empty() && !seen.contains(key.as_str()) && !conns.is_empty() {
            result.push(ConnectionGroup {
                group: key.clone(),
                connections: conns.clone(),
            });
        }
    }
    if let Some((_, ungrouped)) = map.iter().find(|(key, _)| key.is_empty()) {
        if !ungrouped.is_empty() {
            result.push(ConnectionGroup {
                group: String::new(),
                connections: ungrouped.clone(),
            });
        }
    }
    result
}
