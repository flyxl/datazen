//! Grid text projection, ported from
//! `packages/frontend/src/components/DataTable/editors/toEditString.ts`.
//!
//! The editor needs one canonical string per value, plus a cheap size answer
//! so the grid can refuse a value that would not fit the inline editor.
#![allow(dead_code)] // consumers: T015 GridCell and T009 DetailPanel (inline-edit starting text)

use serde_json::Value;

use crate::support::formatters::number_text;

/// The text an inline editor starts with. `null` and `undefined` become the
/// empty string; structured values become compact JSON.
///
/// Object keys are emitted in dictionary order because serde_json's default
/// map sorts them; JavaScript's `JSON.stringify` keeps insertion order. The
/// text is semantically equivalent, and turning on `preserve_order` would
/// change every crate in the workspace, so the order is pinned by a test and
/// left to a future decision.
pub(crate) fn to_edit_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number_text(number),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// Unicode code points, not UTF-16 code units: an emoji is one, not two.
pub(crate) fn count_code_points(text: &str) -> usize {
    text.chars().count()
}

/// The byte size the editor budget is measured in.
///
/// Strings count their UTF-8 bytes; numbers and booleans count their textual
/// form; an array counts its length; everything else (objects, null) is 0,
/// matching the React helper.
pub(crate) fn value_byte_size(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Number(number) => number_text(number).len(),
        Value::Bool(flag) => flag.to_string().len(),
        Value::Array(items) => items.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nullish_values_start_empty() {
        assert_eq!(to_edit_string(&Value::Null), "");
    }

    #[test]
    fn scalars_become_their_text() {
        assert_eq!(to_edit_string(&json!("hello")), "hello");
        assert_eq!(to_edit_string(&json!(42)), "42");
        assert_eq!(to_edit_string(&json!(3.5)), "3.5");
        assert_eq!(to_edit_string(&json!(true)), "true");
        assert_eq!(to_edit_string(&json!(false)), "false");
    }

    #[test]
    fn integral_floats_lose_the_trailing_dot_zero() {
        assert_eq!(to_edit_string(&json!(3.0)), "3");
        assert_eq!(to_edit_string(&json!(3.5)), "3.5");
        assert_eq!(to_edit_string(&json!(-0.0)), "0");
        assert_eq!(value_byte_size(&json!(3.0)), 1);
    }

    #[test]
    fn object_keys_are_dictionary_ordered_until_preserve_order_is_decided() {
        // serde_json sorts; JavaScript preserves insertion order. Pinned so a
        // future enable of `preserve_order` is a deliberate, reviewed change.
        assert_eq!(to_edit_string(&json!({"b": 1, "a": 2})), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn structured_values_become_compact_json() {
        assert_eq!(to_edit_string(&json!([1, 2, 3])), "[1,2,3]");
        assert_eq!(
            to_edit_string(&json!({"key": "value"})),
            r#"{"key":"value"}"#
        );
    }

    #[test]
    fn code_point_counting_uses_characters() {
        assert_eq!(count_code_points("abc"), 3);
        assert_eq!(count_code_points("数据库"), 3);
        assert_eq!(count_code_points("👨‍👩‍👧‍👦"), 7);
        assert_eq!(count_code_points(""), 0);
    }

    #[test]
    fn byte_size_measures_the_editor_budget() {
        assert_eq!(value_byte_size(&json!("abc")), 3);
        // UTF-8: each Chinese character is three bytes.
        assert_eq!(value_byte_size(&json!("数据库")), 9);
        assert_eq!(value_byte_size(&json!(12345)), 5);
        assert_eq!(value_byte_size(&json!(true)), 4);
        assert_eq!(value_byte_size(&json!([1, 2, 3])), 3);
        assert_eq!(value_byte_size(&json!({"a": 1})), 0);
        assert_eq!(value_byte_size(&Value::Null), 0);
    }
}
