//! Workbench probes backing the Redis KV context bar and key-inspector
//! sidebar (`type_distribution`, `key_object_info`).
//!
//! Cost shape is the whole point of this module, so both commands are built
//! from the shared cursor / pipeline primitives instead of per-key round trips:
//!
//! - `type_distribution` fills a bounded sample window with `SCAN`
//!   ([`crate::ops::scan_batch`]) and resolves types with chunked pipelines.
//!   **`KEYS` is never issued** — it blocks the server for the entire keyspace.
//!   The reply carries `sampled` / `dbsize` / `truncated` so the UI is forced
//!   to label an estimate as an estimate (PRD §3.4 hard constraint).
//! - `key_object_info` reads every key attribute in **one** pipeline. Failures
//!   stay per command: a key that expired between two renders, a non-LFU
//!   `maxmemory-policy` (rejects `OBJECT FREQ`), or Redis < 4.0 (no
//!   `MEMORY USAGE`) degrades a single field to `None` instead of failing the
//!   command — the sidebar shows an empty slot, not a red error. On a cluster
//!   connection the same six commands go out one at a time, each addressed to
//!   the shard that owns the key (see below), which keeps the degradation
//!   contract and costs six round trips instead of one.
//!
//! # Why replies are read as a batch, and why Cluster is the exception
//!
//! The crate's existing pipelines (e.g. `ops_tree::list_children_on`) go through
//! `Pipeline::query_async`, which is the right call when no command in the batch
//! may fail: it runs the reply vector through `Value::extract_error_vec`, so the
//! *first* errored command fails the whole batch — exactly the behaviour the
//! degradation contract above forbids.
//! [`redis::aio::ConnectionLike::req_packed_commands`] is what
//! `query_async` calls internally, with the same `offset = 0` / `count = len`
//! arguments `execute_pipelined_async` passes; only the error unwrapping is
//! skipped, which lets the per-command errors survive to be inspected.
//!
//! **That last step is what a cluster connection undoes.** `ClusterConnection`
//! folds the batch *again* in its own dispatch layer
//! (`cluster_async::try_pipeline_request` → `Value::extract_error_vec`) and
//! additionally pins a whole pipeline to one slot (`route_for_pipeline`, so a
//! batch of different keys fails with `CrossSlot`). Since `OBJECT FREQ` errors on
//! every non-LFU server, a cluster pipeline would make `key_object_info` fail
//! always and `type_distribution` fail whenever the sample crosses a slot — i.e.
//! the degradation contract cannot be met with a batch there. So on
//! [`Topology::Cluster`] the very same commands are issued one at a time, each
//! addressed to the shard that owns the key. Degradation semantics are then
//! identical; only the cost differs (one round trip per command), which is why
//! the cluster sample window is bounded far tighter — see
//! [`CLUSTER_TYPE_SAMPLE_LIMIT`].
//!
//! # Cluster: every command names the node it runs on
//!
//! Issuing a command through [`ConnectionLike`] is *not* enough on a cluster
//! connection, because redis derives the target node from its own command-name
//! table (`cluster_routing::RoutingInfo::for_routable`), and that table does not
//! describe these probes:
//!
//! * `MEMORY USAGE` and `OBJECT ENCODING|IDLETIME|FREQ` are two-word commands
//!   that are **absent** from it, so the fallback arm (`_ => arg_idx(1)`) reads
//!   the *subcommand token* as the key and sends the probe to whoever owns
//!   `slot("USAGE")`. The server answers `-MOVED`, the client resends, and — the
//!   expensive part — `RebuildSlots` runs `refresh_slots`, which takes the
//!   connection's write lock and re-queries `CLUSTER SLOTS` on every node. A
//!   sidebar that refreshes per key selection would serialise the whole
//!   connection behind that, so an unaddressed probe is not acceptable here even
//!   though the client does eventually return the right value.
//! * `SCAN` is in the table only as an explicit `None` ⇒ `SingleNode(Random)`,
//!   which would hand every sampling round to a different node and feed one
//!   node's cursor to another.
//!
//! So each probe carries its own address, via [`SlotRoutedConnection`] over the
//! public [`redis::cluster_async::ClusterConnection::route_command`] and
//! [`redis::cluster_routing::get_slot`]. Consequences:
//!
//! * a cluster probe costs exactly one round trip per command — no redirects, no
//!   slot rebuilds;
//! * `type_distribution` pins **every** `SCAN` round to
//!   [`cluster_scan_anchor_slot`], so its sample is one shard's sample and stays
//!   comparable between refreshes instead of mixing nodes;
//! * `DBSIZE` *is* in the table, as `MultiNode(AllMasters, Aggregate(Sum))`, so
//!   `dbsize` on a cluster is the **whole cluster's** count for the selected db
//!   (and one unreachable or non-integer master fails the aggregate). That is
//!   the `M` the "采样 N/M" label needs, so it is deliberately left unrouted.
//!
//! With those three facts, [`is_sample_truncated`] needs no cluster special
//! case: `sampled` counts keys on one shard while `dbsize` counts the whole
//! cluster, so `sampled == dbsize` — the only non-truncated answer — can only
//! mean every key of the cluster lives on the shard that was scanned.
//!
//! Upgrading the `redis` dependency is a documented review point: this module
//! relies on `req_packed_commands` (`#[doc(hidden)]` internal API) answering with
//! exactly one reply per command, and [`key_object_info`] rejects a reply vector
//! of any other length rather than mis-reading the slots.

#[allow(unused_imports)]
use std::collections::BTreeMap;
#[allow(unused_imports)]
use std::future::Future;
#[allow(unused_imports)]
use std::pin::Pin;

#[allow(unused_imports)]
use futures_util::FutureExt;
#[allow(unused_imports)]
use redis::aio::ConnectionLike;
#[allow(unused_imports)]
use redis::cluster_routing::{get_slot, Route, RoutingInfo, SingleNodeRoutingInfo, SlotAddr};
#[allow(unused_imports)]
use redis::RedisFuture;
#[allow(unused_imports)]
use redis::Value as RValue;
#[allow(unused_imports)]
use serde::Serialize;

#[allow(unused_imports)]
use crate::connect::Topology;
#[allow(unused_imports)]
use crate::driver::parse_scan_result;

pub(crate) mod distribution;
pub(crate) mod key_info;
pub(crate) mod memory_sample;
pub(crate) mod primitives;
pub(crate) mod shapes;
pub(crate) mod transport;

pub use distribution::type_distribution;
pub use key_info::key_object_info;
#[allow(unused_imports)]
pub use memory_sample::build_memory_sample_pipeline;
pub(crate) use memory_sample::fetch_memory_sample_fields;
#[allow(unused_imports)]
pub use memory_sample::memory_sample_slots;
#[allow(unused_imports)]
pub use memory_sample::parse_memory_sample_fields;
#[allow(unused_imports)]
pub use memory_sample::MemorySampleFields;
#[allow(unused_imports)]
pub use memory_sample::MEMORY_SAMPLE_FIELDS_PER_KEY;
#[allow(unused_imports)]
pub use memory_sample::MEMORY_SAMPLE_KEYS_PER_PIPELINE;
pub use primitives::cluster_scan_anchor_slot;
#[allow(unused_imports)]
pub use primitives::is_sample_truncated;
#[allow(unused_imports)]
pub use primitives::resolve_type_sample_limit;
#[allow(unused_imports)]
pub use primitives::sample_window_for;
#[allow(unused_imports)]
pub use primitives::CLUSTER_SCAN_ANCHOR;
pub use primitives::CLUSTER_TYPE_SAMPLE_LIMIT;
pub use primitives::DEFAULT_TYPE_SAMPLE_LIMIT;
pub use primitives::KEY_INFO_PIPELINE_LEN;
#[allow(unused_imports)]
pub use primitives::MAX_SCAN_ROUNDS;
#[allow(unused_imports)]
pub use primitives::MAX_STALLED_SCAN_ROUNDS;
pub use primitives::MAX_TYPE_SAMPLE_LIMIT;
pub use primitives::TTL_MISSING;
pub use primitives::TTL_NO_EXPIRY;
pub use primitives::TYPE_PIPELINE_CHUNK;
#[allow(unused_imports)]
pub use primitives::TYPE_SCAN_COUNT;
#[allow(unused_imports)]
pub use shapes::build_key_info_pipeline;
pub use shapes::is_unusable_reply;
#[allow(unused_imports)]
pub use shapes::key_info_slots;
#[allow(unused_imports)]
pub use shapes::parse_key_info;
pub use shapes::parse_opt_int;
#[allow(unused_imports)]
pub use shapes::parse_opt_string;
pub use shapes::parse_type_token;
pub use shapes::type_reply_says_absent;
pub use shapes::KeyObjectInfo;
pub use shapes::TypeDistribution;
#[allow(unused_imports)]
pub use transport::SlotRoutedBatchFuture;
pub use transport::SlotRoutedConnection;

#[allow(unused_imports)]
pub(crate) use distribution::sample_types;
#[allow(unused_imports)]
pub(crate) use shapes::slot;
#[allow(unused_imports)]
pub(crate) use shapes::unreadable_key_state;
#[allow(unused_imports)]
pub(crate) use transport::fetch_dbsize;
#[allow(unused_imports)]
pub(crate) use transport::is_connection_level_failure;
#[allow(unused_imports)]
pub(crate) use transport::issue_batch;
#[allow(unused_imports)]
pub(crate) use transport::master_route;
#[allow(unused_imports)]
pub(crate) use transport::pipeline_raw;
#[allow(unused_imports)]
pub(crate) use transport::routed_sequential;
#[allow(unused_imports)]
pub(crate) use transport::routed_single;

#[cfg(test)]
mod tests;
