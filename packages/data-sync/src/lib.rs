//! Navicat-style Data Synchronization domain.
//!
//! Compare → Review → ChangeSet → SQL Preview → Execute.
//! Does not perform Transfer (heterogeneous copy) or Structure Sync.

pub mod apply_loop;
pub mod changeset;
pub mod compare;
pub mod error;
pub mod execute;
pub mod filter;
mod filter_values;
pub mod gate;
pub mod job;
pub mod keyset;
pub mod legacy;
pub mod mapping;
pub mod model;
pub mod pairing;
pub mod profile;
mod recordset;
pub use datazen_migration_common::recordset_bounds;
pub mod session;
pub mod sql;
pub mod state;
pub mod sync_pairing;
pub mod types_eq;

pub use apply_loop::{apply_changeset_to_rows, remaining_mutating_changes};
pub use changeset::{ChangeSet, TableChangeSet};
pub use compare::{
    cmp_keys, cmp_values, compare_sorted_rows, compare_table_pages, compare_table_pages_to_sink,
    RowChangeSink, RowPageSource, SliceRowSource,
};
pub use error::DataSyncError;
pub use execute::{
    execute_statement_batches_with_policy, execute_statements, execute_statements_with_policy,
    DataSyncExecutionResponse, ExecutionOutcome, ExecutionResult, StatementBatchSource,
    StatementExecutor, SyncConflict,
};
pub use filter::{SyncFilterLogic, SyncSourceFilter};
pub use gate::{check_table_gate, CompatCode, CompatIssue, GateVerdict};
pub use keyset::{
    build_keyset_select_sql, build_keyset_select_sql_with_order,
    build_keyset_select_sql_with_order_and_filter,
    build_keyset_select_sql_with_order_filter_and_pagination, keyset_seek_parameter_count,
};
pub use legacy::{
    is_overwrite_copy_retired_message, refuse_overwrite_copy, OVERWRITE_COPY_RETIRED,
};
pub use mapping::classify_tables;
pub use model::{
    keys_equal, optional_values_equal, rows_equal, values_equal, ChangeOperation, ColumnMapping,
    ComparisonResult, ConflictPolicy, Endpoint, LargeValueMode, MatchingStrategy, Row, RowChange,
    SyncOptions, SyncTask, TableMapping, TableMappingStatus, TableResult,
};
pub use pairing::{classify_data_sync_pair, require_data_sync_family, DataSyncPairingView};
pub use profile::SyncProfile;
pub use recordset::{
    SyncRecordset, SyncRecordsetBound, SyncRecordsetTupleBound, SyncRecordsetTupleRange,
};
pub use session::SyncSession;
pub use sql::{
    generate_table_sql, generate_table_sql_with_preview_formatter,
    generate_table_sql_with_preview_formatter_and_policy,
    generate_table_sql_with_qualified_table_and_policy, mysql_placeholder, postgres_placeholder,
    postgres_typed_placeholder, quote_ident_sql, IdentityInsertTarget, SqlStatement,
};
pub use state::SyncPhase;

// Force-link path drivers so their inventory registrations take effect when
// running this crate's own tests.
#[cfg(test)]
extern crate datazen_driver_mysql;
#[cfg(test)]
extern crate datazen_driver_postgres;
#[cfg(test)]
extern crate datazen_driver_sqlite;
