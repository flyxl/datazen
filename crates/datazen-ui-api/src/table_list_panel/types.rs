use crate::DataSyncOperation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableDiffFilter {
    All,
    Insert,
    Update,
    Delete,
    Unchanged,
    Incompatible,
}

impl TableDiffFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Unchanged => "unchanged",
            Self::Incompatible => "incompatible",
        }
    }
}

pub const DATA_SYNC_FILTERS: [TableDiffFilter; 6] = [
    TableDiffFilter::All,
    TableDiffFilter::Insert,
    TableDiffFilter::Update,
    TableDiffFilter::Delete,
    TableDiffFilter::Unchanged,
    TableDiffFilter::Incompatible,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSyncStatus {
    Matched,
    UnmappedSource,
    UnmappedTarget,
    Disabled,
    Incompatible,
}

impl TableSyncStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "MATCHED",
            Self::UnmappedSource => "UNMAPPED_SOURCE",
            Self::UnmappedTarget => "UNMAPPED_TARGET",
            Self::Disabled => "DISABLED",
            Self::Incompatible => "INCOMPATIBLE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableListRow {
    pub source_table: String,
    pub target_table: String,
    pub status: TableSyncStatus,
    pub rows: Vec<DataSyncOperation>,
    pub insert_count: Option<usize>,
    pub update_count: Option<usize>,
    pub delete_count: Option<usize>,
    pub unchanged_count: Option<usize>,
}

impl TableListRow {
    pub fn matched(name: &str, rows: Vec<DataSyncOperation>) -> Self {
        Self {
            source_table: name.to_owned(),
            target_table: name.to_owned(),
            status: TableSyncStatus::Matched,
            rows,
            insert_count: None,
            update_count: None,
            delete_count: None,
            unchanged_count: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowDiffCounts {
    pub inserts: usize,
    pub updates: usize,
    pub deletes: usize,
    pub unchanged: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_and_statuses_pin_their_wire_literals() {
        assert_eq!(DATA_SYNC_FILTERS.len(), 6);
        assert_eq!(TableDiffFilter::All.as_str(), "all");
        assert_eq!(TableDiffFilter::Incompatible.as_str(), "incompatible");
        assert_eq!(TableSyncStatus::Matched.as_str(), "MATCHED");
        assert_eq!(TableSyncStatus::Incompatible.as_str(), "INCOMPATIBLE");
        assert_eq!(TableSyncStatus::UnmappedSource.as_str(), "UNMAPPED_SOURCE");
    }

    #[test]
    fn matched_row_defaults_have_no_explicit_counts() {
        let row = TableListRow::matched("t", Vec::new());
        assert_eq!(row.status, TableSyncStatus::Matched);
        assert_eq!(row.insert_count, None);
        assert_eq!(row.source_table, row.target_table);
    }
}
