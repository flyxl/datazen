//! Navicat-style Data Synchronization domain.
//!
//! D5（2026-10-26）自独立 crate `datazen-data-sync` 整体归位；旧 crate 已在
//! D8 退役，本模块保留唯一实现。
//!
//! Compare → Review → ChangeSet → SQL Preview → Execute.
//! Does not perform Transfer (heterogeneous copy) or Structure Sync.

pub mod api;
pub mod apply_loop;
pub mod changeset;
pub mod compare;
pub mod comparison_store;
pub mod error;
pub mod execute;
pub mod filter;
pub mod filter_validation;
mod filter_values;
pub mod gate;
pub mod job;
pub mod keyset;
pub mod legacy;
pub mod mapping;
pub mod model;
pub mod pairing;
pub mod plan_store;
pub mod profile;
mod recordset;
pub use crate::migration_common::recordset_bounds;
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

// D5 起驱动注册的 force-link：集成测试（cm43…cm46）在旧 crate 垫片的
// tests/ 下经其 dev-deps 持有驱动；而本模块的内联单元测试（家族归一化
// 判定等）在 modules 的 lib test 二进制里运行，同样需要工厂可查——
// 这里经 [dev-dependencies] 持有驱动并以 extern crate 强制链接
// （cargo 允许 dev-dep 成环：drivers → schema-diff → modules）。
#[cfg(test)]
extern crate datazen_driver_mysql;
#[cfg(test)]
extern crate datazen_driver_postgres;
#[cfg(test)]
extern crate datazen_driver_sqlite;
