// Page-level coverage of behaviours the delivered suite pinned only on the
// batch builders — assembled pages, not builders.

use super::*;

#[tokio::test]
async fn test_tester_standalone_pages_chunk_at_the_pipeline_limit() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 300;
    let keys: Vec<String> = (0..300).map(|i| format!("chunk:{i}")).collect();
    for key in &keys {
        conn.seed_string(key, 1, "v");
    }

    let metas = fetch_key_meta(&mut conn, &keys, true, Topology::Standalone)
        .await
        .expect("chunked meta batch");
    {
        let st = conn.state();
        assert_eq!(
            st.batches.len(),
            2,
            "300 keys at TREE_KEYS_PER_PIPELINE={TREE_KEYS_PER_PIPELINE} = two batches"
        );
        assert_eq!(st.batches[0].len(), TREE_KEYS_PER_PIPELINE * 3);
        assert_eq!(st.batches[1].len(), (300 - TREE_KEYS_PER_PIPELINE) * 3);
    }
    assert_eq!(metas.len(), 300, "one group per key, in key order");
    assert!(metas
        .iter()
        .all(|m| m.mem_bytes == Some(64) || m.mem_bytes.is_none()));
}

#[tokio::test]
async fn test_tester_sentinel_uses_the_node_batch_and_never_addressed_singles() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 4;
    conn.seed_string("s:a", 1, "v");
    conn.seed_string("s:b", 2, "w");
    let keys = vec!["s:a".to_string(), "s:b".to_string()];

    let metas = fetch_key_meta(&mut conn, &keys, false, Topology::Sentinel)
        .await
        .expect("sentinel page meta");
    {
        let st = conn.state();
        assert_eq!(st.batches.len(), 1, "sentinel is a single node: one batch");
        assert!(
            st.addressed.is_empty(),
            "addressing is meaningless on a sentinel and must not appear: {:?}",
            st.addressed
        );
    }
    assert_eq!(metas.len(), 2);
}

#[tokio::test]
async fn test_tester_a_stalled_cursor_reports_truncated_before_the_budget_is_spent() {
    let mut conn = TreeConn::new();
    {
        let mut st = conn.state();
        st.dbsize = 100_000;
        // The cursor never wraps and never yields a key: a proxy or a replica
        // that is not making progress.
        for _ in 0..(MAX_TREE_STALLED_ROUNDS + 8) {
            st.scan_script.push_back((7, Vec::new()));
        }
    }

    let page = scan_keys_page(
        &mut conn,
        "stalled:*",
        0,
        100,
        None,
        false,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("stalled page");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("SCAN"))
                .count(),
            MAX_TREE_STALLED_ROUNDS as usize,
            "the stall guard, not the budget, must end the loop"
        );
    }
    assert!(page.truncated, "an abandoned open cursor is hidden data");
    assert_eq!(
        page.next_cursor, 7,
        "the caller must still be able to resume"
    );
    assert!(
        page.consumed < DEFAULT_TREE_BUDGET,
        "the guard must fire while budget remains: {}",
        page.consumed
    );
}

#[tokio::test]
async fn test_tester_the_round_cap_stops_a_cursor_that_never_wraps() {
    let mut conn = TreeConn::new();
    {
        let mut st = conn.state();
        // A big budget so only the round cap can end the walk, and one new key
        // per round so the stall guard never fires.
        st.dbsize = 100_000;
        for round in 0..(MAX_TREE_SCAN_ROUNDS + 10) {
            st.scan_script.push_back((9, vec![format!("loop:{round}")]));
        }
    }

    let page = scan_budgeted(
        &mut conn,
        0,
        TREE_SCAN_MIN_ROUND_COUNT,
        usize::MAX,
        Some("loop:*"),
        None,
        &mut ScanBudget::new(HARD_MAX_TREE_BUDGET),
        Topology::Standalone,
    )
    .await
    .expect("capped walk");

    assert_eq!(
        page.consumed,
        u64::from(MAX_TREE_SCAN_ROUNDS) * u64::from(TREE_SCAN_MIN_ROUND_COUNT),
        "exactly the capped number of rounds may be charged"
    );
    assert!(page.truncated);
    assert!(!page.exhausted);
    assert_eq!(page.keys.len(), MAX_TREE_SCAN_ROUNDS as usize);
}

#[test]
fn test_tester_preview_rendering_covers_every_key_type() {
    // string: the payload, truncated to the preview budget.
    assert_eq!(render_preview("string", Some(&bulk("plain"))), "plain");
    // list / set / zset: the array form the flat browser already renders.
    assert_eq!(
        render_preview("list", Some(&RValue::Array(vec![bulk("a"), bulk("b")]))),
        r#"["a", "b"]"#
    );
    assert_eq!(
        render_preview("zset", Some(&RValue::Array(vec![bulk("m"), bulk("1")]))),
        r#"["m", "1"]"#
    );
    // hash: pairs come out of the HSCAN envelope, never a raw array.
    assert_eq!(
        render_preview(
            "hash",
            Some(&RValue::Array(vec![
                bulk("0"),
                RValue::Array(vec![bulk("f1"), bulk("v1"), bulk("f2"), bulk("v2")])
            ]))
        ),
        r#"["f1: v1", "f2: v2"]"#
    );
    // stream: no read is issued, the label is fixed.
    assert_eq!(render_preview("stream", None), "(stream)");
    // module / unknown: nothing is invented, and nothing is read.
    assert_eq!(render_preview("ReJSON-RL", Some(&bulk("{}"))), "");
    // A missing reply degrades to the empty rendering for that type.
    assert_eq!(render_preview("list", None), "[]");
    // Long payloads are truncated, not streamed to the IPC layer.
    let long = "x".repeat(400);
    let rendered = render_preview("string", Some(&bulk(&long)));
    assert!(
        rendered.chars().count() < 400,
        "preview must truncate: {rendered}"
    );
}

#[test]
fn test_tester_hscan_envelopes_that_are_not_pairs_answer_empty() {
    // A malformed envelope (short member list / odd length) must not panic and
    // must not fabricate a `field: value` pair.
    assert!(extract_hscan_preview(&RValue::Array(vec![bulk("0")])).is_empty());
    assert!(extract_hscan_preview(&RValue::Array(vec![
        bulk("0"),
        RValue::Array(vec![bulk("only-field")])
    ]))
    .is_empty());
    assert!(extract_hscan_preview(&RValue::Nil).is_empty());
    assert!(extract_hscan_preview(&bulk("not-an-array")).is_empty());
}

#[test]
fn test_tester_value_group_readers_stay_aligned_with_their_builders() {
    // A type with a length command but no preview read (stream): slot 0 is the
    // length and there is no preview slot to steal.
    let stream = parse_value_group(&[RValue::Int(9)], "stream");
    assert_eq!(stream.logical_len, 9);
    assert_eq!(stream.preview, "(stream)");
    // A module type issues nothing at all.
    let module = parse_value_group(&[], "ReJSON-RL");
    assert_eq!(module.logical_len, 0);
    assert_eq!(module.preview, "");
    // An absent length reply degrades to 0 rather than to a plausible number.
    let degraded = parse_value_group(&[RValue::Nil, bulk("v")], "string");
    assert_eq!(degraded.logical_len, 0);
    assert_eq!(degraded.preview, "v");
    // Every type the builder knows about must round-trip through the reader.
    for key_type in ["string", "list", "set", "zset", "hash", "stream"] {
        let items = vec![PageKey {
            key: "k".to_string(),
            key_type: key_type.to_string(),
        }];
        let (_, sizes) = build_value_pipeline(&items);
        let expected = usize::from(length_command_for(key_type).is_some())
            + usize::from(has_preview_command(key_type));
        assert_eq!(sizes, vec![expected], "{key_type} builder arity");
    }
}
/// `noTtlOnly` on the flat browser: rows are filtered *after* enrichment, so the
/// surviving keys keep their own attributes and the dropped one disappears.
#[tokio::test]
async fn test_tester_scan_keys_no_ttl_only_keeps_only_permanent_keys() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 5;
    for (key, ttl) in [("nt:p", -1i64), ("nt:e", 30)] {
        let mut st = conn.state();
        st.types.insert(key.to_string(), "string".to_string());
        st.ttls.insert(key.to_string(), ttl);
        st.lens.insert(key.to_string(), 2);
        st.previews.insert(key.to_string(), bulk("v"));
    }
    conn.state()
        .scan_script
        .push_back((0, vec!["nt:p".to_string(), "nt:e".to_string()]));

    let page = scan_keys_page(
        &mut conn,
        "nt:*",
        0,
        100,
        None,
        false,
        true,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("noTtlOnly page");

    let keys: Vec<&str> = page.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["nt:p"], "only keys without expiry survive");
    assert_eq!(page.entries[0].ttl, -1, "and it keeps its OWN ttl");
}

/// Regression guard for redis-tree-backend-BUG-001: `list_children` fills leaf
/// attributes **by key**, so the `noTtlOnly` filter (a shortened row set) can
/// never shift type/TTL/size onto a neighbour's slot.
#[tokio::test]
async fn test_tester_list_children_no_ttl_only_keeps_rows_aligned() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 6;
    // a and c never expire (TTL -1) so `noTtlOnly` must keep both; b carries an
    // expiry and must be the one dropped. Each surviving key has a distinct
    // logical length (1 vs 3), so a one-slot shift is visible rather than
    // silently plausible.
    for (key, ttl, len) in [("app:a", -1i64, 1i64), ("app:b", 7, 2), ("app:c", -1, 3)] {
        let mut st = conn.state();
        st.types.insert(key.to_string(), "string".to_string());
        st.ttls.insert(key.to_string(), ttl);
        st.lens.insert(key.to_string(), len);
        st.previews.insert(key.to_string(), bulk("v"));
    }
    conn.state().scan_script.push_back((
        0,
        vec![
            "app:a".to_string(),
            "app:b".to_string(),
            "app:c".to_string(),
        ],
    ));

    let page = list_children_page(
        &mut conn,
        "app:",
        0,
        100,
        None,
        true,
        None,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("noTtlOnly children page");

    let leaves: Vec<(String, String, i64, u64)> = page
        .entries
        .iter()
        .filter_map(|e| match e {
            ChildEntry::Key {
                key,
                key_type,
                ttl,
                logical_len,
                ..
            } => Some((key.clone(), key_type.clone(), *ttl, *logical_len)),
            _ => None,
        })
        .collect();
    assert_eq!(
        leaves,
        vec![
            ("app:a".to_string(), "string".to_string(), -1, 1),
            ("app:c".to_string(), "string".to_string(), -1, 3),
        ],
        "noTtlOnly must return exactly the non-expiring leaves, each with its OWN attributes"
    );
}

/// Regression guard for redis-tree-backend-BUG-001, second door: a key that
/// expires between the SCAN and the meta batch is dropped as `absent`, which
/// used to shift every later leaf's attributes by one slot.
#[tokio::test]
async fn test_tester_list_children_survives_a_key_that_vanished_mid_page() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 6;
    for (key, ttl, len) in [("gone:a", -1i64, 1i64), ("gone:b", 7, 2), ("gone:c", 9, 3)] {
        let mut st = conn.state();
        st.types.insert(key.to_string(), "string".to_string());
        st.ttls.insert(key.to_string(), ttl);
        st.lens.insert(key.to_string(), len);
        st.previews.insert(key.to_string(), bulk("v"));
    }
    // The server now answers `none` for app b: it expired after the SCAN.
    conn.state()
        .types
        .insert("gone:b".to_string(), "none".to_string());
    conn.state().scan_script.push_back((
        0,
        vec![
            "gone:a".to_string(),
            "gone:b".to_string(),
            "gone:c".to_string(),
        ],
    ));

    let page = list_children_page(
        &mut conn,
        "gone:",
        0,
        100,
        None,
        false,
        None,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("page with a vanished leaf");

    let leaves: Vec<(String, String, u64)> = page
        .entries
        .iter()
        .filter_map(|e| match e {
            ChildEntry::Key {
                key,
                key_type,
                logical_len,
                ..
            } => Some((key.clone(), key_type.clone(), *logical_len)),
            _ => None,
        })
        .collect();
    assert_eq!(
        leaves,
        vec![
            ("gone:a".to_string(), "string".to_string(), 1),
            ("gone:c".to_string(), "string".to_string(), 3),
        ],
        "the vanished key must disappear and its neighbours keep their own sizes"
    );
}

/// #56 discipline, made checkable in-process: `list_children` must not add a
/// second way to fail on a cluster. The delivered suite only exercises the
/// hierarchical scan standalone, so "this track did not worsen the existing
/// CrossSlot defect" had no counter-proof. On a cluster every batch this op
/// issues is one key's own attributes, addressed to *that key's* slot (not the
/// scan anchor), and there is no cross-key pipeline to be rejected.
#[tokio::test]
async fn test_tester_list_children_on_cluster_addresses_every_batch_by_key() {
    // `sep = "."` keeps these three keys leaves at the requested level, so the
    // leaf batches (the only place a CROSSSLOT could come from) are exercised.
    const TAGGED_A: &str = "app:{user1000}:profile";
    const TAGGED_B: &str = "app:{user1000}:token";
    const OTHER: &str = "app:{other9}:token";

    let mut conn = TreeConn::new();
    conn.state().dbsize = 3;
    for key in [TAGGED_A, TAGGED_B, OTHER] {
        conn.seed_string(key, 2, "v");
    }
    conn.state().scan_script.push_back((
        0,
        vec![
            TAGGED_A.to_string(),
            TAGGED_B.to_string(),
            OTHER.to_string(),
        ],
    ));

    let page = list_children_page(
        &mut conn,
        "app:",
        0,
        100,
        Some("."),
        false,
        None,
        true,
        None,
        Topology::Cluster,
        Instant::now(),
    )
    .await
    .expect("cluster children page");
    assert_eq!(page.entries.len(), 3, "all three are leaves at this level");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles),
            ["DBSIZE"],
            "DBSIZE is the only unrouted command in this op"
        );
        assert!(
            st.batches.is_empty(),
            "no cross-key pipeline may be issued on a cluster (that is #56's shape): {:?}",
            st.batches
        );

        let scans: Vec<u16> = st
            .addressed
            .iter()
            .filter(|(_, cmds)| cmds[0][0] == "SCAN")
            .map(|(slot, _)| *slot)
            .collect();
        assert_eq!(scans, vec![cluster_scan_anchor_slot()], "SCAN anchored");

        let batches: Vec<&(u16, Vec<Vec<String>>)> = st
            .addressed
            .iter()
            .filter(|(_, cmds)| cmds[0][0] != "SCAN")
            .collect();
        assert_eq!(batches.len(), 6, "3 leaves x (meta + value)");
        let all_keys = [TAGGED_A, TAGGED_B, OTHER];
        for (slot, cmds) in &batches {
            // The subject is wherever the command puts it: `TYPE k` names it at
            // index 1, `MEMORY USAGE k` at index 2 — the two-word form is the
            // whole reason the batch has to be *explicitly* addressed.
            let subject = all_keys
                .iter()
                .find(|key| cmds.iter().any(|cmd| cmd.iter().any(|a| a == *key)))
                .expect("a batch must name a key");
            for cmd in cmds.iter() {
                assert!(
                    cmd.iter().any(|arg| arg == subject),
                    "every command of the batch is about {subject}: {cmd:?}"
                );
                for other in all_keys.iter().filter(|k| *k != subject) {
                    assert!(
                        !cmd.iter().any(|arg| arg == other),
                        "{subject}'s batch leaked {other} — a cross-slot batch: {cmds:?}"
                    );
                }
            }
            assert_eq!(
                *slot,
                get_slot(subject.as_bytes()),
                "{subject} must run on the shard owning it, not on the scan anchor"
            );
        }
        // Counter-proof for tag-blind addressing: both hash-tag keys share a
        // shard, the third does not. If `get_slot` were bypassed (or everything
        // were pinned to the anchor), the per-key equality above turns red.
        assert_eq!(get_slot(TAGGED_A.as_bytes()), get_slot(TAGGED_B.as_bytes()));
        assert_ne!(get_slot(TAGGED_A.as_bytes()), get_slot(OTHER.as_bytes()));
    }
    assert_eq!(page.dbsize, 3);
    assert!(!page.truncated);
}

/// Delivered unit 4 says `withMemory` makes `size` the `MEMORY USAGE` answer.
/// The delivered suite only asserted that claim on the batch builder, never on
/// an assembled page — so a page that quietly kept the logical length would
/// have stayed green. One key answers memory, one does not (Redis < 4.0 or
/// `NOPERM`), and the fallback must be the logical length, not zero.
#[tokio::test]
async fn test_tester_scan_keys_page_with_memory_reports_bytes_and_falls_back_to_length() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 4;
    conn.seed_string("mem:bytes", 12, "payload");
    conn.state().mems.insert("mem:bytes".to_string(), 96);
    conn.seed_string("mem:unsupported", 7, "other");
    // `mem:unsupported` has no MEMORY USAGE answer at all → the double replies Nil.
    conn.state().scan_script.push_back((
        0,
        vec!["mem:bytes".to_string(), "mem:unsupported".to_string()],
    ));

    let page = scan_keys_page(
        &mut conn,
        "mem:*",
        0,
        100,
        None,
        true,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("withMemory page");

    assert_eq!(page.entries.len(), 2);
    assert_eq!(
        page.entries[0].size, 96,
        "withMemory makes `size` the MEMORY USAGE byte count"
    );
    assert_eq!(
        page.entries[1].size, 7,
        "an unsupported MEMORY USAGE falls back to the logical length, never to 0"
    );
    let st = conn.state();
    assert_eq!(
        joined(&st.batches[0]),
        [
            "TYPE mem:bytes",
            "TTL mem:bytes",
            "MEMORY USAGE mem:bytes",
            "TYPE mem:unsupported",
            "TTL mem:unsupported",
            "MEMORY USAGE mem:unsupported",
        ],
        "MEMORY USAGE rides the same meta batch (no extra round trip)"
    );
}

/// A cluster `SCAN` must carry `MATCH` and `TYPE` into the *addressed* command:
/// the anchor routing and the server-side filter are two independent
/// requirements, and the round trip is built from scratch on this arm, so the
/// filter could silently be dropped.
#[tokio::test]
async fn test_tester_cluster_scan_round_carries_match_and_type_to_the_anchor() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 2;
    conn.state()
        .types
        .insert("h:{user1000}:a".to_string(), "string".to_string());
    conn.state().ttls.insert("h:{user1000}:a".to_string(), -1);
    conn.state().lens.insert("h:{user1000}:a".to_string(), 1);
    conn.state()
        .previews
        .insert("h:{user1000}:a".to_string(), bulk("v"));
    conn.state()
        .scan_script
        .push_back((0, vec!["h:{user1000}:a".to_string()]));

    let page = scan_keys_page(
        &mut conn,
        "h:*",
        0,
        100,
        Some("string"),
        false,
        false,
        None,
        Topology::Cluster,
        Instant::now(),
    )
    .await
    .expect("cluster filtered page");
    assert_eq!(page.entries.len(), 1);

    let st = conn.state();
    let (_, scan) = st
        .addressed
        .iter()
        .find(|(_, cmds)| cmds[0][0] == "SCAN")
        .expect("the SCAN round was addressed");
    let line = scan[0].join(" ");
    assert!(
        line.contains("MATCH h:*"),
        "the pattern must survive into the addressed SCAN: {line}"
    );
    assert!(
        line.contains("TYPE string"),
        "the type filter must be applied server-side, not client-side: {line}"
    );
    let unaddressed: Vec<String> = st
        .batches
        .iter()
        .flat_map(|batch| batch.iter().map(|cmd| cmd.join(" ")))
        .collect();
    assert!(
        !unaddressed.iter().any(|l| l.contains("SCAN")),
        "SCAN must never leave as an unaddressed batch on a cluster: {unaddressed:?}"
    );
}

/// `key_probe` on a single node is the common production shape: one pipeline,
/// one round trip, and no addressing at all. The delivered suite pins the
/// cluster arm only, so `fetch_key_group`'s non-cluster branch was never run.
#[tokio::test]
async fn test_tester_key_probe_on_a_single_node_is_one_pipeline_round_trip() {
    const KEY: &str = "app:users:42";
    let mut conn = TreeConn::new();
    {
        let mut st = conn.state();
        st.types.insert(KEY.to_string(), "hash".to_string());
        st.ptls.insert(KEY.to_string(), 2_500);
        st.mems.insert(KEY.to_string(), 88);
    }

    let probe = key_probe(&mut conn, KEY, Topology::Standalone)
        .await
        .expect("standalone probe");
    assert!(probe.exists);
    assert_eq!(probe.key_type.as_deref(), Some("hash"));
    assert_eq!(probe.ttl_ms, 2_500);
    assert_eq!(probe.memory_bytes, Some(88));

    let st = conn.state();
    assert_eq!(st.batches.len(), 1, "one batch, one round trip");
    assert_eq!(
        joined(&st.batches[0]),
        [
            format!("EXISTS {KEY}"),
            format!("TYPE {KEY}"),
            format!("PTTL {KEY}"),
            format!("MEMORY USAGE {KEY}"),
        ],
        "the frozen four, in order, no value read"
    );
    assert!(st.singles.is_empty(), "and nothing issued one at a time");
    assert!(
        st.addressed.is_empty(),
        "addressing is meaningless on a single node and must not appear"
    );
}

/// Presence decided by `TYPE` alone: `EXISTS` was folded away and `TYPE`
/// positively answers `none`. That is an absence the probe must report as a
/// fact, not as "unreadable".
#[test]
fn test_tester_probe_reports_absence_when_only_type_answers() {
    let values = vec![
        crate::ops::key_probe::err_reply("connection reset"),
        bulk("none"),
        RValue::Int(-2),
        RValue::Nil,
    ];
    assert_eq!(
        parse_key_probe(&values),
        Some(KeyProbe {
            exists: false,
            key_type: None,
            ttl_ms: -2,
            memory_bytes: None,
        }),
        "TYPE saying `none` is enough to say the key is gone"
    );
}
