// Pure batch shapes: what the meta / value builders put on the wire, with no
// reply of either page involved — plus the literal-pinned constants (BUG-008
// style) that the PRD ladder and the loop guards are frozen at.

use super::*;

// ---------------------------------------------------------------------------
// Pure batch shapes
// ---------------------------------------------------------------------------

#[test]
fn meta_pipeline_is_type_ttl_per_key_with_optional_memory() {
    let keys = vec!["app:a".to_string(), "app:b".to_string()];
    let (pipe, sizes) = build_meta_pipeline(&keys, false);
    assert_eq!(sizes, vec![2, 2]);
    let flat: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
    assert_eq!(
        flat,
        [
            vec!["TYPE".to_string(), "app:a".to_string()],
            vec!["TTL".to_string(), "app:a".to_string()],
            vec!["TYPE".to_string(), "app:b".to_string()],
            vec!["TTL".to_string(), "app:b".to_string()],
        ],
        "TYPE + TTL per key, in key order"
    );
    assert_eq!(meta_fields_per_key(false), 2);

    let (pipe, sizes) = build_meta_pipeline(&keys, true);
    assert_eq!(sizes, vec![3, 3]);
    let flat: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
    assert_eq!(
        flat,
        [
            vec!["TYPE".to_string(), "app:a".to_string()],
            vec!["TTL".to_string(), "app:a".to_string()],
            vec![
                "MEMORY".to_string(),
                "USAGE".to_string(),
                "app:a".to_string()
            ],
            vec!["TYPE".to_string(), "app:b".to_string()],
            vec!["TTL".to_string(), "app:b".to_string()],
            vec![
                "MEMORY".to_string(),
                "USAGE".to_string(),
                "app:b".to_string()
            ],
        ],
        "MEMORY USAGE joins each key's group only when asked for"
    );
    assert_eq!(meta_fields_per_key(true), 3);
}

#[test]
fn value_pipeline_shapes_follow_the_type_and_never_read_streams() {
    let items = vec![
        PageKey {
            key: "a".to_string(),
            key_type: "string".to_string(),
        },
        PageKey {
            key: "h".to_string(),
            key_type: "hash".to_string(),
        },
        PageKey {
            key: "s".to_string(),
            key_type: "stream".to_string(),
        },
        PageKey {
            key: "m".to_string(),
            key_type: "ReJSON-RL".to_string(),
        },
    ];
    let (pipe, sizes) = build_value_pipeline(&items);
    // A stream answers only its cardinality (XLEN) — its preview is a fixed
    // label, so no payload read; a module type (ReJSON-RL) answers nothing at
    // all rather than being read blind.
    assert_eq!(sizes, vec![2, 2, 1, 0]);
    let flat: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
    assert_eq!(
        flat,
        [
            vec!["STRLEN".to_string(), "a".to_string()],
            vec!["GET".to_string(), "a".to_string()],
            vec!["HLEN".to_string(), "h".to_string()],
            vec![
                "HSCAN".to_string(),
                "h".to_string(),
                "0".to_string(),
                "COUNT".to_string(),
                "3".to_string(),
            ],
            vec!["XLEN".to_string(), "s".to_string()],
        ]
    );
}

#[test]
fn short_reply_vector_is_nil_filled_not_the_next_keys_answers() {
    let groups = scatter(vec![RValue::Int(7)], &[2, 2]);
    assert_eq!(
        groups,
        vec![
            vec![RValue::Int(7), RValue::Nil],
            vec![RValue::Nil, RValue::Nil],
        ],
        "a short batch must degrade, never borrow a neighbour's replies"
    );
}

#[test]
fn parse_meta_group_tells_absent_apart_from_unreadable() {
    let absent = parse_meta_group(&[bulk("none"), RValue::Int(-2)], false);
    assert!(absent.absent, "TYPE 'none' is positive absence");
    assert_eq!(absent.key_type, "none");

    let degraded = parse_meta_group(&[RValue::Nil, RValue::Nil, RValue::Nil], true);
    assert!(
        !degraded.absent,
        "an unreadable TYPE must not claim the key is gone"
    );
    assert_eq!(degraded.key_type, "none");
    assert_eq!(degraded.ttl, crate::ops::workbench::TTL_MISSING);
    assert_eq!(
        crate::ops::workbench::TTL_MISSING,
        -2,
        "Redis's missing-key TTL"
    );
    assert_eq!(degraded.mem_bytes, None);

    let present = parse_meta_group(&[bulk("hash"), RValue::Int(-1), RValue::Int(64)], true);
    assert!(!present.absent);
    assert_eq!(present.ttl, -1, "-1 means no expiry, not missing");
    assert_eq!(present.mem_bytes, Some(64));
}
// ---------------------------------------------------------------------------
// Literal-pinned constants (BUG-008 style)
// ---------------------------------------------------------------------------

#[test]
fn tree_scan_constants_are_pinned_by_literal() {
    // The PRD §3.2 ladder and the loop guards are a contract; re-deriving any
    // of them from value search (or vice versa) must go red here.
    assert_eq!(DEFAULT_TREE_BUDGET, 50_000);
    assert_eq!(HARD_MAX_TREE_BUDGET, 1_000_000);
    assert_eq!(TREE_BUDGET_DBSIZE_FACTOR, 2);
    assert_eq!(TREE_SCAN_MIN_ROUND_COUNT, 1_000);
    assert_eq!(MIN_TREE_SCAN_COUNT, 10);
    assert_eq!(MAX_TREE_SCAN_ROUNDS, 64);
    assert_eq!(MAX_TREE_STALLED_ROUNDS, 16);
    assert_eq!(TREE_KEYS_PER_PIPELINE, 256);
    assert_eq!(meta_fields_per_key(false), 2);
    assert_eq!(meta_fields_per_key(true), 3);
    // The key-tree cap must never be the value-search cap.
    assert_ne!(
        HARD_MAX_TREE_BUDGET,
        crate::ops::value_search::HARD_MAX_KEYS
    );
}
