//! Hierarchical key browsing for Redis.
//!
//! `list_children` splits SCAN results by separator to produce a flat list
//! of direct children (leaf keys + virtual folder nodes) for one tree level.

use datazen_driver_api::DriverError;
use redis::AsyncCommands;
use std::collections::HashMap;

/// Default separator: colon.
const DEFAULT_SEP: &str = ":";

/// A child entry returned by `list_children`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
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

/// Execute a hierarchical `list_children` scan.
///
/// Scans keys matching `{prefix}*` using SCAN, splits by separator, and
/// returns the direct children (leaf keys + virtual folder nodes) with
/// their metadata.
pub(crate) async fn list_children_on<C>(
    conn: &mut C,
    prefix: &str,
    cursor: u64,
    count: u32,
    sep: Option<&str>,
    no_ttl_only: bool,
    key_type: Option<&str>,
) -> Result<(Vec<ChildEntry>, u64), DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let pattern = format!("{prefix}*");
    let sep_str = sep.unwrap_or(DEFAULT_SEP);
    let sep_chars: Vec<String> = sep_str.chars().map(|c| c.to_string()).collect();
    let seps: Vec<&str> = sep_chars.iter().map(|s| s.as_str()).collect();

    // SCAN with the pattern; optional TYPE filter (Redis >= 6.0).
    let (next_cursor, keys) = crate::ops::scan_batch(
        conn,
        cursor,
        count,
        Some(&pattern),
        crate::redis_driver_on::normalize_type_filter(key_type),
    )
    .await
    .map_err(DriverError::QueryFailed)?;

    // Split into children (folders + leaf keys).
    let mut children = split_children(&keys, prefix, &seps);

    // Enrich leaf keys with TYPE + TTL via a real client-side pipeline.
    let leaf_keys: Vec<String> = children
        .iter()
        .filter_map(|e| match e {
            ChildEntry::Key { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect();

    if !leaf_keys.is_empty() {
        let mut pipe = redis::pipe();
        for k in &leaf_keys {
            pipe.cmd("TYPE").arg(k);
            pipe.cmd("TTL").arg(k);
        }
        let results: Vec<redis::Value> = pipe
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        // results come back in 2*leaf_keys order: [TYPE, TTL, TYPE, TTL, ...]
        let mut enriched: Vec<(String, String, i64)> = Vec::with_capacity(leaf_keys.len());
        for (i, key) in leaf_keys.iter().enumerate() {
            let type_val = results.get(i * 2);
            let ttl_val = results.get(i * 2 + 1);
            let key_type = type_val
                .map(crate::redis_value_preview::value_to_string)
                .unwrap_or_default();
            let ttl: i64 = match ttl_val {
                Some(redis::Value::Int(n)) => *n,
                Some(redis::Value::BulkString(b)) => {
                    String::from_utf8_lossy(b).parse::<i64>().unwrap_or(-1)
                }
                _ => -1,
            };
            enriched.push((key.clone(), key_type, ttl));
        }

        // Rebuild the leaf entries in place (folders keep their positions first).
        let mut leaf_iter = enriched.into_iter();
        for child in children.iter_mut() {
            if let ChildEntry::Key {
                key, key_type, ttl, ..
            } = child
            {
                if let Some((_, t, n)) = leaf_iter.next() {
                    *key_type = t;
                    *ttl = n;
                } else {
                    let _ = key;
                }
            }
        }

        if no_ttl_only {
            children.retain(|e| match e {
                ChildEntry::Key { ttl, .. } => *ttl == -1,
                ChildEntry::Folder { .. } => true,
            });
        }
    }

    Ok((children, next_cursor))
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
}
