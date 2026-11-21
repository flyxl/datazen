use super::matrix::cell_types_for_operator;
use crate::CellType;
use crate::FilterOperator;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterValueInputKind {
    None,
    Single,
    Pair,
    Tokens,
}

impl FilterValueInputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Single => "single",
            Self::Pair => "pair",
            Self::Tokens => "tokens",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOperatorGroup {
    Compare,
    Pattern,
    Set,
    Null,
    Empty,
    Dialect,
}

impl FilterOperatorGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compare => "compare",
            Self::Pattern => "pattern",
            Self::Set => "set",
            Self::Null => "null",
            Self::Empty => "empty",
            Self::Dialect => "dialect",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterOperatorMeta {
    pub id: FilterOperator,
    pub value_input: FilterValueInputKind,
    pub cell_types: Vec<CellType>,
    pub dialect: bool,
    pub group: FilterOperatorGroup,
    pub escapes_like_pattern: bool,
}

pub fn meta(
    id: FilterOperator,
    value_input: FilterValueInputKind,
    group: FilterOperatorGroup,
    escapes_like_pattern: bool,
) -> FilterOperatorMeta {
    FilterOperatorMeta {
        id,
        value_input,
        cell_types: cell_types_for_operator(id),
        dialect: group == FilterOperatorGroup::Dialect,
        group,
        escapes_like_pattern,
    }
}

pub type FilterValue = Option<serde_json::Value>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterDriverMeta {
    pub filter_operators: Option<Vec<FilterOperator>>,
    pub like_case_insensitive: bool,
}
