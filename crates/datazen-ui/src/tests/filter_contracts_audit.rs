//! T216 independent audit: `filterValues` / `filterLabels` / `FilterColumn`.
//!
//! Probes React boundary cases the coder's suite does not pin, especially the
//! `formatCell` path of `conditionSummary` when the stored value shape does not
//! match the operator's value kind (a legacy draft), plus source guards that
//! no T056/T166 test or API was deleted or weakened.

use crate::components::filter::{
    condition_summary, op_label, to_pair, to_tokens, FilterColumn, FilterCondition, FilterValue,
};
use crate::i18n::t;

/// React `formatCell` on an array is `JSON.stringify` (`["a","b"]`), while a
/// string is rendered unchanged. A legacy draft can hold an array under a
/// single-value operator (`eq`), which React's `conditionSummary` renders via
/// `formatCell(f.value)`.
#[test]
fn condition_summary_single_kind_with_array_value_uses_format_cell() {
    let tokens = FilterCondition::new("name", "eq", FilterValue::Tokens(vec!["a".to_owned(), "b".to_owned()]));
    let expected = format!("name {} [\"a\",\"b\"]", t("filter.eq"));
    assert_eq!(
        condition_summary(&tokens),
        expected,
        "React formatCell(array) is JSON.stringify, not a comma join"
    );

    let pair = FilterCondition::new("score", "eq", FilterValue::Pair("10".to_owned(), "20".to_owned()));
    let expected_pair = format!("score {} [\"10\",\"20\"]", t("filter.eq"));
    assert_eq!(condition_summary(&pair), expected_pair);
}

/// `toPair` splits on the FIRST comma only and keeps an unsplit scalar whole
/// without trimming (React `String(value)`).
#[test]
fn to_pair_preserves_react_scalar_and_first_comma_rules() {
    assert_eq!(to_pair(&FilterValue::Single("  x  ".to_owned())), ("  x  ".to_owned(), String::new()));
    assert_eq!(
        to_pair(&FilterValue::Single("10,20,30".to_owned())),
        ("10".to_owned(), "20,30".to_owned())
    );
    assert_eq!(
        to_pair(&FilterValue::Single(" , ".to_owned())),
        (String::new(), String::new())
    );
}

/// React's array path keeps every entry (including empty strings); the comma
/// string path trims and drops empties.
#[test]
fn to_tokens_keeps_array_entries_and_splits_strings() {
    assert_eq!(
        to_tokens(&FilterValue::Tokens(vec![String::new(), "b".to_owned()])),
        vec![String::new(), "b".to_owned()]
    );
    assert_eq!(
        to_tokens(&FilterValue::Pair(String::new(), "hi".to_owned())),
        vec![String::new(), "hi".to_owned()]
    );
    assert_eq!(to_tokens(&FilterValue::Single("123".to_owned())), vec!["123".to_owned()]);
    assert_eq!(to_tokens(&FilterValue::Single(",".to_owned())), Vec::<String>::new());
}

/// A `none`-kind operator ignores its value entirely (React returns early).
#[test]
fn none_kind_ignores_the_value_shape() {
    let with_tokens = FilterCondition::new("age", "isNull", FilterValue::Tokens(vec!["x".to_owned()]));
    assert_eq!(condition_summary(&with_tokens), format!("age {}", t("filter.isNull")));
    let with_pair = FilterCondition::new("age", "isNotNull", FilterValue::Pair("1".to_owned(), "2".to_owned()));
    assert_eq!(condition_summary(&with_pair), format!("age {}", t("filter.isNotNull")));
}

/// The typed literal `NULL` is a value the user typed and is not elided; only
/// the empty string becomes an ellipsis.
#[test]
fn literal_null_text_is_not_elided() {
    let value = FilterCondition::new("name", "eq", FilterValue::Single("NULL".to_owned()));
    assert_eq!(condition_summary(&value), format!("name {} NULL", t("filter.eq")));
    let empty = FilterCondition::new("name", "eq", FilterValue::Single(String::new()));
    assert!(condition_summary(&empty).ends_with('…'));
}

/// Unknown operators echo the exact i18n key, including unusual characters.
#[test]
fn op_label_builds_the_exact_key_without_invention() {
    assert_eq!(op_label("legacy/op"), "filter.legacy/op");
    assert_eq!(op_label("eq"), t("filter.eq"));
    assert_eq!(FilterColumn::new("c", None).data_type, None);
}

/// Source guards: the single definitions are the real ones, and the prior
/// tracks' test files still carry their key assertions (not deleted/weakened).
#[test]
fn no_duplicate_definitions_and_prior_tests_intact() {
    let chip = include_str!("../components/filter/filter_condition_chip.rs");
    let editor = include_str!("../components/filter/filter_condition_editor.rs");
    let values = include_str!("../components/filter/filter_values.rs");
    assert!(!editor.contains("struct FilterColumn"), "T056 local FilterColumn gone");
    assert!(
        chip.contains("super::filter_labels::condition_summary(filter)"),
        "chip delegates to the single definition"
    );
    assert!(
        !chip.contains("format!(\"filter.{}\""),
        "chip no longer carries inline label logic"
    );
    assert_eq!(
        values.matches("pub(crate) fn to_tokens").count(),
        1,
        "one definition of to_tokens"
    );
    assert_eq!(
        values.matches("pub(crate) fn to_pair").count(),
        1,
        "one definition of to_pair"
    );

    let token_tests = include_str!("filter_token_value_input.rs");
    assert!(
        token_tests.contains("fn value_input_keeps_its_key_placeholder_and_commits_on_enter"),
        "T166 value-input test retained"
    );
    let editor_tests = include_str!("filter_condition_editor.rs");
    assert!(
        editor_tests.contains("T056: `components/filter/FilterConditionEditor.tsx`"),
        "T056 editor test file retained"
    );
}
