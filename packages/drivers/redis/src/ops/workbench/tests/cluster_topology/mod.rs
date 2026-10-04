// Cluster / Sentinel topology tests for `ops_workbench` — declared from
// `tests.rs`, so it reuses `ScriptedConn`, `args_of`, `normalized` and the
// journal helpers there.
//
// Two bugs live in this file, and the second one is a bug about what a *double*
// may assume:
//
// * BUG-001 / BUG-002 — a `ClusterConnection` folds item errors a *second* time
//   (`cluster_async/mod.rs:668-681`, `:658-662`) and pins a pipeline to one slot
//   (`cluster_async/routing.rs:71-105`). `ClusterFoldingConn` reproduces both
//   behaviours on top of the scripted replies instead of hiding them.
// * BUG-007 — the first version of this double routed a command by "its last
//   argument", i.e. it *assumed* the client routes by key. redis does not for
//   `MEMORY USAGE` / `OBJECT *`: its table falls through to `arg_idx(1)`, so the
//   shard is picked from the subcommand token. The optimistic assumption made
//   that class of defect unobservable in-process, which is exactly what the
//   round's cost claims were built on. The double now keeps the two facts apart:
//
//     - [`keyed_probe_slot`] is the **server's** truth — where the data lives. It
//       reads the argument the ops place the key in, a shape pinned by
//       `key_info_pipeline_is_one_batch_and_never_scans_the_keyspace`;
//     - [`table_route`] is the **client's** notion of where to send it, answered
//       by redis' own public `RoutingInfo::for_routable`, never by a guess.
//
//   Where the two disagree the double answers the way the server does: `-MOVED`,
//   which the client would then act on with `RebuildSlots`
//   (`cluster_async/request.rs:212-220`, and `refresh_slots` starts by taking the
//   connection's write lock). That is what makes "the probes are addressed"
//   falsifiable instead of decorative.

use super::*;

use redis::aio::ConnectionLike;
use redis::cluster_routing::{
    get_slot, MultipleNodeRoutingInfo, Routable, Route, RoutingInfo, SingleNodeRoutingInfo,
    SlotAddr,
};
use redis::{ErrorKind, RedisError, RedisFuture};

/// Redis' own slot count; used only to pick "some shard that is not this one".
const LAST_SLOT: u16 = 16_383;

/// Which shard the **data** of a keyed probe lives on.
///
/// `None` for the commands that have no key (`SCAN`, `DBSIZE`), and for any
/// unexpected shape — an argument the ops do not document must not be silently
/// treated as a key. The exact arities are asserted in `tests.rs`, so this is a
/// statement about the server, not about routing.
fn keyed_probe_slot(cmd: &redis::Cmd) -> Option<u16> {
    let args = args_of(cmd);
    let name = args.first()?.to_ascii_uppercase();
    let key_index = match name.as_str() {
        "TYPE" | "PTTL" if args.len() == 2 => 1,
        "MEMORY" | "OBJECT" if args.len() == 3 => 2,
        _ => return None,
    };
    Some(get_slot(Routable::arg_idx(cmd, key_index)?))
}

/// Where redis' own routing table sends a command.
#[derive(Debug, Clone, PartialEq)]
enum TableRoute {
    /// A specific node, picked by the table.
    Shard(Route),
    /// Every master (`MultiNode(AllMasters, …)`) — `DBSIZE`'s aggregate.
    FanOut,
    /// Nothing in the table ⇒ `req_packed_command` falls back to
    /// `SingleNodeRoutingInfo::Random`, i.e. an arbitrary node.
    ArbitraryNode,
}

/// `RoutingInfo::for_routable`, i.e. redis' answer to "where would the client put
/// this command if we did not say". Public API, so the double's routing model is
/// no longer a free variable — this is the `BUG-007` concern spelled out at the
/// top of this file.
fn table_route(cmd: &redis::Cmd) -> TableRoute {
    match RoutingInfo::for_routable(cmd) {
        Some(RoutingInfo::SingleNode(SingleNodeRoutingInfo::SpecificNode(route))) => {
            TableRoute::Shard(route)
        }
        Some(RoutingInfo::MultiNode((
            MultipleNodeRoutingInfo::AllMasters | MultipleNodeRoutingInfo::AllNodes,
            _,
        ))) => TableRoute::FanOut,
        _ => TableRoute::ArbitraryNode,
    }
}

/// Does a table route name the shard that actually holds the command's key?
///
/// `Route`'s slot is private (`slot()` is `pub(crate)` in redis 0.27.6), so the
/// answer comes from equality against the only two routes `get_route` can build
/// for that key: `is_readonly_cmd` selects `Master` or `ReplicaOptional`, and
/// this driver never enables `read_from_replicas` (`connect.rs` builds a plain
/// `ClusterClient`), so both resolve to the primary — this is the `BUG-007` exclusion
/// recorded at the top of this file.
fn table_names_the_key_shard(cmd: &redis::Cmd, route: &Route) -> bool {
    keyed_probe_slot(cmd).is_some_and(|slot| {
        *route == Route::new(slot, SlotAddr::Master)
            || *route == Route::new(slot, SlotAddr::ReplicaOptional)
    })
}

/// `route_for_pipeline` (`cluster_async/routing.rs:71-105`), rebuilt from
/// [`table_route`]: the first command with a specific route picks the shard, and
/// any later command routed to a *different* shard fails the whole batch.
///
/// redis compares slots and keeps the `SlotAddr` only to break ties; this
/// compares whole routes, which is equivalent for every batch built here because
/// all six probes are read-only per `is_readonly_cmd`, so their `SlotAddr` is
/// uniform.
fn route_for_batch(pipe: &redis::Pipeline) -> Result<Option<Route>, RedisError> {
    let specific = |cmd: &redis::Cmd| match table_route(cmd) {
        TableRoute::Shard(route) => Some(route),
        _ => None,
    };
    pipe.cmd_iter().map(specific).try_fold(
        None,
        |chosen: Option<Route>, next: Option<Route>| match (chosen, next) {
            (None, _) => Ok(next),
            (_, None) => Ok(chosen),
            (Some(chosen), Some(next)) if chosen != next => Err(RedisError::from((
                ErrorKind::CrossSlot,
                "Received crossed slots in pipeline",
            ))),
            // Same slot: redis would prefer a `Master` route here, which never
            // happens among read-only probes, so the batch stays routable.
            (Some(_), next) => Ok(next),
        },
    )
}

fn cmd_name(args: &[String]) -> String {
    args.first().cloned().unwrap_or_default()
}

fn transport_error(detail: &str) -> RedisError {
    RedisError::from((ErrorKind::IoError, "Connection failed", detail.to_string()))
}

/// The reply a server gives when the command arrived at a shard that does not
/// hold the key — and with it the `RebuildSlots` the client performs afterwards.
fn moved_error(slot: u16) -> RedisError {
    RedisError::from((
        ErrorKind::Moved,
        "An existing key was moved to a different node",
        format!("{slot} 127.0.0.1:7001"),
    ))
}

/// What the dispatch layer decides a command is going to hit.
enum Delivery {
    /// Run it and answer from the script.
    Run,
    /// The node has no such key: `-MOVED`, and one slot rebuild follows.
    Moved(u16),
    /// No route in the table ⇒ an arbitrary node answers. `SCAN` landing here is
    /// how one node's cursor ends up on another; nothing errors, which is the
    /// dangerous part, so it is counted instead.
    ArbitraryNode,
}

/// A `ScriptedConn` that answers like redis' async cluster connection.
struct ClusterFoldingConn {
    inner: ScriptedConn,
    /// Command name whose dispatch fails at the transport level, i.e. the probe
    /// never ran — as opposed to being answered with an error.
    transport_failure_for: Option<String>,
    /// `(args, slot)` for every command the ops **addressed** themselves.
    addressed: Vec<(Vec<String>, u16)>,
    /// Commands that reached a shard without the key, whether because the table
    /// picked the token's shard or because an address named the wrong one.
    misrouted: Vec<Vec<String>>,
    /// `RebuildSlots` triggered by the above — the cost BUG-007 is about, since
    /// `refresh_slots` holds the connection's write lock while it re-queries
    /// every node.
    slot_refreshes: usize,
    /// `SCAN` rounds left to the table, i.e. rounds whose shard is unknown.
    unpinned_rounds: usize,
}

impl ClusterFoldingConn {
    fn new() -> Self {
        Self {
            inner: ScriptedConn::new(),
            transport_failure_for: None,
            addressed: Vec::new(),
            misrouted: Vec::new(),
            slot_refreshes: 0,
            unpinned_rounds: 0,
        }
    }

    fn journal(&self) -> Journal {
        self.inner.journal()
    }

    /// Names of the addressed commands, in order.
    fn addressed_names(&self) -> Vec<String> {
        self.addressed
            .iter()
            .map(|(args, _)| cmd_name(args))
            .collect()
    }

    fn slots_for(&self, name: &str) -> Vec<u16> {
        self.addressed
            .iter()
            .filter(|(args, _)| cmd_name(args) == name)
            .map(|(_, slot)| *slot)
            .collect()
    }

    /// The cluster dispatch layer for one command, addressed or not.
    ///
    /// Both shapes end in `try_cmd_request` (`cluster_async/mod.rs:633-655`):
    /// `req_packed_command` gets its route from the table, `route_command` from
    /// the caller, and in both cases an errored reply is folded into `Err` by
    /// `Value::extract_error`.
    async fn deliver(
        &mut self,
        cmd: &redis::Cmd,
        addressed_slot: Option<u16>,
    ) -> Result<RValue, RedisError> {
        let args = normalized(&args_of(cmd));
        let name = cmd_name(&args);
        let key_slot = keyed_probe_slot(cmd);
        let delivery = match addressed_slot {
            // We named the node, so the only way to be wrong is to name a shard
            // that does not hold the key — which is what a blanket "address
            // everything at one slot" shortcut would do.
            Some(slot) => match key_slot {
                Some(real) if real != slot => Delivery::Moved(real),
                _ => Delivery::Run,
            },
            None => match table_route(cmd) {
                TableRoute::FanOut => Delivery::Run,
                TableRoute::Shard(route) if table_names_the_key_shard(cmd, &route) => Delivery::Run,
                // The table named a shard that does not hold the key.
                TableRoute::Shard(_) => match key_slot {
                    Some(real) => Delivery::Moved(real),
                    // A command whose key this file cannot locate is not given a
                    // fabricated `-MOVED`; it is only counted.
                    None => Delivery::ArbitraryNode,
                },
                TableRoute::ArbitraryNode => Delivery::ArbitraryNode,
            },
        };
        match delivery {
            Delivery::Moved(real) => {
                self.misrouted.push(args);
                self.slot_refreshes += 1;
                tracing::debug!(
                    command = %name,
                    key_slot = real,
                    "cluster double: the node that received this command does not hold the key"
                );
                return Err(moved_error(real));
            }
            Delivery::ArbitraryNode => {
                if name == "SCAN" {
                    self.unpinned_rounds += 1;
                } else {
                    self.misrouted.push(args.clone());
                }
            }
            Delivery::Run => {}
        }
        if self
            .transport_failure_for
            .as_deref()
            .is_some_and(|bad| bad.eq_ignore_ascii_case(&name))
        {
            return Err(transport_error("connection reset by peer"));
        }
        let value = self.inner.req_packed_command(cmd).await?;
        match value {
            // redis' own `Value::extract_error` turns the reply into the error it
            // carries, so the detail the server wrote is what reaches us.
            RValue::ServerError(error) => Err(error.into()),
            other => Ok(other),
        }
    }
}

impl ConnectionLike for ClusterFoldingConn {
    /// Left to the table: `ClusterConnection::req_packed_command` =
    /// `for_routable(cmd).unwrap_or(Random)` + one folded answer.
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> RedisFuture<'a, RValue> {
        async move { self.deliver(cmd, None).await }.boxed()
    }

    /// `ClusterConnection::req_packed_commands` = `route_for_pipeline` (one slot
    /// for the whole batch) + `extract_error_vec` (any item error fails it).
    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        // `route_for_pipeline` really does fail before anything is written.
        if let Err(error) = route_for_batch(pipe) {
            return async move { Err(error) }.boxed();
        }
        async move {
            // `extract_error_vec` again: the first item error aborts the batch.
            let mut values = Vec::new();
            for value in self.inner.req_packed_commands(pipe, offset, count).await? {
                match value {
                    RValue::ServerError(server_error) => {
                        // `Value::extract_error` is what turns the reply into an
                        // error here; the cluster layer then aborts the batch.
                        let error: RedisError = server_error.into();
                        tracing::debug!(
                            %error,
                            "cluster dispatch folds this item into the batch error"
                        );
                        return Err(error);
                    }
                    other => values.push(other),
                }
            }
            Ok(values)
        }
        .boxed()
    }

    fn get_db(&self) -> i64 {
        0
    }
}

impl SlotRoutedConnection for ClusterFoldingConn {
    /// `ClusterConnection::route_command(cmd, SpecificNode(Route(slot, Master)))`
    /// — same delivery and same error folding, only the *address* differs, so
    /// this is where the double can see what the ops asked for.
    fn command_at_slot<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
        slot: u16,
    ) -> RedisFuture<'a, RValue> {
        async move {
            self.addressed.push((normalized(&args_of(cmd)), slot));
            self.deliver(cmd, Some(slot)).await
        }
        .boxed()
    }
}

/// The `SCAN` round `collect_sample` issues on a cluster, built the same way.
fn scan_round_cmd(cursor: u64) -> redis::Cmd {
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(cursor).arg("COUNT").arg(TYPE_SCAN_COUNT);
    cmd
}

/// `SCAN` is the silent case: no `-MOVED`, just a different shard per round.
#[tokio::test]
async fn an_unaddressed_scan_round_has_no_pinned_shard() {
    let mut conn = ClusterFoldingConn::new();
    assert_eq!(
        table_route(&scan_round_cmd(0)),
        TableRoute::ArbitraryNode,
        "redis' table must still answer 'no route' for SCAN, or the anchor wording can be relaxed"
    );

    conn.inner.push_scan(0, &["a"]);
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(0u64).arg("COUNT").arg(TYPE_SCAN_COUNT);
    conn.req_packed_command(&cmd)
        .await
        .expect("an arbitrary node answers a scan round happily — that is the problem");
    assert_eq!(conn.unpinned_rounds, 1);
    assert_eq!(
        conn.misrouted,
        Vec::<Vec<String>>::new(),
        "no error is raised, so only a counted shard change can catch this"
    );

    // Addressed at the anchor, the same round contributes to one shard's cursor.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_scan(0, &["a"]);
    conn.command_at_slot(&cmd, cluster_scan_anchor_slot())
        .await
        .expect("addressed scan round");
    assert_eq!(conn.unpinned_rounds, 0);
    assert_eq!(conn.slots_for("SCAN"), vec![cluster_scan_anchor_slot()]);
}

/// `DBSIZE` is deliberately left to the table, because the table is right about
/// it: fan-out to every master plus a sum (`fix_round1_retest`'s `test_tester_`
/// case pins the routing itself; this pins that the ops kept using it).
#[tokio::test]
async fn dbsize_is_left_to_the_clients_all_master_fan_out() {
    let cmd = redis::cmd("DBSIZE");
    assert_eq!(
        table_route(&cmd),
        TableRoute::FanOut,
        "the 口径 depends on DBSIZE being summed over masters"
    );

    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 40_000);
    let total = fetch_dbsize(&mut conn).await.expect("dbsize");
    assert_eq!(total, 40_000);
    assert!(
        conn.addressed.is_empty(),
        "pinning DBSIZE to the sample shard would report one shard as the database: {:?}",
        conn.addressed
    );
    assert_eq!(conn.slot_refreshes, 0);
}

// 本文件的测试面按主题拆成子模块（`use super::*;` 逐层可见，故子模块
// 直接复用上面的 `ClusterFoldingConn` 双替身与共享 helper）：
//   * `routing`  — 路由事实：两词探针必须寻址，否则走错分片。
//   * `batch`    — 折叠式 ClusterConnection 上的批形状与逐字段降级。
//   * `census`   — 采样路径：单分片单游标、DBSIZE 全 master 扇出。
//   * `hash_tag` — [tester] 寻址槽位必须等于 redis 计算的槽位（含 hash tag）。

mod batch;
mod census;
mod hash_tag;
mod routing;
