// --- contract helpers (pure) ------------------------------------------
use super::*;

#[test]
fn sample_limit_defaults_and_clamps_without_erroring() {
    assert_eq!(resolve_type_sample_limit(None), DEFAULT_TYPE_SAMPLE_LIMIT);
    assert_eq!(
        resolve_type_sample_limit(Some(0)),
        DEFAULT_TYPE_SAMPLE_LIMIT
    );
    assert_eq!(resolve_type_sample_limit(Some(200)), 200);
    assert_eq!(resolve_type_sample_limit(Some(5_000)), 5_000);
    // Oversized requests are capped, never rejected.
    assert_eq!(
        resolve_type_sample_limit(Some(5_001)),
        MAX_TYPE_SAMPLE_LIMIT
    );
    assert_eq!(
        resolve_type_sample_limit(Some(u64::MAX)),
        MAX_TYPE_SAMPLE_LIMIT
    );
}

#[test]
fn truncated_flag_is_the_sample_vs_census_bit() {
    assert!(is_sample_truncated(1_000, 52_000));
    assert!(!is_sample_truncated(52, 52));
    assert!(!is_sample_truncated(0, 0));

    let mut counts = BTreeMap::new();
    counts.insert("string".to_string(), 30);
    counts.insert("hash".to_string(), 12);
    let full = TypeDistribution::from_sample(counts.clone(), 42);
    assert_eq!(full.sampled, 42);
    assert!(!full.truncated, "a fully scanned db reads as a census");

    let sample = TypeDistribution::from_sample(counts, 1_000);
    assert!(sample.truncated);
    // Invariant the "采样 N/M" label relies on.
    assert_eq!(sample.sampled, sample.counts.values().sum::<u64>());
}

#[test]
fn key_info_pipeline_is_one_batch_and_never_scans_the_keyspace() {
    let pipe = build_key_info_pipeline("app:cache:session:1");
    let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
    assert_eq!(batch.len(), KEY_INFO_PIPELINE_LEN);
    let names: Vec<&str> = batch.iter().map(|c| c[0].as_str()).collect();
    assert_eq!(
        names,
        vec!["MEMORY", "OBJECT", "OBJECT", "OBJECT", "PTTL", "TYPE"]
    );
    assert!(
        !names.iter().any(|n| n.eq_ignore_ascii_case("KEYS")),
        "KEYS is forbidden: it blocks the server for the whole keyspace"
    );
    let object_subs: Vec<&str> = batch
        .iter()
        .filter(|c| c[0] == "OBJECT")
        .map(|c| c[1].as_str())
        .collect();
    assert_eq!(object_subs, vec!["ENCODING", "IDLETIME", "FREQ"]);
    for args in &batch {
        assert_eq!(args.last().map(String::as_str), Some("app:cache:session:1"));
    }
}

#[test]
fn error_and_absent_replies_are_distinguishable() {
    assert!(is_unusable_reply(&RValue::Nil));
    assert!(is_unusable_reply(&err_reply("no such key")));
    assert!(is_unusable_reply(&RValue::Map(vec![])));
    assert!(!is_unusable_reply(&RValue::Int(0)), "0 is a value");
    assert!(!is_unusable_reply(&RValue::Int(-1)));
    assert!(!is_unusable_reply(&RValue::BulkString(b"embstr".to_vec())));
    // An unrecognised shape must not be read as "the key is gone".
    assert!(is_unusable_reply(&RValue::Set(vec![])));
}

#[test]
fn type_token_parsing_drops_missing_and_errored_replies() {
    assert_eq!(
        parse_type_token(&RValue::BulkString(b"hash".to_vec())),
        Some("hash".to_string())
    );
    assert_eq!(
        parse_type_token(&RValue::SimpleString("ReJSON-RL".into())),
        Some("ReJSON-RL".to_string())
    );
    assert_eq!(
        parse_type_token(&RValue::BulkString(b"  string ".to_vec())),
        Some("string".to_string())
    );
    assert_eq!(
        parse_type_token(&RValue::BulkString(b"none".to_vec())),
        None
    );
    assert_eq!(parse_type_token(&RValue::Nil), None);
    assert_eq!(parse_type_token(&err_reply("ERR wrong")), None);
}

#[test]
fn numeric_parsing_tolerates_bulk_form_and_rejects_garbage() {
    assert_eq!(parse_opt_int(&RValue::Int(7)), Some(7));
    assert_eq!(
        parse_opt_int(&RValue::BulkString(b"12000".to_vec())),
        Some(12_000)
    );
    assert_eq!(parse_opt_int(&err_reply("not supported")), None);
    assert_eq!(parse_opt_int(&RValue::Nil), None);
    assert_eq!(parse_opt_int(&RValue::BulkString(b"embstr".to_vec())), None);
    assert_eq!(
        parse_opt_string(&RValue::BulkString(b"ziplist".to_vec())),
        Some("ziplist".to_string())
    );
    assert_eq!(parse_opt_string(&err_reply("no such key")), None);
}
