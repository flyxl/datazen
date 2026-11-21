use crate::filter_expression::parse::parse_filter_expression;
use crate::filter_expression::types::{
    FilterExpression, FilterExpressionLogic, FilterExpressionOperator, FilterExpressionValue,
    FlattenedFilterCondition, ParseFailure, ParsedFilterApplication,
};
use crate::{FilterCondition, FilterOperator};

fn to_filter_operator(operator: FilterExpressionOperator) -> FilterOperator {
    match operator {
        FilterExpressionOperator::Eq => FilterOperator::Eq,
        FilterExpressionOperator::Ne => FilterOperator::Ne,
        FilterExpressionOperator::Gt => FilterOperator::Gt,
        FilterExpressionOperator::Gte => FilterOperator::Gte,
        FilterExpressionOperator::Lt => FilterOperator::Lt,
        FilterExpressionOperator::Lte => FilterOperator::Lte,
        FilterExpressionOperator::IsNull => FilterOperator::IsNull,
        FilterExpressionOperator::IsNotNull => FilterOperator::IsNotNull,
    }
}

fn to_filter_value(value: FilterExpressionValue) -> Option<serde_json::Value> {
    match value {
        FilterExpressionValue::Str(text) => Some(serde_json::Value::String(text)),
        FilterExpressionValue::Num(number) => Some(serde_json::json!(number)),
        FilterExpressionValue::Bool(flag) => Some(serde_json::Value::Bool(flag)),
        FilterExpressionValue::Null => Some(serde_json::Value::Null),
    }
}

pub fn filter_expression_to_conditions(expression: &FilterExpression) -> Vec<FilterCondition> {
    match expression {
        FilterExpression::Condition { column, operator, value } => vec![FilterCondition {
            column: column.clone(),
            operator: to_filter_operator(*operator),
            value: to_filter_value(value.clone()),
            connector: None,
        }],
        FilterExpression::Logical { left, right, .. } => {
            let mut out = filter_expression_to_conditions(left);
            out.extend(filter_expression_to_conditions(right));
            out
        }
    }
}

pub fn has_or_group_under_and(node: &FilterExpression) -> bool {
    let FilterExpression::Logical { operator, left, right } = node else {
        return false;
    };
    if *operator == FilterExpressionLogic::And {
        let left_is_or = matches!(&**left, FilterExpression::Logical { operator: FilterExpressionLogic::Or, .. });
        let right_is_or = matches!(&**right, FilterExpression::Logical { operator: FilterExpressionLogic::Or, .. });
        if left_is_or || right_is_or {
            return true;
        }
    }
    has_or_group_under_and(left) || has_or_group_under_and(right)
}

pub fn flatten_parts(node: &FilterExpression) -> Vec<FlattenedFilterCondition> {
    match node {
        FilterExpression::Condition { .. } => {
            vec![FlattenedFilterCondition { condition: node.clone(), connector: None }]
        }
        FilterExpression::Logical { operator, left, right } => {
            let mut parts = flatten_parts(left);
            let mut right_parts = flatten_parts(right).into_iter();
            if let Some(first) = right_parts.next() {
                parts.push(FlattenedFilterCondition {
                    condition: first.condition,
                    connector: Some(*operator),
                });
                parts.extend(right_parts);
            }
            parts
        }
    }
}

pub fn flatten_to_connectors(node: &FilterExpression) -> Option<Vec<FlattenedFilterCondition>> {
    if has_or_group_under_and(node) {
        return None;
    }
    Some(flatten_parts(node))
}

pub fn parse_filter_for_apply(
    input: &str,
    columns: &[String],
) -> Result<ParsedFilterApplication, ParseFailure> {
    let expression = parse_filter_expression(input, columns)?;
    Ok(ParsedFilterApplication {
        conditions: filter_expression_to_conditions(&expression),
        expression,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_expression::types::FilterExpressionValue as ValueKind;

    fn cond(column: &str, operator: FilterExpressionOperator, value: ValueKind) -> FilterExpression {
        FilterExpression::condition(column, operator, value)
    }

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_condition_becomes_one_condition_with_no_connector() {
        let tree = cond("a", FilterExpressionOperator::Eq, ValueKind::Str("x".to_owned()));
        assert_eq!(
            filter_expression_to_conditions(&tree),
            vec![FilterCondition {
                column: "a".to_owned(),
                operator: FilterOperator::Eq,
                value: Some(serde_json::Value::String("x".to_owned())),
                connector: None,
            }]
        );
    }

    #[test]
    fn logical_nodes_concatenate_conditions_without_connectors() {
        let tree = FilterExpression::logical(
            FilterExpressionLogic::And,
            cond("a", FilterExpressionOperator::Eq, ValueKind::Num(1.0)),
            cond("b", FilterExpressionOperator::Ne, ValueKind::Bool(true)),
        );
        let conditions = filter_expression_to_conditions(&tree);
        assert_eq!(conditions.len(), 2);
        assert_eq!(conditions[0].operator, FilterOperator::Eq);
        assert_eq!(conditions[1].operator, FilterOperator::Ne);
        assert!(conditions.iter().all(|condition| condition.connector.is_none()));
    }

    #[test]
    fn null_value_is_present_not_absent() {
        let tree = cond("a", FilterExpressionOperator::Eq, ValueKind::Null);
        assert_eq!(
            filter_expression_to_conditions(&tree)[0].value,
            Some(serde_json::Value::Null)
        );
    }

    #[test]
    fn every_operator_maps_to_the_shared_vocabulary() {
        for (operator, expected) in [
            (FilterExpressionOperator::Eq, FilterOperator::Eq),
            (FilterExpressionOperator::Ne, FilterOperator::Ne),
            (FilterExpressionOperator::Gt, FilterOperator::Gt),
            (FilterExpressionOperator::Gte, FilterOperator::Gte),
            (FilterExpressionOperator::Lt, FilterOperator::Lt),
            (FilterExpressionOperator::Lte, FilterOperator::Lte),
            (FilterExpressionOperator::IsNull, FilterOperator::IsNull),
            (FilterExpressionOperator::IsNotNull, FilterOperator::IsNotNull),
        ] {
            let tree = cond("a", operator, ValueKind::Null);
            assert_eq!(filter_expression_to_conditions(&tree)[0].operator, expected);
        }
    }

    #[test]
    fn a_single_condition_flattens_without_a_connector() {
        let tree = cond("a", FilterExpressionOperator::Eq, ValueKind::Num(1.0));
        let parts = flatten_to_connectors(&tree);
        assert_eq!(parts.as_ref().map(|parts| parts.len()), Some(1));
        let parts = parts.unwrap_or_default();
        assert_eq!(parts[0].connector, None);
    }

    #[test]
    fn or_groups_of_and_conditions_flatten_with_the_right_connectors() {
        let parsed = parse_filter_expression(
            "a = 1 AND b = 2 OR c = 3",
            &cols(&["a", "b", "c"]),
        );
        let parts = flatten_to_connectors(&parsed.unwrap_or_else(|_| panic!("parse failed")));
        let parts = parts.unwrap_or_default();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].connector, None);
        assert_eq!(parts[1].connector, Some(FilterExpressionLogic::And));
        assert_eq!(parts[2].connector, Some(FilterExpressionLogic::Or));
    }

    #[test]
    fn an_or_group_under_and_is_refused_without_guessing() {
        let parsed = parse_filter_expression(
            "a = 1 AND (b = 2 OR c = 3)",
            &cols(&["a", "b", "c"]),
        );
        let tree = parsed.unwrap_or_else(|_| panic!("parse failed"));
        assert!(has_or_group_under_and(&tree));
        assert_eq!(flatten_to_connectors(&tree), None);
    }

    #[test]
    fn nested_or_groups_under_and_are_detected_recursively() {
        let parsed = parse_filter_expression(
            "a = 1 AND b = 2 AND (c = 3 OR d = 4)",
            &cols(&["a", "b", "c", "d"]),
        );
        let tree = parsed.unwrap_or_else(|_| panic!("parse failed"));
        assert_eq!(flatten_to_connectors(&tree), None);
    }

    #[test]
    fn parse_for_apply_carries_both_the_ast_and_the_conditions() {
        let applied = parse_filter_for_apply("a = 1 AND b IS NULL", &cols(&["a", "b"]));
        let applied = applied.unwrap_or_else(|_| panic!("parse failed"));
        assert!(matches!(applied.expression, FilterExpression::Logical { .. }));
        assert_eq!(applied.conditions.len(), 2);
        assert_eq!(applied.conditions[1].operator, FilterOperator::IsNull);
        assert_eq!(applied.conditions[1].value, Some(serde_json::Value::Null));
        assert!(applied.conditions.iter().all(|condition| condition.connector.is_none()));
    }

    #[test]
    fn parse_for_apply_passes_failures_through() {
        let applied = parse_filter_for_apply("zz = 1", &cols(&["a"]));
        assert_eq!(applied.err(), Some(ParseFailure::new("Unknown filter column: zz.", 0)));
    }
}
