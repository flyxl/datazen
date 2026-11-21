//! Rune ports of `packages/frontend/src/components/DataTable/**`.
//!
//! | React source                         | Rune module                 |
//! |--------------------------------------|-----------------------------|
//! | `ColumnPresentationDialog.tsx`     | `column_presentation_dialog` (T054-S1: draft model + pure fns) |
//! |                                      | + `column_presentation_validation` (T054-S1: JSON validators) |
//! |                                      | + `column_presentation_plan` (T054-S2: plan / preview) |
//! |                                      | + `column_presentation_rules` (T054-S2: registry + rules) |
//! |                                      | + `column_presentation_dialog_state` (T054-S2: state + save/enable) |
//! |                                      | + `column_presentation_dialog_view` (T054-S2: render) |
//! | `ColumnStatsDialog.tsx`              | `column_stats_dialog`       |
//! | `DataExportDialog.tsx`                | `data_export_dialog` (T132: VM + view) + `data_export_dialog_options` (T132: the `lib/exportDialogShared.ts` vocabulary) |
//! | `DefaultOrderSelect.tsx`             | `default_order_select`      |
//! | `GridKeyboardHints.tsx`              | `grid_keyboard_hints`       |
//! | `Pagination.tsx`                     | `pagination`                |
//! | `DataTableSelectionBar.tsx`           | `data_table_selection_bar`  |
//! | `TableHeader.tsx`                    | `table_header`              |
//! | `dirtyCellText.ts` (T405)            | `dirty_cell_text`           |
//! | `fkCellText.ts` (T405)               | `fk_cell_text`              |
//! | `editors/activeEditor.ts` (T405)     | `editors::active_editor`    |

pub(crate) mod cell_renderer;
pub(crate) mod column_presentation_dialog;
pub(crate) mod column_presentation_dialog_state;
pub(crate) mod column_presentation_dialog_view;
pub(crate) mod column_presentation_plan;
pub(crate) mod column_presentation_rules;
pub(crate) mod column_presentation_validation;
pub(crate) mod column_stats_dialog;
pub(crate) mod data_export_dialog;
pub(crate) mod data_export_dialog_options;
pub(crate) mod data_table_selection_bar;
pub(crate) mod default_order_select;
pub(crate) mod dirty_cell_text;
pub(crate) mod editors;
pub(crate) mod fk_cell_text;
pub(crate) mod grid_keyboard_hints;
pub(crate) mod pagination;
pub(crate) mod table_header;
