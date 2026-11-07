//! The batch-transport arms: on a folding `ClusterConnection` a cross-key batch is not even routable, so `key_object_info` degrades per field over addressed singles and keeps working when the key is gone.

use super::*;

// --- the ops on that transport -----------------------------------------

/// Premise for the topology branch: on this transport the *old* batch shape
/// cannot work at all, so a regression back to it must be caught here.
#[tokio::test]
async fn a_cluster_batch_is_not_even_routable_for_the_key_info_probes() {
    // `route_for_pipeline` stops at the first pair of different shards and the
    // batch is never written: the key-info pipeline mixes the token-routed
    // probes (`slot("USAGE")`, `slot("ENCODING")`, …) with `PTTL`/`TYPE` at the
    // key's own shard, so it fails on the very first pair.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("MEMORY", 10);
    let err = pipeline_raw(&mut conn, &build_key_info_pipeline("k"))
        .await
        .expect_err("a batch the table cannot route to one shard never runs");
    assert!(
        err.contains("crossed slots"),
        "expected the CrossSlot the cluster client raises, got {err}"
    );
    assert!(
        conn.journal().batches.is_empty(),
        "a batch that cannot be routed is never sent: {:?}",
        conn.journal().batches
    );

    // Same shard for every command, one errored item: still fatal, because
    // `try_pipeline_request` folds the vector with `extract_error_vec`. That is
    // BUG-001's original symptom, and it is why per-item degradation needs the
    // one-at-a-time path on a cluster.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_str("TYPE", "string");
    conn.inner
        .push("TYPE", err_reply("freq counter is not available"));
    let mut pipe = redis::Pipeline::new();
    pipe.cmd("TYPE").arg("one-key").cmd("TYPE").arg("one-key");
    let err = pipeline_raw(&mut conn, &pipe)
        .await
        .expect_err("OBJECT FREQ-class errors are folded into the whole batch");
    assert!(err.contains("freq counter"), "got {err}");
    assert_eq!(
        conn.journal().batches.len(),
        1,
        "the batch was sent and then folded, not skipped"
    );

    // Contrast: the same two commands on a single node keep their error per item.
    let mut single = ScriptedConn::new();
    single.push_str("TYPE", "string");
    single.push("TYPE", err_reply("freq counter is not available"));
    let values = pipeline_raw(&mut single, &pipe)
        .await
        .expect("a `MultiplexedConnection` leaves item errors inside the vector");
    assert_eq!(values.len(), 2);
    assert!(is_unusable_reply(&values[1]));

    // A cross-key TYPE batch — the shape `sample_types` used to build — is not
    // routable either.
    let mut conn = ClusterFoldingConn::new();
    let mut pipe = redis::Pipeline::new();
    for key in ["slot-a-key", "slot-b-key"] {
        pipe.cmd("TYPE").arg(key);
    }
    let err = pipeline_raw(&mut conn, &pipe)
        .await
        .expect_err("a cross-key TYPE batch cannot be routed to one slot");
    assert!(
        err.contains("crossed slots"),
        "expected the CrossSlot failure the cluster client raises, got {err}"
    );
    assert!(
        conn.journal().batches.is_empty(),
        "a batch that cannot be routed is never sent: {:?}",
        conn.journal().batches
    );
}

#[tokio::test]
async fn cluster_key_object_info_degrades_per_field_over_six_addressed_singles() {
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("MEMORY", 88);
    conn.inner.push_str("OBJECT", "listpack");
    conn.inner.push_int("OBJECT", 12);
    conn.inner
        .push("OBJECT", err_reply("freq counter is not available"));
    conn.inner.push_int("PTTL", 60_000);
    conn.inner.push_str("TYPE", "list");

    let key = "lqueue";
    let info = key_object_info(&mut conn, key, Topology::Cluster)
        .await
        .expect("the sidebar must survive a non-LFU cluster, not show a red error");

    assert_eq!(
        info,
        KeyObjectInfo {
            missing: false,
            key_type: Some("list".to_string()),
            memory_bytes: Some(88),
            encoding: Some("listpack".to_string()),
            idle_seconds: Some(12),
            freq: None,
            ttl_ms: 60_000,
        }
    );
    let journal = conn.journal();
    assert!(
        journal.batches.is_empty(),
        "no batch may be sent through a cluster connection: {:?}",
        journal.batches
    );
    assert_eq!(
        journal.total(),
        KEY_INFO_PIPELINE_LEN,
        "the same six commands, one round trip each"
    );
    // BUG-007: the six round trips must be six *addressed* round trips. A
    // regression to `req_packed_command` would still return the right values (the
    // client retries after -MOVED) but would double the cost and rebuild the slot
    // map, so both counters are part of the contract.
    assert_eq!(
        conn.addressed_names(),
        vec!["MEMORY", "OBJECT", "OBJECT", "OBJECT", "PTTL", "TYPE"],
        "every probe must go through the addressed path, in pipeline order"
    );
    let expected = get_slot(key.as_bytes());
    assert!(
        conn.addressed.iter().all(|(_, slot)| *slot == expected),
        "all six belong to slot {expected}: {:?}",
        conn.addressed
    );
    assert_eq!(conn.misrouted, Vec::<Vec<String>>::new());
    assert_eq!(
        conn.slot_refreshes, 0,
        "an addressed probe must never trigger a slot rebuild"
    );
    assert_eq!(conn.unpinned_rounds, 0);
}

#[tokio::test]
async fn cluster_key_object_info_keeps_working_when_the_key_is_gone() {
    // `TYPE` answers `none` while the attribute commands answer with an error —
    // the missing branch has to be reached through the addressed path too.
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push("MEMORY", RValue::Nil);
    conn.inner.push("OBJECT", err_reply("no such key"));
    conn.inner.push("OBJECT", err_reply("no such key"));
    conn.inner.push("OBJECT", err_reply("no such key"));
    conn.inner.push_int("PTTL", -2);
    conn.inner.push_str("TYPE", "none");

    let info = key_object_info(&mut conn, "gone:soon", Topology::Cluster)
        .await
        .expect("a missing key is a success case on every topology");
    assert_eq!(info, KeyObjectInfo::missing());
    assert_eq!(conn.addressed.len(), KEY_INFO_PIPELINE_LEN);
    assert_eq!(conn.slot_refreshes, 0);
}
