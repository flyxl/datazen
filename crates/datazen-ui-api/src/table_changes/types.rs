use std::collections::HashMap;

use crate::{Counter, Value};

pub type RowIdentity = HashMap<String, Value>;
pub type ColumnValues = HashMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Eq)]
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
pub struct TableChangeContext {
    pub connection_id: Option<String>,
    pub db_session_id: String,
    pub driver_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingRowChange {
    pub row_index: Option<i64>,
    pub row_identity: RowIdentity,
    pub original_values: ColumnValues,
    pub current_values: ColumnValues,
    pub changed_columns: Vec<String>,
    pub delete_marked: bool,
    pub insert_marked: Option<bool>,
    pub expression_values: Option<HashMap<String, WriteExpressionId>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingInsert {
    pub draft_id: String,
    pub values: ColumnValues,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedStatement {
    pub row_identity: RowIdentity,
    pub original_values: ColumnValues,
    pub current_values: ColumnValues,
    pub changed_columns: Vec<String>,
    pub expression_values: Option<HashMap<String, WriteExpressionId>>,
    pub sql_template: String,
    pub parameter_summary: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeSeverity {
    Info,
    Warning,
    Error,
}

impl ChangeSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeWarning {
    pub code: String,
    pub message: String,
    pub severity: ChangeSeverity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowChangePlan {
    pub plan_id: String,
    pub fingerprint: String,
    pub config_revision: Option<Option<Counter>>,
    pub table: TableChangeContext,
    pub updates: Vec<PlannedStatement>,
    pub deletes: Vec<PlannedStatement>,
    pub inserts: Vec<PlannedStatement>,
    pub warnings: Vec<ChangeWarning>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitOperation {
    Insert,
    Update,
    Delete,
}

impl CommitOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitStatementResult {
    pub operation: CommitOperation,
    pub row_identity: RowIdentity,
    pub affected_rows: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitPendingChangesResponse {
    pub plan_id: String,
    pub fingerprint: String,
    pub statements: Vec<CommitStatementResult>,
    pub affected_rows: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingStatus {
    Idle,
    Previewing,
    Committing,
}

impl PendingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Previewing => "previewing",
            Self::Committing => "committing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertLocation {
    Located,
    Unavailable,
    NotApplicable,
}

impl InsertLocation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Located => "located",
            Self::Unavailable => "unavailable",
            Self::NotApplicable => "not-applicable",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitPendingChangesResult {
    pub plan_id: String,
    pub fingerprint: String,
    pub statements: Vec<CommitStatementResult>,
    pub affected_rows: i64,
    pub status: CommitStatus,
    pub refreshed: bool,
    pub refresh_required: bool,
    pub refresh_status: Option<RefreshStatus>,
    pub error: Option<String>,
    pub insert_location: Option<InsertLocation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitStatus {
    Committed,
    Failed,
    Noop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshStatus {
    Completed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_literals_are_pinned() {
        assert_eq!(
            WriteExpressionId::CurrentTimestamp.as_str(),
            "currentTimestamp"
        );
        assert_eq!(ChangeSeverity::Info.as_str(), "info");
        assert_eq!(ChangeSeverity::Warning.as_str(), "warning");
        assert_eq!(ChangeSeverity::Error.as_str(), "error");
        assert_eq!(CommitOperation::Insert.as_str(), "insert");
        assert_eq!(CommitOperation::Update.as_str(), "update");
        assert_eq!(CommitOperation::Delete.as_str(), "delete");
        assert_eq!(PendingStatus::Idle.as_str(), "idle");
        assert_eq!(PendingStatus::Previewing.as_str(), "previewing");
        assert_eq!(PendingStatus::Committing.as_str(), "committing");
        assert_eq!(InsertLocation::Located.as_str(), "located");
        assert_eq!(InsertLocation::Unavailable.as_str(), "unavailable");
        assert_eq!(InsertLocation::NotApplicable.as_str(), "not-applicable");
    }
}
