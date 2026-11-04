// --- key_object_info -------------------------------------------------
use super::*;

#[test]
fn key_info_reads_every_field_when_the_server_supports_them() {
    assert_eq!(
        parse_key_info(&full_key_info_replies()),
        KeyObjectInfo {
            missing: false,
            key_type: Some("string".to_string()),
            memory_bytes: Some(104),
            encoding: Some("embstr".to_string()),
            idle_seconds: Some(37),
            freq: Some(9),
            ttl_ms: 12_000,
        }
    );
}

#[test]
fn key_info_reports_missing_as_success() {
    let mut values = full_key_info_replies();
    values[key_info_slots::TYPE] = RValue::BulkString(b"none".to_vec());
    values[key_info_slots::MEMORY] = RValue::Nil;
    values[key_info_slots::ENCODING] = err_reply("no such key");
    values[key_info_slots::TTL] = RValue::Int(-2);

    let info = parse_key_info(&values);
    assert_eq!(info, KeyObjectInfo::missing());
    assert!(info.missing);
    assert_eq!(info.ttl_ms, TTL_MISSING);
    assert_eq!(info.key_type, None);
    assert_eq!(info.memory_bytes, None);
}

#[test]
fn key_info_degrades_freq_and_memory_independently() {
    // maxmemory-policy is not LFU and MEMORY USAGE is unavailable
    // (Redis < 4.0): those two fields go None, everything else survives.
    let mut values = full_key_info_replies();
    values[key_info_slots::MEMORY] =
        err_reply("unknown subcommand or wrong number of args for 'USAGE'");
    values[key_info_slots::FREQ] =
        err_reply("an object access frequency counter is not available with this configuration");
    values[key_info_slots::TTL] = RValue::Int(-1);
    values[key_info_slots::TYPE] = RValue::BulkString(b"list".to_vec());

    let info = parse_key_info(&values);
    assert!(!info.missing);
    assert_eq!(info.key_type.as_deref(), Some("list"));
    assert_eq!(info.memory_bytes, None);
    assert_eq!(info.freq, None);
    assert_eq!(info.encoding.as_deref(), Some("embstr"));
    assert_eq!(info.idle_seconds, Some(37));
    assert_eq!(info.ttl_ms, TTL_NO_EXPIRY);
}

#[test]
fn key_info_keeps_a_live_key_when_only_type_errors() {
    let mut values = full_key_info_replies();
    values[key_info_slots::TYPE] = err_reply("ERR unknown command 'TYPE'");
    let info = parse_key_info(&values);
    assert!(
        !info.missing,
        "an errored TYPE must not claim the key is gone"
    );
    assert_eq!(info.key_type, None);
    assert_eq!(info.ttl_ms, TTL_NO_EXPIRY);
    // Without a type the key's state is unknown, so per-key attributes are
    // not reported either — a partly-trusted payload would read as fact.
    assert_eq!(info.idle_seconds, None);
    assert_eq!(info.memory_bytes, None);
    assert_eq!(info.encoding, None);
    assert_eq!(info.freq, None);
}

#[test]
fn key_info_survives_a_short_or_empty_reply_vector() {
    // An absent slot is not a server answer, so a truncated vector reads as
    // "attributes unreadable" rather than falsely claiming the key is gone.
    let empty = parse_key_info(&[]);
    assert!(!empty.missing);
    assert_eq!(
        empty,
        KeyObjectInfo {
            ttl_ms: TTL_NO_EXPIRY,
            ..KeyObjectInfo::default()
        }
    );
    assert_eq!(
        empty.ttl_ms, TTL_NO_EXPIRY,
        "a live-but-unreadable key still reports a defined ttl"
    );

    let short = vec![RValue::Int(10), RValue::BulkString(b"intset".to_vec())];
    let info = parse_key_info(&short);
    assert!(!info.missing);
    assert_eq!(info.key_type, None);
    assert_eq!(
        info.memory_bytes, None,
        "no type means no trusted attributes"
    );
}

#[tokio::test]
async fn key_object_info_uses_one_round_trip_and_survives_a_rejected_field() {
    // Scope of the "one round trip" claim: this is the *op*, called directly.
    // The command path additionally issues the db `SELECT` that
    // `with_live_op!` sends on every call, so a real `key_object_info` costs two
    // round trips (and the whole probe is replayed once after a Sentinel
    // failover). MONITOR-based regression checks must expect that SELECT —
    // counting only the probe's own traffic is what this journal can see.
    let mut conn = ScriptedConn::new();
    conn.push_int("MEMORY", 88);
    conn.push_str("OBJECT", "listpack");
    conn.push_int("OBJECT", 12);
    conn.push("OBJECT", err_reply("freq counter is not available"));
    conn.push_int("PTTL", 60_000);
    conn.push_str("TYPE", "list");

    let info = key_object_info(&mut conn, "lqueue", Topology::Standalone)
        .await
        .expect("key_object_info must not fail because one field errored");

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
    assert_eq!(
        journal.round_trips(),
        1,
        "the op's own payload must cost exactly one pipeline round trip, \
         excluding the db SELECT issued by with_live_op!"
    );
    assert!(journal.singles.is_empty(), "no per-command round trips");
    assert_eq!(journal.batches[0].len(), KEY_INFO_PIPELINE_LEN);
}

#[tokio::test]
async fn key_object_info_reports_missing_over_the_wire() {
    let mut conn = ScriptedConn::new();
    conn.push("MEMORY", RValue::Nil);
    conn.push("OBJECT", err_reply("no such key"));
    conn.push("OBJECT", err_reply("no such key"));
    conn.push("OBJECT", err_reply("no such key"));
    conn.push_int("PTTL", -2);
    conn.push_str("TYPE", "none");

    let info = key_object_info(&mut conn, "gone:soon", Topology::Standalone)
        .await
        .expect("a missing key is a success case");
    assert_eq!(info, KeyObjectInfo::missing());
    // Op-level count again (see the scope note on the round-trip test above):
    // the command layer pays one more round trip for `with_live_op!`'s SELECT.
    assert_eq!(conn.journal().round_trips(), 1);
}
