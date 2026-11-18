//! Rune ports of `packages/frontend/src/windows/data-transfer/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `ColumnMappingEditor.tsx` — `ColumnMappingRow` | `column_mapping_row` | T023-S1 |
//! | `ColumnMappingEditor.tsx` — `ColumnMappingEditor` shell | `column_mapping_editor` | T023-S2 |
//! | `ColumnMappingEditor.tsx` — `RecordsetEditor` | _not yet ported_ | T023-S3 |
//! | `transferMappingView.ts` (T307 VM placeholder) | `transfer_mapping_view` | T023-S2 |

pub(crate) mod column_mapping_editor;
pub(crate) mod column_mapping_row;
pub(crate) mod transfer_mapping_view;

#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use column_mapping_editor::{
    ColumnMappingEditor, ColumnMappingEditorState,
};
#[allow(unused_imports)] // T307 replaces this module with the real port
pub(crate) use transfer_mapping_view::{
    auto_match_column_mappings, clear_unmapped_column_mappings,
    normalize_column_mappings, show_create_new_toggle, show_target_type,
    unmapped_target_columns, ColumnMappingTablePatch, TransferMappingStatus,
    TransferTableVm,
};
#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use column_mapping_row::{
    is_unmapped, target_options, ColumnMappingPatch, ColumnMappingRow, ColumnMappingRowVm,
    TargetOption, TransferColumnMapping,
};
