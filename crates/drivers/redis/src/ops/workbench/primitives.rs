//! Shared cursor / pipeline primitives: the sample-window ladder, the cluster scan anchor, and every batch-size cap the two commands size themselves from.

use crate::connect::Topology;
#[allow(unused_imports)]
use redis::cluster_routing::{get_slot, Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};

/// Default sample window for `type_distribution` when `sampleLimit` is omitted.
pub const DEFAULT_TYPE_SAMPLE_LIMIT: u64 = 1_000;

/// Hard ceiling for `type_distribution`. Larger requests are clamped to it and
/// never rejected, so a stale UI preference cannot hammer a large keyspace.
pub const MAX_TYPE_SAMPLE_LIMIT: u64 = 5_000;

/// Sample ceiling on a cluster connection. `TYPE` cannot be batched across keys
/// there (see the module docs), so every sampled key costs a round trip and the
/// window is kept an order of magnitude smaller than [`MAX_TYPE_SAMPLE_LIMIT`].
/// `sampled` / `truncated` keep their exact meaning — the sample is just
/// shorter, never presented as more.
pub const CLUSTER_TYPE_SAMPLE_LIMIT: u64 = 200;

/// `SCAN COUNT` used while filling the sample window.
pub const TYPE_SCAN_COUNT: u32 = 500;

/// Key whose hash slot a cluster sample is pinned to. The key is never sent to
/// the server — only its slot is used to pick a node — but it is named so the
/// choice is reproducible and cannot collide with a real workload key.
///
/// `SCAN` carries no cluster route (redis' own table answers `None` ⇒ a random
/// node per round), which would feed one node's cursor to another node. Pinning
/// every round to this slot keeps a cluster sample a *single shard's* sample.
pub const CLUSTER_SCAN_ANCHOR: &str = "datazen:redis:workbench:sample-anchor";

/// The shard a cluster sample covers — see [`CLUSTER_SCAN_ANCHOR`]. Stable
/// across calls, so consecutive `type_distribution` results describe the same
/// node and stay comparable over refreshes.
pub fn cluster_scan_anchor_slot() -> u16 {
    get_slot(CLUSTER_SCAN_ANCHOR.as_bytes())
}

/// Keys per `TYPE` pipeline: one round trip per 500 sampled keys.
pub const TYPE_PIPELINE_CHUNK: usize = 500;

/// Commands in the single `key_object_info` pipeline.
pub const KEY_INFO_PIPELINE_LEN: usize = 6;

/// Absolute cap on `SCAN` round trips per `type_distribution`. Filling the
/// largest window needs `MAX_TYPE_SAMPLE_LIMIT / TYPE_SCAN_COUNT` = 10 rounds;
/// the headroom covers sparse keyspaces, and the cap is what stops a server
/// (proxy, downgraded replica) whose cursor never wraps from pinning this
/// connection forever — the context bar re-runs on every db switch.
pub const MAX_SCAN_ROUNDS: u32 = 64;

/// Stop sampling after this many consecutive `SCAN` rounds that return no new
/// key. A healthy cursor either yields keys or reaches the window; neither
/// happens in a stuck loop, and 16 empty rounds is already long enough to
/// conclude it, well before [`MAX_SCAN_ROUNDS`] would.
pub const MAX_STALLED_SCAN_ROUNDS: u32 = 16;

/// PTTL sentinel: the key does not exist.
pub const TTL_MISSING: i64 = -2;

/// PTTL sentinel: the key exists and has no expiry.
pub const TTL_NO_EXPIRY: i64 = -1;

/// Clamp a requested sample window: `None` / `Some(0)` fall back to the
/// default, anything above [`MAX_TYPE_SAMPLE_LIMIT`] is silently capped.
pub fn resolve_type_sample_limit(requested: Option<u64>) -> u64 {
    match requested {
        None | Some(0) => DEFAULT_TYPE_SAMPLE_LIMIT,
        Some(n) => n.min(MAX_TYPE_SAMPLE_LIMIT),
    }
}

/// Sample window actually used for a topology: same clamping rules, but a
/// cluster connection is additionally capped at [`CLUSTER_TYPE_SAMPLE_LIMIT`]
/// because typing a key there is one round trip.
pub fn sample_window_for(requested: Option<u64>, topology: Topology) -> u64 {
    let limit = resolve_type_sample_limit(requested);
    match topology {
        Topology::Cluster => limit.min(CLUSTER_TYPE_SAMPLE_LIMIT),
        Topology::Standalone | Topology::Sentinel => limit,
    }
}

/// Whether a distribution is an estimate rather than a census.
///
/// Drives the mandatory "采样 N/M" annotation; `sampled >= dbsize` (a small db
/// scanned in full) is the only case the UI may present as an exact count.
///
/// This one rule is also what makes a cluster answer honest, and it is the
/// reason no cluster-only variant is needed any more: on a cluster `sampled`
/// comes from one pinned shard ([`CLUSTER_SCAN_ANCHOR`]) while `dbsize` is the
/// `Aggregate(Sum)` over every master, so equality can only be reached when the
/// whole cluster's keys happen to live on the shard that was scanned.
pub fn is_sample_truncated(sampled: u64, dbsize: u64) -> bool {
    sampled < dbsize
}
