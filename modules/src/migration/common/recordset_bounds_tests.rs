use super::*;
use std::cmp::Ordering;

fn bound(raw: serde_json::Value, data_type: &str) -> (Value, BoundKey) {
    canonical_bound_value(&raw, data_type, "start").unwrap()
}

#[test]
fn unsigned_bigint_preserves_values_above_i64_without_float_rounding() {
    let (value, maximum) = bound(serde_json::json!("18446744073709551615"), "bigint unsigned");
    let (_, previous) = bound(serde_json::json!("18446744073709551614"), "bigint unsigned");
    assert!(matches!(value, Value::String(text) if text == "18446744073709551615"));
    assert_eq!(
        compare_bound_keys(&previous, &maximum).unwrap(),
        Ordering::Less
    );
    assert!(canonical_bound_value(
        &serde_json::json!("18446744073709551616"),
        "bigint unsigned",
        "end"
    )
    .is_err());
}

#[test]
fn decimal_order_is_exact_and_normalizes_equivalent_spellings() {
    let (_, a) = bound(serde_json::json!("9007199254740993.01"), "numeric(30,2)");
    let (_, b) = bound(serde_json::json!("9007199254740993.02"), "numeric(30,2)");
    assert_eq!(compare_bound_keys(&a, &b).unwrap(), Ordering::Less);
    for (left, right) in [("1.20", "12e-1"), ("-0.000", "0"), ("-2.5", "-25e-1")] {
        let (_, left) = bound(serde_json::json!(left), "decimal");
        let (_, right) = bound(serde_json::json!(right), "decimal");
        assert_eq!(compare_bound_keys(&left, &right).unwrap(), Ordering::Equal);
    }
    let (_, negative) = bound(serde_json::json!("-1.3"), "decimal");
    let (_, larger) = bound(serde_json::json!("-1.2"), "decimal");
    assert_eq!(
        compare_bound_keys(&negative, &larger).unwrap(),
        Ordering::Less
    );
}

#[test]
fn text_bounds_keep_whitespace_and_reject_null_or_numeric_coercion() {
    let (value, _) = bound(serde_json::json!(" 001 "), "varchar(20)");
    assert!(matches!(value, Value::String(text) if text == " 001 "));
    for raw in [serde_json::Value::Null, serde_json::json!(1)] {
        assert!(canonical_bound_value(&raw, "text", "start").is_err());
    }
}

#[test]
fn unknown_types_nonfinite_values_and_mixed_key_types_fail_closed() {
    assert!(ensure_supported_bound_type("custom_type", "start").is_err());
    for raw in ["NaN", "inf", "-inf"] {
        assert!(canonical_bound_value(&serde_json::json!(raw), "double", "start").is_err());
    }
    let (_, number) = bound(serde_json::json!(1), "integer");
    let (_, text) = bound(serde_json::json!("1"), "text");
    assert!(compare_bound_keys(&number, &text).is_err());
    assert!(canonical_bound_value(&serde_json::json!(128), "tinyint", "start").is_err());
}
