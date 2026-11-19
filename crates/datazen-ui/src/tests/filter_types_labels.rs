//! T216: the real `filterValues` / `FilterColumn` / `filterLabels` contracts.
//!
//! Boundary cases are pinned against React's `toTokens` / `toPair` and the
//! `conditionSummary` cases from `filterOperatorResolution.test.tsx`. Source
//! guards make sure the T056 local stand-ins are gone, not merely shadowed.

use crate::components::filter::{
    condition_summary, op_label, to_pair, to_tokens, FilterColumn, FilterCondition, FilterValue,
};
use crate::i18n::t;

#[test]
fn to_tokens_matches_react_boundaries() {
    assert_eq!(to_tokens(&FilterValue::None), Vec::<String>::new(), "null → []");
    assert_eq!(to_tokens(&FilterValue::Single(String::new())), Vec::<String>::new());
    assert_eq!(to_tokens(&FilterValue::Single("   ".to_owned())), Vec::<String>::new());
    assert_eq!(
        to_tokens(&FilterValue::Single("paid, open, ".to_owned())),
        vec!["paid".to_owned(), "open".to_owned()],
        "comma string → trimmed, non-empty parts"
    );
    assert_eq!(
        to_tokens(&FilterValue::Tokens(vec!["a".to_owned(), String::new(), "b".to_owned()])),
        vec!["a".to_owned(), String::new(), "b".to_owned()],
        "React's array path keeps empty entries"
    );
    assert_eq!(
        to_tokens(&FilterValue::Pair("lo".to_owned(), "hi".to_owned())),
        vec!["lo".to_owned(), "hi".to_owned()]
    );
}

#[test]
fn to_pair_matches_react_boundaries() {
    assert_eq!(to_pair(&FilterValue::None), (String::new(), String::new()));
    assert_eq!(to_pair(&FilterValue::Single(String::new())), (String::new(), String::new()));
    assert_eq!(to_pair(&FilterValue::Single("x".to_owned())), ("x".to_owned(), String::new()));
    assert_eq!(
        to_pair(&FilterValue::Single("10,20".to_owned())),
        ("10".to_owned(), "20".to_owned())
    );
    assert_eq!(
        to_pair(&FilterValue::Single("10, 20, 30".to_owned())),
        ("10".to_owned(), "20, 30".to_owned()),
        "React splits on the FIRST comma only"
    );
    assert_eq!(to_pair(&FilterValue::Tokens(Vec::new())), (String::new(), String::new()));
    assert_eq!(
        to_pair(&FilterValue::Tokens(vec!["a".to_owned()])),
        ("a".to_owned(), String::new())
    );
    assert_eq!(
        to_pair(&FilterValue::Tokens(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])),
        ("a".to_owned(), "b".to_owned())
    );
    assert_eq!(
        to_pair(&FilterValue::Pair("18".to_owned(), "30".to_owned())),
        ("18".to_owned(), "30".to_owned())
    );
}

#[test]
fn filter_column_is_the_single_contract_shape() {
    let column = FilterColumn::new("name", Some("varchar(64)"));
    assert_eq!(column.name, "name");
    assert_eq!(column.data_type.as_deref(), Some("varchar(64)"));
    assert_eq!(column, FilterColumn::new("name", Some("varchar(64)")));
    assert_eq!(FilterColumn::new("age", None).data_type, None);

    // The local stand-in was removed, not duplicated under another name.
    let editor = include_str!("../components/filter/filter_condition_editor.rs");
    assert!(
        !editor.contains("struct FilterColumn"),
        "T056's local FilterColumn stand-in must be gone"
    );
    assert!(
        editor.contains("use super::filter_types::FilterColumn;"),
        "the editor imports the real contract"
    );
}

#[test]
fn op_label_uses_the_exact_i18n_key_and_echoes_unknowns() {
    assert_eq!(op_label("eq"), t("filter.eq"));
    assert_eq!(op_label("between"), t("filter.between"));
    // React's key-style mock returns the key; Rune's `t` falls back to it.
    assert_eq!(op_label("legacyOperator"), "filter.legacyOperator");
}

#[test]
fn condition_summary_matches_the_react_operator_kinds() {
    let null = FilterCondition::new("deleted_at", "isNull", FilterValue::None);
    assert_eq!(condition_summary(&null), format!("deleted_at {}", t("filter.isNull")));

    // The comma-string legacy draft must read the same as the array form.
    let pair_text = FilterCondition::new("score", "between", FilterValue::Single("10,20".to_owned()));
    let pair_array = FilterCondition::new(
        "score",
        "between",
        FilterValue::Pair("10".to_owned(), "20".to_owned()),
    );
    assert_eq!(condition_summary(&pair_text), format!("score {} 10, 20", t("filter.between")));
    assert_eq!(condition_summary(&pair_array), condition_summary(&pair_text));

    let tokens_text = FilterCondition::new("name", "in", FilterValue::Single("paid, open".to_owned()));
    let tokens_array = FilterCondition::new(
        "name",
        "in",
        FilterValue::Tokens(vec!["paid".to_owned(), "open".to_owned()]),
    );
    assert_eq!(condition_summary(&tokens_text), format!("name {} paid, open", t("filter.in")));
    assert_eq!(condition_summary(&tokens_array), condition_summary(&tokens_text));

    let empty = FilterCondition::new("name", "eq", FilterValue::Single(String::new()));
    assert!(condition_summary(&empty).ends_with('…'), "empty value is elided");
    let value = FilterCondition::new("name", "eq", FilterValue::Single("alice".to_owned()));
    assert_eq!(condition_summary(&value), format!("name {} alice", t("filter.eq")));
}

#[test]
fn no_duplicate_label_definition_and_no_invented_keys() {
    let labels = include_str!("../components/filter/filter_labels.rs");
    assert!(labels.contains("t(&format!(\"filter.{operator}\"))"), "the operator key is built from the real prefix");
    let chip = include_str!("../components/filter/filter_condition_chip.rs");
    assert!(
        chip.contains("super::filter_labels::condition_summary(filter)"),
        "the chip delegates to the single label definition"
    );
}
