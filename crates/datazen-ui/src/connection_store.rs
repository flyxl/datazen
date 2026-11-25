//! `stores/connectionStore.ts` (T125) ported to Rune.
//!
//! The persisted-connections store: the connection list, the declared
//! groups, the navigator's selected group and search query, the load
//! state, and the actions that mutate the persisted set through the
//! backend's connection commands. The React store is a zustand store
//! whose actions are `async` (IPC promises awaited in chains, a
//! cross-window broadcast); Rune is immediate-mode and single-threaded,
//! so this port is a **synchronous state machine**: every React action is
//! one or more pure transitions, and every step that needs the outside
//! world (an IPC call, the cross-window emit) is returned to the host as a
//! [`ConnectionStoreEffect`] instead of being awaited here — the
//! `activeConnectionStore` (T039) precedent.
//!
//! # Host driving contract
//!
//! Every async action is a `begin_*` transition that returns the effects
//! to run now, and a `finish_*` transition that applies an IPC answer and
//! returns the *continuation* effects (React's awaited follow-up calls),
//! so a multi-step action is driven by feeding each answer back until the
//! returned effect list is empty:
//!
//! * `fetchConnections` — [`ConnectionStore::begin_fetch_connections`]
//!   sets `loading`, clears `error`, and asks the host for
//!   [`ConnectionStoreEffect::GetConnections`];
//!   [`ConnectionStore::finish_fetch_connections`] lands the list (or
//!   the failure) and marks `connections_loaded` — the first attempt,
//!   success or error, is what the flag records.
//! * `saveConnection` — [`ConnectionStore::begin_save_connection`] asks
//!   for [`ConnectionStoreEffect::SaveConnection`]; a successful save
//!   returns the refetch continuation (`GetConnections` + `GetGroups`
//!   through the same begin/finish pairs as the bare fetches, with the
//!   broadcast first — React's `emitCrossWindow` order); a failure sets
//!   `loading: false` and the error. A failing *refetch* inside the
//!   successful-save chain lands through `finish_fetch_connections`'s own
//!   failure arm — React's outer `catch` only re-writes the same fields
//!   there, so the observable state is identical.
//! * `duplicateConnection` — the same shape over
//!   [`ConnectionStoreEffect::DuplicateConnection`], with the copy's
//!   **name** derived here (`{source.name} ({t('conn.copyName')})`) and
//!   its **id supplied by the host**: React mints
//!   `conn_${Math.random().toString(36).slice(2, 10)}` at runtime, and
//!   the store stays pure (see Deliberate differences).
//! * `deleteConnection` — the same shape over
//!   [`ConnectionStoreEffect::DeleteConnection`].
//! * `testConnection` — a pure pass-through: no state changes, just
//!   [`ConnectionStoreEffect::TestConnection`] for the host to await; the
//!   `ServerInfo` answer belongs to the caller, not this store.
//! * `addGroup` / `renameGroup` / `deleteGroup` — group-list edits over
//!   [`ConnectionStoreEffect::SaveGroups`] plus, for rename/delete, the
//!   per-connection regrouping saves; the `groups` field is written when
//!   React writes it (after the `saveGroups` IPC for add/rename, after
//!   the whole chain for delete).
//! * `moveConnectionToGroup` / `toggleConnectionPinned` — one
//!   [`ConnectionStoreEffect::SaveConnection`] with the regrouped /
//!   re-pinned config, then the refetch continuation.
//!
//! [`ConnectionStoreEffect::BroadcastConnectionsChanged`] — React's
//! `emitCrossWindow(EVENT_CONNECTIONS_CHANGED)`: no Rune cross-window bus
//! exists yet (`lib/crossWindowBus.ts` is T437, segment 2), so the host
//! delivers it through whatever bus the windows share.
//!
//! # T403 placeholder (must be read before extending)
//!
//! The display helpers (`filterConnections`, `sortConnectionsInGroup`,
//! `groupConnections`, `groupConnectionsWithPinnedSection`) rank and group
//! through `lib/connectionLocator.ts`, whose real port is **T403 (segment
//! 2, pending)** — a cross-segment dependency. Following the
//! `transferMappingView` / `DataTransferTerminalResults` (T561) precedent,
//! the needed subset is carried here as pure functions transliterated
//! line for line from the TypeScript source, **with the empty usage
//! state the store passes**: only the connection's own fields are scored
//! (name, host, database, database type, the registry label) — the
//! live-session schema/table/object candidates never run from this store.
//! The `timestamp`/`compareStrings` helpers ride along. **Do not extend
//! them with new semantics here**: T403's port replaces the whole subset
//! at the seams.
//!
//! # Deliberate differences from React
//!
//! * **The duplicate id is the host's.** React mints a random
//!   `conn_`-prefixed id inside the action; the port stays pure, so
//!   [`ConnectionStore::begin_duplicate_connection`] takes the minted id
//!   as a parameter (the host owns the entropy, mirroring the
//!   `Date.now()` decisions of other state modules).
//! * **DEV `console.error` drops.** React logs
//!   `[connectionStore] fetchGroups failed` to the console and leaves the
//!   state untouched; native Rune has no console channel, and no ported
//!   state module keeps one — the failure lands through the returned
//!   `finish` arm as a no-op state change (documented per transition).
//! * **`bindConnectionStore` is the host's.** React injects the store into
//!   `@datazen/driver-sdk` for drivers that read persisted connection
//!   options. No Rune driver-sdk bridge exists yet; the host performs the
//!   equivalent binding when it lands.
//! * **`testConnection` returns nothing to the store.** React's action
//!   resolves the caller's promise; the port's effect is the host's to
//!   await and hand straight to the caller — no state moves, so nothing
//!   is modelled beyond the effect.


use datazen_ui_api::ConnectionConfig;

use crate::i18n::t;
use crate::views::db_label;

/// React `EVENT_CONNECTIONS_CHANGED` (`stores/connectionStore.ts:10`).
pub const EVENT_CONNECTIONS_CHANGED: &str = "datazen:connections-changed";

/// React `RECENT_GROUP_KEY` (`lib/connectionLocator.ts:44`): the synthetic
/// recent section key (not persisted).
#[allow(dead_code)] // T403 replaces this placeholder; the navigator consumes it
pub const RECENT_GROUP_KEY: &str = "__recent__";

/// React `PINNED_GROUP_KEY` (`lib/connectionLocator.ts:45`): the synthetic
/// pinned section key (not persisted).
#[allow(dead_code)] // T403 replaces this placeholder; the navigator consumes it
pub const PINNED_GROUP_KEY: &str = "__pinned__";

/// React `RECENT_CONNECTION_LIMIT` (`lib/connectionLocator.ts:43`): how
/// many recent connections the promoted section holds.
pub const RECENT_CONNECTION_LIMIT: usize = 5;

// ---------------------------------------------------------------------------
// T403 placeholder: the `lib/connectionLocator.ts` subset the store uses.
// ---------------------------------------------------------------------------

/// The `rankConnections` match reason, restricted to the reasons the
/// connection's own fields produce (the store's usage state is empty).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchReason {
    Name,
    Host,
    Database,
    DatabaseType,
}

impl MatchReason {
    /// The reason string `bestCandidate` sorts on.
    fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Host => "host",
            Self::Database => "database",
            Self::DatabaseType => "database-type",
        }
    }
}

/// One `rankConnections` match (`ConnectionMatch`,
/// `connectionLocator.ts:19-24`): lower rank is a stronger match.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConnectionMatch {
    rank: u32,
    reason: MatchReason,
    context: String,
}

/// `normalize` (`connectionLocator.ts:60-62`): trim + lowercase.
fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// `matchCandidate` (`connectionLocator.ts:64-77`): a case-insensitive
/// substring hit, ranked exact (offset) / prefix (offset + 1) / contains
/// (offset + 2). The context is the raw value, not the normalized one.
fn match_candidate(
    value: Option<&str>,
    query: &str,
    reason: MatchReason,
    offset: u32,
) -> Option<ConnectionMatch> {
    let raw = value.unwrap_or("");
    let normalized = normalize(raw);
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
        context: raw.to_owned(),
    })
}

/// `connectionCandidates` (`connectionLocator.ts:119-144`) restricted to
/// the connection's own fields — the empty usage state the store passes:
/// name, host, database, database type, and the registry label.
fn connection_candidates(connection: &ConnectionConfig, query: &str) -> Vec<ConnectionMatch> {
    let label = db_label(&connection.database_type);
    let mut out = Vec::new();
    for candidate in [
        match_candidate(Some(&connection.name), query, MatchReason::Name, 0),
        match_candidate(connection.host.as_deref(), query, MatchReason::Host, 10),
        match_candidate(connection.database.as_deref(), query, MatchReason::Database, 20),
        match_candidate(
            Some(&connection.database_type),
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
/// wins; ties break on reason, then on context, both compared the way
/// `localeCompare` with base sensitivity behaves for the strings the
/// navigator ranks (lowercased byte order).
fn best_candidate(mut candidates: Vec<ConnectionMatch>) -> Option<ConnectionMatch> {
    candidates.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.reason.as_str().cmp(b.reason.as_str()))
            .then_with(|| a.context.to_lowercase().cmp(&b.context.to_lowercase()))
    });
    candidates.into_iter().next()
}

/// `timestamp` (`connectionLocator.ts:207-211`): `Date.parse` with an
/// invalid date as `None`. Only ISO-8601 shapes are accepted, the same
/// subset the Backup window's locator port accepts (see
/// `views/backup/backup_grouping.rs`, the shared precedent).
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

/// `compareStrings` (`connectionLocator.ts:214-216`): `localeCompare`
/// with base sensitivity, approximated by lowercased byte order (the
/// Backup locator's documented approximation).
fn compare_base(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// `compareResults` (`connectionLocator.ts:219-242`): match rank (no
/// match sorts last), pinned first, `lastConnectedAt` descending with
/// nulls last, then group, name, and id.
fn compare_results(
    a: (&ConnectionConfig, Option<u32>),
    b: (&ConnectionConfig, Option<u32>),
) -> std::cmp::Ordering {
    let a_rank = a.1.unwrap_or(u32::MAX);
    let b_rank = b.1.unwrap_or(u32::MAX);
    if a_rank != b_rank {
        return a_rank.cmp(&b_rank);
    }
    let a_pinned = a.0.pinned.unwrap_or(false);
    let b_pinned = b.0.pinned.unwrap_or(false);
    if a_pinned != b_pinned {
        return b_pinned.cmp(&a_pinned);
    }
    let a_time = a.0.last_connected_at.as_deref().and_then(parse_iso_timestamp);
    let b_time = b.0.last_connected_at.as_deref().and_then(parse_iso_timestamp);
    if a_time != b_time {
        return match (a_time, b_time) {
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(x), Some(y)) => y.cmp(&x),
            (None, None) => std::cmp::Ordering::Equal,
        };
    }
    let group = compare_base(
        a.0.group.as_deref().unwrap_or(""),
        b.0.group.as_deref().unwrap_or(""),
    );
    if group != std::cmp::Ordering::Equal {
        return group;
    }
    let name = compare_base(&a.0.name, &b.0.name);
    if name != std::cmp::Ordering::Equal {
        return name;
    }
    a.0.id.cmp(&b.0.id)
}

/// `rankConnections` (`connectionLocator.ts:244-262`) with the store's
/// empty usage state: dedupe by id (first wins), keep only matches for a
/// non-empty query, and sort by [`compare_results`]. An empty query
/// returns every connection unranked, still sorted.
fn rank_connections(connections: &[ConnectionConfig], query: &str) -> Vec<ConnectionConfig> {
    let normalized = normalize(query);
    let mut unique: Vec<&ConnectionConfig> = Vec::new();
    for connection in connections {
        if !unique.iter().any(|seen| seen.id == connection.id) {
            unique.push(connection);
        }
    }
    let mut ranked: Vec<(&ConnectionConfig, Option<u32>)> = unique
        .into_iter()
        .map(|connection| {
            let matched = if normalized.is_empty() {
                None
            } else {
                best_candidate(connection_candidates(connection, &normalized)).map(|m| m.rank)
            };
            (connection, matched)
        })
        .filter(|(_, matched)| normalized.is_empty() || matched.is_some())
        .collect();
    ranked.sort_by(|a, b| compare_results((a.0, a.1), (b.0, b.1)));
    ranked.into_iter().map(|(connection, _)| connection.clone()).collect()
}

/// React `orderConnectionsForDisplay` (`connectionLocator.ts:264-268`):
/// pinned first, stable — the persisted order is kept otherwise.
fn order_connections_for_display(connections: &[ConnectionConfig]) -> Vec<ConnectionConfig> {
    let pinned: Vec<ConnectionConfig> = connections
        .iter()
        .filter(|c| c.pinned.unwrap_or(false))
        .cloned()
        .collect();
    if pinned.is_empty() {
        return connections.to_vec();
    }
    let mut out = pinned;
    out.extend(
        connections
            .iter()
            .filter(|c| !c.pinned.unwrap_or(false))
            .cloned(),
    );
    out
}

/// One `groupConnections` / `groupConnectionsWithRecentSections` result
/// row: a group key and its connections.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConnectionGroupSection {
    pub group: String,
    pub connections: Vec<ConnectionConfig>,
}

/// `groupConnectionsWithRecentSections`
/// (`connectionLocator.ts:270-324`): the persisted grouping with the
/// pinned and recent sections hoisted on top (pinned first, then the
/// `RECENT_CONNECTION_LIMIT` most recent, then every declared group in
/// order, then extra non-empty groups in encounter order). Without any
/// promoted connection it is the plain grouping with
/// [`order_connections_for_display`] per section.
fn group_connections_with_recent_sections(
    connections: &[ConnectionConfig],
    groups: &[String],
) -> Vec<ConnectionGroupSection> {
    // `grouped`: declared groups first (even when empty), then groups met
    // while bucketing — React's `Map` insertion order.
    let mut grouped: Vec<(String, Vec<ConnectionConfig>)> = Vec::new();
    for group in groups {
        if !grouped.iter().any(|(key, _)| key == group) {
            grouped.push((group.clone(), Vec::new()));
        }
    }
    for connection in connections {
        let key = connection.group.clone().unwrap_or_default();
        match grouped.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(connection.clone()),
            None => grouped.push((key, vec![connection.clone()])),
        }
    }

    let pinned: Vec<ConnectionConfig> = connections
        .iter()
        .filter(|c| c.pinned.unwrap_or(false))
        .cloned()
        .collect();
    let mut recent: Vec<ConnectionConfig> = connections
        .iter()
        .filter(|c| c.last_connected_at.as_deref().is_some_and(|v| parse_iso_timestamp(v).is_some()))
        .cloned()
        .collect();
    recent.sort_by(|a, b| {
        let a_time = a.last_connected_at.as_deref().and_then(parse_iso_timestamp).unwrap_or(0);
        let b_time = b.last_connected_at.as_deref().and_then(parse_iso_timestamp).unwrap_or(0);
        if a_time != b_time {
            return b_time.cmp(&a_time);
        }
        match compare_base(&a.name, &b.name) {
            std::cmp::Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        }
    });
    recent.truncate(RECENT_CONNECTION_LIMIT);

    let has_promoted = !pinned.is_empty() || !recent.is_empty();
    if !has_promoted {
        return grouped
            .into_iter()
            .map(|(group, section)| ConnectionGroupSection {
                group,
                connections: order_connections_for_display(&section),
            })
            .collect();
    }

    let mut sections = Vec::new();
    if !pinned.is_empty() {
        sections.push(ConnectionGroupSection {
            group: PINNED_GROUP_KEY.to_owned(),
            connections: order_connections_for_display(&pinned),
        });
    }
    if !recent.is_empty() {
        sections.push(ConnectionGroupSection {
            group: RECENT_GROUP_KEY.to_owned(),
            connections: recent,
        });
    }
    for group in groups {
        let section = grouped
            .iter()
            .find(|(key, _)| key == group)
            .map(|(_, list)| list.clone())
            .unwrap_or_default();
        sections.push(ConnectionGroupSection {
            group: group.clone(),
            connections: order_connections_for_display(&section),
        });
    }
    for (group, section) in &grouped {
        if groups.contains(group) || section.is_empty() {
            continue;
        }
        sections.push(ConnectionGroupSection {
            group: group.clone(),
            connections: order_connections_for_display(section),
        });
    }
    sections
}

// ---------------------------------------------------------------------------
// The store's own pure display helpers (the module's public API).
// ---------------------------------------------------------------------------

/// React `filterConnections` (`stores/connectionStore.ts:12-21`): the
/// selected group's connections (all when no group is selected), ranked
/// by the search query.
#[allow(dead_code)] // reusable store; wired by the connection navigator views
pub fn filter_connections(
    connections: &[ConnectionConfig],
    selected_group: Option<&str>,
    search_query: &str,
) -> Vec<ConnectionConfig> {
    let selected: Vec<ConnectionConfig> = match selected_group {
        Some(group) => connections
            .iter()
            .filter(|c| c.group.as_deref() == Some(group))
            .cloned()
            .collect(),
        None => connections.to_vec(),
    };
    rank_connections(&selected, search_query)
}

/// React `sortConnectionsInGroup` (`stores/connectionStore.ts:30-33`):
/// pinned first, then the persisted order.
#[allow(dead_code)] // reusable store; wired by the connection navigator views
pub fn sort_connections_in_group(connections: &[ConnectionConfig]) -> Vec<ConnectionConfig> {
    order_connections_for_display(connections)
}

/// React `groupConnectionsWithPinnedSection`
/// (`stores/connectionStore.ts:35-40`): while searching, the plain
/// grouping; otherwise the grouping with pinned and recent sections.
#[allow(dead_code)] // reusable store; wired by the connection navigator views
pub fn group_connections_with_pinned_section(
    connections: &[ConnectionConfig],
    groups: &[String],
    search_query: &str,
) -> Vec<ConnectionGroupSection> {
    if !search_query.trim().is_empty() {
        return group_connections(connections, groups, search_query);
    }
    group_connections_with_recent_sections(connections, groups)
}

/// React `groupConnections` (`stores/connectionStore.ts:45-77`): group by
/// the `group` field; declared groups come first in `groups` order (even
/// when empty), then extra non-empty groups in encounter order, and the
/// ungrouped (`''`) group last when non-empty. While searching, the ranked
/// order is kept; otherwise each group is pinned-first.
#[allow(dead_code)] // reusable store; wired by the connection navigator views
pub fn group_connections(
    connections: &[ConnectionConfig],
    groups: &[String],
    search_query: &str,
) -> Vec<ConnectionGroupSection> {
    let searching = !search_query.trim().is_empty();
    let filtered: Vec<ConnectionConfig> = if searching {
        rank_connections(connections, search_query)
    } else {
        connections.to_vec()
    };
    let mut map: Vec<(String, Vec<ConnectionConfig>)> = Vec::new();
    for group in groups {
        if !map.iter().any(|(key, _)| key == group) {
            map.push((group.clone(), Vec::new()));
        }
    }
    for connection in filtered {
        let key = connection.group.clone().unwrap_or_default();
        match map.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(connection),
            None => map.push((key, vec![connection])),
        }
    }
    if !searching {
        for (_, list) in map.iter_mut() {
            let ordered = sort_connections_in_group(list);
            *list = ordered;
        }
    }
    let mut result = Vec::new();
    let seen: Vec<&str> = groups.iter().map(String::as_str).collect();
    for group in groups {
        let conns = map
            .iter()
            .find(|(key, _)| key == group)
            .map(|(_, list)| list.clone())
            .unwrap_or_default();
        result.push(ConnectionGroupSection {
            group: group.clone(),
            connections: conns,
        });
    }
    for (key, conns) in &map {
        if !key.is_empty() && !seen.contains(&key.as_str()) && !conns.is_empty() {
            result.push(ConnectionGroupSection {
                group: key.clone(),
                connections: conns.clone(),
            });
        }
    }
    if let Some((_, ungrouped)) = map.iter().find(|(key, _)| key.is_empty()) {
        if !ungrouped.is_empty() {
            result.push(ConnectionGroupSection {
                group: String::new(),
                connections: ungrouped.clone(),
            });
        }
    }
    result
}

// ---------------------------------------------------------------------------
// The store's state machine.
// ---------------------------------------------------------------------------

/// One host-run step. Every async action's IPC await becomes one of
/// these; the host applies them in order, feeding each answer back
/// through the matching `finish_*` transition, and keeps going until the
/// returned continuation is empty.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable store; wired by the connection navigator views
pub enum ConnectionStoreEffect {
    /// `connectionCommands.getConnections()` — feed the list (or the error
    /// message) back through `finish_fetch_connections`.
    GetConnections,
    /// `connectionCommands.getGroups()` — feed the list back through
    /// `finish_fetch_groups`.
    GetGroups,
    /// `connectionCommands.saveConnection(config, clear_missing)`.
    SaveConnection {
        config: ConnectionConfig,
        clear_missing_credentials: bool,
    },
    /// `connectionCommands.duplicateConnection(id, new_id, new_name)`.
    DuplicateConnection {
        id: String,
        new_id: String,
        new_name: String,
    },
    /// `connectionCommands.deleteConnection(id)`.
    DeleteConnection { id: String },
    /// `connectionCommands.testConnection(config)` — the host awaits it
    /// and answers the *caller*; no store state moves.
    TestConnection { config: ConnectionConfig },
    /// `connectionCommands.saveGroups(groups)`.
    SaveGroups { groups: Vec<String> },
    /// `emitCrossWindow(EVENT_CONNECTIONS_CHANGED)` — the host delivers it
    /// through whatever bus the windows share (`lib/crossWindowBus.ts` is
    /// T437, segment 2).
    BroadcastConnectionsChanged,
}

/// The store's state: the React `ConnectionStore` data fields (the
/// actions are the transitions below).
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)] // reusable store; wired by the connection navigator views
pub struct ConnectionStore {
    pub connections: Vec<ConnectionConfig>,
    pub groups: Vec<String>,
    pub selected_group: Option<String>,
    pub search_query: String,
    pub loading: bool,
    /// True after the first `fetchConnections` attempt completes (success
    /// or error).
    pub connections_loaded: bool,
    pub error: Option<String>,
}

#[allow(dead_code)] // reusable store; wired by the connection navigator views
impl ConnectionStore {
    /// React's `fetchConnections` preamble: `set({ loading: true, error:
    /// null })`, then the `getConnections` IPC.
    pub fn begin_fetch_connections(&mut self) -> Vec<ConnectionStoreEffect> {
        self.loading = true;
        self.error = None;
        vec![ConnectionStoreEffect::GetConnections]
    }

    /// React's `fetchConnections` settlement: the list lands with
    /// `loading: false` and `connectionsLoaded: true`; an error keeps the
    /// old list, marks the attempt, and records the message — the host's
    /// message when there is one, else `t('connStore.loadFailed')`.
    pub fn finish_fetch_connections(
        &mut self,
        result: Result<Vec<ConnectionConfig>, Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        match result {
            Ok(connections) => {
                self.connections = connections;
                self.loading = false;
                self.connections_loaded = true;
            }
            Err(message) => {
                self.loading = false;
                self.connections_loaded = true;
                self.error = Some(message.unwrap_or_else(|| t("connStore.loadFailed").to_owned()));
            }
        }
        Vec::new()
    }

    /// React's `fetchGroups`: no state moves first, just the `getGroups`
    /// IPC. A failure is React's `console.error` — no state change at
    /// all (see Deliberate differences).
    pub fn begin_fetch_groups(&mut self) -> Vec<ConnectionStoreEffect> {
        vec![ConnectionStoreEffect::GetGroups]
    }

    /// React's `fetchGroups` settlement: the groups land, or nothing
    /// moves on failure.
    pub fn finish_fetch_groups(
        &mut self,
        result: Result<Vec<String>, Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        if let Ok(groups) = result {
            self.groups = groups;
        }
        Vec::new()
    }

    /// React's `saveConnection(config, clearMissingCredentials)`
    /// preamble: `set({ loading: true, error: null })`, then the save IPC.
    pub fn begin_save_connection(
        &mut self,
        config: &ConnectionConfig,
        clear_missing_credentials: bool,
    ) -> Vec<ConnectionStoreEffect> {
        self.loading = true;
        self.error = None;
        vec![ConnectionStoreEffect::SaveConnection {
            config: config.clone(),
            clear_missing_credentials,
        }]
    }

    /// React's `saveConnection` settlement. A success keeps `loading`
    /// raised (the refetch chain ends it) and returns the continuation in
    /// React's order — the connections refetch, the groups refetch, then
    /// the `void emitCrossWindow` fired after both awaits resolve. A
    /// failure sets `loading: false` and the error (the host's message
    /// or `t('connStore.saveFailed')`).
    pub fn finish_save_connection(
        &mut self,
        result: Result<(), Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        match result {
            Ok(()) => vec![
                ConnectionStoreEffect::GetConnections,
                ConnectionStoreEffect::GetGroups,
                ConnectionStoreEffect::BroadcastConnectionsChanged,
            ],
            Err(message) => {
                self.loading = false;
                self.error = Some(message.unwrap_or_else(|| t("connStore.saveFailed").to_owned()));
                Vec::new()
            }
        }
    }

    /// React's `duplicateConnection(id)`: a missing source is a silent
    /// no-op (nothing moves, nothing is returned). The copy's name is
    /// derived here — `{source.name} ({t('conn.copyName')})` — and its id
    /// is the host's (React mints a random one; see Deliberate
    /// differences).
    pub fn begin_duplicate_connection(
        &mut self,
        id: &str,
        new_id: &str,
    ) -> Vec<ConnectionStoreEffect> {
        let Some(source) = self.connections.iter().find(|c| c.id == id) else {
            return Vec::new();
        };
        let new_name = format!("{} ({})", source.name, t("conn.copyName"));
        self.loading = true;
        self.error = None;
        vec![ConnectionStoreEffect::DuplicateConnection {
            id: id.to_owned(),
            new_id: new_id.to_owned(),
            new_name,
        }]
    }

    /// React's `duplicateConnection` settlement: the save-shaped
    /// continuation and failure arms (`t('connStore.saveFailed')` is
    /// React's key here too).
    pub fn finish_duplicate_connection(
        &mut self,
        result: Result<(), Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        self.finish_save_connection(result)
    }

    /// React's `deleteConnection(id)` preamble.
    pub fn begin_delete_connection(&mut self, id: &str) -> Vec<ConnectionStoreEffect> {
        self.loading = true;
        self.error = None;
        vec![ConnectionStoreEffect::DeleteConnection {
            id: id.to_owned(),
        }]
    }

    /// React's `deleteConnection` settlement: the save-shaped
    /// continuation; `t('connStore.deleteFailed')` on failure.
    pub fn finish_delete_connection(
        &mut self,
        result: Result<(), Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        match result {
            Ok(()) => vec![
                ConnectionStoreEffect::GetConnections,
                ConnectionStoreEffect::GetGroups,
                ConnectionStoreEffect::BroadcastConnectionsChanged,
            ],
            Err(message) => {
                self.loading = false;
                self.error = Some(message.unwrap_or_else(|| t("connStore.deleteFailed").to_owned()));
                Vec::new()
            }
        }
    }

    /// React's `testConnection(config)`: a pure pass-through — no state
    /// changes, just the effect the host awaits for the caller. The
    /// `ServerInfo` answer belongs to the caller, not this store.
    pub fn test_connection(&mut self, config: &ConnectionConfig) -> Vec<ConnectionStoreEffect> {
        vec![ConnectionStoreEffect::TestConnection {
            config: config.clone(),
        }]
    }

    /// React's `addGroup(name)`: a blank or already-declared name is a
    /// silent no-op; otherwise the `saveGroups` IPC with the appended
    /// list. The `groups` field lands with the IPC's success.
    pub fn begin_add_group(&mut self, name: &str) -> Vec<ConnectionStoreEffect> {
        let trimmed = name.trim();
        if trimmed.is_empty() || self.groups.iter().any(|g| g == trimmed) {
            return Vec::new();
        }
        let mut updated = self.groups.clone();
        updated.push(trimmed.to_owned());
        vec![ConnectionStoreEffect::SaveGroups { groups: updated }]
    }

    /// React's `addGroup` settlement: the appended list lands after the
    /// IPC (`set({ groups: updated })` follows the await). A failure is
    /// React's unhandled rejection — nothing moves here.
    pub fn finish_add_group(
        &mut self,
        result: Result<Vec<String>, Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        if let Ok(updated) = result {
            self.groups = updated;
        }
        Vec::new()
    }

    /// React's `renameGroup(oldName, newName)`: a blank name or one equal
    /// to the old name is a silent no-op; otherwise the `saveGroups` IPC
    /// with the renamed list. The name stays available for the
    /// settlement through `finish_rename_group`.
    pub fn begin_rename_group(
        &mut self,
        old_name: &str,
        new_name: &str,
    ) -> Vec<ConnectionStoreEffect> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() || trimmed == old_name {
            return Vec::new();
        }
        // `[...new Set(groups.map(...))]`: rename every occurrence, then
        // keep each result's first occurrence.
        let mut updated: Vec<String> = Vec::new();
        for group in &self.groups {
            let renamed = if group == old_name {
                trimmed.to_owned()
            } else {
                group.clone()
            };
            if !updated.contains(&renamed) {
                updated.push(renamed);
            }
        }
        vec![ConnectionStoreEffect::SaveGroups { groups: updated }]
    }

    /// React's `renameGroup` settlement: after the successful save, every
    /// member connection is re-saved with the new group (one
    /// [`ConnectionStoreEffect::SaveConnection`] each, in list order —
    /// React awaits each in turn, failures unhandled), then the
    /// connections refetch, and the renamed group list lands. A failed
    /// `saveGroups` is React's unhandled rejection: nothing moves.
    pub fn finish_rename_group(
        &mut self,
        result: Result<Vec<String>, Option<String>>,
        old_name: &str,
        new_name: &str,
    ) -> Vec<ConnectionStoreEffect> {
        let Ok(updated) = result else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        for connection in self.connections.clone() {
            if connection.group.as_deref() == Some(old_name) {
                let mut renamed = connection;
                renamed.group = Some(new_name.to_owned());
                effects.push(ConnectionStoreEffect::SaveConnection {
                    config: renamed,
                    clear_missing_credentials: false,
                });
            }
        }
        effects.push(ConnectionStoreEffect::GetConnections);
        self.groups = updated;
        effects
    }

    /// React's `deleteGroup(name)`: the `saveGroups` IPC with the name
    /// removed (the name is kept for the settlement, which needs it for
    /// the member re-saves).
    pub fn begin_delete_group(&mut self, name: &str) -> Vec<ConnectionStoreEffect> {
        let updated: Vec<String> = self
            .groups
            .iter()
            .filter(|group| group != &name)
            .cloned()
            .collect();
        vec![ConnectionStoreEffect::SaveGroups { groups: updated }]
    }

    /// React's `deleteGroup` settlement: after the successful save every
    /// member connection is re-saved with `group: undefined`, then the
    /// connections refetch; the groups land and the selected group resets
    /// to null (React's `set({ groups: updated, selectedGroup: null })`
    /// after the chain). A failed `saveGroups` leaves everything
    /// untouched.
    pub fn finish_delete_group(
        &mut self,
        result: Result<Vec<String>, Option<String>>,
        deleted_name: &str,
    ) -> Vec<ConnectionStoreEffect> {
        let Ok(updated) = result else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        for connection in self.connections.clone() {
            if connection.group.as_deref() == Some(deleted_name) {
                let mut ungrouped = connection;
                ungrouped.group = None;
                effects.push(ConnectionStoreEffect::SaveConnection {
                    config: ungrouped,
                    clear_missing_credentials: false,
                });
            }
        }
        effects.push(ConnectionStoreEffect::GetConnections);
        self.groups = updated;
        self.selected_group = None;
        effects
    }

    /// React's `moveConnectionToGroup(connectionId, group)`: a missing
    /// connection is a silent no-op; otherwise one save with the new
    /// group (`None` for React's `undefined`), then the refetch and the
    /// broadcast.
    pub fn begin_move_connection_to_group(
        &mut self,
        connection_id: &str,
        group: Option<&str>,
    ) -> Vec<ConnectionStoreEffect> {
        let Some(conn) = self.connections.iter().find(|c| c.id == connection_id) else {
            return Vec::new();
        };
        let mut moved = conn.clone();
        moved.group = group.map(str::to_owned);
        self.loading = true;
        self.error = None;
        vec![ConnectionStoreEffect::SaveConnection {
            config: moved,
            clear_missing_credentials: false,
        }]
    }

    /// React's `moveConnectionToGroup` settlement: the refetch and the
    /// broadcast (no groups refetch — React's chain skips it);
    /// `t('connStore.saveFailed')` on failure.
    pub fn finish_move_connection_to_group(
        &mut self,
        result: Result<(), Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        match result {
            Ok(()) => vec![
                ConnectionStoreEffect::GetConnections,
                ConnectionStoreEffect::BroadcastConnectionsChanged,
            ],
            Err(message) => {
                self.loading = false;
                self.error = Some(message.unwrap_or_else(|| t("connStore.saveFailed").to_owned()));
                Vec::new()
            }
        }
    }

    /// React's `toggleConnectionPinned(connectionId)`: a missing
    /// connection is a silent no-op; otherwise one save with the pin
    /// flipped, then the refetch and the broadcast.
    pub fn begin_toggle_connection_pinned(
        &mut self,
        connection_id: &str,
    ) -> Vec<ConnectionStoreEffect> {
        let Some(conn) = self.connections.iter().find(|c| c.id == connection_id) else {
            return Vec::new();
        };
        let mut flipped = conn.clone();
        flipped.pinned = Some(!conn.pinned.unwrap_or(false));
        self.loading = true;
        self.error = None;
        vec![ConnectionStoreEffect::SaveConnection {
            config: flipped,
            clear_missing_credentials: false,
        }]
    }

    /// React's `toggleConnectionPinned` settlement: the move/toggle
    /// continuation (refetch + broadcast, no groups refetch).
    pub fn finish_toggle_connection_pinned(
        &mut self,
        result: Result<(), Option<String>>,
    ) -> Vec<ConnectionStoreEffect> {
        self.finish_move_connection_to_group(result)
    }

    /// React's `setSelectedGroup`.
    pub fn set_selected_group(&mut self, group: Option<&str>) {
        self.selected_group = group.map(str::to_owned);
    }

    /// React's `setSearchQuery`.
    pub fn set_search_query(&mut self, query: &str) {
        self.search_query = query.to_owned();
    }
}
