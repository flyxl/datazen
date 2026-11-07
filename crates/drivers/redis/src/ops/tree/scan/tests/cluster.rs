// Cluster discipline: one slot per key, `SCAN` pinned to the anchor, and the
// `{hash-tag}` slotting that the routing contract depends on.

use super::*;

// ---------------------------------------------------------------------------
// Cluster discipline: one slot per key, SCAN pinned to the anchor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cluster_pages_address_one_single_key_batch_per_key_to_its_own_slot() {
    const TAGGED_A: &str = "app:{user1000}:profile";
    const TAGGED_B: &str = "session:{user1000}:token";
    const SOLO: &str = "solo:key";

    let mut conn = TreeConn::new();
    conn.state().dbsize = 3;
    for key in [TAGGED_A, TAGGED_B, SOLO] {
        conn.seed_string(key, 2, "v");
    }
    conn.state().scan_script.push_back((
        0,
        vec![TAGGED_A.to_string(), TAGGED_B.to_string(), SOLO.to_string()],
    ));

    let page = scan_keys_page(
        &mut conn,
        "*",
        0,
        100,
        None,
        false,
        false,
        None,
        Topology::Cluster,
        Instant::now(),
    )
    .await
    .expect("cluster page");
    assert_eq!(page.entries.len(), 3);

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles),
            ["DBSIZE"],
            "only the unrouted DBSIZE is a plain single on a cluster"
        );
        assert!(
            st.batches.is_empty(),
            "a cross-key pipeline on a cluster is a CROSSSLOT waiting to fail \
             the whole command: every batch must be slot-addressed instead"
        );

        let scans: Vec<(u16, Vec<Vec<String>>)> = st
            .addressed
            .iter()
            .filter(|(_, cmds)| cmds[0][0] == "SCAN")
            .cloned()
            .collect();
        assert_eq!(scans.len(), 1, "one wrapped round is one addressed SCAN");
        assert_eq!(
            scans[0].0,
            cluster_scan_anchor_slot(),
            "SCAN must stay pinned to the anchor shard or the cursor hops nodes"
        );

        let batches: Vec<(u16, Vec<Vec<String>>)> = st
            .addressed
            .iter()
            .filter(|(_, cmds)| cmds[0][0] != "SCAN")
            .cloned()
            .collect();
        assert_eq!(batches.len(), 6, "3 keys × (meta + value), one batch each");

        for key in [TAGGED_A, TAGGED_B, SOLO] {
            let for_key: Vec<&(u16, Vec<Vec<String>>)> = batches
                .iter()
                .filter(|(_, cmds)| cmds.iter().any(|cmd| cmd.get(1).is_some_and(|a| a == key)))
                .collect();
            assert_eq!(
                for_key.len(),
                2,
                "one meta batch + one value batch for {key}"
            );
            for (slot, cmds) in for_key {
                assert_eq!(
                    *slot,
                    get_slot(key.as_bytes()),
                    "{key} must run on the shard owning its slot"
                );
                for cmd in cmds {
                    assert_eq!(
                        cmd.get(1).map(String::as_str),
                        Some(key),
                        "every command in an addressed batch is about {key} only \
                         (batch: {cmd:?}) — anything else risks CROSSSLOT"
                    );
                }
            }
        }

        // Both {hash-tag} keys land on one slot through their tag.
        let slot_of = |key: &str| {
            batches
                .iter()
                .find(|(_, cmds)| cmds.iter().any(|cmd| cmd.get(1).is_some_and(|a| a == key)))
                .map(|(slot, _)| *slot)
                .expect("key was addressed")
        };
        assert_eq!(
            slot_of(TAGGED_A),
            slot_of(TAGGED_B),
            "one tag, one slot — both keys, hence every batch, must agree"
        );
    }
}

#[test]
fn hash_tag_slotting_is_the_routing_contract_and_breaks_without_tag_handling() {
    // Counterfactual (BUG-008 style): every equality here becomes a full-key
    // hash the moment tag extraction is removed, so the test turns red on the
    // exact regression it guards.
    let profile = get_slot(b"app:{user1000}:profile");
    let session = get_slot(b"session:{user1000}:token");
    assert_eq!(
        profile, session,
        "two keys sharing the user1000 hash tag must hash to one slot"
    );
    assert_eq!(
        profile,
        get_slot(b"user1000"),
        "the tag's content — not the whole key — picks the slot"
    );
    assert_eq!(
        get_slot(b"{user1000}"),
        get_slot(b"user1000"),
        "the braces themselves carry no hash weight"
    );
    // …and the equality above would be vacuous if two different tags collided.
    assert_ne!(
        get_slot(b"user1000"),
        get_slot(b"user2000"),
        "different tags are expected to differ; same-tag equality means nothing otherwise"
    );
    // The anchor SCAN slot is a legal slot number.
    assert!(cluster_scan_anchor_slot() < 16_384);
}

#[tokio::test]
async fn key_probe_is_one_addressed_batch_that_never_reads_a_value() {
    const KEY: &str = "app:{user1000}:session";
    let mut conn = TreeConn::new();
    {
        let mut st = conn.state();
        st.types.insert(KEY.to_string(), "hash".to_string());
        st.ptls.insert(KEY.to_string(), 1_500);
        st.mems.insert(KEY.to_string(), 64);
        st.lens.insert(KEY.to_string(), 2);
    }

    let probe = key_probe(&mut conn, KEY, Topology::Cluster)
        .await
        .expect("key probe");

    assert!(probe.exists);
    assert_eq!(probe.key_type.as_deref(), Some("hash"));
    assert_eq!(probe.ttl_ms, 1_500, "ttlMs is milliseconds (PTTL)");
    assert_eq!(probe.memory_bytes, Some(64));

    {
        let st = conn.state();
        assert!(
            st.singles.is_empty(),
            "the probe issues no unaddressed command at all"
        );
        assert!(st.batches.is_empty(), "and no cross-key pipeline either");
        assert_eq!(st.addressed.len(), 1, "one addressed batch, one round trip");
        let (slot, cmds) = &st.addressed[0];
        assert_eq!(
            joined(cmds),
            [
                format!("EXISTS {KEY}"),
                format!("TYPE {KEY}"),
                format!("PTTL {KEY}"),
                format!("MEMORY USAGE {KEY}"),
            ],
            "attributes only — a value command here would be a contract break"
        );
        assert_eq!(*slot, get_slot(KEY.as_bytes()));
        assert_eq!(
            *slot,
            get_slot(b"user1000"),
            "the hash tag decides the shard"
        );
    }
}
