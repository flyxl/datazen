//! T163-S1: the shell state model and the derived gating of
//! `packages/frontend/src/components/DataTable/DataTable.tsx`.
//!
//! The React component is the grid's orchestrator: ~150 props in, three
//! local `useState` + one `useRef` counter, and a render tree that mounts
//! the filter surface, the selection bar, the header, the virtual body and
//! the pagination, each under a gating condition. This module is the
//! **local** state and the pure derivations; the composition itself is
//! [`super::data_table`] (S2). The virtual body is T396 and stays a slot.
//!
//! # What the shell owns (React → here)
//!
//! | React state / derived                          | here                                     |
//! |------------------------------------------------|------------------------------------------|
//! | `exportOpen`, `exportInitialScope`             | [`DataTableVm::export_open`] / [`DataTableVm::export_initial_scope`] |
//! | `signalSeenRef` + the `exportOpenSignal` effect | [`DataTableVm::signal_seen`] + [`apply_export_signal`] |
//! | `presentationColumn` + `presentationSample`    | [`DataTableVm::presentation_column`] + [`presentation_sample`] |
//! | `coveredRows` / `coveredColumns` / `coveredCells` | [`covered_rows`] / [`covered_columns`] / [`covered_cells`] |
//! | `deleteRowIndices` / `deleteRowCount`          | [`delete_row_indices`] / [`delete_row_count`] |
//! | `gridRowCount` (ARIA row count incl. header)   | [`grid_row_count`] |
//! | `activeCell` (keyboard cursor fallback)        | [`active_cell`] |
//! | `viewportRowCount` (no viewport measuring)     | [`viewport_row_count`] |
//! | `exportEnabled`, `hasPagination`, `hasFilters`, `hasFilterEditor` | [`export_enabled`], [`has_pagination`], [`has_filter_bar`], [`has_filter_editor`] |
//!
//! # The dialog coherence contract
//!
//! React mounts `DataExportDialog` from local state alone; the Rune dialog
//! port reads a host-owned [`DataExportVm`] whose `open` / `open_epoch` /
//! `initial_scope` must move **with** the shell's flags or the scrim never
//! appears. The host therefore opens and closes the pair through
//! [`open_export_dialog`] / [`close_export_dialog`] — one coherent write for
//! both halves — and replays the counter signal through
//! [`apply_export_signal`] each frame before the view runs. Tests pin the
//! state machine here and the mount in S2; the dialog's own widgets are
//! T132's and stay audited there.

use serde_json::Value;

use crate::components::connection::batch_export_dialog::DataExportCapability;
use crate::components::data_table::data_export_dialog::DataExportVm;
use crate::components::data_table::data_export_dialog_options::ExportScope;
use crate::components::data_table::table_header::Column;
use crate::table_data_grid_selection::{
    columns_covered_by_selection, grid_selection_cell_count, rows_covered_by_selection,
    CellCoord, GridSelection, GridSelectionMode, GridSelectionState,
};

/// `DATA_TABLE_ROW_HEIGHT` from `rowHeight.ts` (T414 owns the module; the
/// shell keeps the same number so the two agree until it lands).
pub(crate) const DATA_TABLE_ROW_HEIGHT: f32 = 28.0;

/// React's `exportInitialScope`: the scope the export dialog opens with
/// (DB-15). `None` is React's `undefined` — the dialog's own default (the
/// page scope).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportInitialScope {
    CurrentPage,
    FilteredTable,
}

impl ExportInitialScope {
    /// The dialog VM's scope value for this initial scope.
    pub(crate) fn to_export_scope(self) -> ExportScope {
        match self {
            ExportInitialScope::CurrentPage => ExportScope::CurrentPage,
            ExportInitialScope::FilteredTable => ExportScope::FilteredTable,
        }
    }
}

/// The shell's local state — React's three `useState` and the signal
/// counter, all host-owned so callbacks in `S` can write them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DataTableVm {
    /// React `exportOpen`.
    pub(crate) export_open: bool,
    /// React `exportInitialScope` (`undefined` ⇒ `None`).
    pub(crate) export_initial_scope: Option<ExportInitialScope>,
    /// React `signalSeenRef.current`: the last `exportOpenSignal` value the
    /// shell already opened on. A replayed old value never re-opens.
    pub(crate) signal_seen: u32,
    /// React `presentationColumn` — the column whose presentation dialog is
    /// up (`null` ⇒ none).
    pub(crate) presentation_column: Option<String>,
}

impl DataTableVm {
    /// React's `handleExport` / the toolbar export button: open the dialog
    /// the way it always opened (no filtered preselect).
    pub(crate) fn open_export_plain(&mut self) {
        self.export_initial_scope = None;
        self.export_open = true;
    }

    /// The filter panel's "Export…" exit (DB-15): open with "current filter
    /// results" preselected.
    pub(crate) fn open_export_filtered(&mut self) {
        self.export_initial_scope = Some(ExportInitialScope::FilteredTable);
        self.export_open = true;
    }
}

/// The host-side state the shell owns, one struct so its two halves move
/// together inside a single `&mut S` borrow: React's local state
/// (`DataTableVm`) and the export dialog's host VM (T132). Hosts store this
/// struct wherever they like and hand the view an accessor closure; the
/// shell never opens or closes one half without the other.
#[derive(Debug, Default)]
pub(crate) struct DataTableShellState {
    /// React's `exportOpen` / `exportInitialScope` / `presentationColumn`.
    pub(crate) vm: DataTableVm,
    /// The export dialog's own host VM (`DataExportVm`), owned here so the
    /// `open` / `open_epoch` / `initial_scope` triplet moves with the flags.
    pub(crate) export: DataExportVm,
}

impl DataTableShellState {
    /// React's initial state: no dialog, no signal seen.
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// Open the export dialog coherently: the shell's flags and the host's
/// dialog VM move together (see the module doc's contract).
pub(crate) fn open_export_dialog(
    vm: &mut DataTableVm,
    export_vm: Option<&mut DataExportVm>,
    scope: Option<ExportInitialScope>,
) {
    vm.export_initial_scope = scope;
    vm.export_open = true;
    if let Some(dialog) = export_vm {
        dialog.open = true;
        dialog.open_epoch = dialog.open_epoch.wrapping_add(1);
        dialog.initial_scope = scope.map(ExportInitialScope::to_export_scope);
    }
}

/// Close it (the dialog's `onClose` / the scrim's press) — both halves again.
pub(crate) fn close_export_dialog(vm: &mut DataTableVm, export_vm: Option<&mut DataExportVm>) {
    vm.export_open = false;
    if let Some(dialog) = export_vm {
        dialog.open = false;
    }
}

/// React's `exportOpenSignal` effect, replayed on the host state before the
/// frame renders. A `None` (React `undefined`) or a value at or below the
/// already-seen counter is a no-op; a higher value opens the dialog once
/// with the filtered preselect exactly when the statement builder exists.
///
/// Returns whether this call opened the dialog, so the state machine can be
/// pinned without reading the flags back.
pub(crate) fn apply_export_signal(
    vm: &mut DataTableVm,
    signal: Option<u32>,
    has_filtered_builder: bool,
) -> bool {
    let Some(signal) = signal else { return false };
    if signal <= vm.signal_seen {
        return false;
    }
    vm.signal_seen = signal;
    vm.export_initial_scope = if has_filtered_builder {
        Some(ExportInitialScope::FilteredTable)
    } else {
        None
    };
    vm.export_open = true;
    true
}

/// React `exportEnabled`: a table name, at least one loaded row, and a
/// capability that is not `none`.
pub(crate) fn export_enabled(
    table_name: Option<&str>,
    rows_len: usize,
    capability: DataExportCapability,
) -> bool {
    table_name.is_some() && rows_len > 0 && capability != DataExportCapability::None
}

/// React `hasPagination` — the four inputs the pager needs, all present.
pub(crate) fn has_pagination(
    page: Option<usize>,
    page_size: Option<usize>,
    total_rows: Option<u64>,
    on_page_change: bool,
    on_page_size_change: bool,
) -> bool {
    page.is_some() && page_size.is_some() && total_rows.is_some() && on_page_change && on_page_size_change
}

/// React `hasFilters` — the read-only bar: applied filters plus both
/// callbacks.
pub(crate) fn has_filter_bar(filters_len: usize, on_remove: bool, on_clear: bool) -> bool {
    filters_len > 0 && on_remove && on_clear
}

/// React `hasFilterEditor` — the full editor: the six callbacks.
pub(crate) fn has_filter_editor(
    on_add: bool,
    on_update: bool,
    on_remove: bool,
    on_clear: bool,
    on_logic_change: bool,
    on_apply: bool,
    on_open_change: bool,
) -> bool {
    on_add
        && on_update
        && on_remove
        && on_clear
        && on_logic_change
        && on_apply
        && on_open_change
}

/// React `coveredRows`: the row indices a cell selection covers, sorted.
/// `None` when the cell-selection contract is off or the mode is not
/// `cell` — React's `null`, which makes the delete path fall back to the
/// row set.
pub(crate) fn covered_rows(
    has_cell_selection: bool,
    selection: &GridSelection,
    selected_rows: &std::collections::HashSet<usize>,
) -> Option<Vec<usize>> {
    if !has_cell_selection || selection.mode != GridSelectionMode::Cell {
        return None;
    }
    let state = GridSelectionState {
        selection,
        selected_rows,
    };
    let mut rows: Vec<usize> = rows_covered_by_selection(&state).into_iter().collect();
    rows.sort_unstable();
    Some(rows)
}

/// React `coveredColumns`: how many columns the selection touches (0 when
/// the contract is off — React's ternary).
pub(crate) fn covered_columns(
    has_cell_selection: bool,
    selection: &GridSelection,
    column_order: &[&str],
) -> usize {
    if !has_cell_selection {
        return 0;
    }
    columns_covered_by_selection(selection, column_order)
}

/// React `coveredCells`: how many cells the selection covers.
pub(crate) fn covered_cells(
    has_cell_selection: bool,
    selection: &GridSelection,
    column_order: &[&str],
) -> usize {
    if !has_cell_selection {
        return 0;
    }
    grid_selection_cell_count(selection, column_order)
}

/// React `deleteRowIndices`: the covered rows in cell mode, else the row
/// set — always sorted, because the back end deletes deterministically.
pub(crate) fn delete_row_indices(
    covered: Option<&Vec<usize>>,
    selected_rows: &std::collections::HashSet<usize>,
) -> Vec<usize> {
    match covered {
        Some(rows) => rows.clone(),
        None => {
            let mut rows: Vec<usize> = selected_rows.iter().copied().collect();
            rows.sort_unstable();
            rows
        }
    }
}

/// React `deleteRowCount` — the count the toolbar's confirmation shows.
pub(crate) fn delete_row_count(
    covered: Option<&Vec<usize>>,
    selected_rows_len: usize,
) -> usize {
    match covered {
        Some(rows) => rows.len(),
        None => selected_rows_len,
    }
}

/// React `gridRowCount`: the ARIA row count of the grid surface — the data
/// rows plus the header row plus the status row the overlays mount (when
/// there is one), or exactly 1 when a caller-provided empty placeholder
/// replaces the whole body.
pub(crate) fn grid_row_count(has_placeholder: bool, rows_len: usize, loading: bool) -> usize {
    if has_placeholder {
        return 1;
    }
    rows_len + 1 + usize::from(loading || rows_len == 0)
}

/// React `presentationSample`: the column's first non-null loaded value —
/// the sample the presentation dialog previews (`undefined` when the column
/// is unknown or holds only nulls; the dialog falls back to its own anchor
/// sample, exactly as React's does).
pub(crate) fn presentation_sample<'a>(
    rows: &'a [Vec<Value>],
    columns: &[Column],
    presentation_column: Option<&str>,
) -> Option<&'a Value> {
    let name = presentation_column?;
    let index = columns.iter().position(|c| c.name == name)?;
    // React: `value !== null && value !== undefined` — a short row (no such
    // column) counts as undefined and is skipped, and so is `null`.
    rows.iter().find_map(|row| {
        row.get(index).filter(|value| !value.is_null())
    })
}

/// React `viewportRowCount` — the rows one screen holds. The React shell
/// measures the scroll element; the Rune runtime hands the surface its
/// height, so the math (never the measuring) is what this keeps: a missing
/// viewport is one row, a partial row is `max(1, floor)`.
pub(crate) fn viewport_row_count(viewport_height: Option<f32>, row_height: f32) -> usize {
    match viewport_height {
        Some(height) if row_height > 0.0 => {
            let rows = (height / row_height).floor() as usize;
            rows.max(1)
        }
        _ => 1,
    }
}

/// React `activeCell`: the keyboard cursor, falling back to the focused
/// cell of a cell-mode selection when no cursor has moved yet.
pub(crate) fn active_cell(
    cursor: Option<&CellCoord>,
    selection: &GridSelection,
) -> Option<CellCoord> {
    match cursor {
        Some(coord) => Some(coord.clone()),
        None if selection.mode == GridSelectionMode::Cell => selection.focus.clone(),
        None => None,
    }
}

/// The scroll surface's `aria-label` key: the grid role's label when the
/// cell-selection contract is on, the plain table label otherwise.
pub(crate) fn aria_grid_label_key(has_cell_selection: bool) -> &'static str {
    if has_cell_selection {
        "tableView.a11y.gridLabel"
    } else {
        "dataTable.tableLabel"
    }
}

#[cfg(test)]
mod vm_tests {
    use super::*;
    use std::collections::HashSet;

    fn cell_selection(anchor: (usize, &str), focus: (usize, &str)) -> GridSelection {
        GridSelection {
            anchor: Some(CellCoord {
                row_index: anchor.0,
                column_name: anchor.1.to_owned(),
            }),
            focus: Some(CellCoord {
                row_index: focus.0,
                column_name: focus.1.to_owned(),
            }),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        }
    }

    #[test]
    fn vm_defaults_match_react_initial_state() {
        let vm = DataTableVm::default();
        assert!(!vm.export_open);
        assert_eq!(vm.export_initial_scope, None);
        assert_eq!(vm.signal_seen, 0);
        assert_eq!(vm.presentation_column, None);
    }

    #[test]
    fn export_signal_opens_once_per_increment() {
        let mut vm = DataTableVm::default();
        assert!(!apply_export_signal(&mut vm, None, true));
        assert!(!apply_export_signal(&mut vm, Some(0), true)); // 0 <= seen(0)
        assert!(apply_export_signal(&mut vm, Some(1), true));
        assert_eq!(vm.export_initial_scope, Some(ExportInitialScope::FilteredTable));
        assert!(vm.export_open);
        assert!(!apply_export_signal(&mut vm, Some(1), true)); // replay: no-op
        assert!(apply_export_signal(&mut vm, Some(2), false));
        // Without the statement builder the dialog opens the plain way.
        assert_eq!(vm.export_initial_scope, None);
        // A stale value that arrives late is still a no-op.
        assert!(!apply_export_signal(&mut vm, Some(1), true));
        assert_eq!(vm.signal_seen, 2);
    }

    #[test]
    fn export_enabled_gates_on_all_three() {
        assert!(export_enabled(Some("t"), 1, DataExportCapability::LoadedOnly));
        assert!(!export_enabled(Some("t"), 0, DataExportCapability::LoadedOnly));
        assert!(!export_enabled(None, 5, DataExportCapability::LoadedOnly));
        assert!(!export_enabled(Some("t"), 5, DataExportCapability::None));
        assert!(export_enabled(Some("t"), 5, DataExportCapability::FullTable));
    }

    #[test]
    fn covered_math_needs_cell_mode_and_contract() {
        let names: Vec<&str> = vec!["a", "b", "c"];
        let selection = cell_selection((0, "a"), (2, "c"));
        let empty: HashSet<usize> = HashSet::new();

        // Contract off: all three derivations collapse, covered_rows is None.
        assert_eq!(covered_rows(false, &selection, &empty), None);
        assert_eq!(covered_columns(false, &selection, &names), 0);
        assert_eq!(covered_cells(false, &selection, &names), 0);

        // Cell mode over 3 rows: every row, every column, all cells.
        assert_eq!(covered_rows(true, &selection, &empty), Some(vec![0, 1, 2]));
        assert_eq!(covered_columns(true, &selection, &names), 3);
        assert_eq!(covered_cells(true, &selection, &names), 9);

        // Row mode: covered_rows stays None (React's rule 3 — the row set is
        // the owner's, cell coverage does not apply).
        let row_mode = GridSelection {
            mode: GridSelectionMode::Row,
            ..selection
        };
        assert_eq!(covered_rows(true, &row_mode, &empty), None);
    }

    #[test]
    fn delete_indices_prefer_covered_then_sorted_row_set() {
        let covered = vec![4, 2];
        let mut rows: HashSet<usize> = [9, 1, 5].into_iter().collect();
        assert_eq!(delete_row_indices(Some(&covered), &rows), vec![4, 2]);
        assert_eq!(delete_row_count(Some(&covered), rows.len()), 2);
        let none: Option<&Vec<usize>> = None;
        assert_eq!(delete_row_indices(none, &rows), vec![1, 5, 9]);
        assert_eq!(delete_row_count(none, rows.len()), 3);
        rows.clear();
        assert_eq!(delete_row_indices(None, &rows), Vec::<usize>::new());
    }

    #[test]
    fn grid_row_count_follows_the_aria_formula() {
        assert_eq!(grid_row_count(false, 10, false), 11); // rows + header, no status row
        assert_eq!(grid_row_count(false, 0, false), 2); // header + the empty status row
        assert_eq!(grid_row_count(false, 0, true), 2);
        assert_eq!(grid_row_count(false, 5, true), 7);
        assert_eq!(grid_row_count(true, 10, false), 1); // placeholder replaces all
    }

    #[test]
    fn presentation_sample_is_first_non_null() {
        let columns = vec![
            Column::new("c0", "id").ty("integer"),
            Column::new("c1", "note").ty("text"),
        ];
        let rows: Vec<Vec<Value>> = vec![
            vec![Value::Null, serde_json::json!("first")],
            vec![serde_json::json!(7), serde_json::json!("second")],
        ];
        let sample = presentation_sample(&rows, &columns, Some("note"));
        assert_eq!(sample, Some(&serde_json::json!("first")));
        assert_eq!(presentation_sample(&rows, &columns, Some("id")), Some(&serde_json::json!(7)));
        assert_eq!(presentation_sample(&rows, &columns, Some("missing")), None);
        assert_eq!(presentation_sample(&rows, &columns, None), None);
        let nulls: Vec<Vec<Value>> = vec![vec![Value::Null, Value::Null]];
        assert_eq!(presentation_sample(&nulls, &columns, Some("note")), None);
    }

    #[test]
    fn viewport_math_never_renders_zero_rows() {
        assert_eq!(viewport_row_count(None, 28.0), 1);
        assert_eq!(viewport_row_count(Some(0.0), 28.0), 1);
        assert_eq!(viewport_row_count(Some(27.9), 28.0), 1);
        assert_eq!(viewport_row_count(Some(28.0), 28.0), 1);
        assert_eq!(viewport_row_count(Some(83.9), 28.0), 2);
        assert_eq!(viewport_row_count(Some(84.0), 28.0), 3);
        assert_eq!(viewport_row_count(Some(56.0), 0.0), 1); // degenerate row height
    }

    #[test]
    fn active_cell_falls_back_to_cell_focus() {
        let selection = cell_selection((1, "b"), (3, "c"));
        assert_eq!(active_cell(None, &selection), selection.focus);
        let cursor = CellCoord {
            row_index: 0,
            column_name: "a".to_owned(),
        };
        assert_eq!(active_cell(Some(&cursor), &selection), Some(cursor));
        let row_mode = GridSelection {
            mode: GridSelectionMode::Row,
            ..selection
        };
        assert_eq!(active_cell(None, &row_mode), None);
    }

    #[test]
    fn gating_helpers_pin_react_conditions() {
        assert!(has_pagination(Some(1), Some(50), Some(100), true, true));
        assert!(!has_pagination(Some(1), Some(50), None, true, true));
        assert!(!has_pagination(None, Some(50), Some(100), true, true));
        assert!(!has_pagination(Some(1), Some(50), Some(100), false, true));

        assert!(has_filter_bar(1, true, true));
        assert!(!has_filter_bar(0, true, true));
        assert!(!has_filter_bar(2, true, false));
        assert!(!has_filter_bar(2, false, true));

        assert!(has_filter_editor(true, true, true, true, true, true, true));
        assert!(!has_filter_editor(true, true, true, true, true, true, false));
        assert!(!has_filter_editor(true, false, true, true, true, true, true));
    }

    #[test]
    fn dialog_open_close_moves_both_halves() {
        let mut vm = DataTableVm::default();
        let mut dialog = DataExportVm::default();
        open_export_dialog(&mut vm, Some(&mut dialog), Some(ExportInitialScope::FilteredTable));
        assert!(vm.export_open);
        assert!(dialog.open);
        assert_eq!(dialog.open_epoch, 1);
        assert_eq!(dialog.initial_scope, Some(ExportScope::FilteredTable));
        // Opening without a dialog half still arms the shell.
        open_export_dialog(&mut vm, None, Some(ExportInitialScope::CurrentPage));
        assert_eq!(vm.export_initial_scope, Some(ExportInitialScope::CurrentPage));
        close_export_dialog(&mut vm, Some(&mut dialog));
        assert!(!vm.export_open);
        assert!(!dialog.open);
        close_export_dialog(&mut vm, None); // closing without the half is fine
        assert!(!vm.export_open);
    }

    #[test]
    fn aria_label_key_follows_the_contract() {
        assert_eq!(aria_grid_label_key(true), "tableView.a11y.gridLabel");
        assert_eq!(aria_grid_label_key(false), "dataTable.tableLabel");
    }
}
