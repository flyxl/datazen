//! The addressed-batch transport: `SlotRoutedBatchFuture`, the `SlotRoutedConnection` extension point, the primitive-per-key routers, the batch issuer and `fetch_dbsize`.

use super::shapes::parse_opt_int;
#[allow(unused_imports)]
use super::shapes::slot;
use crate::connect::Topology;
use futures_util::FutureExt;
use redis::aio::ConnectionLike;
#[allow(unused_imports)]
use redis::cluster_routing::{get_slot, Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};
use redis::RedisFuture;
use redis::Value as RValue;
use std::future::Future;
use std::pin::Pin;

/// The transport capability the cluster path needs and [`ConnectionLike`]
/// cannot express: naming the node a command runs on.
///
/// Left to itself, redis' cluster client picks the node from its own command-name
/// table, which does not describe these probes — see the module docs for what
/// that costs. Only [`Topology::Cluster`] asks for it.
///
/// Future returned by [`SlotRoutedConnection::pipeline_at_slot`]: the raw
/// per-command replies of the batch, or one error for the whole batch (which
/// is also how a real `route_pipeline` dispatch reports a folded rejection).
pub type SlotRoutedBatchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<RValue>, String>> + Send + 'a>>;

/// A single-node connection has exactly one node, so the slot is vacuous and the
/// default implementation just sends the command.
pub trait SlotRoutedConnection: ConnectionLike + Send {
    /// Issue `cmd` against the master that owns `slot`.
    fn command_at_slot<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
        _slot: u16,
    ) -> RedisFuture<'a, RValue> {
        self.req_packed_command(cmd)
    }

    /// Issue `pipe` against the master that owns `slot` as **one addressed
    /// batch**, answering with one value per command.
    ///
    /// Used by [`fetch_memory_sample_fields`], whose three probes of one key
    /// (`MEMORY USAGE` / `TYPE` / `PTTL`) all hash to that key's slot, so the
    /// batch can never cross slots. The default is the honest single-node
    /// shape — one command per round trip, address vacuous.
    fn pipeline_at_slot<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        slot: u16,
    ) -> SlotRoutedBatchFuture<'a>
    where
        Self: Sized,
    {
        routed_sequential(self, pipe, slot).boxed()
    }
}

impl SlotRoutedConnection for redis::aio::MultiplexedConnection {}

impl<C> SlotRoutedConnection for redis::cluster_async::ClusterConnection<C>
where
    C: ConnectionLike + redis::cluster_async::Connect + Clone + Send + Sync + Unpin + 'static,
{
    fn command_at_slot<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
        slot: u16,
    ) -> RedisFuture<'a, RValue> {
        self.route_command(cmd, master_route(slot)).boxed()
    }

    fn pipeline_at_slot<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        slot: u16,
    ) -> SlotRoutedBatchFuture<'a> {
        // `route_pipeline` takes an explicit node, bypassing the
        // `route_for_pipeline` pre-check that would misread `MEMORY USAGE`
        // (see the module docs) and reject this batch with a client-side
        // `CROSSSLOT`. The target master owns `slot`, hence every key in the
        // batch, so the server executes all commands. One round trip per key.
        //
        // The known cost of this shape: the dispatch layer re-folds the reply
        // vector with `Value::extract_error_vec` (module docs), so a *rejected*
        // command surfaces as one batch error rather than one error slot —
        // [`fetch_memory_sample_fields`] re-runs the key through
        // [`routed_sequential`] in that case, which restores per-command
        // degradation at the price of de-batching just that key.
        let count = pipe.cmd_iter().count();
        (async move {
            if count == 0 {
                return Ok(Vec::new());
            }
            let route = SingleNodeRoutingInfo::SpecificNode(Route::new(slot, SlotAddr::Master));
            self.route_pipeline(pipe, 0, count, route)
                .await
                .map_err(|error| error.to_string())
        })
        .boxed()
    }
}

/// Address a command at the master owning `slot` — the same route shape redis
/// builds for a keyed command, but computed from the real key rather than from
/// whatever token its table happens to read.
pub(crate) fn master_route(slot: u16) -> RoutingInfo {
    RoutingInfo::SingleNode(SingleNodeRoutingInfo::SpecificNode(Route::new(
        slot,
        SlotAddr::Master,
    )))
}

/// Issue a batch and return the raw per-command replies (see module docs).
///
/// The outer `Err` is reserved for failures of the connection itself, i.e. a
/// batch that never happened at all; per-command errors stay inside the
/// returned vector.
///
/// `Pipeline::query_async` cannot be used here: it folds the vector with
/// `Value::extract_error_vec`, so one rejected command would fail the whole
/// probe.
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

/// Is this failure about the connection rather than about one command?
///
/// Used only on the per-command path, where an answered error (`OBJECT FREQ` on
/// a non-LFU server, `MEMORY USAGE` below Redis 4.0) degrades a single field
/// while an I/O, timeout or topology failure means the probe never ran and must
/// not be dressed up as "no value". A `MOVED` / `ASK` reaching this point means
/// the redirects were exhausted, i.e. the cluster cannot serve the key right
/// now — explicitly addressed probes (see [`SlotRoutedConnection`]) are what
/// keeps that path quiet in the first place.
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

/// Shared tail of the per-command paths: sort "the server answered that it has
/// no value for this field" from "the read never happened".
fn fold_command_answer(answer: Result<RValue, redis::RedisError>) -> Result<RValue, String> {
    match answer {
        Ok(value) => Ok(value),
        Err(error) if is_connection_level_failure(&error) => Err(error.to_string()),
        Err(error) => {
            tracing::debug!(
                error = %error,
                "redis workbench: batch item rejected, degrading that field"
            );
            // A server error on the single-command path arrives as `Err`, not as
            // a `ServerError` value; `Nil` puts it back into the vector so the
            // same parsers degrade it exactly as they degrade a pipeline error.
            Ok(RValue::Nil)
        }
    }
}

/// Issue one command per round trip, addressed to `slot`, degrading answered
/// errors to "no value" exactly like a single-node pipeline degrades one item.
pub(crate) async fn routed_single<C>(
    conn: &mut C,
    cmd: &redis::Cmd,
    slot: u16,
) -> Result<RValue, String>
where
    C: SlotRoutedConnection + Send,
{
    fold_command_answer(conn.command_at_slot(cmd, slot).await)
}

/// Send a batch as one addressed command at a time — the cluster-safe form of
/// [`pipeline_raw`], answering with one value per command.
pub(crate) async fn routed_sequential<C>(
    conn: &mut C,
    pipe: &redis::Pipeline,
    slot: u16,
) -> Result<Vec<RValue>, String>
where
    C: SlotRoutedConnection + Send,
{
    let commands: Vec<&redis::Cmd> = pipe.cmd_iter().collect();
    tracing::debug!(
        slot,
        commands = commands.len(),
        "redis workbench: addressing every probe at one shard master"
    );
    let mut values = Vec::with_capacity(commands.len());
    for cmd in commands {
        values.push(routed_single(conn, cmd, slot).await?);
    }
    Ok(values)
}

/// Run a batch on the transport that keeps per-command errors per command.
///
/// `key_slot` is the slot the probed key hashes to. A single-node connection has
/// one node, so it ignores the address.
pub(crate) async fn issue_batch<C>(
    conn: &mut C,
    pipe: &redis::Pipeline,
    topology: Topology,
    key_slot: u16,
) -> Result<Vec<RValue>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    match topology {
        Topology::Cluster => routed_sequential(conn, pipe, key_slot).await,
        Topology::Standalone | Topology::Sentinel => pipeline_raw(conn, pipe).await,
    }
}

/// `DBSIZE` of the currently selected database (read once per command).
///
/// Deliberately **not** addressed, on any topology. `DBSIZE` is one of the
/// commands redis' cluster table does describe — `MultiNode(AllMasters,
/// Aggregate(Sum))` — so the client fans it out and adds the answers up, which
/// is the `M` the context bar's "采样 N/M" label needs. Pinning it to the sample
/// shard instead would report a shard's count as if it were the database's.
/// Two consequences of that policy: the number spans the whole cluster, and
/// one unreachable (or
/// non-integer-answering) master fails the aggregate, so the command errors
/// rather than reporting a partial total.
pub(crate) async fn fetch_dbsize<C>(conn: &mut C) -> Result<u64, String>
where
    C: ConnectionLike + Send,
{
    let raw: RValue = redis::cmd("DBSIZE")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(parse_opt_int(&raw).unwrap_or(0).max(0) as u64)
}
