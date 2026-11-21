#[derive(Clone, Debug, PartialEq)]
pub enum FilterExpressionValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterExpressionOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    IsNull,
    IsNotNull,
}

impl FilterExpressionOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
            Self::IsNull => "isNull",
            Self::IsNotNull => "isNotNull",
        }
    }

    pub fn is_comparison(self) -> bool {
        !matches!(self, Self::IsNull | Self::IsNotNull)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterExpressionLogic {
    And,
    Or,
}

impl FilterExpressionLogic {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilterExpression {
    Condition {
        column: String,
        operator: FilterExpressionOperator,
        value: FilterExpressionValue,
    },
    Logical {
        operator: FilterExpressionLogic,
        left: Box<FilterExpression>,
        right: Box<FilterExpression>,
    },
}

impl FilterExpression {
    pub fn condition(column: &str, operator: FilterExpressionOperator, value: FilterExpressionValue) -> Self {
        Self::Condition {
            column: column.to_owned(),
            operator,
            value,
        }
    }

    pub fn logical(operator: FilterExpressionLogic, left: Self, right: Self) -> Self {
        Self::Logical {
            operator,
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseFailure {
    pub error: String,
    pub position: usize,
}

impl ParseFailure {
    pub fn new(error: &str, position: usize) -> Self {
        Self {
            error: error.to_owned(),
            position,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlattenedFilterCondition {
    pub condition: FilterExpression,
    pub connector: Option<FilterExpressionLogic>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedFilterApplication {
    pub expression: FilterExpression,
    pub conditions: Vec<crate::FilterCondition>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_literals_and_the_comparison_split() {
        for (operator, literal) in [
            (FilterExpressionOperator::Eq, "eq"),
            (FilterExpressionOperator::Ne, "ne"),
            (FilterExpressionOperator::Gt, "gt"),
            (FilterExpressionOperator::Gte, "gte"),
            (FilterExpressionOperator::Lt, "lt"),
            (FilterExpressionOperator::Lte, "lte"),
            (FilterExpressionOperator::IsNull, "isNull"),
            (FilterExpressionOperator::IsNotNull, "isNotNull"),
        ] {
            assert_eq!(operator.as_str(), literal);
        }
        assert!(FilterExpressionOperator::Eq.is_comparison());
        assert!(!FilterExpressionOperator::IsNull.is_comparison());
        assert!(!FilterExpressionOperator::IsNotNull.is_comparison());
    }

    #[test]
    fn logic_literals() {
        assert_eq!(FilterExpressionLogic::And.as_str(), "and");
        assert_eq!(FilterExpressionLogic::Or.as_str(), "or");
    }

    #[test]
    fn expression_constructors_box_both_children() {
        let left = FilterExpression::condition("a", FilterExpressionOperator::Eq, FilterExpressionValue::Null);
        let right = FilterExpression::condition("b", FilterExpressionOperator::IsNotNull, FilterExpressionValue::Null);
        let tree = FilterExpression::logical(FilterExpressionLogic::And, left, right);
        match tree {
            FilterExpression::Logical { left, right, .. } => {
                assert!(matches!(*left, FilterExpression::Condition { .. }));
                assert!(matches!(*right, FilterExpression::Condition { .. }));
            }
            _ => panic!("expected a logical node"),
        }
    }

    #[test]
    fn failures_carry_message_and_position() {
        let failure = ParseFailure::new("bad", 7);
        assert_eq!(failure.error, "bad");
        assert_eq!(failure.position, 7);
    }
}
