//! Pure helpers and Redis mutate/batch operations for plugin commands.
//!
//! Split by responsibility; the original flat `ops` module's public paths are
//! preserved by the `pub use` re-exports below, so `crate::ops::<name>` keeps
//! resolving for every caller.

pub mod batch;
pub mod cluster;
pub mod exec;
pub mod flush;
pub mod hash;
pub mod io;
pub mod json;
pub mod key_probe;
pub mod keys;
pub mod list;
pub mod monitor;
pub mod observe;
pub mod parse;
pub mod pubsub;
pub mod scan;
pub mod set;
pub mod stream;
pub mod tree;
pub mod ttl;
pub mod types;
pub mod value_search;
pub mod workbench;
pub mod write;
pub mod zset;

pub use batch::batch_delete_pattern;
pub use batch::batch_rename_prefix;
pub use batch::batch_set_ttl;
pub use flush::ensure_flush_allowed;
pub use flush::flush_all;
pub use flush::flush_db;
pub use flush::set_settings_allow_flush;
pub use flush::settings_allow_flush;
pub use hash::hash_del;
pub use hash::hash_scan;
pub use hash::hash_set;
pub use keys::delete_keys;
pub use keys::rename_key;
pub use keys::set_expire_at;
pub use keys::set_ttl;
pub use list::list_index;
pub use list::list_pop;
pub use list::list_push;
pub use list::list_range;
pub use list::list_rem;
pub use list::list_set;
pub(crate) use scan::scan_batch;
pub use scan::scan_keys;
#[allow(unused_imports)]
pub use scan::scan_matching_keys;
pub use set::set_add;
pub use set::set_remove;
pub use set::set_scan;
#[allow(unused_imports)]
pub(crate) use ttl::apply_ttl_command;
pub use types::BatchDeleteResult;
pub use types::BatchRenameResult;
pub use types::BatchSetTtlResult;
pub use types::KeyError;
pub use zset::zset_add;
pub use zset::zset_remove;
pub use zset::zset_scan;
pub use zset::ZsetMember;

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::ops::hash::parse_hash_scan_result;
    #[allow(unused_imports)]
    use crate::ops::parse::{
        parse_cursor_from_value, parse_flat_string_array, parse_flat_string_pairs,
        parse_scan_result_generic, parse_string_array, parse_zscan_result, value_to_string,
    };
    #[allow(unused_imports)]
    use crate::ops::types::{plan_rename_prefix, resolve_expire_at, resolve_ttl, TtlCommand};

    #[test]
    fn plan_rename_prefix_rewrites() {
        let planned = plan_rename_prefix("user:", "u:", &["user:1".into(), "user:2".into()]);
        assert_eq!(
            planned,
            vec![
                ("user:1".into(), "u:1".into()),
                ("user:2".into(), "u:2".into())
            ]
        );
    }

    #[test]
    fn plan_rename_prefix_skips_non_matching() {
        let planned = plan_rename_prefix("user:", "u:", &["other:1".into(), "user:x".into()]);
        assert_eq!(planned, vec![("user:x".into(), "u:x".into())]);
    }

    #[test]
    fn plan_rename_prefix_empty_keys() {
        assert!(plan_rename_prefix("a:", "b:", &[]).is_empty());
    }

    #[test]
    fn plan_rename_prefix_empty_old_prefix_matches_all() {
        let planned = plan_rename_prefix("", "pre:", &["a".into(), "b".into()]);
        assert_eq!(
            planned,
            vec![("a".into(), "pre:a".into()), ("b".into(), "pre:b".into())]
        );
    }

    // ---- PR-1: resolve_ttl / resolve_expire_at ----

    #[test]
    fn ttl_sentinel_persist() {
        assert_eq!(resolve_ttl(-1).unwrap(), TtlCommand::Persist);
    }

    #[test]
    fn ttl_sentinel_expire() {
        assert_eq!(resolve_ttl(3600).unwrap(), TtlCommand::Expire(3600));
    }

    #[test]
    fn ttl_zero_is_expire_zero() {
        assert_eq!(resolve_ttl(0).unwrap(), TtlCommand::Expire(0));
    }

    #[test]
    fn ttl_large_value() {
        assert_eq!(
            resolve_ttl(i64::from(u32::MAX)).unwrap(),
            TtlCommand::Expire(u32::MAX as u64)
        );
    }

    #[test]
    fn ttl_sentinel_rejects_invalid() {
        assert!(resolve_ttl(-2).is_err());
        assert!(resolve_ttl(-100).is_err());
        let err = resolve_ttl(-2).unwrap_err();
        assert!(err.contains("invalid ttl_seconds"), "err={err}");
    }

    #[test]
    fn expire_at_resolves() {
        assert_eq!(
            resolve_expire_at(1_700_000_000).unwrap(),
            TtlCommand::ExpireAt(1_700_000_000)
        );
        assert!(resolve_expire_at(0).is_err());
        assert!(resolve_expire_at(-1).is_err());
    }

    #[test]
    fn expire_at_rejects_zero_and_negative_with_message() {
        let err0 = resolve_expire_at(0).unwrap_err();
        assert!(err0.contains("expire_at"), "err0={err0}");
        assert!(err0.contains("0"), "err0={err0}");
        let err_neg = resolve_expire_at(-5).unwrap_err();
        assert!(err_neg.contains("invalid expire_at"), "err_neg={err_neg}");
    }

    #[test]
    fn expire_at_accepts_far_future() {
        let ts = 4_102_444_800_i64;
        assert_eq!(resolve_expire_at(ts).unwrap(), TtlCommand::ExpireAt(ts));
    }

    #[test]
    fn expire_at_accepts_one() {
        assert_eq!(resolve_expire_at(1).unwrap(), TtlCommand::ExpireAt(1));
    }

    #[test]
    fn ttl_command_variants_are_distinct() {
        assert_ne!(TtlCommand::Persist, TtlCommand::Expire(0));
        assert_ne!(TtlCommand::Expire(1), TtlCommand::ExpireAt(1));
        assert_eq!(TtlCommand::ExpireAt(42), TtlCommand::ExpireAt(42));
    }

    #[test]
    fn flush_gate_requires_settings_and_client_flag() {
        set_settings_allow_flush(false);
        assert!(ensure_flush_allowed(true).is_err());
        assert!(ensure_flush_allowed(false).is_err());

        set_settings_allow_flush(true);
        assert!(ensure_flush_allowed(false).is_err());
        assert!(ensure_flush_allowed(true).is_ok());

        set_settings_allow_flush(false);
    }

    // ---- PR-3: Collection scan / range parse helpers (tester) ----

    // -- value_to_string --

    #[test]
    fn test_tester_value_to_string_nil() {
        assert_eq!(value_to_string(&redis::Value::Nil), "");
    }

    #[test]
    fn test_tester_value_to_string_int() {
        assert_eq!(value_to_string(&redis::Value::Int(42)), "42");
    }

    #[test]
    fn test_tester_value_to_string_bulk_string() {
        let v = redis::Value::BulkString(b"hello".to_vec());
        assert_eq!(value_to_string(&v), "hello");
    }

    #[test]
    fn test_tester_value_to_string_simple_string() {
        assert_eq!(
            value_to_string(&redis::Value::SimpleString("ok".into())),
            "ok"
        );
    }

    #[test]
    fn test_tester_value_to_string_okay() {
        assert_eq!(value_to_string(&redis::Value::Okay), "OK");
    }

    #[test]
    fn test_tester_value_to_string_double_format() {
        // `value_to_string` has no `Double` arm, so a Double lands in the
        // `other => format!("{other:?}")` catch-all and the caller receives the
        // Debug rendering of the whole `redis::Value`, payload included. That is
        // why the payload is load-bearing here: the assertion checks that the
        // number survives the conversion, so the expectation is derived from the
        // same value that was constructed instead of being re-typed, which makes
        // the two unable to drift apart. `10.5` is a plain float counter of the
        // kind INCRBYFLOAT produces, deliberately not an approximation of any
        // named mathematical constant.
        let payload = 10.5;
        let v = redis::Value::Double(payload);
        let s = value_to_string(&v);
        assert!(s.contains(&format!("{payload:?}")), "got: {s}");
    }

    // -- parse_cursor_from_value --

    #[test]
    fn test_tester_cursor_from_int() {
        assert_eq!(parse_cursor_from_value(&redis::Value::Int(0)).unwrap(), 0);
        assert_eq!(
            parse_cursor_from_value(&redis::Value::Int(12345)).unwrap(),
            12345
        );
    }

    #[test]
    fn test_tester_cursor_from_bulk_string() {
        let v = redis::Value::BulkString(b"42".to_vec());
        assert_eq!(parse_cursor_from_value(&v).unwrap(), 42);
    }

    #[test]
    fn test_tester_cursor_from_bulk_string_with_whitespace() {
        let v = redis::Value::BulkString(b"  100  ".to_vec());
        assert_eq!(parse_cursor_from_value(&v).unwrap(), 100);
    }

    #[test]
    fn test_tester_cursor_from_simple_string() {
        let v = redis::Value::SimpleString("7".into());
        assert_eq!(parse_cursor_from_value(&v).unwrap(), 7);
    }

    #[test]
    fn test_tester_cursor_from_invalid_bulk_string() {
        let v = redis::Value::BulkString(b"not_a_number".to_vec());
        assert!(parse_cursor_from_value(&v).is_err());
    }

    #[test]
    fn test_tester_cursor_from_unexpected_type() {
        let v = redis::Value::Array(vec![]);
        assert!(parse_cursor_from_value(&v).is_err());
    }

    // -- parse_flat_string_pairs --

    #[test]
    fn test_tester_flat_pairs_normal() {
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"f1".to_vec()),
            redis::Value::BulkString(b"v1".to_vec()),
            redis::Value::BulkString(b"f2".to_vec()),
            redis::Value::BulkString(b"v2".to_vec()),
        ]);
        let pairs = parse_flat_string_pairs(&v).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("f1".to_string(), "v1".to_string()));
        assert_eq!(pairs[1], ("f2".to_string(), "v2".to_string()));
    }

    #[test]
    fn test_tester_flat_pairs_odd_length() {
        // Odd-length array: last element is skipped (chunk len != 2)
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"f1".to_vec()),
            redis::Value::BulkString(b"v1".to_vec()),
            redis::Value::BulkString(b"orphan".to_vec()),
        ]);
        let pairs = parse_flat_string_pairs(&v).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("f1".to_string(), "v1".to_string()));
    }

    #[test]
    fn test_tester_flat_pairs_empty_array() {
        let v = redis::Value::Array(vec![]);
        let pairs = parse_flat_string_pairs(&v).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_tester_flat_pairs_non_array() {
        let v = redis::Value::SimpleString("err".into());
        assert!(parse_flat_string_pairs(&v).is_err());
    }

    // -- parse_flat_string_array --

    #[test]
    fn test_tester_flat_array_normal() {
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"a".to_vec()),
            redis::Value::Int(1),
            redis::Value::Nil,
        ]);
        let arr = parse_flat_string_array(&v).unwrap();
        assert_eq!(arr, vec!["a".to_string(), "1".to_string(), "".to_string()]);
    }

    #[test]
    fn test_tester_flat_array_non_array() {
        let v = redis::Value::Okay;
        assert!(parse_flat_string_array(&v).is_err());
    }

    // -- parse_hash_scan_result --

    #[test]
    fn test_tester_hash_scan_result_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"field1".to_vec()),
                redis::Value::BulkString(b"val1".to_vec()),
            ]),
        ]);
        let (cursor, entries) = parse_hash_scan_result(&v).unwrap();
        assert_eq!(cursor, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], ("field1".to_string(), "val1".to_string()));
    }

    #[test]
    fn test_tester_hash_scan_result_wrong_length() {
        let v = redis::Value::Array(vec![redis::Value::Int(0)]);
        assert!(parse_hash_scan_result(&v).is_err());
    }

    #[test]
    fn test_tester_hash_scan_result_non_array() {
        let v = redis::Value::BulkString(b"err".to_vec());
        assert!(parse_hash_scan_result(&v).is_err());
    }

    // -- parse_scan_result_generic --

    #[test]
    fn test_tester_scan_generic_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(10),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"m1".to_vec()),
                redis::Value::BulkString(b"m2".to_vec()),
            ]),
        ]);
        let (cursor, members) = parse_scan_result_generic(&v).unwrap();
        assert_eq!(cursor, 10);
        assert_eq!(members, vec!["m1".to_string(), "m2".to_string()]);
    }

    #[test]
    fn test_tester_scan_generic_cursor_zero() {
        let v = redis::Value::Array(vec![redis::Value::Int(0), redis::Value::Array(vec![])]);
        let (cursor, members) = parse_scan_result_generic(&v).unwrap();
        assert_eq!(cursor, 0);
        assert!(members.is_empty());
    }

    #[test]
    fn test_tester_scan_generic_non_array() {
        let v = redis::Value::Int(5);
        assert!(parse_scan_result_generic(&v).is_err());
    }

    // -- parse_zscan_result --

    #[test]
    fn test_tester_zscan_result_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"member1".to_vec()),
                redis::Value::BulkString(b"1.5".to_vec()),
                redis::Value::BulkString(b"member2".to_vec()),
                redis::Value::BulkString(b"2.0".to_vec()),
            ]),
        ]);
        let (cursor, members) = parse_zscan_result(&v).unwrap();
        assert_eq!(cursor, 0);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].0, "member1");
        assert!((members[0].1 - 1.5).abs() < f64::EPSILON);
        assert_eq!(members[1].0, "member2");
        assert!((members[1].1 - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tester_zscan_result_non_numeric_score() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"m".to_vec()),
                redis::Value::BulkString(b"not_a_number".to_vec()),
            ]),
        ]);
        let (_, members) = parse_zscan_result(&v).unwrap();
        assert_eq!(members.len(), 1);
        // unwrap_or(0.0) fallback
        assert!((members[0].1 - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tester_zscan_result_odd_length_data() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"m".to_vec()),
                redis::Value::BulkString(b"1.0".to_vec()),
                redis::Value::BulkString(b"orphan".to_vec()),
            ]),
        ]);
        let (_, members) = parse_zscan_result(&v).unwrap();
        assert_eq!(members.len(), 1); // orphan skipped
    }

    #[test]
    fn test_tester_zscan_result_wrong_length() {
        let v = redis::Value::Array(vec![redis::Value::Int(0)]);
        assert!(parse_zscan_result(&v).is_err());
    }

    #[test]
    fn test_tester_zscan_result_non_array() {
        let v = redis::Value::BulkString(b"err".to_vec());
        assert!(parse_zscan_result(&v).is_err());
    }

    // -- parse_string_array --

    #[test]
    fn test_tester_string_array_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"item1".to_vec()),
            redis::Value::Int(42),
            redis::Value::Nil,
        ]);
        let arr = parse_string_array(&v).unwrap();
        assert_eq!(
            arr,
            vec!["item1".to_string(), "42".to_string(), "".to_string()]
        );
    }

    #[test]
    fn test_tester_string_array_empty() {
        let v = redis::Value::Array(vec![]);
        let arr = parse_string_array(&v).unwrap();
        assert!(arr.is_empty());
    }

    #[test]
    fn test_tester_string_array_non_array() {
        let v = redis::Value::Int(0);
        assert!(parse_string_array(&v).is_err());
    }

    // -- value_to_string edge cases --

    #[test]
    fn test_tester_value_to_string_bulk_string_utf8_lossy() {
        // Invalid UTF-8 bytes => lossy conversion
        let v = redis::Value::BulkString(vec![0xFF, 0xFE]);
        let s = value_to_string(&v);
        assert!(!s.is_empty());
    }
}
