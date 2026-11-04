//! `memory_sample`: the type / TTL / size field read behind `ops_observe`'s large-key path, batched per key on a cluster.

use super::primitives::TTL_MISSING;
use super::shapes::is_unusable_reply;
use super::shapes::parse_opt_int;
use super::shapes::parse_type_token;
use super::shapes::slot;
use super::shapes::type_reply_says_absent;
use super::transport::pipeline_raw;
use super::transport::routed_sequential;
use super::transport::SlotRoutedConnection;
use crate::connect::Topology;
use redis::aio::ConnectionLike;
#[allow(unused_imports)]
use redis::cluster_routing::{get_slot, Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};
use redis::Value as RValue;

/// Commands read per key in the big-key sample pipeline.
pub const MEMORY_SAMPLE_FIELDS_PER_KEY: usize = 3;

/// Sampled keys resolved per pipeline on a single node. The screen-A Top-5 and
/// the 200-key default window each fit inside one batch, so the whole field
/// read costs a single round trip there; larger windows add one round trip per
/// chunk — never one per key.
pub const MEMORY_SAMPLE_KEYS_PER_PIPELINE: usize = 256;

/// Reply slots inside one key's group in [`build_memory_sample_pipeline`].
pub mod memory_sample_slots {
    pub const MEMORY: usize = 0;
    pub const TYPE: usize = 1;
    pub const TTL: usize = 2;
}

/// Build the batched big-key field pipeline: for each key, in order,
/// `MEMORY USAGE` + `TYPE` + `PTTL`. Side-effect free, so it is the contract
/// surface the tests assert against.
pub fn build_memory_sample_pipeline(keys: &[String]) -> redis::Pipeline {
    let mut pipe = redis::Pipeline::with_capacity(keys.len() * MEMORY_SAMPLE_FIELDS_PER_KEY);
    for key in keys {
        pipe.cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .cmd("TYPE")
            .arg(key)
            .cmd("PTTL")
            .arg(key);
    }
    pipe
}

/// Attributes read for one sampled key. `missing` marks a key that expired or
/// was deleted between the `SCAN` that found it and this read — a distinguishable
/// empty state, not an error.
#[derive(Debug, Clone, Default)]
pub struct MemorySampleFields {
    /// `MEMORY USAGE` in bytes; `None` when unavailable (Redis < 4.0) or gone.
    pub bytes: Option<u64>,
    /// Redis `TYPE`; `None` when absent / unreadable / the key is gone.
    pub key_type: Option<String>,
    /// `PTTL` in ms: `-1` no expiry, `-2` gone, `>0` remaining; `None` unreadable.
    pub ttl_ms: Option<i64>,
    pub missing: bool,
}

/// Assemble one key's `(MEMORY USAGE, TYPE, PTTL)` replies. `TYPE` decides
/// presence exactly as [`parse_key_info`] does, and every slot degrades through
/// the shared parsers, so a reply that is simply not there is never invented
/// into a type name or an expiry.
pub fn parse_memory_sample_fields(values: &[RValue]) -> MemorySampleFields {
    let type_reply = slot(values, memory_sample_slots::TYPE);
    if !is_unusable_reply(&type_reply) && type_reply_says_absent(&type_reply) {
        // TYPE == "none": gone between SCAN and read. A clean empty state —
        // not an error — that keeps the -2 PTTL sentinel for the UI.
        return MemorySampleFields {
            bytes: None,
            key_type: None,
            ttl_ms: Some(TTL_MISSING),
            missing: true,
        };
    }
    MemorySampleFields {
        bytes: parse_opt_int(&slot(values, memory_sample_slots::MEMORY))
            .and_then(|n| u64::try_from(n).ok()),
        key_type: parse_type_token(&type_reply),
        ttl_ms: parse_opt_int(&slot(values, memory_sample_slots::TTL)),
        missing: false,
    }
}

/// Resolve bytes/type/ttl for a set of sampled keys.
///
/// On a single-node connection the whole sample goes out in
/// [`MEMORY_SAMPLE_KEYS_PER_PIPELINE`]-sized batches — one round trip for the
/// screen-A Top-5 — so field reads cost at most `ceil(keys / chunk)` instead of
/// one round trip per key. That is the improvement this bug asks for.
///
/// **Cluster addresses each key as one batch.** A pipeline spanning hash slots
/// is rejected with `CROSSSLOT`, but the three probes of *one* key always hash
/// to that key's slot, so each key goes out as a single
/// [`SlotRoutedConnection::pipeline_at_slot`] batch aimed at the shard owning
/// it — the same N round trips the old per-key `MEMORY USAGE` loop cost, never
/// a mixed-slot pipeline. The one cluster-specific shape to know:
/// `route_pipeline` folds per-command rejections into one batch error (see the
/// module docs), so a rejected `TYPE` / `PTTL` would take the whole key down
/// with it; on a batch error this fn re-runs *that key* through
/// [`routed_sequential`], which answers one value per command and restores the
/// "one bad field degrades alone" contract. Connection-level failures still
/// surface as an `Err` for the command, exactly as before.
pub(crate) async fn fetch_memory_sample_fields<C>(
    conn: &mut C,
    keys: &[String],
    topology: Topology,
) -> Result<Vec<MemorySampleFields>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let mut fields = Vec::with_capacity(keys.len());
    if matches!(topology, Topology::Cluster) {
        for key in keys {
            let pipe = build_memory_sample_pipeline(std::slice::from_ref(key));
            let slot = get_slot(key.as_bytes());
            let values = match conn.pipeline_at_slot(&pipe, slot).await {
                Ok(values) if values.len() == MEMORY_SAMPLE_FIELDS_PER_KEY => values,
                // Folded batch rejection (or a malformed short vector): replay
                // this one key command-by-command so the rejected field degrades
                // alone instead of failing the whole sample.
                Ok(partial) => {
                    tracing::debug!(
                        key,
                        replied = partial.len(),
                        "redis memory_sample: addressed batch answered a short vector, replaying per command"
                    );
                    routed_sequential(conn, &pipe, slot).await?
                }
                Err(error) => {
                    tracing::debug!(
                        key,
                        error = %error,
                        "redis memory_sample: addressed batch rejected, replaying per command"
                    );
                    routed_sequential(conn, &pipe, slot).await?
                }
            };
            fields.push(parse_memory_sample_fields(&values));
        }
        return Ok(fields);
    }
    for chunk in keys.chunks(MEMORY_SAMPLE_KEYS_PER_PIPELINE) {
        let pipe = build_memory_sample_pipeline(chunk);
        let values = pipeline_raw(conn, &pipe).await?;
        let expected = chunk.len() * MEMORY_SAMPLE_FIELDS_PER_KEY;
        if values.len() != expected {
            tracing::warn!(
                expected,
                replied = values.len(),
                "redis memory_sample: field pipeline answered a short vector, degrading the trailing keys"
            );
        }
        let mut groups = values.chunks_exact(MEMORY_SAMPLE_FIELDS_PER_KEY);
        for _ in chunk {
            // A short vector leaves the remaining keys with no reply rather than
            // guessing values into the wrong slot: `chunks_exact` hands out only
            // full groups, and `slot()` reads an absent group's fields as `Nil`.
            let group = groups.next().unwrap_or(&[][..]);
            fields.push(parse_memory_sample_fields(group));
        }
    }
    Ok(fields)
}
