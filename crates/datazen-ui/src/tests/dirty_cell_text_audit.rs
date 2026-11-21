//! T405 audit (tester): independent checks of the dirty-cell words. Fresh
//! vectors: nested JSON serialisation, the empty-string boundary between
//! the word and the value, the expression marker's exact spelling rule, and
//! aria labels whose interpolated values are themselves JSON. Expectations
//! re-derived from the zh-CN packs by the tester.

use serde_json::json;

use crate::components::data_table::dirty_cell_text::{
    dirty_cell_aria_label, dirty_cell_tooltip, format_dirty_value,
};

#[test]
fn audit_nested_json_serialises_compactly() {
    // JSON.stringify has no padding; nesting keeps the same rule.
    assert_eq!(
        format_dirty_value(&json!({"k": [1, null]})),
        r#"{"k":[1,null]}"#
    );
    // An empty object/array still serialises rather than stringifying.
    assert_eq!(format_dirty_value(&json!({})), "{}");
    assert_eq!(format_dirty_value(&json!([])), "[]");
    // String escaping inside a JSON column value follows JSON rules.
    assert_eq!(format_dirty_value(&json!("a\"b")), "a\"b");
}

#[test]
fn audit_the_word_boundaries() {
    // The word list: null, empty string, expression. Everything else is a
    // value — including the one-space string.
    assert_eq!(format_dirty_value(&json!(" ")), " ");
    assert_eq!(format_dirty_value(&json!("0")), "0");
    assert_eq!(format_dirty_value(&json!(-0)), "0");
}

#[test]
fn audit_the_tooltip_arrow_is_the_unicode_arrow() {
    // \u2192 — a single code point, not "->" and not "→" spelled another way.
    let tooltip = dirty_cell_tooltip(&json!("x"), &json!("y"), None);
    assert_eq!(tooltip, "x → y");
    assert_eq!(tooltip.as_bytes().iter().filter(|b| **b == b' ').count(), 2, "exactly the two template spaces");
}

#[test]
fn audit_an_interpolated_json_value_reads_in_the_aria_label() {
    // The {next} side can itself be a JSON serialisation: the label says
    // what the cell becomes, spelled the same way as the tooltip's to-side.
    assert_eq!(
        dirty_cell_aria_label(&json!(1), &json!({"a": 1}), None),
        r#"已更改的单元格：1 → {"a":1}"#
    );
    // And the {original} side gets the same treatment.
    assert_eq!(
        dirty_cell_aria_label(&json!({"b": 2}), &json!(3), None),
        r#"已更改的单元格：{"b":2} → 3"#
    );
}

#[test]
fn audit_the_marker_must_match_the_whole_string() {
    // A prefix or a suffix is not the marker; the value reads.
    for not_marker in ["currentTimestamps", "theCurrentTimestamp", " currentTimestamp"] {
        assert_eq!(
            dirty_cell_tooltip(&json!("a"), &json!("b"), Some(not_marker)),
            "a → b",
            "{not_marker:?} must not trigger the expression word"
        );
    }
    // The exact marker does.
    assert_eq!(
        dirty_cell_tooltip(&json!("a"), &json!("b"), Some("currentTimestamp")),
        "a → 数据库当前时间（表达式）"
    );
}
