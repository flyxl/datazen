//! ChangeSet is the execution input. It never includes unselected or disallowed rows.

use serde::{Deserialize, Serialize};

use super::error::DataSyncError;
use super::model::{
    ChangeOperation, ComparisonResult, RowChange, SyncOptions, TableMappingStatus, TableResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableChangeSet {
    pub source_table: String,
    pub target_table: String,
    pub changes: Vec<RowChange>,
}

impl TableChangeSet {
    pub fn insert_count(&self) -> usize {
        self.count(ChangeOperation::Insert)
    }

    pub fn update_count(&self) -> usize {
        self.count(ChangeOperation::Update)
    }

    pub fn delete_count(&self) -> usize {
        self.count(ChangeOperation::Delete)
    }

    fn count(&self, op: ChangeOperation) -> usize {
        self.changes.iter().filter(|c| c.operation == op).count()
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    pub task_id: String,
    pub tables: Vec<TableChangeSet>,
}

impl ChangeSet {
    pub fn empty(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            tables: Vec::new(),
        }
    }

    /// Build from a comparison: only selected + option-allowed mutating rows.
    /// Incompatible / disabled / unmapped tables contribute nothing.
    pub fn from_comparison(
        task_id: impl Into<String>,
        comparison: &ComparisonResult,
        options: &SyncOptions,
    ) -> Self {
        let tables = comparison
            .tables
            .iter()
            .filter(|t| t.status == TableMappingStatus::Matched)
            .filter_map(|t| table_changeset(t, options))
            .collect();
        Self {
            task_id: task_id.into(),
            tables,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tables.iter().all(TableChangeSet::is_empty)
    }

    pub fn insert_count(&self) -> usize {
        self.tables.iter().map(TableChangeSet::insert_count).sum()
    }

    pub fn update_count(&self) -> usize {
        self.tables.iter().map(TableChangeSet::update_count).sum()
    }

    pub fn delete_count(&self) -> usize {
        self.tables.iter().map(TableChangeSet::delete_count).sum()
    }

    pub fn total_changes(&self) -> usize {
        self.insert_count() + self.update_count() + self.delete_count()
    }

    pub fn requires_delete_confirmation(&self) -> bool {
        self.delete_count() > 0
    }

    pub fn validate_executable(&self) -> Result<(), DataSyncError> {
        if self.is_empty() {
            return Err(DataSyncError::validation(
                "change set is empty; nothing to execute",
            ));
        }
        Ok(())
    }
}

fn table_changeset(table: &TableResult, options: &SyncOptions) -> Option<TableChangeSet> {
    let changes: Vec<RowChange> = table
        .rows
        .iter()
        .filter(|row| row.eligible_for_changeset(options))
        .cloned()
        .collect();
    if changes.is_empty() {
        return None;
    }
    Some(TableChangeSet {
        source_table: table.source_table.clone(),
        target_table: table.target_table.clone(),
        changes,
    })
}

#[cfg(test)]
mod tests {
    use datazen_driver_api::Value;

    use super::*;
    use crate::data_sync::model::{RowChange, SyncOptions, TableResult};

    fn key(n: i64) -> Vec<Value> {
        vec![Value::Integer(n)]
    }

    fn row(n: i64) -> Vec<Option<Value>> {
        vec![Some(Value::Integer(n))]
    }

    #[test]
    fn changeset_excludes_unselected_deletes_and_unchanged() {
        let opts = SyncOptions::default();
        let users = TableResult::matched(
            "users",
            "users",
            vec![
                RowChange::insert(key(1), row(1), &opts),
                RowChange::update(key(2), row(2), row(20), vec!["n".into()], &opts),
                RowChange::delete(key(3), row(3), &opts),
                RowChange::unchanged(key(4), row(4), row(4)),
            ],
        );
        let skip = TableResult::incompatible("logs", "logs", "no pk");
        let cmp = ComparisonResult::new(vec![users, skip]);
        let set = ChangeSet::from_comparison("task-1", &cmp, &opts);
        assert_eq!(set.insert_count(), 1);
        assert_eq!(set.update_count(), 1);
        assert_eq!(set.delete_count(), 0);
        assert_eq!(set.total_changes(), 2);
        assert!(!set.requires_delete_confirmation());
        set.validate_executable().unwrap();
    }

    #[test]
    fn selected_delete_enters_changeset_only_when_option_on() {
        let mut opts = SyncOptions::default();
        opts.delete = true;
        let mut del = RowChange::delete(key(3), row(3), &opts);
        assert!(!del.selected);
        del.selected = true;
        let table = TableResult::matched("users", "clients", vec![del]);
        let set = ChangeSet::from_comparison("t", &ComparisonResult::new(vec![table]), &opts);
        assert_eq!(set.delete_count(), 1);
        assert!(set.requires_delete_confirmation());
        assert_eq!(set.tables[0].target_table, "clients");
    }

    #[test]
    fn empty_changeset_is_not_executable() {
        let set = ChangeSet::empty("t");
        assert!(set.is_empty());
        assert!(set.validate_executable().is_err());
        let opts = SyncOptions::default();
        let table = TableResult::matched(
            "users",
            "users",
            vec![RowChange::unchanged(key(1), row(1), row(1))],
        );
        let set = ChangeSet::from_comparison("t", &ComparisonResult::new(vec![table]), &opts);
        assert!(set.is_empty());
        assert!(set.validate_executable().is_err());
    }

    #[test]
    fn unselected_insert_is_dropped() {
        let opts = SyncOptions::default();
        let mut ins = RowChange::insert(key(1), row(1), &opts);
        ins.selected = false;
        let table = TableResult::matched("users", "users", vec![ins]);
        let set = ChangeSet::from_comparison("t", &ComparisonResult::new(vec![table]), &opts);
        assert!(set.is_empty());
    }
}
