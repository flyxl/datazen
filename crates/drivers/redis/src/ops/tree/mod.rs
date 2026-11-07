//! Hierarchical key browsing for Redis.
//!
//! `list_children` splits SCAN results by separator to produce a flat list
//! of direct children (leaf keys + virtual folder nodes) for one tree level.
//!
//! The scan itself, the per-page batches and the COUNT ledger live in
//! [`scan`]; this module owns only the separator splitting and the
//! `ChildEntry` shape. Splitting stays a pure function ([`split_children`])
//! because it is the whole contract of a tree level and is tested without a
//! connection.

pub mod budget;
pub mod scan;

use datazen_driver_api::DriverError;
use redis::AsyncCommands;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::connect::Topology;
use crate::ops::tree::budget::{tree_scan_budget, ScanBudget};
use crate::ops::tree::scan::{
    fetch_key_meta, fetch_key_values, read_dbsize, scan_budgeted, KeyMeta, PageKey, ValueFields,
};

/// Default separator: colon.
const DEFAULT_SEP: &str = ":";

/// A child entry returned by `list_children`.
///
/// `rename_all` on an enum only renames variants; fields need
/// `rename_all_fields` or the IPC payload stays snake_case (frontend expects
/// camelCase `keyType` / `logicalLen` / `memBytes`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChildEntry {
    /// A leaf key (no further separator after stripping prefix).
    Key {
        key: String,
        key_type: String,
        ttl: i64,
        logical_len: u64,
        mem_bytes: Option<u64>,
    },
    /// A virtual folder node (further keys exist under this prefix).
    Folder { prefix: String, count: u64 },
}

/// Find the next occurrence of any separator in `rest`.
/// Returns `(segment_before_sep, sep_len)` or `(rest, 0)` if no sep found.
fn find_next_segment<'a>(rest: &'a str, seps: &[&str]) -> (&'a str, usize) {
    let mut best_pos = rest.len();
    let mut best_sep_len = 0usize;
    for sep in seps {
        if let Some(pos) = rest.find(sep) {
            if pos < best_pos {
                best_pos = pos;
                best_sep_len = sep.len();
            }
        }
    }
    if best_sep_len > 0 {
        (&rest[..best_pos], best_sep_len)
    } else {
        (rest, 0)
    }
}

/// Split a list of keys (all starting with `prefix`) into child entries.
///
/// - Keys with no separator after the prefix → `ChildEntry::Key`
/// - Keys with a separator → grouped into `ChildEntry::Folder` (count = how many times seen)
fn split_children(keys: &[String], prefix: &str, seps: &[&str]) -> Vec<ChildEntry> {
    let mut folders: HashMap<String, u64> = HashMap::new();
    let mut leaves: Vec<String> = Vec::new();
    let prefix_len = prefix.len();

    for key in keys {
        let rest = &key[prefix_len..];
        let (segment, sep_len) = find_next_segment(rest, seps);
        if sep_len == 0 {
            // Leaf key — no separator after prefix.
            leaves.push(key.clone());
        } else {
            // Folder prefix includes the segment and its trailing separator.
            let folder_full = &key[..prefix_len + segment.len() + sep_len];
            *folders.entry(folder_full.to_string()).or_insert(0) += 1;
        }
    }

    let mut result: Vec<ChildEntry> = Vec::new();

    // Folders first, sorted by name.
    let mut folder_list: Vec<_> = folders.into_iter().collect();
    folder_list.sort_by(|a, b| a.0.cmp(&b.0));
    for (prefix, count) in folder_list {
        result.push(ChildEntry::Folder { prefix, count });
    }

    // Then leaves, sorted by key.
    leaves.sort();
    for key in leaves {
        result.push(ChildEntry::Key {
            key,
            key_type: String::new(), // caller fills via pipeline
            ttl: -1,
            logical_len: 0,
            mem_bytes: None,
        });
    }

    result
}

/// One level of the tree, as the reply needs it.
///
/// `entries` / `cursor` keep the established meaning and nesting (the driver UI
/// reads `children` + `cursor` today); `consumed` / `truncated` / `dbsize` are
/// the budget fields of PRD §3.2 and are appended, never replacing anything.
#[derive(Debug, Clone)]
pub struct ChildrenPage {
    /// Direct children of `prefix`: folders first, then leaves.
    pub entries: Vec<ChildEntry>,
    /// Resume cursor (`0` = this level was walked to the end).
    pub next_cursor: u64,
    /// Cumulative `COUNT` actually spent on this action.
    pub consumed: u64,
    /// The budget or the round guard was hit while the cursor was still open,
    /// so this level has children the caller has not seen and the folder counts
    /// are a floor, not a census.
    pub truncated: bool,
    /// `DBSIZE`, read exactly once for this call.
    pub dbsize: u64,
}

/// Execute a hierarchical, budgeted `list_children` scan.
///
/// Scans keys matching `{prefix}*` under one [`ScanBudget`], splits the page by
/// separator, then resolves the leaf keys' attributes in **two batches for the
/// whole page** ([`fetch_key_meta`] for `TYPE`/`TTL` (+`MEMORY USAGE`),
/// [`fetch_key_values`] for the logical length). The previous shape typed only
/// TYPE/TTL — and left `logicalLen` at `0` and `memBytes` at `null` always —
/// because both were produced by the same per-key round trips this replaces.
///
/// `with_memory` also decides whether `MEMORY USAGE` joins the meta batch; on a
/// cluster each key is one addressed batch of its own (see
/// [`crate::ops::tree::scan`]'s module docs), which is what keeps a page that
/// crosses shards from failing on a `CROSSSLOT`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_children_page<C>(
    conn: &mut C,
    prefix: &str,
    cursor: u64,
    count: u32,
    sep: Option<&str>,
    no_ttl_only: bool,
    key_type: Option<&str>,
    with_memory: bool,
    budget: Option<u64>,
    topology: Topology,
    t0: Instant,
) -> Result<ChildrenPage, DriverError>
where
    C: AsyncCommands
        + redis::aio::ConnectionLike
        + Send
        + crate::ops::workbench::SlotRoutedConnection,
{
    let pattern = format!("{prefix}*");
    let sep_str = sep.unwrap_or(DEFAULT_SEP);
    let sep_chars: Vec<String> = sep_str.chars().map(|c| c.to_string()).collect();
    let seps: Vec<&str> = sep_chars.iter().map(|s| s.as_str()).collect();

    // DBSIZE first: the budget is scaled off it, so it cannot be read later. A
    // refused DBSIZE yields 0 and the default tier — it never fails the level
    // (redis-tree-backend-BUG-003).
    let dbsize = read_dbsize(conn).await;
    let mut ledger = ScanBudget::new(tree_scan_budget(budget, dbsize));
    let page = scan_budgeted(
        conn,
        cursor,
        count.max(1),
        usize::try_from(count.max(1)).unwrap_or(usize::MAX),
        Some(&pattern),
        crate::driver::session::normalize_type_filter(key_type),
        &mut ledger,
        topology,
    )
    .await
    .map_err(DriverError::QueryFailed)?;

    // Split into children (folders + leaf keys) before spending anything on
    // attributes: a folder's keys are never typed, which is the whole point of a
    // virtual node.
    let mut children = split_children(&page.keys, prefix, &seps);
    let leaf_keys: Vec<String> = children
        .iter()
        .filter_map(|e| match e {
            ChildEntry::Key { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect();

    if !leaf_keys.is_empty() {
        let metas = fetch_key_meta(conn, &leaf_keys, with_memory, topology)
            .await
            .map_err(DriverError::QueryFailed)?;
        // A batch answering with the wrong number of rows is a server/transport
        // defect, not data — warn in release builds too, so the row/leaf
        // agreement this function relies on is never debug-only.
        if metas.len() != leaf_keys.len() {
            tracing::warn!(
                rows = metas.len(),
                leaves = leaf_keys.len(),
                "redis list_children: meta batch length differs from leaf count"
            );
        }

        // Attributes join to children **by key, never by position**
        // (redis-tree-backend-BUG-001): shortening or reordering `rows` before
        // the join used to shift type/TTL/size onto the next key's slot.
        let mut attrs: HashMap<String, (KeyMeta, ValueFields)> = HashMap::new();
        let mut gone: HashSet<String> = HashSet::new();
        let mut items: Vec<PageKey> = Vec::new();
        for (key, meta) in leaf_keys.into_iter().zip(metas) {
            // A key the server now says is gone is not a child; an *unreadable*
            // reply is (`KeyMeta::absent` false), so a degraded batch can never
            // shrink a level.
            if meta.absent {
                gone.insert(key);
                continue;
            }
            items.push(PageKey {
                key_type: meta.key_type.clone(),
                key: key.clone(),
            });
            attrs.insert(key, (meta, ValueFields::default()));
        }
        let values = fetch_key_values(conn, &items, topology)
            .await
            .map_err(DriverError::QueryFailed)?;
        for (item, value) in items.iter().zip(values) {
            if let Some(slot) = attrs.get_mut(&item.key) {
                slot.1 = value;
            }
        }

        let mut filled = 0usize;
        for child in children.iter_mut() {
            if let ChildEntry::Key {
                key,
                key_type,
                ttl,
                logical_len,
                mem_bytes,
            } = child
            {
                if let Some((meta, value)) = attrs.get(key) {
                    *key_type = meta.key_type.clone();
                    *ttl = meta.ttl;
                    *logical_len = value.logical_len;
                    *mem_bytes = meta.mem_bytes;
                    filled += 1;
                }
            }
        }
        // Row/slot agreement is checked in release as well: a `debug_assert`
        // alone compiles away exactly where a silent mislabelling would ship
        // (the old "more rows than slots" warn covered only one direction).
        debug_assert_eq!(
            filled,
            attrs.len(),
            "every resolved leaf row must bind to its own slot"
        );
        if filled != attrs.len() {
            tracing::warn!(
                filled,
                resolved = attrs.len(),
                "redis list_children: resolved leaf rows and child slots disagree"
            );
        }

        // Filters run **after** attributes are attached (the baseline order):
        // the surviving leaves are exactly the ones the server described, so
        // `noTtlOnly` can no longer shift properties across keys.
        children.retain(|child| match child {
            ChildEntry::Key { key, ttl, .. } => !gone.contains(key) && (!no_ttl_only || *ttl == -1),
            ChildEntry::Folder { .. } => true,
        });
    }

    tracing::info!(
        elapsed_ms = t0.elapsed().as_millis() as u64,
        children = children.len(),
        with_memory,
        consumed = page.consumed,
        truncated = page.truncated,
        dbsize,
        "redis list_children page done"
    );
    Ok(ChildrenPage {
        entries: children,
        next_cursor: page.next_cursor,
        consumed: page.consumed,
        truncated: page.truncated,
        dbsize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_next_segment_colon() {
        let (seg, len) = find_next_segment("user:profile:123", &[":"]);
        assert_eq!(seg, "user");
        assert_eq!(len, 1);
    }

    #[test]
    fn find_next_segment_no_sep() {
        let (seg, len) = find_next_segment("barekey", &[":"]);
        assert_eq!(seg, "barekey");
        assert_eq!(len, 0);
    }

    #[test]
    fn find_next_segment_dot() {
        let (seg, len) = find_next_segment("ns.key.rest", &["."]);
        assert_eq!(seg, "ns");
        assert_eq!(len, 1);
    }

    #[test]
    fn split_children_leaves_only() {
        let keys = vec!["prefix:key1".to_string(), "prefix:key2".to_string()];
        let children = split_children(&keys, "prefix:", &[":"]);
        assert_eq!(children.len(), 2);
        assert!(matches!(&children[0], ChildEntry::Key { key, .. } if key == "prefix:key1"));
        assert!(matches!(&children[1], ChildEntry::Key { key, .. } if key == "prefix:key2"));
    }

    #[test]
    fn split_children_folders_and_leaves() {
        let keys = vec![
            "prefix:users:1".to_string(),
            "prefix:users:2".to_string(),
            "prefix:sessions:abc".to_string(),
            "prefix:orphan".to_string(),
        ];
        let children = split_children(&keys, "prefix:", &[":"]);
        // 2 folders (prefix:sessions: and prefix:users:) + 1 leaf
        let folders: Vec<_> = children
            .iter()
            .filter(|e| matches!(e, ChildEntry::Folder { .. }))
            .collect();
        let leaves: Vec<_> = children
            .iter()
            .filter(|e| matches!(e, ChildEntry::Key { .. }))
            .collect();
        assert_eq!(folders.len(), 2);
        assert_eq!(leaves.len(), 1);
    }

    #[test]
    fn split_children_empty() {
        let keys: Vec<String> = vec![];
        let children = split_children(&keys, "prefix:", &[":"]);
        assert!(children.is_empty());
    }

    #[test]
    fn child_entry_serializes_camel_case_fields() {
        let key = ChildEntry::Key {
            key: "app:users:1".into(),
            key_type: "hash".into(),
            ttl: 42,
            logical_len: 7,
            mem_bytes: Some(128),
        };
        let json = serde_json::to_value(&key).unwrap();
        assert_eq!(json["kind"], "key");
        assert_eq!(json["keyType"], "hash");
        assert_eq!(json["logicalLen"], 7);
        assert_eq!(json["memBytes"], 128);
        let folder = ChildEntry::Folder {
            prefix: "app:users:".into(),
            count: 3,
        };
        let fjson = serde_json::to_value(&folder).unwrap();
        assert_eq!(fjson["kind"], "folder");
        assert_eq!(fjson["prefix"], "app:users:");
        assert_eq!(fjson["count"], 3);
    }
}
