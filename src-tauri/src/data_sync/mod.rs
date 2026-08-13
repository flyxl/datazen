//! Navicat-style Data Synchronization domain.
//!
//! Compare → Review → ChangeSet → SQL Preview → Execute.
//! Does not perform Transfer (heterogeneous copy) or Structure Sync.

pub mod changeset;
pub mod error;
pub mod model;
pub mod session;
pub mod state;

pub use changeset::{ChangeSet, TableChangeSet};
pub use error::DataSyncError;
pub use model::{
    keys_equal, optional_values_equal, rows_equal, values_equal, ChangeOperation, ColumnMapping,
    ComparisonResult, Endpoint, LargeValueMode, MatchingStrategy, Row, RowChange, SyncOptions,
    SyncTask, TableMapping, TableMappingStatus, TableResult,
};
pub use session::SyncSession;
pub use state::SyncPhase;
