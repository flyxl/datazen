//! `type_distribution`: bounded `SCAN` sampling plus chunked `TYPE` pipelines — one shard, one cursor space, DBSIZE from the client's fan-out.

use super::primitives::cluster_scan_anchor_slot;
use super::primitives::sample_window_for;
use super::primitives::MAX_SCAN_ROUNDS;
use super::primitives::MAX_STALLED_SCAN_ROUNDS;
use super::primitives::TYPE_PIPELINE_CHUNK;
use super::primitives::TYPE_SCAN_COUNT;
use super::shapes::parse_type_token;
#[allow(unused_imports)]
use super::shapes::slot;
use super::shapes::TypeDistribution;
use super::transport::fetch_dbsize;
use super::transport::pipeline_raw;
use super::transport::routed_single;
use super::transport::SlotRoutedConnection;
use crate::connect::Topology;
use crate::driver::parse_scan_result;
use redis::aio::ConnectionLike;
#[allow(unused_imports)]
use redis::cluster_routing::{get_slot, Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};
use std::collections::BTreeMap;

/// One `SCAN` round: `(next cursor, keys)`.
///
/// On a cluster the round is addressed to [`cluster_scan_anchor_slot`] because
/// redis gives `SCAN` no route at all and falls back to a random node, which
/// would pass one node's cursor to another. A rejected `SCAN` is a failed read,
/// never an empty page, so — unlike [`routed_single`] — nothing is degraded
/// here: any error aborts the sampling.
async fn scan_round<C>(
    conn: &mut C,
    cursor: u64,
    topology: Topology,
) -> Result<(u64, Vec<String>), String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    if !matches!(topology, Topology::Cluster) {
        return crate::ops::scan_batch(conn, cursor, TYPE_SCAN_COUNT, None, None).await;
    }
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(cursor).arg("COUNT").arg(TYPE_SCAN_COUNT.max(1));
    let raw = conn
        .command_at_slot(&cmd, cluster_scan_anchor_slot())
        .await
        .map_err(|e| e.to_string())?;
    Ok(parse_scan_result(&raw))
}

/// Fill a bounded sample window with `SCAN`, stopping as soon as the window is
/// full or the cursor wraps.
///
/// Both guards below exist because the loop's two natural exits can fail to
/// happen against a proxy or a downgraded replica that keeps handing out a
/// non-zero cursor with no keys: `type_distribution` runs on every db switch and
/// holds this connection's write lock while it runs, so an unbounded scan would
/// stall every other command on the connection. Leaving early is always
/// honest — `sampled` stays the number of keys actually typed, which makes
/// `truncated` report the gap.
async fn collect_sample<C>(
    conn: &mut C,
    limit: u64,
    topology: Topology,
) -> Result<Vec<String>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let limit = limit as usize;
    let mut keys: Vec<String> = Vec::new();
    let mut cursor = 0u64;
    let mut rounds = 0u32;
    let mut stalled = 0u32;
    loop {
        let (next, batch) = scan_round(conn, cursor, topology).await?;
        rounds += 1;
        let seen_before = keys.len();
        keys.extend(batch);
        cursor = next;
        stalled = if keys.len() == seen_before {
            stalled + 1
        } else {
            0
        };
        if keys.len() >= limit || cursor == 0 {
            break;
        }
        if rounds >= MAX_SCAN_ROUNDS || stalled >= MAX_STALLED_SCAN_ROUNDS {
            tracing::warn!(
                rounds,
                stalled,
                cursor,
                limit,
                "redis type_distribution: SCAN stopped making progress, sampling what we have"
            );
            break;
        }
    }
    // SCAN may hand the same key out twice across iterations; counting it twice
    // would skew the distribution, so the sample window is de-duplicated.
    keys.sort();
    keys.dedup();
    keys.truncate(limit);
    Ok(keys)
}

/// Resolve `TYPE` for the sample: [`TYPE_PIPELINE_CHUNK`]-sized pipelines, or
/// one addressed command per key on a cluster connection (see the module docs).
pub(crate) async fn sample_types<C>(
    conn: &mut C,
    keys: &[String],
    topology: Topology,
) -> Result<BTreeMap<String, u64>, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    if matches!(topology, Topology::Cluster) {
        for key in keys {
            let mut cmd = redis::cmd("TYPE");
            cmd.arg(key);
            // Keys returned by the pinned shard's `SCAN` hash into that shard's
            // slot range, so addressing by the key's own slot keeps the whole
            // sample on one node.
            let slot = get_slot(key.as_bytes());
            if let Some(token) = parse_type_token(&routed_single(conn, &cmd, slot).await?) {
                *counts.entry(token).or_insert(0) += 1;
            }
        }
        return Ok(counts);
    }
    for chunk in keys.chunks(TYPE_PIPELINE_CHUNK) {
        let mut pipe = redis::Pipeline::with_capacity(chunk.len());
        for key in chunk {
            pipe.cmd("TYPE").arg(key);
        }
        let values = pipeline_raw(conn, &pipe).await?;
        if values.len() != chunk.len() {
            tracing::warn!(
                expected = chunk.len(),
                replied = values.len(),
                "redis type_distribution: short TYPE pipeline reply"
            );
        }
        for value in &values {
            if let Some(token) = parse_type_token(value) {
                *counts.entry(token).or_insert(0) += 1;
            }
        }
    }
    Ok(counts)
}

/// Cursor-sampled type distribution for the selected database.
///
/// On a cluster the sample covers the single pinned shard of
/// [`CLUSTER_SCAN_ANCHOR`] while `dbsize` is the whole cluster's count — see
/// [`scan_round`] and [`fetch_dbsize`] for why each is addressed the way it is.
pub async fn type_distribution<C>(
    conn: &mut C,
    requested_limit: Option<u64>,
    topology: Topology,
) -> Result<TypeDistribution, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let limit = sample_window_for(requested_limit, topology);
    let dbsize = fetch_dbsize(conn).await?;
    let keys = collect_sample(conn, limit, topology).await?;
    let counts = sample_types(conn, &keys, topology).await?;
    let type_round_trips = match topology {
        Topology::Cluster => keys.len(),
        Topology::Standalone | Topology::Sentinel => keys.len().div_ceil(TYPE_PIPELINE_CHUNK),
    };
    let result = TypeDistribution::from_sample(counts, dbsize);
    tracing::info!(
        dbsize = result.dbsize,
        sampled = result.sampled,
        truncated = result.truncated,
        type_round_trips,
        anchor_slot = match topology {
            Topology::Cluster => cluster_scan_anchor_slot(),
            Topology::Standalone | Topology::Sentinel => 0,
        },
        topology = ?topology,
        "redis type_distribution done"
    );
    Ok(result)
}
