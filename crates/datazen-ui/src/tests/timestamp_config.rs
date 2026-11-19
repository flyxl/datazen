//! Independent audit for T230 (the valuePresentation timestampConfig port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/timestampConfig.ts` and its
//! React test `__tests__/timestampConfig.test.ts` — the valid shapes, the
//! pathed root/shape errors, the two-stage IANA name checks, the
//! sourceTimeZone semantics (unix must not carry one) and full problem
//! accumulation (no early stop).
//!
//! Elisions: `Object.freeze` has no Rust counterpart (immutability is
//! ownership); its test becomes an exact-JSON-shape pin. React's
//! `sourceTimeZone: undefined` is JSON's absent key; the present-`null`
//! case still fails the shape check.

use serde_json::json;

use crate::support::value_presentation::timestamp_config::{
    FormatSerde, TimestampInterpretation, TimestampOptions, TimestampOptionsProblem,
    TimestampOptionsValidation, UnixUnitSerde, validate_timestamp_options,
};
use crate::support::value_presentation::timestamp_format::TimeZoneSpec;

/// Assert the validation failed and the problems contain the given
/// (path, message) pair.
fn assert_problem(
    result: &TimestampOptionsValidation,
    path: &str,
    message: &str,
) -> Vec<TimestampOptionsProblem> {
    match result {
        TimestampOptionsValidation::Err(problems) => {
            assert!(
                problems
                    .iter()
                    .any(|problem| problem.path == path && problem.message == message),
                "expected problem {path:?} / {message:?} in {problems:#?}"
            );
            problems.clone()
        }
        TimestampOptionsValidation::Ok(options) => {
            panic!("expected failure, got {options:?}")
        }
    }
}

fn ok_options(result: TimestampOptionsValidation) -> TimestampOptions {
    match result {
        TimestampOptionsValidation::Ok(options) => options,
        TimestampOptionsValidation::Err(problems) => {
            panic!("expected success, got {problems:#?}")
        }
    }
}

fn minimal_native() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    })
}

// ---------------------------------------------------------------------------
// Valid shapes
// ---------------------------------------------------------------------------

#[test]
fn accepts_minimal_native_configuration() {
    let options = ok_options(validate_timestamp_options(&minimal_native()));
    assert_eq!(options.interpretation, TimestampInterpretation::Native);
    assert_eq!(options.format, FormatSerde::Iso);
    assert_eq!(options.display_time_zone, TimeZoneSpec::Utc);
    // sourceTimeZone must not appear in the serialized shape when absent.
    let serialized = serde_json::to_value(&options).expect("serializable");
    assert!(
        serialized.get("sourceTimeZone").is_none(),
        "sourceTimeZone must be absent: {serialized}"
    );
}

#[test]
fn accepts_unix_seconds_and_milliseconds() {
    let seconds = ok_options(validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })));
    assert_eq!(
        seconds.interpretation,
        TimestampInterpretation::Unix { unit: UnixUnitSerde::Seconds }
    );
    assert_eq!(
        seconds.display_time_zone,
        TimeZoneSpec::Iana { name: "Asia/Shanghai".to_string() }
    );
    assert_eq!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "locale",
            "displayTimeZone": { "kind": "local" },
        })),
        TimestampOptionsValidation::Ok(TimestampOptions {
            interpretation: TimestampInterpretation::Unix {
                unit: UnixUnitSerde::Milliseconds,
            },
            format: FormatSerde::Locale,
            display_time_zone: TimeZoneSpec::Local,
            source_time_zone: None,
        })
    );
}

#[test]
fn accepts_text_interpretation_with_source_time_zone() {
    let options = ok_options(validate_timestamp_options(&json!({
        "interpretation": { "kind": "text" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        "sourceTimeZone": { "kind": "iana", "name": "America/New_York" },
    })));
    assert_eq!(
        options.source_time_zone,
        Some(TimeZoneSpec::Iana { name: "America/New_York".to_string() })
    );
}

#[test]
fn accepts_source_time_zone_for_native_interpretation() {
    // native local-datetime values may carry a source zone (design §5).
    assert!(matches!(
        validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
            "sourceTimeZone": { "kind": "local" },
        })),
        TimestampOptionsValidation::Ok(_)
    ));
}

#[test]
fn json_round_trips_canonical_options() {
    // text (not unix) so the sourceTimeZone below is semantically legal.
    let legal = json!({
        "interpretation": { "kind": "text" },
        "format": "locale",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Kolkata" },
        "sourceTimeZone": { "kind": "iana", "name": "Etc/GMT+8" },
    });
    let first = ok_options(validate_timestamp_options(&legal));
    let serialized = serde_json::to_value(&first).expect("serializable");
    // The serialized shape is exactly React's stored JSON (freeze's
    // protected contract, pinned as data instead of as immutability).
    assert_eq!(
        serialized,
        json!({
            "interpretation": { "kind": "text" },
            "format": "locale",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Kolkata" },
            "sourceTimeZone": { "kind": "iana", "name": "Etc/GMT+8" },
        })
    );
    let second = ok_options(validate_timestamp_options(&serialized));
    assert_eq!(second, first);
}

// ---------------------------------------------------------------------------
// Root shape errors
// ---------------------------------------------------------------------------

#[test]
fn rejects_non_object_roots_with_pathless_problem() {
    for bad in [
        json!(null),
        json!(42),
        json!("nope"),
        json!(true),
        json!([{}]),
    ] {
        assert_eq!(
            validate_timestamp_options(&bad),
            TimestampOptionsValidation::Err(vec![TimestampOptionsProblem {
                path: String::new(),
                message: "expected an object".to_string(),
            }]),
            "root {bad}"
        );
    }
}

#[test]
fn rejects_missing_interpretation() {
    let result = validate_timestamp_options(&json!({
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_problem(&result, "interpretation", "expected an object");
}

#[test]
fn rejects_unknown_interpretation_kind() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "julian" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_problem(&result, "interpretation.kind", "expected \"native\", \"unix\" or \"text\"");
}

#[test]
fn rejects_unix_without_unit_and_with_bogus_unit() {
    for unit in [json!(null), json!("nanoseconds"), json!(5)] {
        let result = validate_timestamp_options(&json!({
            "interpretation": { "kind": "unix", "unit": unit },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        }));
        assert_problem(&result, "interpretation.unit", "expected \"seconds\" or \"milliseconds\"");
    }
    // Missing unit (absent key) is the same rejection.
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_problem(&result, "interpretation.unit", "expected \"seconds\" or \"milliseconds\"");
}

#[test]
fn rejects_unit_on_native_and_text_interpretations() {
    for kind in ["native", "text"] {
        let result = validate_timestamp_options(&json!({
            "interpretation": { "kind": kind, "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        }));
        assert_problem(
            &result,
            "interpretation.unit",
            &format!("unknown field (\"{kind}\" interpretation has no unit)"),
        );
    }
}

#[test]
fn rejects_invalid_format() {
    for format in [json!(null), json!("rfc2822"), json!(7)] {
        let result = validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": format,
            "displayTimeZone": { "kind": "utc" },
        }));
        assert_problem(&result, "format", "expected \"iso\", \"iso-tz\" or \"locale\"");
    }
    // Missing format is the same rejection.
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_problem(&result, "format", "expected \"iso\", \"iso-tz\" or \"locale\"");
}

// ---------------------------------------------------------------------------
// TimeZoneSpec errors
// ---------------------------------------------------------------------------

#[test]
fn rejects_missing_or_malformed_display_time_zone() {
    for bad in [json!(null), json!("UTC"), json!(42)] {
        let result = validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": bad,
        }));
        assert_problem(&result, "displayTimeZone", "expected an object");
    }
    // Missing spec is the same rejection.
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
    }));
    assert_problem(&result, "displayTimeZone", "expected an object");
}

#[test]
fn rejects_unknown_time_zone_spec_kind() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "gps" },
    }));
    assert_problem(&result, "displayTimeZone.kind", "expected \"utc\", \"local\" or \"iana\"");
}

#[test]
fn applies_iana_format_checks_to_iana_names() {
    for name in [
        json!(""),
        json!("Foo Bar"),
        json!("Asia/上海"),
        json!(42),
        json!(null),
        json!("A".repeat(101)),
    ] {
        let result = validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "iana", "name": name },
        }));
        assert_problem(
            &result,
            "displayTimeZone.name",
            "expected a non-empty IANA time zone name without whitespace",
        );
    }
}

#[test]
fn accepts_well_formed_iana_names() {
    // 'Mars/Olympus' passes the format check by design: full validation is
    // delegated to the formatter (documented two-stage approach).
    for name in ["UTC", "Asia/Shanghai", "Etc/GMT+8", "Mars/Olympus", "asia/shanghai"] {
        let result = validate_timestamp_options(&json!({
            "interpretation": { "kind": "native" },
            "format": "iso",
            "displayTimeZone": { "kind": "iana", "name": name },
        }));
        assert!(
            matches!(result, TimestampOptionsValidation::Ok(_)),
            "name {name} must pass the format stage: {result:?}"
        );
    }
}

#[test]
fn rejects_name_field_on_utc_and_local_specs() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc", "name": "UTC" },
    }));
    assert_problem(
        &result,
        "displayTimeZone.name",
        "unknown field (only \"iana\" carries a name)",
    );
}

#[test]
fn rejects_unknown_fields_inside_time_zone_spec() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc", "autoDst": true },
    }));
    assert_problem(&result, "displayTimeZone.autoDst", "unknown field");
}

// ---------------------------------------------------------------------------
// sourceTimeZone semantics
// ---------------------------------------------------------------------------

#[test]
fn rejects_source_time_zone_on_unix_interpretation() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    }));
    assert_problem(
        &result,
        "sourceTimeZone",
        "unix interpretation must not carry a source time zone",
    );
}

#[test]
fn reports_shape_problems_for_malformed_source_time_zone() {
    // A present null fails the shape check (JSON has no undefined).
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "text" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": null,
    }));
    assert_problem(&result, "sourceTimeZone", "expected an object");
}

#[test]
fn accumulates_shape_and_semantic_source_time_zone_problems() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix", "unit": "seconds" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "sourceTimeZone": { "kind": "gps" },
    }));
    let problems = assert_problem(&result, "sourceTimeZone.kind", "expected \"utc\", \"local\" or \"iana\"");
    assert_eq!(
        problems.iter().filter(|problem| problem.path == "sourceTimeZone").count(),
        1,
        "exactly one semantic sourceTimeZone problem: {problems:#?}"
    );
}

// ---------------------------------------------------------------------------
// Unknown fields and accumulation
// ---------------------------------------------------------------------------

#[test]
fn rejects_unknown_root_fields() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "native" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
        "timezone": { "kind": "utc" },
    }));
    assert_problem(&result, "timezone", "unknown field");
}

#[test]
fn rejects_unknown_fields_inside_interpretation() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix", "unit": "seconds", "calendar": "gregorian" },
        "format": "iso",
        "displayTimeZone": { "kind": "utc" },
    }));
    assert_problem(&result, "interpretation.calendar", "unknown field");
}

#[test]
fn collects_every_problem_instead_of_stopping_at_first() {
    let result = validate_timestamp_options(&json!({
        "interpretation": { "kind": "julian" },
        "format": "rfc2822",
        "displayTimeZone": { "kind": "gps" },
        "bogus": 1,
    }));
    match result {
        TimestampOptionsValidation::Err(problems) => {
            let paths: Vec<&str> = problems.iter().map(|p| p.path.as_str()).collect();
            for expected in ["interpretation.kind", "format", "displayTimeZone.kind", "bogus"] {
                assert!(paths.contains(&expected), "missing {expected} in {paths:?}");
            }
            assert_eq!(paths.len(), 4, "exactly four problems: {problems:#?}");
        }
        TimestampOptionsValidation::Ok(options) => panic!("expected failure: {options:?}"),
    }
}

// ---------------------------------------------------------------------------
// Canonical result shape
// ---------------------------------------------------------------------------

#[test]
fn produces_canonical_options_usable_as_timestamp_options() {
    let options = ok_options(validate_timestamp_options(&json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })));
    assert_eq!(
        options.interpretation,
        TimestampInterpretation::Unix { unit: UnixUnitSerde::Milliseconds }
    );
    assert_eq!(options.format, FormatSerde::IsoTz);
    assert_eq!(
        options.display_time_zone,
        TimeZoneSpec::Iana { name: "Asia/Shanghai".to_string() }
    );
}
