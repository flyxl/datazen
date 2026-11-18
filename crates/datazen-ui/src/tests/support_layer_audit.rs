//! Independent audit of the `.ts` support layer: JS-visible string/shape
//! parity and the coverage holes the inline tests do not exercise.

use serde_json::{json, Value};

use crate::support::cell_editor_kind::{
    resolve_cell_editor_kind, CellEditorKind, CellEditorKindInput,
};
use crate::support::cell_types::{
    is_integer_cell_type, is_zoned_cell_type, is_zoned_cell_type_with_hints, resolve_cell_type,
    CellType, CellTypeHints,
};
use crate::support::data_type_colors::{data_type_color, family_color, DataTypeFamily};
use crate::support::formatters::{
    format_cell, format_last_connected, format_last_connected_with_offset, format_timestamp,
};
use crate::support::to_edit_string::{count_code_points, to_edit_string, value_byte_size};

#[test]
fn rules_keep_the_load_bearing_react_order() {
    assert_eq!(resolve_cell_type(Some("binary json"), None), CellType::Json);
    assert_eq!(resolve_cell_type(Some("point"), None), CellType::Text);
    assert_eq!(resolve_cell_type(Some("uint64"), None), CellType::Number);
    assert_eq!(resolve_cell_type(Some("text[]"), None), CellType::Json);
    assert_eq!(resolve_cell_type(Some("timestamptz"), None), CellType::DateTime);
    assert_eq!(resolve_cell_type(Some("USER-DEFINED"), None), CellType::Unknown);
    assert!(is_zoned_cell_type("datetimeoffset"));
    assert!(!is_zoned_cell_type("timestamp"));
    assert!(is_integer_cell_type(Some("serial8"), None));
    assert!(!is_integer_cell_type(Some("decimal(10,2)"), None));
}

#[test]
fn every_editor_kind_branch_matches_the_ts_switch() {
    let input = |cell_type, zoned, long| CellEditorKindInput {
        cell_type,
        zoned,
        has_candidates: false,
        value_code_points: if long { 201 } else { 0 },
        has_line_break: false,
    };
    assert_eq!(resolve_cell_editor_kind(input(CellType::Bool, false, false)), CellEditorKind::Boolean);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Enum, false, false)), CellEditorKind::Enum);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Date, false, false)), CellEditorKind::Date);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Time, true, false)), CellEditorKind::Text);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Time, false, false)), CellEditorKind::Date);
    assert_eq!(resolve_cell_editor_kind(input(CellType::DateTime, true, false)), CellEditorKind::Text);
    assert_eq!(resolve_cell_editor_kind(input(CellType::DateTime, false, false)), CellEditorKind::Date);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Number, false, false)), CellEditorKind::Number);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Json, false, false)), CellEditorKind::Json);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Bytes, false, false)), CellEditorKind::Binary);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Text, false, true)), CellEditorKind::LongText);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Unknown, false, true)), CellEditorKind::LongText);
    assert_eq!(resolve_cell_editor_kind(input(CellType::Uuid, false, true)), CellEditorKind::Text);
}

#[test]
fn colour_families_are_distinct_in_both_themes() {
    let families = [
        DataTypeFamily::Null,
        DataTypeFamily::Bool,
        DataTypeFamily::Number,
        DataTypeFamily::Datetime,
        DataTypeFamily::Json,
        DataTypeFamily::Binary,
        DataTypeFamily::Text,
    ];
    for dark in [true, false] {
        let mut seen = Vec::new();
        for family in families {
            let color = family_color(family, dark);
            assert!(!seen.contains(&color), "{family:?} reuses a token in dark={dark}");
            seen.push(color);
        }
    }
    assert_eq!(data_type_color(Some("int4"), None, true), family_color(DataTypeFamily::Number, true));
    assert_eq!(data_type_color(Some("bytea"), None, true), family_color(DataTypeFamily::Binary, true));
}

#[test]
fn hints_stay_scoped_to_the_driver_that_supplied_them() {
    let hints = CellTypeHints {
        zoned_types: vec!["my-zoned".into()],
        read_only_types: vec!["rowversion".into()],
        ..Default::default()
    };
    assert!(is_zoned_cell_type_with_hints("my-zoned", Some(&hints)));
    assert_eq!(resolve_cell_type(Some("rowversion"), Some(&hints)), CellType::Unknown);
    assert_eq!(resolve_cell_type(Some("timestamp"), Some(&hints)), CellType::DateTime);
}

#[test]
fn text_measurements_use_code_points_and_utf8_bytes() {
    assert_eq!(count_code_points("👨‍👩‍👧‍👦"), 7);
    assert_eq!(count_code_points("数据库"), 3);
    assert_eq!(value_byte_size(&json!("数据库")), 9);
    assert_eq!(value_byte_size(&json!([1, 2, 3])), 3);
    assert_eq!(value_byte_size(&Value::Null), 0);
}

#[test]
fn timestamp_zone_less_text_is_never_reinterpreted() {
    assert_eq!(format_timestamp(&json!("2026-03-01 00:15:30")), "2026-03-01 00:15:30");
    assert_eq!(format_timestamp(&json!("2024-01-15T10:00:00+08:00")), "2024-01-15T02:00:00.000Z");
    assert_eq!(format_timestamp(&json!(1_700_000_000_000i64)), "2023-11-14T22:13:20.000Z");
}

#[test]
fn js_number_text_has_no_trailing_dot_zero() {
    assert_eq!(format_cell(&json!(3.0)), "3");
    assert_eq!(to_edit_string(&json!(3.0)), "3");
    assert_eq!(format_cell(&json!(3.5)), "3.5");
    assert_eq!(format_cell(&json!(-0.0)), "0");
    assert_eq!(format_cell(&json!(1e21)), "1e+21");
    assert_eq!(value_byte_size(&json!(3.0)), 1);
}

#[test]
fn json_object_keys_are_dictionary_ordered_until_preserve_order_is_decided() {
    // JS JSON.stringify preserves insertion order; serde_json's default map
    // sorts. The text is semantically equivalent, and enabling preserve_order
    // would change every workspace consumer, so the order is pinned here.
    assert_eq!(format_cell(&json!({"b": 1, "a": 2})), r#"{"a":2,"b":1}"#);
    assert_eq!(to_edit_string(&json!({"z": 1, "a": 2})), r#"{"a":2,"z":1}"#);
}

#[test]
fn non_scalar_timestamp_falls_back_to_js_string() {
    assert_eq!(format_timestamp(&json!({"a": 1})), "[object Object]");
    assert_eq!(format_timestamp(&json!([1, 2])), "1,2");
    assert_eq!(format_timestamp(&json!(["a", true, null])), "a,true,null");
}

#[test]
fn blank_column_type_uses_the_text_family_not_the_null_family() {
    let text = family_color(DataTypeFamily::Text, true);
    assert_eq!(data_type_color(None, None, true), text);
    assert_eq!(data_type_color(Some("   "), None, true), text);
}

#[test]
fn last_connected_renders_with_the_supplied_timezone() {
    // 12:00Z is 20:00 in Asia/Shanghai (UTC+8) and 07:00 in UTC-5. The helper
    // takes the offset explicitly, so this assertion is deterministic even on a
    // UTC CI host; the production wrapper supplies chrono::Local's offset.
    assert_eq!(
        format_last_connected_with_offset(Some("2024-06-01T12:00:00.000Z"), 8 * 3_600),
        "2024-06-01 20:00:00"
    );
    assert_eq!(
        format_last_connected_with_offset(Some("2024-06-01T12:00:00.000Z"), -5 * 3_600),
        "2024-06-01 07:00:00"
    );
    let local = chrono::Local::now().offset().local_minus_utc();
    assert_eq!(
        format_last_connected(Some("2024-06-01T12:00:00.000Z")),
        format_last_connected_with_offset(Some("2024-06-01T12:00:00.000Z"), local),
        "production must pass the local UTC offset"
    );
}
