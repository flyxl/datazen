//! Navicat-style Data Synchronization domain.
//!
//! Compare → Review → ChangeSet → SQL Preview → Execute.
//! Does not perform Transfer (heterogeneous copy) or Structure Sync.

pub mod apply_loop;
pub mod changeset;
pub mod compare;
pub mod error;
pub mod execute;
pub mod gate;
pub mod legacy;
pub mod mapping;
pub mod model;
pub mod pairing;
pub mod session;
pub mod sql;
pub mod state;
pub mod types_eq;

pub use apply_loop::{apply_changeset_to_rows, remaining_mutating_changes};
pub use changeset::{ChangeSet, TableChangeSet};
pub use compare::{
    cmp_keys, cmp_values, compare_sorted_rows, compare_table_pages, RowPageSource, SliceRowSource,
};
pub use error::DataSyncError;
pub use execute::{execute_statements, ExecutionResult, StatementExecutor};
pub use gate::{check_table_gate, CompatCode, CompatIssue, GateVerdict};
pub use legacy::{
    is_overwrite_copy_retired_message, refuse_overwrite_copy, OVERWRITE_COPY_RETIRED,
};
pub use mapping::classify_tables;
pub use model::{
    keys_equal, optional_values_equal, rows_equal, values_equal, ChangeOperation, ColumnMapping,
    ComparisonResult, Endpoint, LargeValueMode, MatchingStrategy, Row, RowChange, SyncOptions,
    SyncTask, TableMapping, TableMappingStatus, TableResult,
};
pub use pairing::{classify_data_sync_pair, require_data_sync_family, DataSyncPairingView};
pub use session::SyncSession;
pub use sql::{generate_table_sql, SqlStatement};
pub use state::SyncPhase;
