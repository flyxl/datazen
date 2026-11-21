use serde_json::json;

use super::*;
use crate::{CellType, FilterOperator, SHARED_FILTER_OPERATORS};

fn all_supported() -> Vec<FilterOperator> {
    ALL_OPERATORS.to_vec()
}

#[test]
fn all_operators_and_wire_literals_are_pinned() {
    let literals = [
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "between",
        "notBetween",
        "like",
        "contains",
        "notContains",
        "startsWith",
        "endsWith",
        "in",
        "notIn",
        "isNull",
        "isNotNull",
        "isEmpty",
        "isNotEmpty",
        "regex",
        "notRegex",
        "jsonContains",
    ];
    assert_eq!(ALL_OPERATORS.len(), 22);
    assert_eq!(SHARED_FILTER_OPERATORS.len(), 19);
    for (operator, literal) in ALL_OPERATORS.iter().zip(literals) {
        assert_eq!(operator.as_str(), literal);
    }
}

#[test]
fn shipped_order_is_frozen_and_enum_uuid_have_no_like() {
    let shipped = [
        FilterOperator::Eq,
        FilterOperator::Ne,
        FilterOperator::Gt,
        FilterOperator::Lt,
        FilterOperator::Gte,
        FilterOperator::Lte,
        FilterOperator::Like,
        FilterOperator::In,
        FilterOperator::IsNull,
        FilterOperator::IsNotNull,
    ];
    let text = operators_by_cell_type(CellType::Text);
    let positions: Vec<_> = shipped
        .iter()
        .map(|op| text.iter().position(|c| c == op).unwrap())
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(text, operators_by_cell_type(CellType::Unknown));
    assert!(!operators_by_cell_type(CellType::Enum).contains(&FilterOperator::Like));
    assert!(!operators_by_cell_type(CellType::Uuid).contains(&FilterOperator::Like));
    assert!(operators_by_cell_type(CellType::Number).contains(&FilterOperator::Between));
    assert!(operators_by_cell_type(CellType::Json).contains(&FilterOperator::IsNull));
    assert_eq!(ALL_CELL_TYPES.len(), 11);
    assert_eq!(ALL_CELL_TYPES[0], CellType::Text);
    assert_eq!(ALL_CELL_TYPES[10], CellType::Unknown);
}

#[test]
fn dialect_operators_are_appended_only_for_thier_types() {
    assert_eq!(
        dialect_operators_by_cell_type(CellType::Text),
        &[FilterOperator::Regex, FilterOperator::NotRegex]
    );
    assert_eq!(
        dialect_operators_by_cell_type(CellType::Unknown),
        &[FilterOperator::Regex, FilterOperator::NotRegex]
    );
    assert_eq!(
        dialect_operators_by_cell_type(CellType::Json),
        &[FilterOperator::JsonContains]
    );
    assert!(dialect_operators_by_cell_type(CellType::Number).is_empty());
    assert!(dialect_operators_by_cell_type(CellType::Uuid).is_empty());
}

#[test]
fn cell_types_for_operator_is_the_inverse_of_the_matrix() {
    assert_eq!(
        cell_types_for_operator(FilterOperator::Like),
        vec![CellType::Text, CellType::Unknown]
    );
    assert_eq!(
        cell_types_for_operator(FilterOperator::JsonContains),
        vec![CellType::Json]
    );
    assert_eq!(
        cell_types_for_operator(FilterOperator::Regex),
        vec![CellType::Text, CellType::Unknown]
    );
    assert_eq!(cell_types_for_operator(FilterOperator::Eq).len(), 11);
    assert!(cell_types_for_operator(FilterOperator::Between).contains(&CellType::Date));
    assert!(!cell_types_for_operator(FilterOperator::Between).contains(&CellType::Bool));
}

#[test]
fn meta_table_is_complete_and_derived() {
    let metas = filter_operator_metas();
    assert_eq!(metas.len(), 22);
    for item in &metas {
        assert_eq!(item.cell_types, cell_types_for_operator(item.id));
        assert_eq!(item.dialect, item.group == FilterOperatorGroup::Dialect);
        assert_eq!(&operator_meta(item.id), item);
    }
    assert_eq!(
        operator_meta(FilterOperator::Eq).value_input,
        FilterValueInputKind::Single
    );
    assert_eq!(
        operator_meta(FilterOperator::Between).value_input,
        FilterValueInputKind::Pair
    );
    assert_eq!(
        operator_meta(FilterOperator::In).value_input,
        FilterValueInputKind::Tokens
    );
    assert_eq!(
        operator_meta(FilterOperator::IsNull).value_input,
        FilterValueInputKind::None
    );
    assert!(operator_meta(FilterOperator::Like).escapes_like_pattern == false);
    assert!(operator_meta(FilterOperator::Contains).escapes_like_pattern);
    assert_eq!(
        operator_meta(FilterOperator::Regex).group,
        FilterOperatorGroup::Dialect
    );
    assert_eq!(FilterValueInputKind::Tokens.as_str(), "tokens");
    assert_eq!(FilterOperatorGroup::Pattern.as_str(), "pattern");
}

#[test]
fn empty_driver_declaration_means_shared_operators() {
    assert_eq!(operators_for_driver(None), SHARED_FILTER_OPERATORS);
    assert_eq!(operators_for_driver(Some(&[])), SHARED_FILTER_OPERATORS);
    let declared = [FilterOperator::Regex];
    assert_eq!(operators_for_driver(Some(&declared)), &declared);
}

#[test]
fn driver_metadata_seams_default_to_shared_and_false() {
    assert_eq!(supported_operators_for_meta(None), SHARED_FILTER_OPERATORS);
    assert_eq!(
        supported_operators_for_meta(Some(&FilterDriverMeta::default())),
        SHARED_FILTER_OPERATORS
    );
    assert!(!like_case_insensitive_for_meta(None));
    assert!(!like_case_insensitive_for_meta(Some(
        &FilterDriverMeta::default()
    )));
    let meta = FilterDriverMeta {
        filter_operators: Some(vec![FilterOperator::Eq]),
        like_case_insensitive: true,
    };
    assert_eq!(
        supported_operators_for_meta(Some(&meta)),
        &[FilterOperator::Eq]
    );
    assert!(like_case_insensitive_for_meta(Some(&meta)));
}

#[test]
fn column_operators_are_the_type_and_driver_intersection_in_order() {
    let supported = all_supported();
    let text = operators_for_column(Some("varchar(20)"), &supported, None);
    assert!(text.contains(&FilterOperator::Like));
    assert!(text.contains(&FilterOperator::Regex));
    assert!(!text.contains(&FilterOperator::Between));

    let only_eq = [FilterOperator::Eq, FilterOperator::Like];
    assert_eq!(
        operators_for_column(Some("varchar"), &only_eq, None),
        &only_eq
    );
    let no_match = [FilterOperator::Between];
    assert!(operators_for_column(Some("uuid"), &no_match, None).is_empty());
}

#[test]
fn value_input_has_a_single_fallback_for_unknown_operators() {
    assert_eq!(
        value_input_for(FilterOperator::Like),
        FilterValueInputKind::Single
    );
    assert_eq!(
        value_input_for_literal("notBetween"),
        FilterValueInputKind::Pair
    );
    assert_eq!(
        value_input_for_literal("legacyUnknown"),
        FilterValueInputKind::Single
    );
}

#[test]
fn operator_normalization_keeps_keeps_value_and_falls_back() {
    let supported = all_supported();
    assert_eq!(
        normalize_operator_for_column(FilterOperator::Like, Some("text"), &supported, None),
        FilterOperator::Like
    );
    assert_eq!(
        normalize_operator_for_column(FilterOperator::Like, Some("enum"), &supported, None),
        FilterOperator::Eq
    );
    assert_eq!(
        normalize_operator_for_column(
            FilterOperator::Like,
            Some("enum"),
            &[FilterOperator::Ne],
            None
        ),
        FilterOperator::Ne
    );
    assert_eq!(
        normalize_operator_for_column(FilterOperator::Like, Some("enum"), &[], None),
        FilterOperator::Like
    );
    for data_type in [
        "text", "enum", "number", "bool", "date", "json", "bytes", "uuid", "weird",
    ] {
        assert_eq!(
            default_operator_for_column(Some(data_type), None),
            FilterOperator::Eq,
            "{data_type}"
        );
    }
    assert_eq!(default_operator_for_column(None, None), FilterOperator::Eq);
}

#[test]
fn conversion_identity_and_none_preserve_the_value() {
    let value = Some(json!("keep"));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Single,
            FilterValueInputKind::Single,
            &value
        ),
        value
    );
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Tokens,
            FilterValueInputKind::None,
            &value
        ),
        value
    );
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::None,
            FilterValueInputKind::Pair,
            &value
        ),
        value
    );
}

#[test]
fn conversion_to_tokens_splits_and_drops_empty_pair_parts() {
    let single = Some(json!(" a, b ,, c "));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Single,
            FilterValueInputKind::Tokens,
            &single
        ),
        Some(json!(["a", "b", "c"]))
    );
    let pair = Some(json!(["low", ""]));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Pair,
            FilterValueInputKind::Tokens,
            &pair
        ),
        Some(json!(["low"]))
    );
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Single,
            FilterValueInputKind::Tokens,
            &None
        ),
        Some(json!([]))
    );
}

#[test]
fn conversion_to_pair_pads_and_takes_two_tokens() {
    let single = Some(json!("low,high"));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Single,
            FilterValueInputKind::Pair,
            &single
        ),
        Some(json!(["low", ""]))
    );
    let tokens = Some(json!(["a", "b", "c"]));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Tokens,
            FilterValueInputKind::Pair,
            &tokens
        ),
        Some(json!(["a", "b"]))
    );
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Tokens,
            FilterValueInputKind::Pair,
            &Some(json!(["only"]))
        ),
        Some(json!(["only", ""]))
    );
}

#[test]
fn conversion_to_single_joins_or_takes_the_lower_bound() {
    let tokens = Some(json!(["a", "b"]));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Tokens,
            FilterValueInputKind::Single,
            &tokens
        ),
        Some(json!("a, b"))
    );
    let pair = Some(json!(["low", "high"]));
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Pair,
            FilterValueInputKind::Single,
            &pair
        ),
        Some(json!("low"))
    );
    assert_eq!(
        convert_filter_value(
            FilterValueInputKind::Single,
            FilterValueInputKind::Pair,
            &Some(json!({"x": 1}))
        ),
        Some(json!(["", ""]))
    );
}
