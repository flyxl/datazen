//! Independent audit for T291 (the valuePresentationRules port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/valuePresentationRules.ts`
//! and its React test `__tests__/valuePresentationRules.test.ts` — the
//! restricted-regex accept/reject table, substring match semantics, the
//! i flag, rule validation (name/enabled/match/options, unknown fields,
//! empty conditions), the storage-boundary array parsing, duplicate-name
//! detection, AND-matching with source-required table-level conditions,
//! first-match freezing, and the describe helpers.
//!
//! The React file's final describe block (7 cases: "rules in snapshot and
//! plan") exercises buildValuePresentationSnapshot (settings.ts, T182)
//! and buildColumnPresentationPlans (presentationPlan.ts, T155) through
//! the timestamp contribution (T251) — those ports land after this one in
//! the area's dependency order, and the end-to-end cases move to their
//! audits. Elisions here: `Object.freeze` (ownership is Rust's
//! immutability).

use serde_json::json;

use datazen_ui::support::value_presentation::timestamp_config::{
    FormatSerde, TimestampInterpretation, TimestampOptions, UnixUnitSerde,
};
use datazen_ui::support::value_presentation::timestamp_format::TimeZoneSpec;
use datazen_ui::support::value_presentation::value_presentation_rules::{
    MAX_PATTERN_LENGTH, MAX_VALUE_PRESENTATION_RULES, ValuePresentationNamePattern,
    ValuePresentationRule, ValuePresentationRuleContext, ValuePresentationRuleMatch,
    ValuePresentationRuleValidation, describe_name_pattern, describe_rule_match,
    find_duplicate_rule_name, find_first_matching_rule, match_name_pattern,
    match_value_presentation_rule, validate_restricted_pattern,
    validate_value_presentation_rule, validate_value_presentation_rules,
};

fn unix_ms_shanghai() -> TimestampOptions {
    TimestampOptions {
        interpretation: TimestampInterpretation::Unix {
            unit: UnixUnitSerde::Milliseconds,
        },
        format: FormatSerde::IsoTz,
        display_time_zone: TimeZoneSpec::Iana { name: "Asia/Shanghai".to_string() },
        source_time_zone: None,
    }
}

/// PRD §3.1 example: BIGINT and column matches ^(created_at|updated_at)$
/// → Unix milliseconds.
fn prd_rule_input() -> serde_json::Value {
    json!({
        "name": "unix-ms-timestamps",
        "enabled": true,
        "match": {
            "column": { "kind": "regex", "source": "^(created_at|updated_at)$" },
            "databaseTypeName": { "kind": "exact", "value": "BIGINT" },
        },
        "options": {
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        },
    })
}

/// The React helper `ruleWith`: validate the PRD input with patches.
fn rule_with(patch: &serde_json::Value) -> ValuePresentationRule {
    let mut input = prd_rule_input();
    if let Some(name) = patch.get("name") {
        input["name"] = name.clone();
    }
    if let Some(enabled) = patch.get("enabled") {
        input["enabled"] = enabled.clone();
    }
    if let Some(r#match) = patch.get("match") {
        input["match"] = r#match.clone();
    }
    if let Some(options) = patch.get("options") {
        input["options"] = options.clone();
    }
    match validate_value_presentation_rule(&input) {
        ValuePresentationRuleValidation::Ok(rule) => *rule,
        ValuePresentationRuleValidation::Err(problems) => {
            panic!("test fixture invalid: {problems:#?}")
        }
    }
}

fn assert_problem_at(
    result: &ValuePresentationRuleValidation,
    path: &str,
) {
    match result {
        ValuePresentationRuleValidation::Err(problems) => {
            assert!(
                problems.iter().any(|problem| problem.path == path),
                "expected a problem at {path:?} in {problems:#?}"
            );
        }
        ValuePresentationRuleValidation::Ok(rule) => {
            panic!("expected failure, got {rule:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// validateRestrictedPattern
// ---------------------------------------------------------------------------

#[test]
fn restricted_pattern_accepts_the_subset() {
    for source in [
        "^(created_at|updated_at)$",
        "created_at",
        "\\d+",
        "[a-z_]+",
        "a{2,3}",
        "a+?",
        "a*?",
        "(?:ab)c",
        ".*",
        ".+",
        "col_[0-9]{2}",
        "^\\w+$",
        "a{0,}",
        "{,3}",
        "price|total",
    ] {
        assert_eq!(validate_restricted_pattern(source, None), Ok(()), "source {source}");
    }
}

#[test]
fn restricted_pattern_accepts_the_i_flag_only() {
    assert_eq!(validate_restricted_pattern("created_at", Some("i")), Ok(()));
    assert_eq!(validate_restricted_pattern("created_at", Some("")), Ok(()));
    assert_eq!(
        validate_restricted_pattern("created_at", Some("g")),
        Err("only the \"i\" flag is allowed".to_string())
    );
    assert_eq!(
        validate_restricted_pattern("created_at", Some("m")),
        Err("only the \"i\" flag is allowed".to_string())
    );
}

#[test]
fn restricted_pattern_rejects_backtracking_and_invalid_forms() {
    let rejections: &[(&str, &str)] = &[
        ("\\1", "backreference"),
        ("(a)\\k<x>", "backreference"),
        ("(?=x)a", "lookahead"),
        ("(?!x)a", "lookahead"),
        ("(?<=x)a", "lookbehind"),
        ("(?<!x)a", "lookbehind"),
        ("(?P<x>a)", "lookaround"),
        ("\\p{L}+", "unsupported escape"),
        ("\\u0041", "unsupported escape"),
        ("\\x41", "unsupported escape"),
        ("(a+)+", "nested quantifier"),
        ("(a*)*", "nested quantifier"),
        ("(a|b)+", "alternation"),
        ("(a|ab)+c", "alternation"),
        ("((a|b)c)+", "alternation"),
        ("(a|)+", "alternation"),
        // A purely nullable quantified group (no alternation, no inner
        // quantifier) — only the nullable check rejects this shape.
        ("()+", "quantified nullable group"),
        ("(^)+", "quantified nullable anchor group"),
        ("a**", "nested quantifier"),
        ("a+*", "nested quantifier"),
        ("a{101}", "repetition count"),
        ("a{3,2}", "repetition minimum"),
        ("a{2}{3}", "repetition without a preceding atom"),
        ("{3}", "repetition without a preceding atom"),
        ("(abc", "unclosed group"),
        ("[abc", "unclosed character class"),
        ("abc\\", "trailing backslash"),
        ("", "empty"),
    ];
    for (source, why) in rejections {
        assert!(
            validate_restricted_pattern(source, None).is_err(),
            "source {source} ({why}) must be rejected"
        );
    }
}

#[test]
fn restricted_pattern_rejects_over_length_patterns() {
    // The cap itself is pinned (the React test derives its inputs from the
    // same constant, which would silently follow a mutated value).
    assert_eq!(MAX_PATTERN_LENGTH, 128);
    let long = format!("a{{{} }}", "1".repeat(MAX_PATTERN_LENGTH)).replace(' ', "");
    assert!(long.chars().count() > MAX_PATTERN_LENGTH);
    assert!(validate_restricted_pattern(&long, None).is_err());
    assert!(validate_restricted_pattern(&"a".repeat(MAX_PATTERN_LENGTH), None).is_ok());
    assert!(validate_restricted_pattern(&"a".repeat(MAX_PATTERN_LENGTH + 1), None).is_err());
    // Hard-coded boundaries independent of the constant.
    assert!(validate_restricted_pattern(&"a".repeat(128), None).is_ok());
    assert!(validate_restricted_pattern(&"a".repeat(129), None).is_err());
}

// ---------------------------------------------------------------------------
// matchNamePattern
// ---------------------------------------------------------------------------

#[test]
fn name_pattern_matches_exact_values_by_equality() {
    let exact = ValuePresentationNamePattern::Exact { value: "orders".to_string() };
    assert!(match_name_pattern(&exact, "orders"));
    assert!(!match_name_pattern(&exact, "Orders"));
    assert!(!match_name_pattern(&exact, "orders_2"));
}

#[test]
fn name_pattern_matches_regex_as_substring_without_anchors() {
    let cases: [(ValuePresentationNamePattern, &str, bool); 5] = [
        (regex_pattern("created", None), "created_at", true),
        (regex_pattern("^created", None), "created_at", true),
        (regex_pattern("^created$", None), "created_at", false),
        (regex_pattern("^(created_at|updated_at)$", None), "updated_at", true),
        (regex_pattern("^(created_at|updated_at)$", None), "deleted_at", false),
    ];
    for (pattern, value, expected) in cases {
        assert_eq!(match_name_pattern(&pattern, value), expected, "{pattern:?} vs {value}");
    }
}

fn regex_pattern(source: &str, flags: Option<&str>) -> ValuePresentationNamePattern {
    ValuePresentationNamePattern::Regex {
        source: source.to_string(),
        flags: flags.map(str::to_string),
    }
}

#[test]
fn name_pattern_honours_the_i_flag() {
    assert!(match_name_pattern(&regex_pattern("created_at", Some("i")), "CREATED_AT"));
    assert!(!match_name_pattern(&regex_pattern("created_at", None), "CREATED_AT"));
}

#[test]
fn name_pattern_returns_false_for_uncompilable_patterns() {
    assert!(!match_name_pattern(&regex_pattern("(a+", None), "a"));
}

// ---------------------------------------------------------------------------
// validateValuePresentationRule
// ---------------------------------------------------------------------------

#[test]
fn rule_validation_accepts_prd_example_and_trims_the_name() {
    let mut input = prd_rule_input();
    input["name"] = json!("  unix-ms-timestamps  ");
    match validate_value_presentation_rule(&input) {
        ValuePresentationRuleValidation::Ok(rule) => {
            assert_eq!(rule.name, "unix-ms-timestamps");
            assert!(rule.enabled);
            assert_eq!(rule.options, unix_ms_shanghai());
        }
        ValuePresentationRuleValidation::Err(problems) => {
            panic!("expected success: {problems:#?}")
        }
    }
}

#[test]
fn rule_validation_rejects_bad_names() {
    let long_name = "x".repeat(65);
    for name in ["", "   ", "has space", "has\ttab", long_name.as_str()] {
        let mut input = prd_rule_input();
        input["name"] = json!(name);
        assert_problem_at(&validate_value_presentation_rule(&input), "name");
    }
}

#[test]
fn rule_validation_rejects_bad_enabled_and_unknown_fields() {
    let mut input = prd_rule_input();
    input["enabled"] = json!("yes");
    assert!(matches!(
        validate_value_presentation_rule(&input),
        ValuePresentationRuleValidation::Err(_)
    ));
    let mut input = prd_rule_input();
    input["priority"] = json!(1);
    assert_problem_at(&validate_value_presentation_rule(&input), "priority");
}

#[test]
fn rule_validation_rejects_no_match_condition() {
    let mut input = prd_rule_input();
    input["match"] = json!({});
    assert_problem_at(&validate_value_presentation_rule(&input), "match");
}

#[test]
fn rule_validation_rejects_empty_exact_and_unknown_match_fields() {
    let mut input = prd_rule_input();
    input["match"] = json!({ "column": { "kind": "exact", "value": "  " } });
    assert!(matches!(
        validate_value_presentation_rule(&input),
        ValuePresentationRuleValidation::Err(_)
    ));
    let mut input = prd_rule_input();
    input["match"]["owner"] = json!("me");
    assert_problem_at(&validate_value_presentation_rule(&input), "match.owner");
}

#[test]
fn rule_validation_rejects_invalid_regex_and_options() {
    let mut input = prd_rule_input();
    input["match"] = json!({ "column": { "kind": "regex", "source": "(a+)+" } });
    assert!(matches!(
        validate_value_presentation_rule(&input),
        ValuePresentationRuleValidation::Err(_)
    ));
    // Unix interpretation must not carry a sourceTimeZone (T02 rule).
    let mut input = prd_rule_input();
    input["options"]["sourceTimeZone"] = json!({ "kind": "utc" });
    assert!(matches!(
        validate_value_presentation_rule(&input),
        ValuePresentationRuleValidation::Err(_)
    ));
    // Invalid format.
    let mut input = prd_rule_input();
    input["options"]["format"] = json!("rfc3339");
    assert!(matches!(
        validate_value_presentation_rule(&input),
        ValuePresentationRuleValidation::Err(_)
    ));
}

#[test]
fn rule_validation_rejects_non_object_input() {
    assert!(matches!(
        validate_value_presentation_rule(&json!(null)),
        ValuePresentationRuleValidation::Err(_)
    ));
    assert!(matches!(
        validate_value_presentation_rule(&json!("rule")),
        ValuePresentationRuleValidation::Err(_)
    ));
}

// ---------------------------------------------------------------------------
// validateValuePresentationRules (storage boundary)
// ---------------------------------------------------------------------------

#[test]
fn rules_parsing_returns_empty_for_non_arrays_and_drops_invalid() {
    assert!(validate_value_presentation_rules(&json!(null)).is_empty());
    assert!(validate_value_presentation_rules(&json!("x")).is_empty());
    let rules = validate_value_presentation_rules(&json!([
        prd_rule_input(),
        { "name": "", "enabled": true, "match": { "column": { "kind": "exact", "value": "c" } }, "options": { "interpretation": { "kind": "native" }, "format": "iso", "displayTimeZone": { "kind": "utc" } } },
        "not-a-rule",
        { "name": "second", "enabled": true, "match": { "table": { "kind": "exact", "value": "t" } }, "options": { "interpretation": { "kind": "native" }, "format": "iso", "displayTimeZone": { "kind": "utc" } } },
    ]));
    let names: Vec<&str> = rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["unix-ms-timestamps", "second"]);
}

#[test]
fn rules_parsing_caps_at_max_rules() {
    let mut many = Vec::new();
    for i in 0..(MAX_VALUE_PRESENTATION_RULES + 5) {
        let mut input = prd_rule_input();
        input["name"] = json!(format!("rule-{i}"));
        many.push(input);
    }
    assert_eq!(
        validate_value_presentation_rules(&serde_json::Value::Array(many)).len(),
        MAX_VALUE_PRESENTATION_RULES
    );
}

// ---------------------------------------------------------------------------
// findDuplicateRuleName / describe helpers
// ---------------------------------------------------------------------------

#[test]
fn duplicate_finder_reports_and_ignores_the_edited_index() {
    let rules = vec![
        rule_with(&json!({ "name": "a" })),
        rule_with(&json!({ "name": "b" })),
        rule_with(&json!({ "name": "a" })),
    ];
    assert_eq!(find_duplicate_rule_name(&rules, None), Some("a".to_string()));
    assert_eq!(find_duplicate_rule_name(&rules, Some(2)), None);
    let distinct =
        vec![rule_with(&json!({ "name": "a" })), rule_with(&json!({ "name": "b" }))];
    assert_eq!(find_duplicate_rule_name(&distinct, None), None);
}

#[test]
fn describe_helpers_render_patterns_and_matches() {
    assert_eq!(
        describe_name_pattern(&ValuePresentationNamePattern::Exact {
            value: "BIGINT".to_string()
        }),
        "BIGINT"
    );
    assert_eq!(describe_name_pattern(&regex_pattern("^a$", Some("i"))), "/^a$/i");
    assert_eq!(describe_name_pattern(&regex_pattern("^a$", None)), "/^a$/");
    let r#match = ValuePresentationRuleMatch {
        connection_id: Some("conn-1".to_string()),
        table: Some(ValuePresentationNamePattern::Exact { value: "orders".to_string() }),
        column: Some(regex_pattern("^created_at$", None)),
        ..ValuePresentationRuleMatch::default()
    };
    assert_eq!(
        describe_rule_match(&r#match),
        "connection=conn-1, table=orders, column=/^created_at$/"
    );
}

// ---------------------------------------------------------------------------
// matchValuePresentationRule
// ---------------------------------------------------------------------------

fn full_context() -> ValuePresentationRuleContext<'static> {
    ValuePresentationRuleContext {
        connection_id: Some("conn-1"),
        database: Some("shop"),
        namespace: Some("public"),
        table: Some("orders"),
        column: "created_at",
        database_type_name: Some("BIGINT"),
    }
}

#[test]
fn rule_matching_requires_every_condition() {
    let rule = rule_with(&json!({
        "match": {
            "connectionId": "conn-1",
            "database": "shop",
            "namespace": "public",
            "table": { "kind": "exact", "value": "orders" },
            "column": { "kind": "regex", "source": "^created_at$" },
            "databaseTypeName": { "kind": "exact", "value": "BIGINT" },
        }
    }));
    assert!(match_value_presentation_rule(&rule, &full_context()));
}

#[test]
fn rule_matching_fails_when_any_condition_fails() {
    let variants = [
        json!({ "connectionId": "conn-2" }),
        json!({ "database": "other" }),
        json!({ "namespace": "private" }),
        json!({ "table": { "kind": "exact", "value": "customers" } }),
        json!({ "column": { "kind": "exact", "value": "updated_at" } }),
        json!({ "databaseTypeName": { "kind": "exact", "value": "VARCHAR" } }),
        json!({ "column": { "kind": "regex", "source": "^updated_at$" } }),
        json!({ "databaseTypeName": { "kind": "regex", "source": "^INT$" } }),
    ];
    for variant in variants {
        let rule = rule_with(&json!({ "match": variant }));
        assert!(
            !match_value_presentation_rule(&rule, &full_context()),
            "variant {variant} must not match"
        );
    }
}

#[test]
fn rule_matching_requires_source_for_table_level_conditions() {
    let no_source = ValuePresentationRuleContext {
        connection_id: None,
        database: None,
        namespace: None,
        table: None,
        column: "created_at",
        database_type_name: Some("BIGINT"),
    };
    // Table-level conditions stay unsatisfied without a source.
    let table_rule = rule_with(&json!({ "match": { "table": { "kind": "exact", "value": "orders" } } }));
    assert!(!match_value_presentation_rule(&table_rule, &no_source));
    let connection_rule = rule_with(&json!({ "match": { "connectionId": "conn-1" } }));
    assert!(!match_value_presentation_rule(&connection_rule, &no_source));
    // Column/type-name-only conditions still match (query-result columns).
    let no_conditions = rule_with(&json!({ "match": { "column": { "kind": "exact", "value": "created_at" } } }));
    assert!(match_value_presentation_rule(&no_conditions, &no_source));
}

#[test]
fn rule_matching_case_insensitivity_only_with_i_flag() {
    let rule = rule_with(&json!({
        "match": { "databaseTypeName": { "kind": "regex", "source": "^bigint$" } }
    }));
    assert!(!match_value_presentation_rule(&rule, &full_context()));
    let rule_i = rule_with(&json!({
        "match": { "databaseTypeName": { "kind": "regex", "source": "^bigint$", "flags": "i" } }
    }));
    assert!(match_value_presentation_rule(&rule_i, &full_context()));
}

// ---------------------------------------------------------------------------
// findFirstMatchingRule
// ---------------------------------------------------------------------------

#[test]
fn first_matching_rule_skips_disabled_and_returns_first_match() {
    let context = ValuePresentationRuleContext {
        connection_id: Some("conn-1"),
        database: None,
        namespace: None,
        table: Some("orders"),
        column: "created_at",
        database_type_name: Some("BIGINT"),
    };
    let disabled = rule_with(&json!({
        "name": "disabled-first",
        "enabled": false,
        "match": { "column": { "kind": "exact", "value": "created_at" } }
    }));
    let first = rule_with(&json!({ "name": "first" }));
    let second = rule_with(&json!({
        "name": "second",
        "options": {
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        }
    }));
    let rules = [disabled, first, second];
    let found = find_first_matching_rule(&rules, &context).expect("first matches");
    assert_eq!(found.name, "first");
    // Identity: the returned reference is the second element.
    assert!(std::ptr::eq(found, &rules[1]));
    let only_disabled = [rules[0].clone()];
    assert!(find_first_matching_rule(&only_disabled, &context).is_none());
    let empty: [ValuePresentationRule; 0] = [];
    assert!(find_first_matching_rule(&empty, &context).is_none());
}
