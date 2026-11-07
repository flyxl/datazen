// ==========================================================================
// [tester] 覆盖率补齐（redis-cmds-p0 测试子代理，只测不修）
//
// Wave 2 writes its TypeScript straight from the *serialized* field names, so
// the JSON shape is the highest-risk surface in this track and had no test at
// all until now. The remaining cases fill branches the Coder's suite never
// reaches: the PTTL degradation fallback, the container shapes of
// `is_unusable_reply`, the `truncate(limit)` overshoot, the empty-batch
// short-circuit, DBSIZE's non-numeric fallback, and a short pipeline reply.
// ==========================================================================
use super::*;

#[test]
fn test_tester_type_distribution_json_shape_is_the_wave2_contract() {
    let mut counts = BTreeMap::new();
    counts.insert("string".to_string(), 30);
    counts.insert("ReJSON-RL".to_string(), 2);
    let dist = TypeDistribution::from_sample(counts, 52);

    let json = serde_json::to_value(&dist).expect("TypeDistribution must serialize");
    let mut fields: Vec<&str> = json
        .as_object()
        .expect("payload must be a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, vec!["counts", "dbsize", "sampled", "truncated"]);
    // `dbsize` stays all-lowercase: `rename_all = "camelCase"` does not touch a
    // word without underscores, and Wave 2 spells the field `dbsize`.
    assert_eq!(
        json["dbsize"],
        serde_json::json!(52),
        "must not serialize as dbSize"
    );
    assert!(json.get("dbSize").is_none());
    assert_eq!(json["sampled"], serde_json::json!(32));
    assert_eq!(json["truncated"], serde_json::json!(true));
    assert_eq!(json["counts"]["string"], serde_json::json!(30));
    assert_eq!(json["counts"]["ReJSON-RL"], serde_json::json!(2));
}

#[test]
fn test_tester_key_object_info_json_shape_is_the_wave2_contract() {
    let info = KeyObjectInfo {
        missing: false,
        key_type: Some("hash".to_string()),
        memory_bytes: Some(4096),
        encoding: Some("listpack".to_string()),
        idle_seconds: Some(12),
        freq: None,
        ttl_ms: -1,
    };
    let json = serde_json::to_value(&info).expect("KeyObjectInfo must serialize");
    let mut fields: Vec<&str> = json
        .as_object()
        .expect("payload must be a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        vec![
            "encoding",
            "freq",
            "idleSeconds",
            "memoryBytes",
            "missing",
            "ttlMs",
            "type"
        ]
    );
    assert_eq!(json["type"], serde_json::json!("hash"));
    assert_eq!(json["memoryBytes"], serde_json::json!(4096));
    assert_eq!(json["idleSeconds"], serde_json::json!(12));
    assert_eq!(json["ttlMs"], serde_json::json!(-1));
    // A degraded field must appear as an explicit `null`, never as an absent
    // key: the sidebar distinguishes "unreadable" from "not implemented".
    assert!(json["freq"].is_null());
    assert!(json.as_object().expect("object").contains_key("freq"));
}

#[test]
fn test_tester_expired_and_unknown_key_states_are_distinguishable() {
    // There is no dedicated state field in the contract, so Wave 2 has to tell
    // "key expired" apart from "attributes unreadable" from the
    // (missing, type, ttlMs) triple alone. Lock that this works.
    let mut expired = full_key_info_replies();
    expired[key_info_slots::TYPE] = RValue::BulkString(b"none".to_vec());
    expired[key_info_slots::TTL] = RValue::Int(-2);
    let expired = parse_key_info(&expired);
    assert!(expired.missing);
    assert_eq!(expired.key_type, None);
    assert_eq!(expired.ttl_ms, TTL_MISSING);

    let mut unknown = full_key_info_replies();
    unknown[key_info_slots::TYPE] = err_reply("ERR unknown command 'TYPE'");
    let unknown = parse_key_info(&unknown);
    assert!(!unknown.missing);
    assert_eq!(unknown.key_type, None);
    assert_eq!(unknown.ttl_ms, TTL_NO_EXPIRY);

    assert_ne!(
        expired, unknown,
        "侧栏必须能区分『键已过期』与『读取失败/未知态』"
    );
}

#[test]
fn test_tester_key_info_pttl_rejection_only_blanks_the_ttl_slot() {
    // PTTL is absent below Redis 2.6 and may be blocked by a proxy: only the
    // TTL slot may degrade — the warn! branch had no test before.
    let mut values = full_key_info_replies();
    values[key_info_slots::TTL] = err_reply("unknown command 'PTTL'");
    let info = parse_key_info(&values);
    assert!(!info.missing);
    assert_eq!(info.ttl_ms, TTL_NO_EXPIRY);
    assert_eq!(info.key_type.as_deref(), Some("string"));
    assert_eq!(info.memory_bytes, Some(104));
    assert_eq!(info.encoding.as_deref(), Some("embstr"));
    assert_eq!(info.idle_seconds, Some(37));
    assert_eq!(info.freq, Some(9));
}

#[test]
fn test_tester_unusable_reply_covers_the_container_shapes() {
    use redis::PushKind;
    // Array / Set: empty, or any unusable member, is unusable; a fully numeric
    // array is a value (some proxies wrap integers).
    assert!(is_unusable_reply(&RValue::Array(vec![])));
    assert!(is_unusable_reply(&RValue::Array(vec![
        RValue::Int(1),
        RValue::Nil
    ])));
    assert!(is_unusable_reply(&RValue::Array(vec![err_reply("x")])));
    assert!(is_unusable_reply(&RValue::Array(vec![RValue::Array(
        vec![RValue::Nil]
    )])));
    assert!(!is_unusable_reply(&RValue::Array(vec![
        RValue::Int(1),
        RValue::Int(2)
    ])));
    assert!(!is_unusable_reply(&RValue::Set(vec![RValue::Int(1)])));
    assert!(is_unusable_reply(&RValue::Set(vec![RValue::Nil])));
    // Shapes that cannot carry a scalar answer are unusable by construction.
    assert!(is_unusable_reply(&RValue::Map(vec![(
        RValue::Int(1),
        RValue::Int(2)
    )])));
    assert!(is_unusable_reply(&RValue::Attribute {
        data: Box::new(RValue::Int(1)),
        attributes: vec![],
    }));
    assert!(is_unusable_reply(&RValue::Push {
        kind: PushKind::Message,
        data: vec![RValue::Int(1)],
    }));
}

#[test]
fn test_tester_reply_parsers_cover_the_remaining_value_shapes() {
    assert_eq!(
        parse_opt_string(&RValue::SimpleString("hashtable".into())),
        Some("hashtable".to_string()),
        "RESP3 servers reply to OBJECT ENCODING with a simple string"
    );
    assert_eq!(
        parse_opt_string(&RValue::VerbatimString {
            format: redis::VerbatimFormat::Text,
            text: "raw".into(),
        }),
        Some("raw".to_string())
    );
    assert_eq!(
        parse_opt_string(&RValue::Okay),
        None,
        "an uninterpretable shape must not be invented into a value"
    );
    assert_eq!(
        parse_opt_int(&RValue::BulkString(b" 42 ".to_vec())),
        Some(42)
    );
    assert_eq!(parse_opt_int(&RValue::SimpleString("7".into())), Some(7));
    assert_eq!(parse_opt_int(&RValue::Okay), None);
    assert_eq!(
        parse_type_token(&RValue::BulkString(b"   ".to_vec())),
        None,
        "a blank token must never enter `counts`"
    );
    assert_eq!(
        parse_type_token(&RValue::VerbatimString {
            format: redis::VerbatimFormat::Text,
            text: "stream".into(),
        }),
        Some("stream".to_string())
    );
    assert_eq!(parse_type_token(&RValue::Okay), None);
}

#[tokio::test]
async fn test_tester_sample_window_is_clamped_to_the_limit_not_the_batch() {
    // One SCAN page already overshoots the window (5 keys vs limit 3): only the
    // first `limit` keys may be typed, or `sampled` would exceed `sampleLimit`.
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 100);
    conn.push_scan(0, &["e", "d", "c", "b", "a"]);
    conn.push_types(&["string", "hash", "list", "zset", "set"]);

    let dist = type_distribution(&mut conn, Some(3), Topology::Standalone)
        .await
        .expect("overshoot inside one batch must be truncated, not fatal");
    assert_eq!(dist.sampled, 3, "sampled may never exceed sampleLimit");
    assert_eq!(dist.dbsize, 100);
    assert!(dist.truncated);
    let journal = conn.journal();
    assert_eq!(
        journal.batches[0].len(),
        3,
        "exactly `limit` TYPE commands may be issued"
    );
    assert_eq!(
        journal
            .flat()
            .iter()
            .filter(|c| c.first().map(String::as_str) == Some("TYPE"))
            .count(),
        3
    );
}

#[tokio::test]
async fn test_tester_pipeline_raw_short_circuits_an_empty_batch() {
    let mut conn = ScriptedConn::new();
    let empty = redis::Pipeline::new();
    let values = pipeline_raw(&mut conn, &empty)
        .await
        .expect("an empty batch is not a failure");
    assert!(values.is_empty());
    assert!(
        conn.journal().batches.is_empty(),
        "an empty batch must not hit the wire at all"
    );
}

#[tokio::test]
async fn test_tester_dbsize_falls_back_to_zero_on_a_non_numeric_reply() {
    // Mirrors the existing DBSIZE handling in `scan_keys_with_info_on`: a
    // well-formed but non-integer reply degrades to 0 rather than failing.
    let mut conn = ScriptedConn::new();
    conn.push("DBSIZE", RValue::Nil);
    conn.push_scan(0, &["a"]);
    conn.push_type("string");
    let dist = type_distribution(&mut conn, Some(10), Topology::Standalone)
        .await
        .expect("an unparsable DBSIZE must not fail the command");
    assert_eq!(dist.dbsize, 0);
    assert_eq!(dist.sampled, 1);

    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", -5);
    conn.push_scan(0, &[]);
    let dist = type_distribution(&mut conn, None, Topology::Standalone)
        .await
        .expect("a negative DBSIZE must not underflow");
    assert_eq!(dist.dbsize, 0);
    assert!(!dist.truncated);
}

#[tokio::test]
async fn test_tester_short_type_reply_never_inflates_the_sample() {
    let keys: Vec<String> = (0..5).map(|i| format!("k{i}")).collect();
    let mut conn = ShortReplyConn { replies: 2 };
    let counts = sample_types(&mut conn, &keys, Topology::Standalone)
        .await
        .expect("a short reply is degraded, not fatal");
    assert_eq!(
        counts.values().sum::<u64>(),
        2,
        "unanswered commands must leave `sampled`, keeping the sum invariant"
    );
    assert_eq!(counts.get("string"), Some(&2));
}

#[tokio::test]
async fn test_tester_module_type_tokens_survive_the_whole_command() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 4);
    conn.push_scan(0, &["j1", "j2", "s1", "h1"]);
    conn.push_types(&["ReJSON-RL", "ReJSON-RL", "string", "hash"]);

    let dist = type_distribution(&mut conn, Some(10), Topology::Standalone)
        .await
        .expect("type distribution");
    assert_eq!(
        dist.counts.get("ReJSON-RL"),
        Some(&2),
        "module types keep their raw server token — no whitelist folding"
    );
    assert_eq!(dist.counts.len(), 3);
    assert_eq!(dist.sampled, 4);
    assert_eq!(
        serde_json::to_value(&dist).expect("serialize")["counts"]["ReJSON-RL"],
        serde_json::json!(2)
    );
}

#[tokio::test]
async fn test_tester_keys_is_issued_in_neither_shape_on_either_path() {
    // The Coder's KEYS assertions scan `journal.single_names()` only, so a
    // future regression that hides KEYS inside a pipeline batch would slip
    // through; sweep singles *and* batches for both commands instead.
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 2);
    conn.push_scan(0, &["a", "b"]);
    conn.push_types(&["string", "hash"]);
    type_distribution(&mut conn, Some(10), Topology::Standalone)
        .await
        .expect("type distribution");

    conn.push_int("MEMORY", 10);
    conn.push_str("OBJECT", "embstr");
    conn.push_int("OBJECT", 1);
    conn.push_int("OBJECT", 2);
    conn.push_int("PTTL", -1);
    conn.push_str("TYPE", "string");
    key_object_info(&mut conn, "a", Topology::Standalone)
        .await
        .expect("key_object_info");

    let journal = conn.journal();
    let calls = journal.flat();
    let offenders: Vec<&Vec<String>> = calls
        .iter()
        .filter(|c| {
            c.first()
                .map(String::as_str)
                .is_some_and(|n| n.eq_ignore_ascii_case("KEYS"))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "KEYS blocks the server for the whole keyspace and must never be issued, got {offenders:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(String::as_str) == Some("SCAN")),
        "sampling must actually go through SCAN"
    );
    // `FLUSHDB`-class guard rail: nothing outside the documented command set.
    let allowed = ["DBSIZE", "SCAN", "TYPE", "MEMORY", "OBJECT", "PTTL"];
    for call in &calls {
        let name = call.first().map(String::as_str).unwrap_or_default();
        assert!(
            allowed.contains(&name),
            "unexpected command {name} issued by the workbench probes: {journal:?}"
        );
    }
}
