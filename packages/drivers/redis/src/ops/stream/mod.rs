//! Redis Stream helpers: XRANGE/XADD, consumer groups, pending, and overview sampling.

#[allow(unused_imports)]
use std::collections::HashMap;

#[allow(unused_imports)]
use redis::AsyncCommands;
#[allow(unused_imports)]
use serde::Serialize;

#[allow(unused_imports)]
use crate::driver::parse_scan_result;

pub mod entries;
pub mod groups;
pub mod overview;
pub mod parse;
pub mod types;

pub use entries::stream_lag;
pub use entries::xadd;
pub use entries::xgroup_create;
pub use entries::xgroup_destroy;
pub use entries::xrange;
pub use groups::xack;
pub use groups::xinfo_consumers;
pub use groups::xinfo_groups;
pub use groups::xpending;
pub use overview::stream_overview;
pub use types::resolve_stream_overview_limit;
pub use types::ConsumerInfo;
#[allow(unused_imports)]
pub use types::StreamEntry;
pub use types::StreamGroupInfo;
pub use types::StreamLagResult;
pub use types::StreamOverviewResult;
#[allow(unused_imports)]
pub use types::StreamOverviewRow;
pub use types::XaddResult;
#[allow(unused_imports)]
pub use types::XpendingEntry;
pub use types::XpendingResult;
pub use types::XrangeResult;
#[allow(unused_imports)]
pub use types::DEFAULT_STREAM_OVERVIEW_LIMIT;

#[allow(unused_imports)]
pub(crate) use parse::value_to_string;
#[allow(unused_imports)]
pub(crate) use parse::value_to_u64;

#[cfg(test)]
mod tests {
    use super::parse::{
        parse_stream_entry, parse_stream_id, parse_xinfo_consumers, parse_xinfo_groups,
        parse_xpending_entries,
    };
    use super::types::validate_xgroup_name;
    use super::*;

    #[test]
    fn validate_xgroup_name_rejects_empty() {
        assert!(validate_xgroup_name("").is_err());
        assert!(validate_xgroup_name("   ").is_err());
    }

    #[test]
    fn validate_xgroup_name_trims_and_accepts() {
        assert_eq!(validate_xgroup_name("  workers  ").unwrap(), "workers");
    }

    #[test]
    fn resolve_stream_overview_limit_defaults() {
        assert_eq!(resolve_stream_overview_limit(None), 100);
        assert_eq!(resolve_stream_overview_limit(Some(0)), 100);
        assert_eq!(resolve_stream_overview_limit(Some(25)), 25);
    }

    #[test]
    fn parse_stream_entry_basic() {
        let raw = redis::Value::Array(vec![
            redis::Value::BulkString(b"1000-0".to_vec()),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"field".to_vec()),
                redis::Value::BulkString(b"value".to_vec()),
            ]),
        ]);
        let entry = parse_stream_entry(&raw).unwrap();
        assert_eq!(entry.id, "1000-0");
        assert_eq!(entry.fields.get("field"), Some(&"value".to_string()));
    }

    #[test]
    fn parse_xinfo_groups_from_array_pairs() {
        let raw = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"name".to_vec()),
            redis::Value::BulkString(b"g1".to_vec()),
            redis::Value::BulkString(b"consumers".to_vec()),
            redis::Value::Int(2),
            redis::Value::BulkString(b"pending".to_vec()),
            redis::Value::Int(3),
            redis::Value::BulkString(b"last-delivered-id".to_vec()),
            redis::Value::BulkString(b"1000-0".to_vec()),
        ])]);
        let groups = parse_xinfo_groups(&raw).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "g1");
        assert_eq!(groups[0].consumers, 2);
        assert_eq!(groups[0].pending, 3);
        assert_eq!(groups[0].last_delivered_id, "1000-0");
    }

    #[test]
    fn parse_xpending_entries_from_detail_rows() {
        let raw = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"1000-0".to_vec()),
            redis::Value::BulkString(b"c1".to_vec()),
            redis::Value::Int(42),
            redis::Value::Int(1),
        ])]);
        let pending = parse_xpending_entries(&raw).unwrap();
        assert_eq!(pending.entries.len(), 1);
        assert_eq!(pending.entries[0].id, "1000-0");
        assert_eq!(pending.entries[0].consumer, "c1");
        assert_eq!(pending.entries[0].idle_ms, 42);
        assert_eq!(pending.entries[0].delivery_count, 1);
    }

    #[test]
    fn parse_xinfo_consumers_from_array_pairs() {
        let raw = redis::Value::Array(vec![
            redis::Value::Array(vec![
                redis::Value::BulkString(b"name".to_vec()),
                redis::Value::BulkString(b"worker1".to_vec()),
                redis::Value::BulkString(b"pending".to_vec()),
                redis::Value::Int(5),
                redis::Value::BulkString(b"idle".to_vec()),
                redis::Value::Int(1200),
                redis::Value::BulkString(b"delivery-count".to_vec()),
                redis::Value::Int(3),
            ]),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"name".to_vec()),
                redis::Value::BulkString(b"worker2".to_vec()),
                redis::Value::BulkString(b"pending".to_vec()),
                redis::Value::Int(2),
                redis::Value::BulkString(b"idle".to_vec()),
                redis::Value::Int(500),
                redis::Value::BulkString(b"delivery-count".to_vec()),
                redis::Value::Int(1),
            ]),
        ]);
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert_eq!(consumers.len(), 2);
        // sorted by name
        assert_eq!(consumers[0].name, "worker1");
        assert_eq!(consumers[0].pending, 5);
        assert_eq!(consumers[0].idle_ms, 1200);
        assert_eq!(consumers[0].delivery_count, 3);
        assert_eq!(consumers[1].name, "worker2");
        assert_eq!(consumers[1].pending, 2);
        assert_eq!(consumers[1].idle_ms, 500);
        assert_eq!(consumers[1].delivery_count, 1);
    }

    #[test]
    fn parse_stream_id_basic() {
        assert_eq!(parse_stream_id("1000-0"), Some((1000, 0)));
        assert_eq!(parse_stream_id("1000-5"), Some((1000, 5)));
        assert_eq!(parse_stream_id("9999999999999-1"), Some((9999999999999, 1)));
        assert_eq!(parse_stream_id("invalid"), None);
        assert_eq!(parse_stream_id(""), None);
    }

    #[test]
    fn parse_xinfo_consumers_empty_array() {
        let raw = redis::Value::Array(vec![]);
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert!(consumers.is_empty());
    }

    #[test]
    fn test_tester_parse_xinfo_consumers_from_map_variant() {
        // XINFO CONSUMERS can return Map entries in some Redis versions
        let raw = redis::Value::Array(vec![redis::Value::Map(vec![
            (
                redis::Value::BulkString(b"name".to_vec()),
                redis::Value::BulkString(b"consumer-a".to_vec()),
            ),
            (
                redis::Value::BulkString(b"pending".to_vec()),
                redis::Value::Int(10),
            ),
            (
                redis::Value::BulkString(b"idle".to_vec()),
                redis::Value::Int(2500),
            ),
            (
                redis::Value::BulkString(b"delivery-count".to_vec()),
                redis::Value::Int(7),
            ),
        ])]);
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0].name, "consumer-a");
        assert_eq!(consumers[0].pending, 10);
        assert_eq!(consumers[0].idle_ms, 2500);
        assert_eq!(consumers[0].delivery_count, 7);
    }

    #[test]
    fn test_tester_parse_xinfo_consumers_skips_entry_without_name() {
        let raw = redis::Value::Array(vec![
            // Entry with name - should be kept
            redis::Value::Array(vec![
                redis::Value::BulkString(b"name".to_vec()),
                redis::Value::BulkString(b"good".to_vec()),
                redis::Value::BulkString(b"pending".to_vec()),
                redis::Value::Int(1),
                redis::Value::BulkString(b"idle".to_vec()),
                redis::Value::Int(0),
                redis::Value::BulkString(b"delivery-count".to_vec()),
                redis::Value::Int(0),
            ]),
            // Entry without name - should be skipped
            redis::Value::Array(vec![
                redis::Value::BulkString(b"pending".to_vec()),
                redis::Value::Int(5),
            ]),
        ]);
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0].name, "good");
    }

    #[test]
    fn test_tester_parse_xinfo_consumers_skips_non_array_non_map() {
        let raw = redis::Value::Array(vec![
            redis::Value::BulkString(b"not a structured entry".to_vec()),
            redis::Value::Int(42),
            redis::Value::Nil,
            redis::Value::Array(vec![
                redis::Value::BulkString(b"name".to_vec()),
                redis::Value::BulkString(b"valid".to_vec()),
                redis::Value::BulkString(b"pending".to_vec()),
                redis::Value::Int(0),
                redis::Value::BulkString(b"idle".to_vec()),
                redis::Value::Int(0),
                redis::Value::BulkString(b"delivery-count".to_vec()),
                redis::Value::Int(0),
            ]),
        ]);
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0].name, "valid");
    }

    #[test]
    fn test_tester_parse_xinfo_consumers_non_array_outer_returns_empty() {
        // When Redis returns a non-Array top-level value (e.g., error string)
        let raw = redis::Value::BulkString(b"ERR key does not exist".to_vec());
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert!(consumers.is_empty());
    }

    #[test]
    fn test_tester_parse_xinfo_consumers_odd_field_count() {
        // Odd number of fields in array - last field has no pair
        let raw = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"name".to_vec()),
            redis::Value::BulkString(b"orphan".to_vec()),
            redis::Value::BulkString(b"pending".to_vec()),
            redis::Value::Int(3),
            redis::Value::BulkString(b"idle".to_vec()),
            // missing value for idle - odd chunk len
        ])]);
        let consumers = parse_xinfo_consumers(&raw).unwrap();
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0].name, "orphan");
        assert_eq!(consumers[0].pending, 3);
        // idle was not set because chunk had len 1
        assert_eq!(consumers[0].idle_ms, 0);
    }

    #[test]
    fn test_tester_parse_stream_id_multiple_dashes() {
        // Stream IDs like "1000-0-extra" should return None (invalid)
        assert_eq!(parse_stream_id("1000-0-extra"), None);
        assert_eq!(parse_stream_id("1-2-3"), None);
    }

    #[test]
    fn test_tester_parse_stream_id_zero_zero() {
        assert_eq!(parse_stream_id("0-0"), Some((0, 0)));
    }

    #[test]
    fn test_tester_consumer_info_serde_roundtrip() {
        let info = ConsumerInfo {
            name: "w1".to_string(),
            pending: 42,
            idle_ms: 1500,
            delivery_count: 8,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "w1");
        assert_eq!(json["pending"], 42);
        assert_eq!(json["idleMs"], 1500);
        assert_eq!(json["deliveryCount"], 8);
    }

    #[test]
    fn test_tester_stream_lag_result_serde() {
        let with_lag = StreamLagResult { lag: Some(42) };
        let json = serde_json::to_value(&with_lag).unwrap();
        assert_eq!(json["lag"], 42);

        let no_lag = StreamLagResult { lag: None };
        let json = serde_json::to_value(&no_lag).unwrap();
        assert!(json["lag"].is_null());
    }
}
