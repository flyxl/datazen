//! Per-key meta batches: type / expiry / memory in one chunked pipeline per page, with the reply layout pinned.

use super::batch::fetch_page_groups;
use super::transport::META_FIELDS_BASE;
use super::transport::META_FIELDS_MEMORY;
use crate::connect::Topology;
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
use redis::aio::ConnectionLike;
use redis::Value as RValue;

/// Attributes of one key that need no knowledge of its type.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyMeta {
    /// Redis `TYPE`, lower-cased. `"none"` when the key is gone *or* the answer
    /// was unusable — [`KeyMeta::absent`] is what tells those apart.
    pub key_type: String,
    /// `TTL` in **seconds**: `-1` no expiry, `-2` missing, `>=0` remaining.
    pub ttl: i64,
    /// `MEMORY USAGE` in bytes; `None` when not requested or unsupported.
    pub mem_bytes: Option<u64>,
    /// `TYPE` positively answered `none`: the key expired or was deleted between
    /// the scan and this read. An unreadable reply is *not* absence.
    pub absent: bool,
}

/// Commands one key contributes to the meta batch.
pub fn meta_fields_per_key(with_memory: bool) -> usize {
    META_FIELDS_BASE + if with_memory { META_FIELDS_MEMORY } else { 0 }
}

/// Reply slots inside one key's meta group.
pub mod meta_slots {
    /// `TYPE`.
    pub const TYPE: usize = 0;
    /// `TTL`.
    pub const TTL: usize = 1;
    /// `MEMORY USAGE`, only present when `withMemory` was requested.
    pub const MEMORY: usize = 2;
}

/// Build the meta batch: per key `TYPE` + `TTL` (+ `MEMORY USAGE`).
///
/// Side-effect free, so it *is* the contract surface: which commands, in what
/// order, in one batch.
pub fn build_meta_pipeline(keys: &[String], with_memory: bool) -> (redis::Pipeline, Vec<usize>) {
    let per_key = meta_fields_per_key(with_memory);
    let mut pipe = redis::Pipeline::with_capacity(keys.len() * per_key);
    for key in keys {
        pipe.cmd("TYPE").arg(key).cmd("TTL").arg(key);
        if with_memory {
            pipe.cmd("MEMORY").arg("USAGE").arg(key);
        }
    }
    (pipe, vec![per_key; keys.len()])
}

pub(crate) fn reply_at(values: &[RValue], index: usize) -> RValue {
    values.get(index).cloned().unwrap_or(RValue::Nil)
}

/// Assemble one key's meta replies. Every slot degrades through the shared
/// parsers, so a reply that is simply not there is never invented into a type
/// name or an expiry.
pub fn parse_meta_group(values: &[RValue], with_memory: bool) -> KeyMeta {
    let type_reply = reply_at(values, meta_slots::TYPE);
    KeyMeta {
        key_type: parse_type_token(&type_reply)
            .unwrap_or_else(|| "none".to_string())
            .to_ascii_lowercase(),
        ttl: parse_opt_int(&reply_at(values, meta_slots::TTL)).unwrap_or(TTL_MISSING),
        mem_bytes: if with_memory {
            parse_opt_int(&reply_at(values, meta_slots::MEMORY)).and_then(|n| u64::try_from(n).ok())
        } else {
            None
        },
        absent: type_reply_says_absent(&type_reply),
    }
}

/// Resolve type / TTL (+ memory) for a page in one batch per chunk.
pub(crate) async fn fetch_key_meta<C>(
    conn: &mut C,
    keys: &[String],
    with_memory: bool,
    topology: Topology,
) -> Result<Vec<KeyMeta>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let groups = fetch_page_groups(conn, keys, topology, |chunk| {
        build_meta_pipeline(chunk, with_memory)
    })
    .await?;
    Ok(groups
        .iter()
        .map(|group| parse_meta_group(group, with_memory))
        .collect())
}

// ---------------------------------------------------------------------------
// Page batch 2: logical length + preview (both type-dependent)
// ---------------------------------------------------------------------------
