//! T405: the dirty-cell words — the two values that need a word, the JSON
//! serialisation branch, the expression marker, and the two labels.

use serde_json::json;

use datazen_ui::components::data_table::dirty_cell_text::{
    dirty_cell_aria_label, dirty_cell_tooltip, format_dirty_expression, format_dirty_value,
    CURRENT_TIMESTAMP_EXPRESSION,
};

#[test]
fn null_and_the_empty_string_read_as_words() {
    // SQL NULL reaches the UI as null; the empty string a paste can produce.
    assert_eq!(format_dirty_value(&serde_json::Value::Null), "NULL 值");
    assert_eq!(format_dirty_value(&json!("")), "（空字符串）");
    // A non-empty string is itself.
    assert_eq!(format_dirty_value(&json!("draft")), "draft");
}

#[test]
fn json_columns_serialise_instead_of_stringifying() {
    // `String({})` would be "[object Object]"; the tooltip shows the value.
    assert_eq!(format_dirty_value(&json!({"a": 1})), r#"{"a":1}"#);
    // Arrays are objects to `typeof` too: serialised, not comma-joined.
    assert_eq!(format_dirty_value(&json!([1, 2])), "[1,2]");
    assert_eq!(format_dirty_value(&json!([1, "x", null])), r#"[1,"x",null]"#);
}

#[test]
fn scalars_follow_string_value() {
    assert_eq!(format_dirty_value(&json!(true)), "true");
    assert_eq!(format_dirty_value(&json!(false)), "false");
    assert_eq!(format_dirty_value(&json!(2)), "2");
    assert_eq!(format_dirty_value(&json!(-3)), "-3");
}

#[test]
fn the_expression_reads_as_its_own_word() {
    assert_eq!(format_dirty_expression(), "数据库当前时间（表达式）");
    // And the marker's spelling is pinned, so a caller that passes anything
    // else falls through to the value branch.
    assert_eq!(CURRENT_TIMESTAMP_EXPRESSION, "currentTimestamp");
}

#[test]
fn the_tooltip_joins_original_and_next_with_an_arrow() {
    assert_eq!(dirty_cell_tooltip(&json!("a"), &json!("b"), None), "a → b");
    // The null word participates like any value.
    assert_eq!(
        dirty_cell_tooltip(&serde_json::Value::Null, &json!(""), None),
        "NULL 值 → （空字符串）"
    );
}

#[test]
fn the_expression_marker_rewrites_the_to_side_only() {
    assert_eq!(
        dirty_cell_tooltip(&json!("a"), &json!("ignored"), Some("currentTimestamp")),
        "a → 数据库当前时间（表达式）"
    );
    // Any other expression string is not the marker: the value reads.
    assert_eq!(
        dirty_cell_tooltip(&json!("a"), &json!("b"), Some("otherExpression")),
        "a → b"
    );
    // No expression at all: the value reads.
    assert_eq!(dirty_cell_tooltip(&json!("a"), &json!("b"), None), "a → b");
}

#[test]
fn the_aria_label_is_the_tooltip_in_words() {
    assert_eq!(
        dirty_cell_aria_label(&json!("a"), &json!("b"), None),
        "已更改的单元格：a → b"
    );
    // The marker drives the {next} side here too.
    assert_eq!(
        dirty_cell_aria_label(&serde_json::Value::Null, &json!("ignored"), Some("currentTimestamp")),
        "已更改的单元格：NULL 值 → 数据库当前时间（表达式）"
    );
}
