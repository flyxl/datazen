//! [tester] The property BUG-007 rests on: the slot the ops ask for must be the slot redis computes, and `get_slot` is hash-tag aware — co-located keys share one address, an untagged key does not.

use super::*;

// --- Tester round 3: the slot the ops ask for must be the slot redis computes --

/// Everything BUG-007 rests on is one call, `get_slot(key)`, and `get_slot` is
/// **hash-tag aware**: it CRCs only the bytes between the first `{` and the
/// following `}` (`cluster_routing.rs:250`, `get_hashtag` before the CRC). That
/// is the whole reason `{tenant7}:cart` and `{tenant7}:profile` may be
/// co-located on one shard at all.
///
/// None of the existing cluster cases uses a tagged key, so every one of them
/// passes unchanged while the tag half of that call is being broken: a
/// hand-rolled brace strip (`get_slot(key.strip_prefix('{')…)`) or a CRC over
/// the literal key addresses `{tenant7}:profile` at a master that does not hold
/// it, which is exactly the `-MOVED` + `refresh_slots` (write-lock) storm
/// 裁定 1 forbids. This double is the judge: [`keyed_probe_slot`] reads the slot
/// off the wire arguments through redis's own `get_slot`, so it *disagrees* with
/// a tag-blind address and answers `-MOVED`.
#[tokio::test]
async fn test_tester_cluster_addressing_follows_the_hash_tag_not_the_whole_key() {
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("MEMORY", 40);
    conn.inner.push_str("OBJECT", "listpack");
    conn.inner.push_int("OBJECT", 7);
    conn.inner
        .push("OBJECT", err_reply("freq counter is not available"));
    conn.inner.push_int("PTTL", -1);
    conn.inner.push_str("TYPE", "hash");

    let key = "{tenant7}:profile";
    let info = key_object_info(&mut conn, key, Topology::Cluster)
        .await
        .expect("a hash-tagged key must not turn the sidebar into a rebuild storm");
    assert_eq!(
        info,
        KeyObjectInfo {
            missing: false,
            key_type: Some("hash".to_string()),
            memory_bytes: Some(40),
            encoding: Some("listpack".to_string()),
            idle_seconds: Some(7),
            freq: None,
            ttl_ms: TTL_NO_EXPIRY,
        },
        "the tag must not change what the probes answer, only where they run"
    );

    // Fixture guard: the tag has to move the answer, or this test would pass
    // under a tag-blind implementation for the wrong reason. `tenant7}:profile`
    // is what a brace strip that forgets the closing `}` would CRC.
    let tag_slot = get_slot(b"tenant7");
    let brace_strip_slot = get_slot(key.strip_prefix('{').unwrap_or(key).as_bytes());
    assert_eq!(
        get_slot(key.as_bytes()),
        tag_slot,
        "redis itself must route this key by its tag, or the fixture is wrong"
    );
    assert_ne!(
        tag_slot, brace_strip_slot,
        "test fixture must make the tagged and tag-blind answers differ"
    );

    assert_eq!(
        conn.addressed_names(),
        vec!["MEMORY", "OBJECT", "OBJECT", "OBJECT", "PTTL", "TYPE"],
        "all six probes still go out one at a time, through the addressed path"
    );
    let asked: Vec<u16> = conn.addressed.iter().map(|(_, slot)| *slot).collect();
    assert!(
        asked.iter().all(|slot| *slot == tag_slot),
        "every probe must be addressed at the tag's slot {tag_slot}, got {asked:?}"
    );
    assert_eq!(
        conn.misrouted,
        Vec::<Vec<String>>::new(),
        "an addressed-by-tag probe must never reach a shard without the key"
    );
    assert_eq!(
        conn.slot_refreshes, 0,
        "the tag slot is the right one, so nothing may rebuild the slot map"
    );
}

/// The same property on the sampling path, where it is sharper: a shard's `SCAN`
/// legitimately returns hash-tagged keys, and co-located keys must be typed at
/// the *shared* tag slot — one address for both — while an untagged key on the
/// same page goes elsewhere. Pinning every `TYPE` at the anchor (M-typeslot) or
/// ignoring the tag both fail here, and the second one fails only because of
/// this fixture.
#[tokio::test]
async fn test_tester_a_hash_tagged_sample_is_typed_at_the_tag_slot() {
    let mut conn = ClusterFoldingConn::new();
    conn.inner.push_int("DBSIZE", 5_000);
    let keys = ["{tenant7}:cart", "{tenant7}:profile", "plain:counter"];
    conn.inner.push_scan(0, &keys);
    // `collect_sample` sorts and de-duplicates before typing, so queue the TYPE
    // replies in sorted order instead of in SCAN page order.
    let mut window: Vec<&str> = keys.to_vec();
    window.sort_unstable();
    for key in &window {
        conn.inner.push_type(if key.starts_with("{tenant7}") {
            "hash"
        } else {
            "string"
        });
    }

    let dist = type_distribution(&mut conn, Some(100), Topology::Cluster)
        .await
        .expect("a sample of tagged keys must not die on slot routing");
    assert_eq!(dist.counts.get("hash"), Some(&2));
    assert_eq!(dist.counts.get("string"), Some(&1));
    assert_eq!(dist.sampled, 3);
    assert_eq!(dist.sampled, dist.counts.values().sum::<u64>());

    let tag_slot = get_slot(b"tenant7");
    let typed: Vec<u16> = conn
        .addressed
        .iter()
        .filter(|(args, _)| cmd_name(args) == "TYPE")
        .map(|(_, slot)| *slot)
        .collect();
    let expected: Vec<u16> = window.iter().map(|k| get_slot(k.as_bytes())).collect();
    assert_eq!(
        typed, expected,
        "each TYPE at its own key's slot, in sorted order"
    );
    // The hash-tag promise, checked on the addresses we actually asked for:
    // the two tagged keys share one address, and it is the tag's slot.
    let tagged: Vec<u16> = window
        .iter()
        .filter(|k| k.starts_with("{tenant7}"))
        .map(|k| get_slot(k.as_bytes()))
        .collect();
    assert_eq!(tagged, vec![tag_slot, tag_slot]);
    assert_ne!(
        get_slot(b"plain:counter"),
        tag_slot,
        "test fixture must keep the untagged key on another shard"
    );
    assert_eq!(conn.slots_for("SCAN"), vec![cluster_scan_anchor_slot()]);
    assert_eq!(
        conn.misrouted,
        Vec::<Vec<String>>::new(),
        "no probe may land on a shard without its key"
    );
    assert_eq!(conn.slot_refreshes, 0);
}
