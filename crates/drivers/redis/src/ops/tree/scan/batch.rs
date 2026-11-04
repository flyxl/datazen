//! The batch/page plumbing built on that transport: `PageKey`, `scatter`, `fetch_page_groups`, the `GroupItem` bridge and one `SCAN` round.

use super::transport::fetch_key_group;
use super::transport::pipeline_raw;
use super::transport::TREE_KEYS_PER_PIPELINE;
use crate::connect::Topology;
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
#[allow(unused_imports)]
use crate::value::{parse_scan_result, preview_value_to_string, truncate_preview, value_to_string};
use redis::aio::ConnectionLike;
use redis::Value as RValue;

/// One key plus the type that decides which follow-up commands it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageKey {
    /// The key name as `SCAN` reported it.
    pub key: String,
    /// Redis `TYPE` token already resolved for it (lower-cased).
    pub key_type: String,
}

/// Cut a flat reply vector into groups of the sizes the builder declared.
///
/// A group that ran out of replies gets `Nil` fillers rather than borrowing the
/// next key's answers, which is what keeps a short vector from being rendered as
/// plausible-looking wrong data. Keys that issued no commands at all still get a
/// group, so caller-side index arithmetic stays valid.
pub(crate) fn scatter(values: Vec<RValue>, sizes: &[usize]) -> Vec<Vec<RValue>> {
    let mut groups: Vec<Vec<RValue>> = Vec::with_capacity(sizes.len());
    let mut rest = values.into_iter();
    for size in sizes {
        let mut group = Vec::with_capacity(*size);
        for _ in 0..*size {
            group.push(rest.next().unwrap_or(RValue::Nil));
        }
        groups.push(group);
    }
    groups
}

/// Run one logical page batch and hand back one reply group per item.
///
/// `build` turns items into `(pipeline, commands-per-item)`; the per-item counts
/// let a variable-arity batch (a vanished key asks for nothing) still land one
/// group per item.
pub(crate) async fn fetch_page_groups<C, I, B>(
    conn: &mut C,
    items: &[I],
    topology: Topology,
    build: B,
) -> Result<Vec<Vec<RValue>>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
    I: GroupItem,
    B: Fn(&[I]) -> (redis::Pipeline, Vec<usize>),
{
    let mut groups: Vec<Vec<RValue>> = Vec::with_capacity(items.len());
    if items.is_empty() {
        return Ok(groups);
    }

    if matches!(topology, Topology::Cluster) {
        for item in items {
            let (pipe, sizes) = build(std::slice::from_ref(item));
            let values = fetch_key_group(conn, item.key(), &pipe, topology).await?;
            for group in scatter(values, &sizes) {
                groups.push(group);
            }
        }
        return Ok(groups);
    }

    for chunk in items.chunks(TREE_KEYS_PER_PIPELINE) {
        let (pipe, sizes) = build(chunk);
        let expected = sizes.iter().sum::<usize>();
        let values = pipeline_raw(conn, &pipe).await?;
        if values.len() != expected {
            tracing::warn!(
                expected,
                replied = values.len(),
                keys = chunk.len(),
                "redis key tree: page batch answered a short vector, degrading the trailing keys"
            );
        }
        groups.extend(scatter(values, &sizes));
    }
    Ok(groups)
}

/// The key a group's commands are about — needed to address it on a cluster.
///
/// The small overloads keep [`fetch_page_groups`] usable for both batches: the
/// meta batch is keyed by the bare key name, the value batch by key + type.
pub(crate) trait GroupItem: Clone {
    fn key(&self) -> &str;
}

impl GroupItem for String {
    fn key(&self) -> &str {
        self
    }
}

impl GroupItem for PageKey {
    fn key(&self) -> &str {
        &self.key
    }
}

// ---------------------------------------------------------------------------
// SCAN under a budget
// ---------------------------------------------------------------------------

/// One `SCAN` round: `SCAN cursor COUNT n [MATCH p] [TYPE t]`.
///
/// Cluster rounds are addressed to [`cluster_scan_anchor_slot`] so a multi-round
/// page stays one shard's scan instead of feeding one node's cursor to another.
pub(crate) async fn scan_round<C>(
    conn: &mut C,
    cursor: u64,
    count: u32,
    pattern: Option<&str>,
    type_filter: Option<&str>,
    topology: Topology,
) -> Result<(u64, Vec<String>), String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    if !matches!(topology, Topology::Cluster) {
        return crate::ops::scan_batch(conn, cursor, count, pattern, type_filter).await;
    }
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(cursor).arg("COUNT").arg(count.max(1));
    if let Some(p) = pattern {
        cmd.arg("MATCH").arg(p);
    }
    if let Some(t) = type_filter {
        cmd.arg("TYPE").arg(t);
    }
    let raw: RValue = conn
        .command_at_slot(&cmd, cluster_scan_anchor_slot())
        .await
        .map_err(|e| e.to_string())?;
    Ok(parse_scan_result(&raw))
}

/// What one budgeted scan pass collected.
#[derive(Debug, Clone)]
pub struct ScannedPage {
    /// Keys seen, de-duplicated across rounds (`SCAN` may repeat a key).
    pub keys: Vec<String>,
    /// Cursor to resume from; `0` means the keyspace was walked to the end.
    pub next_cursor: u64,
    /// Cumulative `COUNT` actually spent — the reply's `consumed`.
    pub consumed: u64,
    /// The pass stopped on a cap (budget or round guard) with the cursor still
    /// open, so there is data the caller did not see.
    pub truncated: bool,
    /// The cursor wrapped: everything reachable was seen.
    pub exhausted: bool,
}
