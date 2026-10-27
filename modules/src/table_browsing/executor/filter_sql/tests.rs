//! Tests for the filter SQL module: what a condition renders into, and what
//! the layer refuses to render.
//!
//! They live out of line so the module itself stays readable; the filter
//! rules are the part of the query executor that has to be read carefully.

use super::test_support::*;
use super::*;
use datazen_driver_api::filters::{DIALECT_FILTER_OPERATORS, SHARED_FILTER_OPERATORS};

#[test]
fn legacy_ten_operators_render_byte_identical() {
    let text = |value: &str| Value::String(value.to_string());
    assert_eq!(
        rendered(&condition("name", FilterOperator::Eq, text("x")), &PLAIN).unwrap(),
        "\"name\" = 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Ne, text("x")), &PLAIN).unwrap(),
        "\"name\" != 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Gt, text("x")), &PLAIN).unwrap(),
        "\"name\" > 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Lt, text("x")), &PLAIN).unwrap(),
        "\"name\" < 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Gte, text("x")), &PLAIN).unwrap(),
        "\"name\" >= 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Lte, text("x")), &PLAIN).unwrap(),
        "\"name\" <= 'x'"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::Like, text("x%")), &PLAIN).unwrap(),
        "\"name\" LIKE 'x%'"
    );
    assert_eq!(
        rendered(
            &condition(
                "id",
                FilterOperator::In,
                Value::Json(serde_json::json!([1, 2, 3]))
            ),
            &PLAIN
        )
        .unwrap(),
        "\"id\" IN ('1', '2', '3')"
    );
    assert_eq!(
        rendered(&condition("name", FilterOperator::In, text("a, b")), &PLAIN).unwrap(),
        "\"name\" IN ('a', 'b')"
    );
    assert_eq!(
        rendered(
            &condition("deleted_at", FilterOperator::IsNull, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"deleted_at\" IS NULL"
    );
    assert_eq!(
        rendered(
            &condition("deleted_at", FilterOperator::IsNotNull, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"deleted_at\" IS NOT NULL"
    );
}

#[test]
fn in_comma_string_with_only_blank_parts_is_rejected() {
    let rejection = rendered(
        &condition("name", FilterOperator::In, Value::String(" , ".into())),
        &PLAIN,
    )
    .expect_err("a blank list must not become an empty IN");
    assert!(matches!(rejection, FilterRejection::Incomplete { .. }));
}

#[test]
fn not_in_mirrors_in_element_formatting() {
    let in_sql = rendered(
        &condition(
            "id",
            FilterOperator::In,
            Value::Json(serde_json::json!([1, 2, 3])),
        ),
        &PLAIN,
    )
    .unwrap();
    let not_in_sql = rendered(
        &condition(
            "id",
            FilterOperator::NotIn,
            Value::Json(serde_json::json!([1, 2, 3])),
        ),
        &PLAIN,
    )
    .unwrap();
    assert_eq!(not_in_sql.replace(" NOT IN ", " IN "), in_sql);
}

#[test]
fn between_renders_two_literals_in_order() {
    assert_eq!(
        rendered(
            &condition(
                "score",
                FilterOperator::Between,
                Value::Json(serde_json::json!([1, 5]))
            ),
            &PLAIN
        )
        .unwrap(),
        "\"score\" BETWEEN '1' AND '5'"
    );
    // The host does not swap the bounds: a reversed range is the user's
    // input, and the database answers it honestly.
    assert_eq!(
        rendered(
            &condition(
                "score",
                FilterOperator::Between,
                Value::Json(serde_json::json!([5, 1]))
            ),
            &PLAIN
        )
        .unwrap(),
        "\"score\" BETWEEN '5' AND '1'"
    );
}

#[test]
fn between_comma_string_splits_on_first_comma_only() {
    assert_eq!(
        rendered(
            &condition(
                "created_at",
                FilterOperator::Between,
                Value::String("2024-01-01,2024-02-01".into())
            ),
            &PLAIN
        )
        .unwrap(),
        "\"created_at\" BETWEEN '2024-01-01' AND '2024-02-01'"
    );
    let rejection = rendered(
        &condition(
            "created_at",
            FilterOperator::Between,
            Value::String("a,b,c".into()),
        ),
        &PLAIN,
    )
    .expect_err("three comma separated parts are not two bounds");
    assert!(matches!(rejection, FilterRejection::Incomplete { .. }));
}

/// A range with an empty bound is incomplete — in **both** value forms.
///
/// The string form always judged the bounds (`"10,"` was refused); the array
/// form did not, so `["10", ""]` rendered `BETWEEN '10' AND ''` and matched
/// nothing without saying so. The two forms are one rule now.
#[test]
fn between_with_an_empty_bound_is_refused_in_both_value_forms() {
    let empty_upper = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::Json(serde_json::json!(["10", ""])),
        ),
        &PLAIN,
    )
    .expect_err("an empty upper bound is not a bound");
    assert!(matches!(empty_upper, FilterRejection::Incomplete { .. }));
    let message = empty_upper.contract_message();
    assert!(
        message.starts_with("grid.filter.incomplete: "),
        "got: {message}"
    );
    // The refusal names the missing bound, so the user knows what to fill in.
    assert!(message.contains("upper bound"), "got: {message}");
    assert!(message.contains("\"score\""), "got: {message}");

    let empty_lower = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::Json(serde_json::json!(["", "10"])),
        ),
        &PLAIN,
    )
    .expect_err("an empty lower bound is not a bound");
    let message = empty_lower.contract_message();
    assert!(message.contains("lower bound"), "got: {message}");

    // The same rule for the comma separated form, which already refused them.
    let empty_upper_text = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::String("10,".into()),
        ),
        &PLAIN,
    )
    .expect_err("an empty upper bound is not a bound");
    let message = empty_upper_text.contract_message();
    assert!(message.contains("upper bound"), "got: {message}");

    // Whitespace is emptiness too, and a null bound is emptiness as well.
    for value in [
        serde_json::json!(["10", "   "]),
        serde_json::json!([null, "10"]),
        serde_json::json!(["10", null]),
    ] {
        let rejection = rendered(
            &condition("score", FilterOperator::Between, Value::Json(value)),
            &PLAIN,
        )
        .expect_err("a blank bound is not a bound");
        assert!(
            matches!(rejection, FilterRejection::Incomplete { .. }),
            "got: {rejection:?}"
        );
    }

    // The negated operator judges the same way.
    let negated = rendered(
        &condition(
            "score",
            FilterOperator::NotBetween,
            Value::Json(serde_json::json!(["10", ""])),
        ),
        &PLAIN,
    )
    .expect_err("the negated range refuses an empty bound as well");
    assert!(matches!(negated, FilterRejection::Incomplete { .. }));
}

/// The completeness predicate agrees with the renderer: an array with an empty
/// bound is incomplete, so the entry gate refuses it before any SQL is built.
#[test]
fn filter_is_complete_refuses_a_range_with_an_empty_bound() {
    let complete = condition(
        "score",
        FilterOperator::Between,
        Value::Json(serde_json::json!(["10", "20"])),
    );
    assert!(filter_is_complete(&complete));
    assert_eq!(incomplete_reason(&complete), None);

    for value in [
        serde_json::json!(["10", ""]),
        serde_json::json!(["", "20"]),
        serde_json::json!([null, "20"]),
    ] {
        let incomplete = condition("score", FilterOperator::Between, Value::Json(value));
        assert!(
            !filter_is_complete(&incomplete),
            "a range with a blank bound must not count as complete"
        );
        let reason = incomplete_reason(&incomplete).expect("a blank bound has a reason");
        assert!(
            reason.contains("bound"),
            "the reason names the missing bound, got: {reason}"
        );
    }

    // A zero is a value, not an emptiness: `BETWEEN 0 AND 10` is a real range.
    let zero_bound = condition(
        "score",
        FilterOperator::Between,
        Value::Json(serde_json::json!([0, 10])),
    );
    assert!(filter_is_complete(&zero_bound));
    assert_eq!(
        rendered(&zero_bound, &PLAIN).unwrap(),
        "\"score\" BETWEEN '0' AND '10'"
    );
}

#[test]
fn contains_escapes_wildcards_and_escape_char() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("50%".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%50!%%' ESCAPE '!'"
    );
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("a_b".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%a!_b%' ESCAPE '!'"
    );
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("a!b".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%a!!b%' ESCAPE '!'"
    );
}

#[test]
fn starts_with_and_ends_with_put_wildcard_on_one_side() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::StartsWith,
                Value::String("a_".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE 'a!_%' ESCAPE '!'"
    );
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::EndsWith, Value::String("a_".into())),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%a!_' ESCAPE '!'"
    );
}

#[test]
fn not_contains_uses_not_like_with_same_escaping() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::NotContains,
                Value::String("50%".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" NOT LIKE '%50!%%' ESCAPE '!'"
    );
}

#[test]
fn is_empty_compares_to_empty_string_only() {
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::IsEmpty, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"name\" = ''"
    );
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::IsNotEmpty, Value::Null),
            &PLAIN
        )
        .unwrap(),
        "\"name\" <> ''"
    );
}

#[test]
fn regex_without_driver_support_is_rejected_with_contract_prefix() {
    let rejection = rendered(
        &condition("name", FilterOperator::Regex, Value::String("^a".into())),
        &PLAIN,
    )
    .expect_err("a dialect operator nobody implements must be refused");
    assert!(matches!(
        rejection,
        FilterRejection::UnsupportedOperator { .. }
    ));
    let message = rejection.contract_message();
    assert!(
        message.starts_with("grid.filter.unsupportedOperator: "),
        "got: {message}"
    );
    assert!(message.contains("'regex'"), "got: {message}");
    assert!(message.contains("\"name\""), "got: {message}");
}

#[test]
fn regex_with_driver_support_uses_driver_sql_verbatim() {
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::Regex, Value::String("^a".into())),
            &WITH_DIALECT_OPERATOR
        )
        .unwrap(),
        "\"name\" ~ '^a'"
    );
}

#[test]
fn incomplete_condition_is_rejected_not_silently_dropped() {
    // Both an empty string and a null value are "no value".
    for value in [Value::String(String::new()), Value::Null] {
        let rejection = rendered(&condition("id", FilterOperator::Eq, value), &PLAIN)
            .expect_err("an incomplete condition must be refused, not skipped");
        assert!(matches!(rejection, FilterRejection::Incomplete { .. }));
        let message = rejection.contract_message();
        assert!(
            message.starts_with("grid.filter.incomplete: "),
            "got: {message}"
        );
        assert!(message.contains("needs a value"), "got: {message}");
        assert!(message.contains("\"id\""), "got: {message}");
        assert!(message.contains("'eq'"), "got: {message}");
    }
}

#[test]
fn complete_and_incomplete_mixed_list_reports_the_offending_column() {
    let filters = vec![
        condition("name", FilterOperator::Eq, Value::String("alice".into())),
        condition("id", FilterOperator::Eq, Value::String(String::new())),
    ];
    let rejection = validate_filters(SHARED_FILTER_OPERATORS, &filters)
        .expect_err("the incomplete condition must be named");
    assert_eq!(
        rejection.contract_message(),
        "grid.filter.incomplete: column \"id\" needs a value (operator 'eq')"
    );
}

#[test]
fn filter_join_defaults_to_and() {
    assert_eq!(filter_join(None), " AND ");
    assert_eq!(filter_join(Some("and")), " AND ");
    assert_eq!(filter_join(Some("or")), " OR ");
}

#[test]
fn multibyte_pattern_is_not_byte_sliced() {
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("中文🙂%".into())
            ),
            &ESCAPING
        )
        .unwrap(),
        "\"name\" LIKE '%中文🙂!%%' ESCAPE '!'"
    );
}

#[test]
fn single_quote_in_value_is_left_to_driver_formatter() {
    assert_eq!(
        rendered(
            &condition("name", FilterOperator::Eq, Value::String("O'Brien".into())),
            &PLAIN
        )
        .unwrap(),
        "\"name\" = 'O''Brien'"
    );
}

#[test]
fn literal_arity_mismatch_is_rejected_for_between_and_in() {
    let one = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::Json(serde_json::json!([1])),
        ),
        &PLAIN,
    );
    assert!(one.is_err());
    let three = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::Json(serde_json::json!([1, 2, 3])),
        ),
        &PLAIN,
    );
    assert!(three.is_err());
    let empty_set = rendered(
        &condition("id", FilterOperator::In, Value::Json(serde_json::json!([]))),
        &PLAIN,
    );
    assert!(empty_set.is_err());
    // The same set operator under its negation, and a range value that names
    // no bound at all.
    let empty_negated_set = rendered(
        &condition(
            "id",
            FilterOperator::NotIn,
            Value::Json(serde_json::json!([])),
        ),
        &PLAIN,
    );
    assert!(empty_negated_set.is_err());
    let no_comma = rendered(
        &condition(
            "score",
            FilterOperator::Between,
            Value::String("only-one".into()),
        ),
        &PLAIN,
    );
    assert!(no_comma.is_err());
}

#[test]
fn like_escape_config_rejects_half_configured_pair() {
    let rejection = like_escape_config(&HALF_CONFIGURED)
        .expect_err("an escape character without a clause must be refused");
    assert!(matches!(
        rejection,
        FilterRejection::InvalidEscapeConfiguration { escape_char: '!' }
    ));
    let message = rejection.contract_message();
    assert!(
        message.starts_with("grid.filter.unsupportedOperator: "),
        "got: {message}"
    );
    assert!(message.contains("'!'"), "got: {message}");
    assert!(message.contains("ESCAPE"), "got: {message}");

    // The same refusal has to happen where the pattern is rendered: a
    // half-configured pair would otherwise turn an escaped `%` back into a
    // wildcard and quietly return the wrong rows.
    let rejection = rendered(
        &condition(
            "name",
            FilterOperator::Contains,
            Value::String("50%".into()),
        ),
        &HALF_CONFIGURED,
    )
    .expect_err("the half-configured escape must be refused");
    assert!(rejection
        .contract_message()
        .starts_with("grid.filter.unsupportedOperator: "));
}

#[test]
fn like_escape_config_none_none_is_identity() {
    let config = like_escape_config(&PLAIN).unwrap();
    assert_eq!(config.escape_char, None);
    assert_eq!(config.clause, None);
    // Without escaping the pattern is the plain text again, so a `%` the
    // user typed stays a wildcard.
    assert_eq!(
        rendered(
            &condition(
                "name",
                FilterOperator::Contains,
                Value::String("50%".into())
            ),
            &PLAIN
        )
        .unwrap(),
        "\"name\" LIKE '%50%%'"
    );
}

#[test]
fn validate_filters_rejects_operator_outside_the_whitelist() {
    let filters = vec![condition(
        "name",
        FilterOperator::Regex,
        Value::String("^a".into()),
    )];
    let rejection = validate_filters(SHARED_FILTER_OPERATORS, &filters)
        .expect_err("a dialect operator is not in the shared whitelist");
    let message = rejection.contract_message();
    assert!(
        message.starts_with("grid.filter.unsupportedOperator: "),
        "got: {message}"
    );
    // The message names the operator, the column and an alternative, so the
    // interface can tell the user what to do next.
    assert!(message.contains("'regex'"), "got: {message}");
    assert!(message.contains("\"name\""), "got: {message}");
    assert!(
        message.contains("try 'like' or 'contains' instead"),
        "got: {message}"
    );
    for operator in DIALECT_FILTER_OPERATORS {
        assert!(!SHARED_FILTER_OPERATORS.contains(operator));
    }
}

#[test]
fn overlong_regex_pattern_is_refused() {
    // A regular expression is handed to the database engine verbatim, so a
    // pathological pattern can pin a whole backend connection. The length
    // guard is the fence: this test is the only thing that notices when it is
    // widened or removed.
    let pattern = "a".repeat(MAX_REGEX_PATTERN_LENGTH + 1);
    let filters = vec![condition(
        "name",
        FilterOperator::Regex,
        Value::String(pattern),
    )];
    // A dialect operator only reaches the length check on a driver that
    // declared it.
    let mut supported = SHARED_FILTER_OPERATORS.to_vec();
    supported.push(FilterOperator::Regex);
    let rejection =
        validate_filters(&supported, &filters).expect_err("an overlong pattern must be refused");
    let message = rejection.contract_message();
    assert!(
        message.starts_with("grid.filter.incomplete: "),
        "got: {message}"
    );
    assert!(message.contains("too long"), "got: {message}");
    assert!(message.contains("\"name\""), "got: {message}");

    // One character below the limit is still fine.
    let at_the_limit = vec![condition(
        "name",
        FilterOperator::Regex,
        Value::String("a".repeat(MAX_REGEX_PATTERN_LENGTH)),
    )];
    validate_filters(&supported, &at_the_limit).expect("the limit itself is allowed");
}

#[test]
fn validate_filters_accepts_the_shared_operators() {
    let filters = vec![
        condition("name", FilterOperator::Eq, Value::String("alice".into())),
        condition(
            "id",
            FilterOperator::In,
            Value::Json(serde_json::json!([1, 2])),
        ),
        condition("deleted_at", FilterOperator::IsNull, Value::Null),
    ];
    assert!(validate_filters(SHARED_FILTER_OPERATORS, &filters).is_ok());
}

#[test]
fn filter_is_complete_forwards_the_shared_predicate() {
    let complete = condition("name", FilterOperator::Eq, Value::String("x".into()));
    let incomplete = condition("name", FilterOperator::Eq, Value::String(String::new()));
    assert!(filter_is_complete(&complete));
    assert!(!filter_is_complete(&incomplete));
}

#[test]
fn incomplete_when_the_condition_has_no_column() {
    // There is no column to name, so the message says what is missing
    // instead of filling the column slot with an empty pair of quotes.
    let filters = vec![linked("", FilterOperator::Eq, Value::Integer(1), None)];
    let rejection = validate_filters(SHARED_FILTER_OPERATORS, &filters)
        .expect_err("a condition without a column must be refused");
    let message = rejection.contract_message();
    assert!(
        message.starts_with("grid.filter.incomplete: "),
        "got: {message}"
    );
    assert_eq!(
        message,
        "grid.filter.incomplete: the condition has no column (operator 'eq')"
    );
}

#[test]
fn the_filter_domain_has_exactly_two_codes() {
    // The filter domain is a closed set: two codes, and a third one would
    // need a contract change before the front end could classify it.
    let rejections = [
        FilterRejection::UnsupportedOperator {
            column: "c".into(),
            operator: FilterOperator::Regex,
        },
        FilterRejection::Incomplete {
            column: "c".into(),
            operator: FilterOperator::Eq,
            reason: "needs a value",
        },
        FilterRejection::InvalidEscapeConfiguration { escape_char: '!' },
    ];
    let prefixes: Vec<String> = rejections
        .iter()
        .map(|rejection| {
            rejection
                .contract_message()
                .split(':')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    for prefix in &prefixes {
        assert!(prefix.starts_with("grid.filter."), "got: {prefix}");
    }
    assert_eq!(
        prefixes,
        vec![
            "grid.filter.unsupportedOperator".to_string(),
            "grid.filter.incomplete".to_string(),
            "grid.filter.unsupportedOperator".to_string(),
        ]
    );
    // The driver's own message travels unchanged — it is not a filter
    // problem, so it carries no contract prefix.
    let driver = FilterRejection::Driver {
        message: "connection refused".into(),
    };
    assert_eq!(driver.contract_message(), "connection refused");
}
