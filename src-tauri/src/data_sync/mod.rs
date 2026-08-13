//! Navicat-style Data Synchronization domain.
//!
//! Compare → Review → ChangeSet → SQL Preview → Execute.
//! Does not perform Transfer (heterogeneous copy) or Structure Sync.

pub mod changeset;
pub mod error;
pub mod gate;
pub mod mapping;
pub mod model;
pub mod pairing;
pub mod session;
pub mod state;
pub mod types_eq;

pub use changeset::{ChangeSet, TableChangeSet};
pub use error::DataSyncError;
pub use gate::{check_table_gate, CompatCode, CompatIssue, GateVerdict};
pub use mapping::classify_tables;
pub use model::{
    keys_equal, optional_values_equal, rows_equal, values_equal, ChangeOperation, ColumnMapping,
    ComparisonResult, Endpoint, LargeValueMode, MatchingStrategy, Row, RowChange, SyncOptions,
    SyncTask, TableMapping, TableMappingStatus, TableResult,
};
pub use pairing::require_data_sync_family;
pub use session::SyncSession;
pub use state::SyncPhase;
