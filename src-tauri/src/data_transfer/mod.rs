//! Data Transfer domain (one-way copy / migration).

pub mod error;
pub mod execute;
pub mod mapping;
pub mod model;
pub mod pairing;
pub mod preview;
pub mod structure;

pub use error::TransferError;
pub use execute::{
    active_column_mappings, build_batch_insert_sql, build_batch_insert_sql_ir, build_insert_sql,
    execute_same_family_data, execute_transfer_data, is_self_table_overwrite, map_row_values,
    DropCreateContext, ValueFormatter,
};
pub use mapping::{auto_map_columns, effective_table_mappings, inspect_tables};
pub use model::{
    ColumnMapping, DdlPreviewItem, Endpoint, TableExecutionResult, TableInspectResult,
    TableMapping, TableMappingStatus, TransferExecutionResult, TransferJob, TransferMode,
    TransferOptions, TransferPairingView, TransferPreview, WriteMode, WritePlanItem,
};
pub use pairing::{classify_transfer_pair, enforce_transfer_pairing, is_same_family};
pub use preview::{build_create_ddl, build_preview, TransferPreviewAdapters};
pub use structure::{
    build_drop_table_sql, column_ir_types_by_source, create_target_tables, drop_and_recreate_table,
    fetch_full_column_types, source_schema_to_target_ir, table_eligible_for_data,
};
