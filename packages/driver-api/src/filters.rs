//! Shared filter DTOs for table data APIs.
//!
//! These shapes are part of the IPC contract; they live in driver-api so the
//! migration domain crates can validate filters without host services.

use crate::Value;

/// Single filter for table data APIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    #[serde(default)]
    pub value: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
}
