//! Data Transfer domain (one-way copy / migration).

pub mod error;
pub mod execute;
pub mod filter;
pub mod job;
pub mod mapping;
pub mod metadata;
pub mod model;
pub mod pairing;
pub mod preview;
pub mod profile;
pub mod transfer;
pub mod recordset;
pub mod resume;
pub mod resume_dispatch;
mod scan;
pub mod sql_file;
mod sql_structure;
pub mod structure;
mod table_order;
pub(crate) mod writer;

pub use error::TransferError;
pub use execute::execute_transfer_data_with_resume_checkpoint;
pub use execute::execute_transfer_data_with_write_observer;
pub use execute::{execute_transfer_data, DropCreateContext, ValueFormatter};
pub use execute::{is_self_table_overwrite, validate_no_self_table_overwrite};
pub use filter::{FilterLogic, SourceFilter};
pub use mapping::inspect_tables;
pub use model::{
    DdlPreviewItem, DdlPreviewKind, SqlFileCompression, SqlFileEncoding, SqlFileTarget,
    TableInspectResult, TableMapping, TransferExecutionResult, TransferJob, TransferMode,
    TransferPairingView, TransferPreview, TransferRecordset, TransferRecordsetBound,
    TransferRecordsetTupleBound, TransferRecordsetTupleRange, TransferRunRequest, WriteMode,
};
pub use pairing::{classify_transfer_pair, enforce_transfer_pairing, is_same_family};
pub use preview::{build_preview, TransferPreviewAdapters};
pub use profile::TransferProfile;
pub use sql_structure::build_database_structure_plan;
pub use structure::create_target_tables_with_write_observer;
pub use structure::{column_ir_types_by_source, create_target_tables, source_schema_to_target_ir};
pub use table_order::{
    capture_target_fk_dependencies, order_selected_tables, reorder_inspected_tables,
    reorder_preview_write_plans, TargetTableDependency,
};

#[cfg(test)]
pub static TEST_COMMIT_ACK_LOSS_TEST_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod execution_tests;
#[cfg(test)]
mod sql_structure_fixtures;
#[cfg(test)]
mod sql_structure_object_coverage_tests;
#[cfg(test)]
mod sql_structure_object_tests;

// Force-link path drivers so their inventory registrations take effect when
// running this crate's own tests.
#[cfg(test)]
extern crate datazen_driver_mysql;
#[cfg(test)]
extern crate datazen_driver_postgres;
#[cfg(test)]
extern crate datazen_driver_sqlite;
