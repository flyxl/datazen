//! The routing facts the cluster path is built on: redis' own table sends the two-word probes to a shard that does not hold the key, so addressing them is a correctness requirement rather than a cost choice.

use super::*;

// --- the routing facts the cluster path is built on ---------------------

/// Premise for every test below, asked of redis' own table rather than asserted
/// by hand: the four two-word probes are routed by their **subcommand token**, so
/// sending them unaddressed is not a cost question but a correctness one.
#[test]
fn redis_table_routes_the_two_word_probes_to_a_shard_that_does_not_hold_the_key() {
    let key = "kvbar:user:1001";
    let real = get_slot(key.as_bytes());
    for segments in [
        ["MEMORY", "USAGE"],
        ["OBJECT", "ENCODING"],
        ["OBJECT", "IDLETIME"],
        ["OBJECT", "FREQ"],
    ] {
        let mut cmd = redis::cmd(segments[0]);
        for segment in &segments[1..] {
            cmd.arg(*segment);
        }
        cmd.arg(key);
        let TableRoute::Shard(route) = table_route(&cmd) else {
            panic!(
                "{} {} should still route to one specific node, got {:?}",
                segments[0],
                segments[1],
                table_route(&cmd)
            );
        };
        // (1) the question the *server* asks, and (2) which shard the table
        // really named — recovered from `get_slot` because `Route::slot` is
        // `pub(crate)` in redis 0.27.6.
        assert!(
            !table_names_the_key_shard(&cmd, &route),
            "{} {} is routed by its key after all: {route:?} vs slot {real}",
            segments[0],
            segments[1]
        );
        let token = segments[1];
        let token_slot = get_slot(token.as_bytes());
        assert_ne!(
            token_slot, real,
            "{token} coincidentally hashes to the key's shard — this fixture proves nothing"
        );
        assert!(
            route == Route::new(token_slot, SlotAddr::Master)
                || route == Route::new(token_slot, SlotAddr::ReplicaOptional),
            "{} {} is routed to a shard this test cannot explain: {route:?}, token slot {token_slot}",
            segments[0],
            segments[1]
        );
    }

    // The keyed probes are the control arm: the table *does* send these to the
    // shard that holds the key, so addressing them is only about skipping the
    // table, not about fixing a wrong answer.
    for name in ["TYPE", "PTTL"] {
        let mut cmd = redis::cmd(name);
        cmd.arg(key);
        let TableRoute::Shard(route) = table_route(&cmd) else {
            panic!("{name} must route to a specific node");
        };
        assert!(
            table_names_the_key_shard(&cmd, &route),
            "{name} is keyed: got {route:?} against slot {real}"
        );
    }
}

/// The other half of the addressing fact: what **we** hand redis once we do say.
///
/// No in-process double can observe `impl SlotRoutedConnection for
/// ClusterConnection` — the doubles implement the trait themselves, so the use of
/// the public `route_command` needs a real cluster. What *is* checkable is the
/// only value that impl produces: the route built from a slot must name that
/// slot's master and nothing else. A drift to `ReplicaOnly`, to `Any`, or to a
/// different slot would otherwise surface only as `-MOVED` storms on a real
/// cluster, which is the cost BUG-007 is about.
#[test]
fn the_address_we_build_names_exactly_the_master_of_the_given_slot() {
    for slot in [0u16, 1, LAST_SLOT, cluster_scan_anchor_slot()] {
        let built = master_route(slot);
        let RoutingInfo::SingleNode(SingleNodeRoutingInfo::SpecificNode(route)) = &built else {
            panic!("a probe address must name exactly one node, got {built:?}");
        };
        assert_eq!(
            *route,
            Route::new(slot, SlotAddr::Master),
            "slot {slot} must be addressed at its master: {built:?}"
        );
    }
}

/// The tripwire that replaces the old optimistic double: an **unaddressed** probe
/// is refused by the shard that received it, at the price of a slot rebuild. If
/// production ever stops addressing, `cluster_key_object_info_*` turns red for
/// this reason.
#[tokio::test]
async fn an_unaddressed_two_word_probe_is_moved_and_rebuilds_the_slot_map() {
    let key = "kvbar:user:1001";
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("MEMORY", 104);

    let error = conn
        .req_packed_command(&{
            let mut cmd = redis::cmd("MEMORY");
            cmd.arg("USAGE").arg(key);
            cmd
        })
        .await
        .expect_err("the node owning slot(USAGE) has no such key");
    assert_eq!(
        error.kind(),
        ErrorKind::Moved,
        "redis answers -MOVED and `request.rs:212-220` turns that into RebuildSlots"
    );
    assert_eq!(
        conn.slot_refreshes, 1,
        "one slot rebuild per misrouted probe"
    );
    assert_eq!(conn.misrouted.len(), 1);
    assert!(
        conn.journal().singles.is_empty(),
        "the scripted value is never read: the key is not on that node"
    );

    // Addressed, the same command runs on the shard that has the key — one round
    // trip, no redirect, no rebuild.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("MEMORY", 104);
    let mut cmd = redis::cmd("MEMORY");
    cmd.arg("USAGE").arg(key);
    let value = conn
        .command_at_slot(&cmd, get_slot(key.as_bytes()))
        .await
        .expect("addressed at the key's own shard");
    assert_eq!(value, RValue::Int(104));
    assert_eq!(conn.addressed.len(), 1);
    assert_eq!(conn.slot_refreshes, 0);
    assert!(conn.misrouted.is_empty());
}

/// Addressing is *checked*, not just recorded: pinning every command at one
/// convenient slot would satisfy a "did you pass a slot?" assertion while
/// quietly reading attributes of keys that are not there.
#[tokio::test]
async fn addressing_a_probe_at_the_wrong_shard_is_refused_too() {
    let key = "kvbar:session:abcd";
    let real = get_slot(key.as_bytes());
    let wrong = if real == LAST_SLOT {
        real - 1
    } else {
        real + 1
    };

    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_str("TYPE", "string");
    let mut cmd = redis::cmd("TYPE");
    cmd.arg(key);
    let error = conn
        .command_at_slot(&cmd, wrong)
        .await
        .expect_err("slot {wrong} does not hold this key");
    assert_eq!(error.kind(), ErrorKind::Moved);
    assert_eq!(
        conn.misrouted,
        vec![vec!["TYPE".to_string(), key.to_string()]]
    );
    assert_eq!(conn.slot_refreshes, 1);
    assert!(
        conn.journal().singles.is_empty(),
        "a refused command must not be answered from the script"
    );
}
