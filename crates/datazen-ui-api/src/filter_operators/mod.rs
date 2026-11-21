mod apply;
mod matrix;
mod tests;
mod types;

pub use apply::{
    convert_filter_value, default_operator_for_column, filter_operator_metas,
    like_case_insensitive_for_meta, normalize_operator_for_column, operator_meta,
    operators_for_column, operators_for_driver, supported_operators_for_meta, value_input_for,
    value_input_for_literal, ALL_OPERATORS,
};
pub use matrix::{
    cell_types_for_operator, dialect_operators_by_cell_type, operators_by_cell_type, ALL_CELL_TYPES,
};
pub use types::{
    meta, FilterDriverMeta, FilterOperatorGroup, FilterOperatorMeta, FilterValue,
    FilterValueInputKind,
};
