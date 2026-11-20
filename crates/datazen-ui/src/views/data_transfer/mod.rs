//! Rune ports of `packages/frontend/src/windows/data-transfer/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `ColumnMappingEditor.tsx` — `ColumnMappingRow` | `column_mapping_row` | T023-S1 |
//! | `ColumnMappingEditor.tsx` — `ColumnMappingEditor` shell | `column_mapping_editor` | T023-S2 |
//! | `ColumnMappingEditor.tsx` — `RecordsetEditor` | `recordset_editor` | T023-S3 |
//! | `transferMappingView.ts` (T305 VM placeholder) | `transfer_mapping_view` | T023-S2 |
//! | `DataTransferAttachedJobs.tsx` | `data_transfer_attached_jobs` | T073 |
//! | `DataTransferTerminalResults.tsx` | `data_transfer_terminal_results` | T113 |
//! | segment-2 pairing/prefill libs (T636/T563/T625 VM placeholders) | `transfer_pairing_vm` | T147-S1 |
//! | `hooks/useTransferJobRun.ts` — the Job run machine | `transfer_job_run` | T147-S2 |
//! | `DataTransferWindow.tsx` — state model | `data_transfer_window_state` | T147-S1 |
//! | `DataTransferWindow.tsx` — rendering | `data_transfer_window` | T147-S3 |
//! | `MappingGateNotice.tsx` | `mapping_gate_notice` | T176 |
//! | `SourceFilterEditor.tsx` | `source_filter_editor` | T202 |

pub(crate) mod column_mapping_editor;
pub(crate) mod column_mapping_row;
pub(crate) mod data_transfer_attached_jobs;
pub(crate) mod data_transfer_terminal_results;
pub(crate) mod data_transfer_window;
pub(crate) mod data_transfer_window_state;
pub(crate) mod mapping_gate_notice;
pub(crate) mod recordset_editor;
pub(crate) mod source_filter_editor;
pub(crate) mod transfer_job_run;
pub(crate) mod transfer_mapping_view;
pub(crate) mod transfer_pairing_vm;

#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use column_mapping_editor::{
    ColumnMappingEditor, ColumnMappingEditorState,
};
#[allow(unused_imports)] // the Data Transfer window track is the production consumer
pub(crate) use data_transfer_attached_jobs::{
    active_attached_jobs, cancel_sent, cancel_unknown, job_counter_or_zero,
    job_state_label, progress_counts_text, read_job_counter, AttachedJob,
    AttachedJobsState, DataTransferAttachedJobs, JobCounter, JobProgress, JobState,
};
#[allow(unused_imports)] // the Data Transfer window track is the production consumer
pub(crate) use data_transfer_terminal_results::{
    derive_migration_job_verdict, is_migration_job_in_flight, latest_apply_job, terminal_job,
    DataTransferTerminalResults, JobRecoveryResult, MigrationJobVerdictInput,
    TerminalJobDetails, TerminalJobView, TerminalResultsState,
};
#[allow(unused_imports)] // T305 replaces this placeholder module with the real port
pub(crate) use transfer_mapping_view::{
    auto_match_column_mappings, clear_unmapped_column_mappings, mapping_gate_allows_advance,
    mapping_gate_block_reason, normalize_column_mappings, show_create_new_toggle,
    show_target_type, unmapped_target_columns, ColumnMappingTablePatch, TransferMappingStatus,
    TransferTableVm,
};
#[allow(unused_imports)] // the Data Transfer window (T147) is the production consumer
pub(crate) use transfer_pairing_vm::{
    is_transfer_target_supported, is_verified_migration_pair, normalize_driver_id,
    normalize_sync_family, pick_prefill_database, resolve_default_database,
    resolve_transfer_pairing, sync_category, MigrationWindowPrefill, SyncCategory,
    TransferPairingResult, TransferPath,
};
#[allow(unused_imports)] // the Data Transfer window (T147) is the production consumer
pub(crate) use data_transfer_window_state::{
    ConnectionOption, DdlKind, DedicatedSessionVm, NARROW_STEPS, SqlFileCompression,
    SqlFileEncoding, TransferConnectionVm, TransferDdlPreviewVm, TransferDestinationMode,
    TransferEndpointVm, TransferJobVm, TransferMode, TransferOptionsVm, TransferPreviewVm,
    TransferProfileVm, TransferSourceFilter, TransferSqlFileTargetVm, TransferTableMapping,
    TransferTablePatch, TransferTableRow, TransferWritePlanVm, WizardStep,
    DataTransferWindowVm, WriteMode, STEPS, DEFAULT_TRANSFER_OPTIONS,
};
#[allow(unused_imports)] // the Data Transfer window (T147) is the production consumer
pub(crate) use data_transfer_window::{
    DataTransferWindow, DataTransferWindowEdits, MigrationJobsHydration,
};
#[allow(unused_imports)] // the Data Transfer window (T147) is the production consumer
pub(crate) use mapping_gate_notice::MappingGateNotice;
#[allow(unused_imports)] // the mapping editors (T023 shell + T336) are the production consumers
pub(crate) use source_filter_editor::{
    rebuild_filter, SourceFilterEditor, SourceFilterSelect, StructuredSourceFilter, ValueDrafts,
    OPERATORS,
};
#[allow(unused_imports)] // the Data Transfer window (T147) is the production consumer
pub(crate) use transfer_job_run::{
    classify_transfer_run_failure, is_ambiguous_apply_submission_error, TransferApplyInput,
    TransferApplyJobViewVm, TransferApplyRequestVm, TransferJobRunVm, TransferPrepareJobViewVm,
    TransferRunFailure, TransferRunFailureKind, TransferRunPhase,
};
#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use column_mapping_row::{
    is_unmapped, target_options, ColumnMappingPatch, ColumnMappingRow, ColumnMappingRowVm,
    TargetOption, TransferColumnMapping,
};
#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use recordset_editor::{
    apply_bound_update, apply_tuple_bound_update, default_order, effective_order,
    enable_recordset, format_limit, order_options, parse_limit, BoundName,
    RecordsetBoundPatch, RecordsetEditor, RecordsetEditorVm, RecordsetHost, RecordsetPatch,
    RecordsetTupleBoundPatch, TransferRecordset, TransferRecordsetBound,
    TransferRecordsetTupleBound, TransferRecordsetTupleRange,
};
