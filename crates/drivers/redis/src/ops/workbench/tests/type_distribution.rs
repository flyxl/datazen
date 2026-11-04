// --- type_distribution -----------------------------------------------
use super::*;

#[tokio::test]
async fn type_distribution_counts_a_fully_scanned_small_db() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 6);
    conn.push_scan(0, &["a", "b", "c", "d", "e", "f"]);
    conn.push_types(&["string", "string", "hash", "list", "string", "stream"]);

    let dist = type_distribution(&mut conn, Some(100), Topology::Standalone)
        .await
        .expect("type distribution");

    assert_eq!(dist.dbsize, 6);
    assert_eq!(dist.sampled, 6);
    assert!(!dist.truncated, "52/52 must not be flagged as a sample");
    assert_eq!(dist.counts.get("string"), Some(&3));
    assert_eq!(dist.counts.get("hash"), Some(&1));
    assert_eq!(dist.counts.get("list"), Some(&1));
    assert_eq!(dist.counts.get("stream"), Some(&1));
    assert_eq!(dist.counts.get("zset"), None);

    let journal = conn.journal();
    assert_eq!(
        journal.count_single("DBSIZE"),
        1,
        "DBSIZE is read exactly once per action"
    );
    assert_eq!(journal.batches.len(), 1, "TYPE is pipelined");
    assert_eq!(journal.flat().iter().filter(|c| c[0] == "TYPE").count(), 6);
}

#[tokio::test]
async fn type_distribution_flags_truncated_when_the_sample_is_partial() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 50_000);
    // Non-zero cursor and a full window after the first batch.
    conn.push_scan(4_096, &["k1", "k2", "k3"]);
    conn.push_types(&["string", "hash", "hash"]);

    let dist = type_distribution(&mut conn, Some(3), Topology::Standalone)
        .await
        .expect("type distribution");

    assert_eq!(dist.dbsize, 50_000);
    assert_eq!(dist.sampled, 3);
    assert!(dist.truncated, "3 of 50000 must be reported as a sample");
    assert_eq!(dist.counts.get("hash"), Some(&2));
    let journal = conn.journal();
    assert_eq!(
        journal.count_single("SCAN"),
        1,
        "scanning stops as soon as the window is full"
    );
    assert_eq!(journal.batches.len(), 1);
}

#[tokio::test]
async fn type_distribution_pipelines_types_in_chunks_and_never_calls_keys() {
    let keys: Vec<String> = (0..(TYPE_PIPELINE_CHUNK + 7))
        .map(|i| format!("key:{i}"))
        .collect();
    let flat: Vec<&str> = keys.iter().map(String::as_str).collect();

    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", keys.len() as i64);
    conn.push_scan(0, &flat);
    conn.push_types(&vec!["string"; keys.len()]);

    let dist = type_distribution(&mut conn, Some(MAX_TYPE_SAMPLE_LIMIT), Topology::Standalone)
        .await
        .expect("type distribution");

    assert_eq!(dist.sampled, keys.len() as u64);
    assert!(!dist.truncated);
    let journal = conn.journal();
    let expected_batches = keys.len().div_ceil(TYPE_PIPELINE_CHUNK);
    assert_eq!(
        journal.batches.len(),
        expected_batches,
        "one TYPE round trip per {TYPE_PIPELINE_CHUNK} keys"
    );
    assert_eq!(
        journal.batches.iter().map(|b| b.len()).collect::<Vec<_>>(),
        vec![TYPE_PIPELINE_CHUNK, 7]
    );
    let names = journal.single_names();
    assert!(
        !names.iter().any(|n| n == "KEYS"),
        "KEYS is forbidden, got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "TYPE"),
        "TYPE must never be issued one key at a time"
    );
}

#[tokio::test]
async fn type_distribution_continues_scanning_until_the_cursor_wraps() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 6);
    conn.push_scan_batches(&[
        (1, vec!["a".to_string(), "b".to_string()]),
        (2, vec!["c".to_string(), "d".to_string()]),
        (0, vec!["e".to_string(), "f".to_string()]),
    ]);
    conn.push_types(&["string", "string", "string", "hash", "hash", "zset"]);

    let dist = type_distribution(&mut conn, Some(500), Topology::Standalone)
        .await
        .expect("type distribution");

    let journal = conn.journal();
    assert_eq!(
        journal.count_single("SCAN"),
        3,
        "the loop follows the cursor until it returns to 0"
    );
    assert_eq!(dist.sampled, 6);
    assert_eq!(dist.counts.get("zset"), Some(&1));
    assert!(!dist.truncated);
}

#[tokio::test]
async fn type_distribution_clamps_an_oversized_window_instead_of_erroring() {
    // Fifteen batches of 500 unique keys; the window must stop at the clamp.
    let batches: Vec<(u64, Vec<String>)> = (0..15)
        .map(|b| {
            (
                7,
                (0..TYPE_SCAN_COUNT as usize)
                    .map(|i| format!("k{b}:{i}"))
                    .collect(),
            )
        })
        .collect();
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 100_000);
    conn.push_scan_batches(&batches);
    conn.push_types(&vec!["string"; MAX_TYPE_SAMPLE_LIMIT as usize * 2]);

    let dist = type_distribution(&mut conn, Some(u64::MAX), Topology::Standalone)
        .await
        .expect("an oversized sampleLimit is clamped, not rejected");

    assert_eq!(dist.sampled, MAX_TYPE_SAMPLE_LIMIT);
    assert!(dist.truncated);
    let journal = conn.journal();
    assert_eq!(
        journal.count_single("SCAN"),
        (MAX_TYPE_SAMPLE_LIMIT / TYPE_SCAN_COUNT as u64) as usize,
        "SCAN stops at the clamped window, not at the keyspace"
    );
    assert_eq!(journal.batches.len(), 10);
}

#[tokio::test]
async fn type_distribution_skips_keys_that_vanished_before_type() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 4);
    conn.push_scan(0, &["a", "b", "c", "d"]);
    conn.push_type("string");
    conn.push_type("none"); // evicted between SCAN and TYPE
    conn.push_type("string");
    conn.push("TYPE", err_reply("ERR unknown key"));

    let dist = type_distribution(&mut conn, Some(10), Topology::Standalone)
        .await
        .expect("type distribution");

    // Dropped keys leave both `counts` and `sampled`, so the sum invariant
    // holds while `truncated` still warns about the gap.
    assert_eq!(dist.sampled, 2);
    assert_eq!(dist.counts.get("string"), Some(&2));
    assert_eq!(dist.counts.len(), 1);
    assert!(dist.truncated);
}

#[tokio::test]
async fn type_distribution_on_an_empty_db_is_not_truncated() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 0);
    conn.push_scan(0, &[]);

    let dist = type_distribution(&mut conn, None, Topology::Standalone)
        .await
        .expect("type distribution");

    assert_eq!(dist.dbsize, 0);
    assert_eq!(dist.sampled, 0);
    assert!(!dist.truncated);
    assert!(dist.counts.is_empty());
    let journal = conn.journal();
    assert!(
        journal.batches.is_empty(),
        "no sampled keys means no TYPE pipeline at all"
    );
    assert_eq!(journal.total(), 2, "DBSIZE + one SCAN");
}

#[tokio::test]
async fn type_distribution_deduplicates_repeated_scan_results() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 2);
    // SCAN is a hint-based cursor: the same key may be returned twice.
    conn.push_scan(1, &["a", "b"]);
    conn.push_scan(0, &["a", "b"]);
    conn.push_types(&["string", "hash", "string", "hash"]);

    let dist = type_distribution(&mut conn, Some(100), Topology::Standalone)
        .await
        .expect("type distribution");

    assert_eq!(dist.sampled, 2, "each key is typed once");
    assert_eq!(dist.counts.get("string"), Some(&1));
    assert_eq!(dist.counts.get("hash"), Some(&1));
    assert!(!dist.truncated);
}

/// Per-command degradation must never turn into "swallow the transport".
/// A rejected DBSIZE or SCAN is a failed read, not an empty database, so
/// the command aborts before it starts sampling.
#[tokio::test]
async fn rejected_dbsize_aborts_the_distribution_instead_of_reading_zero() {
    let mut conn = ScriptedConn::new();
    conn.push("DBSIZE", err_reply("ERR unknown command"));

    let err = type_distribution(&mut conn, Some(10), Topology::Standalone)
        .await
        .expect_err("a rejected DBSIZE must reach the caller");
    assert!(!err.is_empty(), "the failure reason must be carried over");
    assert_eq!(
        conn.journal().singles.len(),
        1,
        "nothing may be sampled against a failed probe: {:?}",
        conn.journal()
    );
}

#[tokio::test]
async fn rejected_scan_aborts_the_distribution_before_typing_anything() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 1_000);
    conn.push("SCAN", err_reply("ERR not allowed on this replica"));

    let err = type_distribution(&mut conn, None, Topology::Standalone)
        .await
        .expect_err("a rejected SCAN must reach the caller");
    assert!(!err.is_empty());
    let journal = conn.journal();
    assert_eq!(journal.singles.len(), 2, "DBSIZE + one SCAN, then stop");
    assert!(
        journal.batches.is_empty(),
        "a failed scan must not type a partial key list"
    );
}

#[tokio::test]
async fn a_missing_reply_stream_is_read_as_empty_not_as_data() {
    // Regression guard for the degenerate case: a connection that answers
    // nothing yields an empty census instead of invented counts, and the
    // `sampled == counts.sum()` invariant still holds.
    let mut conn = ScriptedConn::new();
    let dist = type_distribution(&mut conn, Some(10), Topology::Standalone)
        .await
        .expect("silent nil replies stay parseable");
    assert_eq!(dist.dbsize, 0);
    assert_eq!(dist.sampled, 0);
    assert!(dist.counts.is_empty());
    assert!(!dist.truncated);
}
