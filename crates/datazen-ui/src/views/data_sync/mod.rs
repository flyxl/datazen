//! Rune ports of `packages/frontend/src/windows/data-sync/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `DataSyncCompareStep.tsx` | `data_sync_compare_step` | T024 |
//! | `DataSyncDialogs.tsx` | `data_sync_dialogs` | T074 |
//! | `DataSyncEndpointStep.tsx` | `data_sync_endpoint_step` | T114 |
//! | `DataSyncJobStatusPanel.tsx` | `data_sync_job_status_panel` | T148 |
//! | `RecordsetEditor.tsx` | `recordset_editor` (SyncRecordsetEditor) | T360 |
//! | `useDataSyncFeedback.ts` | `use_data_sync_feedback` | T459 |
//! | `DataSyncWindow.tsx` — state model (the composition seams) | `data_sync_window_state` | T268-S1 |
//! | `DataSyncWindow.tsx` — rendering | `data_sync_window` | T268-S2 |

pub mod data_sync_compare_step;
pub mod data_sync_dialogs;
pub mod data_sync_endpoint_step;
pub mod data_sync_job_status_panel;
pub mod client_sql_preview;
pub mod data_sync_options_step;
pub mod options_bar;
pub mod recordset_editor;
pub mod sql_preview;
pub mod use_data_sync_feedback;
pub mod utils;
pub mod mapping_view;
pub mod use_data_sync_wizard_state;
pub mod mapping_panel;
pub mod data_sync_objects_step;
pub mod compare_summary;
pub mod data_sync_result_step;
pub mod data_sync_preview_step;
pub mod data_sync_wizard_nav;
pub mod data_sync_window;
pub mod data_sync_window_state;

#[allow(unused_imports)] // the Data Sync window track is the production consumer
pub use data_sync_endpoint_step::{DataSyncEndpointStep, DataSyncEndpointStepVm, SyncProfile};

#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub use data_sync_compare_step::{
    has_next_page, has_previous_page, has_table_selection, page_index_for, table_page_key,
    CompareStepBranch, CompareSummaryStats, DataSyncCompareStep, DataSyncCompareStepVm,
    DataSyncMappingStatus, DataSyncOperation, DataSyncPageCursor, DataSyncRowChange,
    DataSyncSelectionExclusion, DataSyncSelectionMode, DataSyncTableResult,
    DataSyncTableSelection, RowOperation, SelectedTablePage, SyncConflictPolicy,
    SyncLargeValueMode, SyncMatchingStrategy, SyncOptions, SyncState, TableDiffFilter,
};

#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub use data_sync_dialogs::{DataSyncDialogs, DataSyncDialogsVm};

#[allow(unused_imports)] // the Data Sync window (T268) is the production consumer
pub use data_sync_job_status_panel::{
    committed_result, durable_job_state_label, js_number, js_number_to_string,
    DataSyncJobDetailsVm, DataSyncJobStatusPanel, DataSyncJobStatusPanelVm, DomainCounter,
    DurableJobView, JobRecoveryVerdict,
};

#[allow(unused_imports)] // the Data Sync window (T268) is the production consumer
pub use data_sync_options_step::DataSyncOptionsStep;

#[allow(unused_imports)] // the preview step (T226/T371) is the production consumer
pub use client_sql_preview::{
    build_client_sql_preview, filter_statements_by_op, statements_to_preview_text,
    DataSyncSqlStatement, OpFilter,
};

#[allow(unused_imports)] // the preview step (T226) is the production consumer
pub use sql_preview::{SqlPreview, SqlPreviewVm};

#[allow(unused_imports)] // the Data Sync options step (T203) is the production consumer
pub use options_bar::OptionsBar;

#[allow(unused_imports)] // the Data Sync window track (T268) is the production consumer
pub use use_data_sync_feedback::DataSyncFeedback;
#[allow(unused_imports)] // the Data Sync window rendering (T268-S2) is the production consumer
pub use data_sync_window_state::{
    DataSyncDialogEpochs, DataSyncEndpointsSeam, DataSyncExecutionSeam, DataSyncExplainSeam,
    DataSyncNavigationSeam, DataSyncOptionsSeam, DataSyncProfilesSeam, DataSyncRowSelectionSeam,
    DataSyncRunStateSeam, DataSyncSelectionSeam, DataSyncWindowEdits, DataSyncWindowVm,
    DataSyncComparisonSeam,
};
#[allow(unused_imports)] // the app's window wiring is the production consumer
pub use data_sync_window::DataSyncWindow;
