//! Rune ports of `packages/frontend/src/windows/data-transfer/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `ColumnMappingEditor.tsx` — `ColumnMappingRow` | `column_mapping_row` | T023-S1 |
//! | `ColumnMappingEditor.tsx` — `ColumnMappingEditor` shell | _not yet ported_ | T023-S2 |
//! | `ColumnMappingEditor.tsx` — `RecordsetEditor` | _not yet ported_ | T023-S3 |

pub(crate) mod column_mapping_row;

#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use column_mapping_row::{
    is_unmapped, target_options, ColumnMappingPatch, ColumnMappingRow, ColumnMappingRowVm,
    TargetOption, TransferColumnMapping,
};
