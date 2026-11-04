// --- memory_sample field read (BUG-003: big-key type/TTL in one round trip) --
use super::*;

#[test]
fn memory_sample_pipeline_is_three_commands_per_key_in_order() {
    let pipe = build_memory_sample_pipeline(&["a".to_string(), "b".to_string()]);
    let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
    assert_eq!(batch.len(), 2 * MEMORY_SAMPLE_FIELDS_PER_KEY);
    let names: Vec<&str> = batch.iter().map(|c| c[0].as_str()).collect();
    assert_eq!(
        names,
        vec!["MEMORY", "TYPE", "PTTL", "MEMORY", "TYPE", "PTTL"]
    );
    assert_eq!(batch[0], vec!["MEMORY", "USAGE", "a"]);
    assert_eq!(batch[1], vec!["TYPE", "a"]);
    assert_eq!(batch[2], vec!["PTTL", "a"]);
    assert_eq!(batch[3], vec!["MEMORY", "USAGE", "b"]);
    assert!(
        !names.iter().any(|n| n.eq_ignore_ascii_case("KEYS")),
        "the big-key sample must never fall back to KEYS"
    );
}

#[test]
fn memory_sample_fields_reads_a_present_key_and_degrades_independently() {
    // bytes + type + ttl all present; TTL = -1 (no expiry) is a real value.
    let full = vec![RValue::Int(4096), bulk("hash"), RValue::Int(-1)];
    let f = parse_memory_sample_fields(&full);
    assert!(!f.missing);
    assert_eq!(f.bytes, Some(4096));
    assert_eq!(f.key_type.as_deref(), Some("hash"));
    assert_eq!(f.ttl_ms, Some(-1));

    // MEMORY USAGE unavailable (Redis < 4.0): only the byte slot degrades.
    let no_mem = vec![
        err_reply("unknown subcommand or wrong number of args for 'USAGE'"),
        bulk("string"),
        RValue::Int(5000),
    ];
    let f = parse_memory_sample_fields(&no_mem);
    assert!(!f.missing);
    assert_eq!(f.bytes, None);
    assert_eq!(f.key_type.as_deref(), Some("string"));
    assert_eq!(f.ttl_ms, Some(5000));

    // A short / absent group reads as unreadable — never as a type name, and
    // never claimed to be gone.
    let empty = parse_memory_sample_fields(&[]);
    assert!(!empty.missing);
    assert_eq!(empty.key_type, None);
    assert_eq!(empty.bytes, None);
    assert_eq!(empty.ttl_ms, None);
}

#[test]
fn memory_sample_fields_marks_a_key_gone_between_scan_and_read() {
    // TYPE == "none" after MEMORY nil / PTTL -2: a distinguishable empty state,
    // NOT an error and NOT conflated with the "unreadable" state above.
    let gone = parse_memory_sample_fields(&[RValue::Nil, bulk("none"), RValue::Int(-2)]);
    assert!(gone.missing);
    assert_eq!(gone.bytes, None);
    assert_eq!(gone.key_type, None);
    assert_eq!(gone.ttl_ms, Some(TTL_MISSING));
}

#[tokio::test]
async fn memory_sample_field_read_is_one_round_trip_for_the_whole_sample() {
    let keys: Vec<String> = ["big:1", "big:2", "big:3"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut conn = ScriptedConn::new();
    // Replies are queued per command name (FIFO), consumed in key order.
    conn.push_int("MEMORY", 300);
    conn.push_int("MEMORY", 100);
    conn.push_int("MEMORY", 200);
    conn.push_str("TYPE", "string");
    conn.push_str("TYPE", "zset");
    conn.push_str("TYPE", "hash");
    conn.push_int("PTTL", -1);
    conn.push_int("PTTL", 9_000);
    conn.push_int("PTTL", 4_000);

    let fields = fetch_memory_sample_fields(&mut conn, &keys, Topology::Standalone)
        .await
        .expect("field read");
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].bytes, Some(300));
    assert_eq!(fields[0].key_type.as_deref(), Some("string"));
    assert_eq!(fields[0].ttl_ms, Some(-1), "no-expiry stays a value");
    assert_eq!(fields[1].key_type.as_deref(), Some("zset"));
    assert_eq!(fields[1].ttl_ms, Some(9_000));
    assert!(fields.iter().all(|f| !f.missing));

    let journal = conn.journal();
    assert_eq!(journal.batches.len(), 1, "three keys in ONE pipeline");
    assert_eq!(journal.batches[0].len(), 3 * MEMORY_SAMPLE_FIELDS_PER_KEY);
    assert!(
        journal.singles.is_empty(),
        "no per-key round trips on a single node (was the N-trip bug)"
    );
    assert_eq!(journal.round_trips(), 1);
}

#[tokio::test]
async fn memory_sample_field_read_scales_by_chunk_not_by_key() {
    // One key past the pipeline budget adds a *second batch*, not a second
    // round trip per key: the count is ceil(n / chunk), never n.
    let n = MEMORY_SAMPLE_KEYS_PER_PIPELINE + 1;
    let keys: Vec<String> = (0..n).map(|i| format!("k{i}")).collect();
    let mut conn = ScriptedConn::new();
    for _ in 0..n {
        conn.push_int("MEMORY", 10);
        conn.push_str("TYPE", "string");
        conn.push_int("PTTL", -1);
    }
    let fields = fetch_memory_sample_fields(&mut conn, &keys, Topology::Standalone)
        .await
        .expect("field read");
    assert_eq!(fields.len(), n);
    let journal = conn.journal();
    assert_eq!(journal.batches.len(), 2, "ceil(n / chunk) == 2 batches");
    assert_eq!(journal.round_trips(), 2);
    assert_eq!(
        journal.batches[0].len(),
        MEMORY_SAMPLE_KEYS_PER_PIPELINE * MEMORY_SAMPLE_FIELDS_PER_KEY
    );
    assert_eq!(journal.batches[1].len(), MEMORY_SAMPLE_FIELDS_PER_KEY);
}

#[tokio::test]
async fn memory_sample_field_read_addresses_each_key_on_cluster() {
    // `ScriptedConn` carries the trait's DEFAULT `pipeline_at_slot`, which is
    // addressed command-by-command — this pins that fallback shape (never a
    // mixed-slot pipeline). The real `ClusterConnection` overrides it with one
    // addressed batch per key; `ClusterBatchConn` below simulates that shape.
    let keys: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let mut conn = ScriptedConn::new();
    conn.push_int("MEMORY", 50);
    conn.push_int("MEMORY", 60);
    conn.push_str("TYPE", "string");
    conn.push_str("TYPE", "list");
    conn.push_int("PTTL", -1);
    conn.push_int("PTTL", 700);

    let fields = fetch_memory_sample_fields(&mut conn, &keys, Topology::Cluster)
        .await
        .expect("cluster field read");
    assert_eq!(fields[0].bytes, Some(50));
    assert_eq!(fields[1].key_type.as_deref(), Some("list"));
    assert_eq!(fields[1].ttl_ms, Some(700));
    let journal = conn.journal();
    assert!(
        journal.batches.is_empty(),
        "cluster must never issue a mixed-slot pipeline"
    );
    assert_eq!(
        journal.singles.len(),
        2 * MEMORY_SAMPLE_FIELDS_PER_KEY,
        "the default shape replays every probe as an addressed single"
    );
}
