//! Rune ports of `packages/frontend/src/components/DataTable/**`.
//!
//! | React source                         | Rune module                 |
//! |--------------------------------------|-----------------------------|
//! | `ColumnPresentationDialog.tsx`     | `column_presentation_dialog` (T054-S1: draft model + pure fns) |
//! |                                      | + `column_presentation_validation` (T054-S1: JSON validators) |
//! | `ColumnStatsDialog.tsx`              | `column_stats_dialog`       |
//! | `DefaultOrderSelect.tsx`             | `default_order_select`      |
//! | `GridKeyboardHints.tsx`              | `grid_keyboard_hints`       |
//! | `Pagination.tsx`                     | `pagination`                |
//! | `DataTableSelectionBar.tsx`           | `data_table_selection_bar`  |
//! | `TableHeader.tsx`                    | `table_header`              |

pub(crate) mod cell_renderer;
pub(crate) mod column_presentation_dialog;
pub(crate) mod column_presentation_validation;
pub(crate) mod column_stats_dialog;
pub(crate) mod data_table_selection_bar;
pub(crate) mod default_order_select;
pub(crate) mod grid_keyboard_hints;
pub(crate) mod pagination;
pub(crate) mod table_header;
