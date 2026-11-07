//! `key_object_info`: one pipeline for every key attribute, degrading per field.

use super::primitives::KEY_INFO_PIPELINE_LEN;
use super::shapes::build_key_info_pipeline;
use super::shapes::parse_key_info;
use super::shapes::KeyObjectInfo;
use super::transport::issue_batch;
use super::transport::SlotRoutedConnection;
use crate::connect::Topology;
use redis::aio::ConnectionLike;
#[allow(unused_imports)]
use redis::cluster_routing::{get_slot, Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};

/// Key attributes for the sidebar, in a single batch.
///
/// On a cluster the six commands go out one at a time, each addressed to the
/// master owning [`get_slot(key)`], so none of them is left to redis'
/// command-name table (see the module docs).
pub async fn key_object_info<C>(
    conn: &mut C,
    key: &str,
    topology: Topology,
) -> Result<KeyObjectInfo, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let pipe = build_key_info_pipeline(key);
    let values = issue_batch(conn, &pipe, topology, get_slot(key.as_bytes())).await?;
    if values.len() != KEY_INFO_PIPELINE_LEN {
        // `req_packed_commands` is an internal API whose contract is "one reply
        // per command". If that ever stops holding, reading the vector by slot
        // would silently move every attribute into the next field, so refuse
        // rather than hand the sidebar a plausible-looking wrong answer.
        // (The per-command path cannot produce this shape.)
        tracing::warn!(
            %key,
            expected = KEY_INFO_PIPELINE_LEN,
            replied = values.len(),
            "redis key_object_info: reply vector does not match the request"
        );
        return Err(format!(
            "key_object_info: expected {KEY_INFO_PIPELINE_LEN} replies for {KEY_INFO_PIPELINE_LEN} commands, got {}",
            values.len()
        ));
    }
    let info = parse_key_info(&values);
    if info.missing {
        tracing::debug!(%key, "redis key_object_info: key expired or deleted");
    }
    Ok(info)
}

// ---------------------------------------------------------------------------
// Big-key sample field read (`memory_sample`)
//
// `ops_observe::memory_sample` scans a db and needs `MEMORY USAGE` for every
// sampled key, now plus `TYPE` and `PTTL` so screen A's Top-5 can render the
// 键名 / 类型 / 字节 / TTL columns PRD §3.1 asks for. Doing that per key would
// triple the round trips, so the three fields are batched here with the same
// primitives `key_object_info` uses — see the module docs for why the batch is
// a pipeline on a single node but *must* be one addressed command at a time on
// a cluster.
// ---------------------------------------------------------------------------
