use std::collections::BTreeMap;

use crate::Value;

pub type RowIdentity = BTreeMap<String, Value>;
pub type ValueMap = BTreeMap<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteExpressionId {
    CurrentTimestamp,
}

impl WriteExpressionId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CurrentTimestamp => "currentTimestamp",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingRowChange {
    pub row_index: Option<u64>,
    pub row_identity: RowIdentity,
    pub original_values: ValueMap,
    pub current_values: ValueMap,
    pub changed_columns: Vec<String>,
    pub delete_marked: bool,
    pub insert_marked: Option<bool>,
    pub expression_values: Option<BTreeMap<String, WriteExpressionId>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageEditInput {
    pub existing: Option<PendingRowChange>,
    pub column: String,
    pub original_value: Option<Value>,
    pub current_value: Option<Value>,
    pub row_identity: RowIdentity,
    pub row_index: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageEditResult {
    pub change: Option<PendingRowChange>,
    pub changed_columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageCellValueInput {
    pub existing: Option<PendingRowChange>,
    pub column: String,
    pub value: Option<Value>,
    pub original_value: Option<Value>,
    pub row_identity: RowIdentity,
    pub row_index: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageExpressionValueInput {
    pub existing: Option<PendingRowChange>,
    pub column: String,
    pub original_value: Option<Value>,
    pub row_identity: RowIdentity,
    pub row_index: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageCellValueResult {
    pub change: Option<PendingRowChange>,
    pub changed_columns: Vec<String>,
}
