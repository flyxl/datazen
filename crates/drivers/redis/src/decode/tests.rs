//! Tests for the `decode_value` command surface: codec set parity with the
//! value viewer, the unified success envelope, and the in-band failure
//! structure (`reason` / `suggestedCodec` / `retryable`).
//!
//! Assertions match on **reason codes only** — never on the human-readable
//! `message`, which is free text and not part of the contract.

use super::*;
use base64::Engine;

/// Every test goes through the command's own JSON shape, not the Rust helper.
fn decode_input(input: JsonValue) -> JsonValue {
    decode_value(&input)
}

fn decode(codec: &str, bytes: &[u8]) -> JsonValue {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    decode_input(serde_json::json!({ "codec": codec, "data": b64 }))
}

fn decoded(value: &JsonValue) -> bool {
    value["ok"].as_bool().unwrap_or(false)
}

fn reason_of(value: &JsonValue) -> Option<&str> {
    value["reason"].as_str()
}

fn suggested_of(value: &JsonValue) -> Option<&str> {
    value["suggestedCodec"].as_str()
}

fn gzip(payload: &[u8]) -> Vec<u8> {
    compress::test_support::gzip(payload)
}

fn zlib(payload: &[u8]) -> Vec<u8> {
    compress::test_support::zlib(payload)
}

fn raw_deflate(payload: &[u8]) -> Vec<u8> {
    compress::test_support::raw_deflate(payload)
}

#[test]
fn rejects_unknown_codec_and_missing_payload() {
    let unknown = decode_input(serde_json::json!({ "codec": "bson", "data": "AAA=" }));
    assert!(!decoded(&unknown));
    assert_eq!(reason_of(&unknown), Some("unknown-codec"));
    assert_eq!(unknown["codec"], serde_json::json!("bson"));
    assert_eq!(suggested_of(&unknown), None);
    assert_eq!(unknown["retryable"], serde_json::json!(false));

    let missing = decode_input(serde_json::json!({ "codec": "msgpack" }));
    assert_eq!(reason_of(&missing), Some("missing-data"));

    let junk = decode_input(serde_json::json!({ "codec": "msgpack", "data": "not!!base64!!" }));
    assert_eq!(reason_of(&junk), Some("invalid-base64"));

    assert_eq!(reason_of(&decode("msgpack", b"")), Some("empty-payload"));
}

#[test]
fn decode_ok_shape_for_supported_codecs() {
    // msgpack {"a":1}
    let r = decode("msgpack", &[0x81, 0xa1, b'a', 0x01]);
    assert!(decoded(&r));
    assert_eq!(r["ok"], serde_json::json!(true));
    assert!(r["json"].as_str().unwrap().contains("\"a\""));
    assert_eq!(r["codec"], serde_json::json!("msgpack"));
    assert_eq!(r["kind"], serde_json::json!("json"));
}

#[test]
fn every_ui_codec_name_is_accepted_by_the_backend() {
    // Mirrors `CODECS` in ui/value-editors/valueView/codecs.ts — if the UI
    // adds an option the backend must answer for it, not shrug.
    let payload = b"datazen".to_vec();
    let vectors: [(&str, Vec<u8>); 5] = [
        ("none", payload.clone()),
        ("base64", b"ZGF0YXplbg==".to_vec()),
        ("gzip", gzip(&payload)),
        ("zlib", zlib(&payload)),
        ("deflate", raw_deflate(&payload)),
    ];
    for (name, stored) in vectors {
        let r = decode(name, &stored);
        assert!(decoded(&r), "codec {name} must decode, got: {r}");
        assert_eq!(r["codec"], serde_json::json!(name));
        assert_eq!(r["kind"], serde_json::json!("bytes"));
        assert_eq!(r["bytes"], serde_json::json!(payload.len()));
        assert_eq!(
            r["text"],
            serde_json::json!("datazen"),
            "codec {name} must surface UTF-8 text"
        );
        let round = base64::engine::general_purpose::STANDARD
            .decode(r["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(round, payload, "codec {name} base64 round trip");
    }
}

#[test]
fn byte_codecs_expose_a_chainable_payload_and_json_when_relevant() {
    // gzip(JSON) => both `data` (for a second decode_value call) and the
    // already-parsed pretty `json` for the viewer.
    let json = br#"{"a":1,"b":[1,2,3]}"#;
    let wrapped = gzip(json);
    let r = decode("gzip", &wrapped);
    assert!(decoded(&r));
    assert_eq!(r["kind"], serde_json::json!("bytes"));
    assert_eq!(r["inBytes"], serde_json::json!(wrapped.len()));
    assert_eq!(r["bytes"], serde_json::json!(json.len()));
    assert!(r["json"].as_str().unwrap().contains("\"b\""));

    // The chained second hop works off `data` alone: gzip -> msgpack.
    let mpk = [0x81u8, 0xa1, b'k', 0x2a];
    let hop1 = decode("gzip", &gzip(&mpk));
    assert_eq!(hop1["kind"], serde_json::json!("bytes"));
    let chained = decode_input(serde_json::json!({ "codec": "msgpack", "data": hop1["data"] }));
    assert!(
        decoded(&chained),
        "gzip+msgpack chain must resolve: {chained}"
    );
    assert!(chained["json"].as_str().unwrap().contains("\"k\""));

    // Non-UTF-8 expansion: `text`/`json` go null, `data` stays lossless.
    let binary = [0u8, 0x1f, 0x8b, 0xff, 0xfe];
    let r = decode("deflate", &raw_deflate(&binary));
    assert!(decoded(&r));
    assert_eq!(r["text"], serde_json::Value::Null);
    assert_eq!(r["json"], serde_json::Value::Null);
    let round = base64::engine::general_purpose::STANDARD
        .decode(r["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(round, binary);
}

#[test]
fn zlib_and_deflate_are_separate_choices() {
    let payload = b"framing matters".to_vec();
    let zipped = zlib(&payload);
    let raw = raw_deflate(&payload);
    // Selecting the wrong framing must fail, not "helpfully" succeed.
    assert!(!decoded(&decode("deflate", &zipped)));
    assert!(!decoded(&decode("zlib", &raw)));
    assert!(decoded(&decode("zlib", &zipped)));
    assert!(decoded(&decode("deflate", &raw)));
}

#[test]
fn wrong_framing_failure_names_the_framing_that_would_work() {
    // PRD §3.3: the UI renders the suggestion as its retry button, so the
    // payload has to carry the *canonical* codec name, not prose.
    let payload = b"retry hint".to_vec();

    let as_deflate = decode("gzip", &zlib(&payload));
    assert_eq!(reason_of(&as_deflate), Some("decode-failed"));
    assert_eq!(suggested_of(&as_deflate), Some("zlib"));
    assert_eq!(as_deflate["retryable"], serde_json::json!(true));
    assert!(decoded(&decode("zlib", &zlib(&payload))));

    let as_gzip = decode("deflate", &gzip(&payload));
    assert_eq!(suggested_of(&as_gzip), Some("gzip"));

    let headerless = decode("msgpack", &raw_deflate(&payload));
    assert_eq!(reason_of(&headerless), Some("decode-failed"));
    assert_eq!(suggested_of(&headerless), Some("deflate"));

    // Nothing salvageable: a corrupt gzip stream keeps its own header, so a
    // framing retry would only fail again — no suggestion, no dead button.
    let mut truncated = gzip(&payload);
    let cut = truncated.len() - 3;
    truncated.truncate(cut);
    let corrupt = decode("gzip", &truncated);
    assert_eq!(reason_of(&corrupt), Some("decode-failed"));
    assert_eq!(suggested_of(&corrupt), None);
    assert_eq!(corrupt["retryable"], serde_json::json!(false));
}

#[test]
fn compressed_payload_under_a_structured_choice_is_the_recommended_retry() {
    // The realistic mistake: a gzipped msgpack blob selected as "msgpack".
    let mpk = [0x81u8, 0xa1, b'a', 0x01];
    let r = decode("msgpack", &gzip(&mpk));
    assert_eq!(reason_of(&r), Some("decode-failed"));
    assert_eq!(suggested_of(&r), Some("gzip"));
    assert!(decoded(&decode("gzip", &gzip(&mpk))));
}

#[test]
fn structured_magics_are_suggested_when_the_format_choice_is_wrong() {
    // Java stream header, chosen as php.
    let java = [0xac, 0xed, 0x00, 0x05];
    let r = decode("php", &java);
    assert_eq!(reason_of(&r), Some("decode-failed"));
    assert_eq!(suggested_of(&r), Some("java"));

    // PHP serialize() text, chosen as msgpack.
    let php = b"a:1:{s:1:\"a\";i:1;}";
    let r = decode("msgpack", php);
    assert_eq!(reason_of(&r), Some("decode-failed"));
    assert_eq!(suggested_of(&r), Some("php"));

    // pickle protocol 4 (FRAME opcode right after the version byte).
    let pickle = [0x80, 0x04, 0x95, 0x04, 0x00, 0x00, 0x00, 0x00];
    let r = decode("msgpack", &pickle);
    assert_eq!(reason_of(&r), Some("decode-failed"));
    assert_eq!(suggested_of(&r), Some("pickle"));
}

#[test]
fn zip_bomb_and_broken_stream_map_to_different_reasons() {
    // The cap branch cannot be reached through the command without inflating
    // 50 MiB, so its reason mapping is pinned at the seam that decides it.
    assert_eq!(
        framing_failure_reason(&compress::DecompressFailure::TooLarge(1234)),
        Reason::DecompressedTooLarge
    );
    assert_eq!(
        framing_failure_reason(&compress::DecompressFailure::Failed("bad header".into())),
        Reason::DecodeFailed
    );
    assert_eq!(
        Reason::DecompressedTooLarge.code(),
        "decompressed-too-large"
    );

    // End to end, a small expansion under the real cap still succeeds, and the
    // ceiling is shared with the input guard.
    assert_eq!(MAX_DECOMPRESSED_BYTES, MAX_INPUT_BYTES);
    let r = decode("gzip", &gzip(&vec![b'7'; 4096]));
    assert!(decoded(&r));
    assert_eq!(r["bytes"], serde_json::json!(4096));
}

#[test]
fn protobuf_is_visible_as_deferred_not_as_garbage() {
    let r = decode("protobuf", &[0x08, 0x01]);
    assert!(!decoded(&r));
    assert_eq!(reason_of(&r), Some("codec-not-implemented"));
    // PRD §6 degradation: base64 + an external tool.
    assert_eq!(suggested_of(&r), Some("base64"));
    assert_eq!(r["retryable"], serde_json::json!(true));
}

#[test]
fn oversized_payload_reports_a_stable_code() {
    // The guard runs before any decoder sees the bytes, so it is checked at the
    // boundary rather than by allocating a 50 MiB payload in a unit test.
    let (reason, _) = payload_rejection(MAX_INPUT_BYTES + 1).expect("over-cap must reject");
    assert_eq!(reason, Reason::PayloadTooLarge);
    assert_eq!(reason.code(), "payload-too-large");
    let (reason, _) = payload_rejection(0).expect("empty must reject");
    assert_eq!(reason, Reason::EmptyPayload);
    assert!(payload_rejection(1).is_none());
    assert!(payload_rejection(MAX_INPUT_BYTES).is_none());
    // End to end, the command surfaces the same code.
    assert_eq!(reason_of(&decode("none", b"")), Some("empty-payload"));
}

#[test]
fn aliases_map_onto_the_same_codec() {
    assert_eq!(Codec::parse("ZLib"), Some(Codec::Zlib));
    assert_eq!(Codec::parse("  deflate-raw "), Some(Codec::Deflate));
    assert_eq!(Codec::parse("b64"), Some(Codec::Base64));
    assert_eq!(Codec::parse("identity"), Some(Codec::None));
    assert_eq!(Codec::parse("mpk"), Some(Codec::Msgpack));
    assert_eq!(Codec::parse("protobuf"), Some(Codec::Protobuf));
    assert_eq!(Codec::parse("bson"), None);
    assert!(Codec::None.is_byte_codec());
    assert!(!Codec::Msgpack.is_byte_codec());
    assert_eq!(Codec::from(compress::Framing::Deflate).name(), "deflate");
}

// ---------------------------------------------------------------------------
// [tester] Stage-C additions: branches the shipped suite left unexercised.
// Assertions stay on stable codes / structural keys, never on `message` prose.
// ---------------------------------------------------------------------------

/// `[tester]` A stored payload that is not base64 fails as `decode-failed`
/// (the codec rejected the bytes) — distinct from `invalid-base64`, which is
/// the *transport* envelope being malformed.
#[test]
fn test_tester_base64_codec_rejects_non_base64_payload() {
    let r = decode("base64", b"definitely not base64 !!!");
    assert_eq!(reason_of(&r), Some("decode-failed"));
    assert_eq!(r["codec"], serde_json::json!("base64"));
    assert_eq!(r["ok"], serde_json::json!(false));
}

/// `[tester]` C-1 promises wrapped/multi-line base64 payloads still decode.
#[test]
fn test_tester_base64_codec_tolerates_wrapped_whitespace() {
    let wrapped = b"QUJD\r\nRA==\n";
    let r = decode("b64", wrapped);
    assert!(decoded(&r), "wrapped base64 text must decode: {r}");
    assert_eq!(r["text"], serde_json::json!("ABCD"));
    assert_eq!(r["inBytes"], serde_json::json!(wrapped.len()));
    assert_eq!(r["bytes"], serde_json::json!(4));
}

/// `[tester]` A missing / non-string `codec` is `unknown-codec`, and the echo
/// carries exactly what the caller sent (empty string when absent).
#[test]
fn test_tester_missing_or_non_string_codec_is_unknown_codec() {
    let absent = decode_input(serde_json::json!({ "data": "QUJD" }));
    assert_eq!(reason_of(&absent), Some("unknown-codec"));
    assert_eq!(absent["codec"], serde_json::json!(""));
    assert_eq!(absent["retryable"], serde_json::json!(false));

    let numeric = decode_input(serde_json::json!({ "codec": 7, "data": "QUJD" }));
    assert_eq!(reason_of(&numeric), Some("unknown-codec"));

    // Whitespace/case are normalised before the lookup, so a padded but valid
    // name is never mistaken for an unknown codec.
    let padded = decode_input(serde_json::json!({ "codec": "  GzIp  ", "data": "QUJD" }));
    assert_eq!(reason_of(&padded), Some("decode-failed"));
    assert_eq!(padded["codec"], serde_json::json!("gzip"));
}

/// `[tester]` `data` must be a string: a structured value is `missing-data`,
/// not a panic and not a silently empty payload.
#[test]
fn test_tester_non_string_data_is_missing_data() {
    for data in [
        serde_json::json!(null),
        serde_json::json!(42),
        serde_json::json!({ "b64": "QUJD" }),
    ] {
        let r = decode_input(serde_json::json!({ "codec": "none", "data": data }));
        assert_eq!(reason_of(&r), Some("missing-data"), "for data={data}");
    }
}

/// `[tester]` The bare-DEFLATE probe must not fire when the user already chose
/// `deflate`: a retry of the same framing is not a suggestion.
#[test]
fn test_tester_failed_deflate_attempt_suggests_nothing() {
    let r = decode("deflate", b"clearly not a deflate stream at all");
    assert_eq!(reason_of(&r), Some("decode-failed"));
    assert_eq!(suggested_of(&r), None);
    assert_eq!(r["retryable"], serde_json::json!(false));
}

/// `[tester]` A byte-codec rejection never points at a structured format: the
/// user must not be bounced from "gzip" to "php" on a coincidence.
#[test]
fn test_tester_byte_codec_failure_never_suggests_structured_codec() {
    // Payload opens with `a:` (a PHP serialize tag) but is not compressed.
    let phpish = b"a:1:{s:1:\"a\";i:1;}";
    for codec in ["gzip", "zlib", "base64", "none"] {
        let r = decode(codec, phpish);
        if decoded(&r) {
            continue; // `none` succeeds by contract; nothing to suggest.
        }
        let suggested = suggested_of(&r).unwrap_or("");
        assert!(
            !matches!(suggested, "php" | "java" | "pickle" | "msgpack"),
            "{codec} must not be bounced to structured codec {suggested}"
        );
    }
}

/// `[tester]` One-byte payloads reach the sniffing helpers without underflow
/// (`sniff_structured` and `sniff_framing` both need >= 2 bytes) and every
/// codec still answers a well-formed envelope. Gzip is the one framing that
/// stays strict here, because its CRC32 + ISIZE trailer cannot be satisfied.
#[test]
fn test_tester_single_byte_payloads_answer_a_well_formed_envelope() {
    for codec in [
        "gzip", "zlib", "deflate", "msgpack", "java", "pickle", "php", "base64", "none",
    ] {
        let r = decode(codec, &[0x80]);
        assert!(
            r.get("ok").is_some() && r.get("codec").is_some(),
            "{codec} answered without ok/codec: {r}"
        );
    }
    let g = decode("gzip", &[0x80]);
    assert!(!decoded(&g), "gzip must reject a one-byte stream, got: {g}");
    assert_eq!(reason_of(&g), Some("decode-failed"));
}

/// `[tester]` **BUG-002 reproduction (run with `-- --ignored`).**
///
/// Contract C-1 promises a zlib selection is validated against RFC 1950
/// (`CM == 8` plus the `% 31` header check) and that a wrong framing "must
/// fail, not helpfully succeed". `flate2::read::{ZlibDecoder,DeflateDecoder}`
/// instead read a truncated stream as end-of-input, so one- and two-byte
/// payloads — including a bare `78 9c` zlib header — come back as
/// `ok: true, bytes: 0`: an empty success the viewer cannot distinguish from a
/// genuinely empty value. The gzip leg of this loop already passes (see
/// [`Framing::Gzip`]'s trailer), which is why the shipped
/// `truncated_container_is_an_error_not_a_short_success` case only covers gzip.
#[test]
#[ignore = "redis-codec-write-BUG-002: zlib/deflate answer truncated streams as empty success"]
fn test_tester_truncated_deflate_streams_must_not_succeed() {
    for (codec, stored) in [
        ("zlib", vec![0x78u8, 0x9c]),
        ("deflate", vec![0x78u8, 0x9c]),
        ("zlib", vec![0x80u8]),
        ("deflate", vec![b'A']),
    ] {
        let r = decode(codec, &stored);
        assert!(
            !decoded(&r),
            "{codec} reported success for a {}-byte truncated stream: {r}",
            stored.len()
        );
        assert_eq!(reason_of(&r), Some("decode-failed"));
    }
}

/// `[tester]` **BUG-003 reproduction (run with `-- --ignored`).**
///
/// `flate2::read::GzDecoder` defaults to single-member mode, so a concatenated
/// gzip stream — what `cat a.gz b.gz` and most log shippers produce, and what
/// `gunzip` decodes in full — silently decodes to the first member only. The
/// answer is `ok: true` with a `bytes` count that is short by the remaining
/// members, and no residual-byte signal for the UI to warn with.
#[test]
#[ignore = "redis-codec-write-BUG-003: multi-member gzip decodes only the first member as a success"]
fn test_tester_multi_member_gzip_is_not_silently_truncated() {
    let first = b"FIRST-MEMBER".to_vec();
    let second = b"+SECOND".to_vec();
    let mut stream = compress::test_support::gzip(&first);
    stream.extend_from_slice(&compress::test_support::gzip(&second));

    let r = decode("gzip", &stream);
    assert!(decoded(&r), "a valid gzip stream must decode: {r}");
    assert_eq!(
        r["bytes"].as_u64(),
        Some((first.len() + second.len()) as u64),
        "gunzip semantics: every member has to reach the viewer"
    );
}

/// `[tester]` C-2: the JSON kind carries the shared key set with explicit
/// nulls, so the viewer can read every field unconditionally.
#[test]
fn test_tester_json_kind_envelope_has_the_full_key_set() {
    let r = decode("msgpack", &[0x81, 0xa1, b'a', 0x01]);
    for key in [
        "ok", "codec", "kind", "inBytes", "bytes", "data", "text", "json",
    ] {
        assert!(r.get(key).is_some(), "json envelope missing key {key}: {r}");
    }
    assert_eq!(r["data"], serde_json::Value::Null);
    assert_eq!(r["text"], serde_json::Value::Null);
    assert_eq!(r["bytes"], r["inBytes"]);

    let b = decode("none", b"hi");
    for key in [
        "ok", "codec", "kind", "inBytes", "bytes", "data", "text", "json",
    ] {
        assert!(
            b.get(key).is_some(),
            "bytes envelope missing key {key}: {b}"
        );
    }
    assert_eq!(b["kind"], serde_json::json!("bytes"));
}
