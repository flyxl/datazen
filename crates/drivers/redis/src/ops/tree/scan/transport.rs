//! The cluster-safe batch transport: the addressed pipeline, the connection-vs-command failure classifier, and the per-command replay that keeps a rejected field from failing the whole batch.

use crate::connect::Topology;
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
use redis::aio::ConnectionLike;
use redis::cluster_routing::get_slot;
use redis::Value as RValue;

/// Keys resolved per batch on a single node. A 200-key page is one round trip.
pub const TREE_KEYS_PER_PIPELINE: usize = 256;

/// Commands per key in the meta batch: `TYPE` + `TTL`.
pub const META_FIELDS_BASE: usize = 2;

/// Extra commands per key when the caller asked for `withMemory` (`MEMORY USAGE`).
pub const META_FIELDS_MEMORY: usize = 1;

/// Commands per key in the value batch when both exist (`length` + `preview`).
pub const VALUE_FIELDS_MAX: usize = 2;

// ---------------------------------------------------------------------------
// Cluster-safe batches
// ---------------------------------------------------------------------------

/// Issue a batch and answer with one value per command, keeping per-command
/// errors *inside* the vector.
///
/// `Pipeline::query_async` is unusable here: it folds the reply vector through
/// `Value::extract_error_vec`, so a single rejected command would fail the batch.
pub(crate) async fn pipeline_raw<C>(
    conn: &mut C,
    pipe: &redis::Pipeline,
) -> Result<Vec<RValue>, String>
where
    C: ConnectionLike + Send,
{
    let count = pipe.cmd_iter().count();
    if count == 0 {
        return Ok(Vec::new());
    }
    conn.req_packed_commands(pipe, 0, count)
        .await
        .map_err(|e| e.to_string())
}

/// Is this failure about the transport rather than about one command?
///
/// An answered error (`MEMORY USAGE` on Redis < 4.0) degrades one field; an I/O,
/// timeout or topology failure means the batch never ran and must not be dressed
/// up as "no value" — the same split `ops_workbench` draws.
pub(crate) fn is_connection_level_failure(error: &redis::RedisError) -> bool {
    error.is_io_error()
        || error.is_unrecoverable_error()
        || error.is_cluster_error()
        || matches!(
            error.kind(),
            redis::ErrorKind::CrossSlot
                | redis::ErrorKind::ClientError
                | redis::ErrorKind::InvalidClientConfig
        )
}

/// Turn one command's answer into a reply slot: a server error becomes `Nil`, so
/// the shared parsers degrade that field exactly as they degrade a pipeline item.
pub(crate) fn fold_command_answer(
    answer: Result<RValue, redis::RedisError>,
) -> Result<RValue, String> {
    match answer {
        Ok(value) => Ok(value),
        Err(error) if is_connection_level_failure(&error) => Err(error.to_string()),
        Err(error) => {
            tracing::debug!(
                error = %error,
                "redis key tree: batch item rejected, degrading that field"
            );
            Ok(RValue::Nil)
        }
    }
}

/// Send a batch one addressed command at a time — the cluster form of
/// [`pipeline_raw`], answering with one value per command.
async fn replay_per_command<C>(
    conn: &mut C,
    pipe: &redis::Pipeline,
    slot: u16,
) -> Result<Vec<RValue>, String>
where
    C: SlotRoutedConnection + Send,
{
    let mut values = Vec::new();
    for cmd in pipe.cmd_iter() {
        values.push(fold_command_answer(conn.command_at_slot(cmd, slot).await)?);
    }
    Ok(values)
}

/// Read one key's whole batch, whatever the topology.
///
/// On a cluster the batch is addressed to the master owning [`get_slot(key)`],
/// which is what keeps a key's commands from ever being a cross-slot pipeline.
/// `route_pipeline` folds a rejected command into one batch error, so a rejection
/// — or a short vector — replays *this key* command by command, restoring the
/// "one bad field degrades alone" contract at the cost of de-batching one key.
///
/// The outer `Err` is reserved for transport failures: a page that never ran.
pub(crate) async fn fetch_key_group<C>(
    conn: &mut C,
    key: &str,
    pipe: &redis::Pipeline,
    topology: Topology,
) -> Result<Vec<RValue>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let count = pipe.cmd_iter().count();
    if count == 0 {
        return Ok(Vec::new());
    }
    if !matches!(topology, Topology::Cluster) {
        return pipeline_raw(conn, pipe).await;
    }
    let slot = get_slot(key.as_bytes());
    match conn.pipeline_at_slot(pipe, slot).await {
        Ok(values) if values.len() == count => Ok(values),
        Ok(partial) => {
            tracing::debug!(
                %key,
                replied = partial.len(),
                expected = count,
                "redis key tree: addressed batch answered a short vector, replaying per command"
            );
            replay_per_command(conn, pipe, slot).await
        }
        Err(error) => {
            tracing::debug!(
                %key,
                error = %error,
                "redis key tree: addressed batch rejected, replaying per command"
            );
            replay_per_command(conn, pipe, slot).await
        }
    }
}
