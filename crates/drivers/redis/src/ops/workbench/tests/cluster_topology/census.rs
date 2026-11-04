//! The sampling path (`type_distribution` / `memory_sample`) on a cluster: one shard and one cursor space per scan round, `DBSIZE` left to the client's all-master fan-out, and a bounded window because every key costs a round trip.

use super::*;

#[tokio::test]
async fn cluster_type_distribution_types_keys_one_at_a_time_on_their_own_shards() {
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 40_000);
    let keys = ["user:1", "session:ab", "cart:9", "flag:x", "metric:qps"];
    conn.inner.push_scan(0, &keys);
    conn.inner
        .push_types(&["string", "hash", "list", "string", "zset"]);

    let dist = type_distribution(&mut conn, Some(100), Topology::Cluster)
        .await
        .expect("a multi-key sample must not die on slot routing");

    assert_eq!(dist.counts.get("string"), Some(&2));
    assert_eq!(dist.sampled, 5);
    assert_eq!(dist.sampled, dist.counts.values().sum::<u64>());
    assert!(dist.truncated, "5 of 40000 keys is a sample");
    let journal = conn.journal();
    assert!(
        journal.batches.is_empty(),
        "TYPE is never batched across keys here: {:?}",
        journal.batches
    );
    assert_eq!(
        journal.count_single("TYPE"),
        keys.len(),
        "one TYPE command per sampled key"
    );
    // The keys a shard's SCAN returned hash to that shard's range, so typing them
    // by their own slot keeps the sample on one node — and proves the ops did not
    // take the shortcut of pinning every TYPE at the anchor slot.
    assert_eq!(conn.slots_for("SCAN"), vec![cluster_scan_anchor_slot()]);
    // `collect_sample` sorts and de-duplicates before typing, so the addressed
    // order is the sorted window, not the SCAN page order.
    let mut window: Vec<&str> = keys.to_vec();
    window.sort_unstable();
    let typed_slots: Vec<u16> = window.iter().map(|k| get_slot(k.as_bytes())).collect();
    assert_eq!(
        conn.addressed
            .iter()
            .filter(|(args, _)| cmd_name(args) == "TYPE")
            .map(|(_, slot)| *slot)
            .collect::<Vec<_>>(),
        typed_slots,
        "each TYPE must be addressed at its own key's shard"
    );
    assert!(
        typed_slots.iter().any(|slot| *slot != typed_slots[0]),
        "test fixture must straddle two shards, otherwise this proves nothing"
    );
    assert_eq!(conn.misrouted, Vec::<Vec<String>>::new());
    assert_eq!(conn.slot_refreshes, 0);
}

#[tokio::test]
async fn cluster_sample_window_is_bounded_because_every_key_costs_a_round_trip() {
    // 500 keys on the first SCAN page: standalone would type them in one batch,
    // a cluster connection may not be left with 500 sequential round trips.
    //
    // BUG-008: 200 is pinned by its literal, not by the constant, because the
    // number is quoted outside this crate — `commands.rs` advertises it in the
    // `sampleLimit` description and Wave 2 budgets `sampled ≤ 200` from it.
    assert_eq!(
        CLUSTER_TYPE_SAMPLE_LIMIT, 200,
        "changing the cluster budget means re-deriving the UI wording and the \
         per-key round-trip cost the clamp exists to bound"
    );
    let page: Vec<String> = (0..500).map(|i| format!("k{i}")).collect();
    let flat: Vec<&str> = page.iter().map(String::as_str).collect();

    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 100_000);
    conn.inner.push_scan(0, &flat);
    conn.inner
        .push_types(&vec!["string"; CLUSTER_TYPE_SAMPLE_LIMIT as usize]);

    let dist = type_distribution(&mut conn, Some(u64::MAX), Topology::Cluster)
        .await
        .expect("an oversized window is clamped to the cluster budget");

    assert_eq!(dist.sampled, CLUSTER_TYPE_SAMPLE_LIMIT);
    assert!(dist.truncated);
    let journal = conn.journal();
    assert_eq!(
        journal.count_single("TYPE"),
        CLUSTER_TYPE_SAMPLE_LIMIT as usize,
        "the clamp must actually bound the round trips"
    );
    assert_eq!(
        journal.count_single("SCAN"),
        1,
        "one pinned round trip is enough to fill the window"
    );

    // Contrast: the same keyspace on a single node keeps the pipeline shape and
    // the wider window — it types all 500 keys in one round trip per chunk.
    let mut single = ScriptedConn::new();
    single.push_int("DBSIZE", 100_000);
    single.push_scan(0, &flat);
    single.push_types(&vec!["string"; page.len()]);
    let dist = type_distribution(&mut single, Some(u64::MAX), Topology::Standalone)
        .await
        .expect("standalone keeps the full window");
    assert_eq!(dist.sampled, page.len() as u64, "no cluster budget here");
    assert!(dist.truncated, "500 typed keys of 100000 is still a sample");
    let journal = single.journal();
    assert_eq!(
        journal.batches.len(),
        page.len().div_ceil(TYPE_PIPELINE_CHUNK),
        "one round trip per chunk, none per key"
    );
    assert_eq!(journal.count_single("TYPE"), 0);
}

#[tokio::test]
async fn a_cluster_census_requires_the_whole_cluster_to_have_been_scanned() {
    // `truncated` is the bit the context bar's "采样 N/M" label depends on. On a
    // cluster `sampled` comes from one pinned shard while `dbsize` is the
    // `Aggregate(Sum)` over every master, so `sampled == dbsize` can only mean
    // the cluster's keys all live on the scanned shard — a census. It is *not*
    // forced to true any more, which is why both directions are pinned here.
    let mut cluster = ClusterFoldingConn::new();
    cluster.inner.push_int("DBSIZE", 2);
    cluster.inner.push_scan(0, &["a", "b"]);
    cluster.inner.push_types(&["string", "hash"]);
    let dist = type_distribution(&mut cluster, Some(100), Topology::Cluster)
        .await
        .expect("cluster distribution");
    assert_eq!(dist.dbsize, 2);
    assert_eq!(dist.sampled, 2);
    assert!(
        !dist.truncated,
        "one shard held every key of the cluster and was scanned in full: that is a census"
    );

    // One key on another master and the same sample is an estimate again.
    let mut cluster = ClusterFoldingConn::new();
    cluster.inner.push_int("DBSIZE", 3);
    cluster.inner.push_scan(0, &["a", "b"]);
    cluster.inner.push_types(&["string", "hash"]);
    let dist = type_distribution(&mut cluster, Some(100), Topology::Cluster)
        .await
        .expect("cluster distribution");
    assert!(
        dist.truncated,
        "2 sampled on one shard against 3 in the cluster must never read as exact"
    );

    let mut single = ScriptedConn::new();
    single.push_int("DBSIZE", 2);
    single.push_scan(0, &["a", "b"]);
    single.push_types(&["string", "hash"]);
    let dist = type_distribution(&mut single, Some(100), Topology::Standalone)
        .await
        .expect("standalone distribution");
    assert!(!dist.truncated, "2/2 on one node is a census");
}

#[tokio::test]
async fn an_empty_cluster_reports_an_empty_census() {
    // `DBSIZE` is the cluster-wide sum, so `dbsize: 0` on a cluster means *every*
    // master is empty — the only case in which an empty sample is not an
    // estimate. (An earlier wording, "an empty shard ⇒ dbsize 0 and truncated
    // false", cannot hold on a multi-shard cluster and is replaced by this.)
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 0);
    conn.inner.push_scan(0, &[]);
    let dist = type_distribution(&mut conn, None, Topology::Cluster)
        .await
        .expect("an empty keyspace is not an error");
    assert_eq!(dist.dbsize, 0);
    assert_eq!(dist.sampled, 0);
    assert!(
        !dist.truncated,
        "nothing was withheld from an empty cluster"
    );
    assert_eq!(conn.journal().count_single("DBSIZE"), 1);
}

#[tokio::test]
async fn every_cluster_scan_round_shares_one_shard_and_cursor_space() {
    // Three pages whose cursor only wraps on the last one: a single cursor must
    // run through them, so all three rounds go to the anchor shard.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 6);
    conn.inner.push_scan_batches(&[
        (1, vec!["a".to_string(), "b".to_string()]),
        (2, vec!["c".to_string(), "d".to_string()]),
        (0, vec!["e".to_string(), "f".to_string()]),
    ]);
    conn.inner
        .push_types(&["string", "string", "string", "hash", "hash", "zset"]);

    let dist = type_distribution(&mut conn, Some(500), Topology::Cluster)
        .await
        .expect("cluster sampling");
    assert_eq!(dist.sampled, 6);
    assert_eq!(dist.counts.get("zset"), Some(&1));

    let scan_slots = conn.slots_for("SCAN");
    assert_eq!(scan_slots.len(), 3, "one addressed round per page");
    assert!(
        scan_slots.windows(2).all(|pair| pair[0] == pair[1]),
        "a cursor from one shard must never be handed to another: {scan_slots:?}"
    );
    assert_eq!(scan_slots[0], cluster_scan_anchor_slot());
    assert_eq!(
        conn.unpinned_rounds, 0,
        "no round may be left to the table's random node"
    );
    // BUG-008, second half: the anchor is a slot we chose, so pin its value too.
    assert_eq!(
        cluster_scan_anchor_slot(),
        get_slot(CLUSTER_SCAN_ANCHOR.as_bytes()),
        "the anchor slot must come from the named key"
    );
}

#[tokio::test]
async fn a_transport_failure_on_the_addressed_path_aborts_the_probe() {
    // Per-command degradation must not swallow the connection: a dropped link is
    // a failed read, not a key with no attributes. This exercises the addressed
    // leg, which is where the cluster path now spends every round trip.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 10);
    conn.inner.push_scan(0, &["a", "b"]);
    conn.inner.push_type("string");
    conn.transport_failure_for = Some("TYPE".to_string());

    let err = type_distribution(&mut conn, Some(10), Topology::Cluster)
        .await
        .expect_err("a dropped connection must reach the caller");
    assert!(err.contains("connection"), "got {err}");
    assert!(
        conn.journal().batches.is_empty(),
        "nothing may be batched on this path either way"
    );
    assert!(
        conn.addressed
            .iter()
            .any(|(args, _)| cmd_name(args) == "TYPE"),
        "the failure must come from the addressed path: {:?}",
        conn.addressed
    );
    assert_eq!(
        conn.slot_refreshes, 0,
        "a transport failure is not a routing failure: {:?}",
        conn.misrouted
    );
}

#[tokio::test]
async fn sentinel_uses_the_single_node_batch_and_its_per_item_errors() {
    // A sentinel-backed master is a `MultiplexedConnection`, so it keeps the one
    // pipeline shape (and the one-round-trip budget the sidebar assumes).
    let mut conn = ScriptedConn::new();
    conn.push_int("MEMORY", 10);
    conn.push_str("OBJECT", "embstr");
    conn.push_int("OBJECT", 1);
    conn.push("OBJECT", err_reply("freq counter is not available"));
    conn.push_int("PTTL", -1);
    conn.push_str("TYPE", "string");

    let info = key_object_info(&mut conn, "k", Topology::Sentinel)
        .await
        .expect("sentinel degrades the freq field like standalone");
    assert_eq!(info.freq, None);
    assert_eq!(info.key_type.as_deref(), Some("string"));
    let journal = conn.journal();
    assert_eq!(journal.total(), 1, "one pipeline, no per-command traffic");
    assert!(journal.singles.is_empty());
}
