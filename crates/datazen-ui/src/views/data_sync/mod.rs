//! Rune ports of `packages/frontend/src/windows/data-sync/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `DataSyncCompareStep.tsx` | `data_sync_compare_step` | T024 |
//! | `DataSyncDialogs.tsx` | `data_sync_dialogs` | T074 |
//! | `DataSyncEndpointStep.tsx` | `data_sync_endpoint_step` | T114 |

pub(crate) mod data_sync_compare_step;
pub(crate) mod data_sync_dialogs;
pub(crate) mod data_sync_endpoint_step;

#[allow(unused_imports)] // the Data Sync window track is the production consumer
pub(crate) use data_sync_endpoint_step::{DataSyncEndpointStep, DataSyncEndpointStepVm, SyncProfile};

#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub(crate) use data_sync_compare_step::{
    has_next_page, has_previous_page, has_table_selection, page_index_for, table_page_key,
    CompareStepBranch, CompareSummaryStats, DataSyncCompareStep, DataSyncCompareStepVm,
    DataSyncMappingStatus, DataSyncOperation, DataSyncPageCursor, DataSyncRowChange,
    DataSyncSelectionExclusion, DataSyncSelectionMode, DataSyncTableResult,
    DataSyncTableSelection, RowOperation, SelectedTablePage, SyncConflictPolicy,
    SyncLargeValueMode, SyncMatchingStrategy, SyncOptions, SyncState, TableDiffFilter,
};

#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub(crate) use data_sync_dialogs::{DataSyncDialogs, DataSyncDialogsVm};
