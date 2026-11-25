//! The copy half of the useGridClipboard machine (React's
//! `buildMatrix` / `buildText` / `buildCopyText` / `onCopied` /
//! `copySelectedCellsAs` / `copyRowsAsNewRows` / `copyRowsAsDrafts`
//! closures). Split out of `mod.rs` to respect the 800-line module
//! cap; a child module sees the machine's private state, so this is
//! the same `impl` continued, not a wrapper.

use serde_json::Value;

use crate::hooks::grid_clipboard_helpers::{
    build_rows_matrix, build_selection_matrix, paste_code, serialize_cells, ClipboardFormat,
    GridClipboardRange, PasteProblem, SerializeCellsOptions,
};
use crate::hooks::grid_paste_plan_helpers::{
    cell_coords_in_range, classify_data_type_family, normalize_selection_range,
    record_clipboard_source, record_row_copy, rows_covered_by_selection, table_change_key,
    ClipboardSource, RowCopyPayload,
};
use crate::hooks::use_clipboard_copy_feedback::ClipboardCopyWrite;
use crate::hooks::use_grid_clipboard::summary::{rows_to_draft_values, GridCopyTarget};
use crate::hooks::use_grid_clipboard::GridClipboardEnv;
use crate::table_data_connection_state::{CellCoord, CellRange, GridSelectionMode};

use super::summary::COPY_FEEDBACK_MS;
use super::GridClipboardHook;

impl GridClipboardHook {
    /// The normalized main region of the selection, if there is one
    /// (`selectionRange`).
    pub fn selection_range(&self, env: &GridClipboardEnv<'_>) -> Option<CellRange> {
        (env.grid_selection.mode == GridSelectionMode::Cell)
            .then(|| normalize_selection_range(env.grid_selection, env.column_order))
            .flatten()
    }

    /// Rows the row-level actions act on — derived, never written back
    /// into `selected_rows` (`coveredRows`).
    pub fn covered_rows(
        &self,
        env: &GridClipboardEnv<'_>,
    ) -> std::collections::HashSet<usize> {
        rows_covered_by_selection(env.grid_selection, env.selected_rows)
    }

    /// The covered rows, sorted ascending (the React
    /// `[...].sort((l, r) => l - r)`).
    fn sorted_covered_rows(&self, env: &GridClipboardEnv<'_>) -> Vec<usize> {
        let mut rows: Vec<usize> = self.covered_rows(env).into_iter().collect();
        rows.sort_unstable();
        rows
    }

    /// `rememberSource`: the copy source travels with the exact text,
    /// so a paste can tell whether the clipboard still holds it.
    fn remember_source(
        &mut self,
        env: &GridClipboardEnv<'_>,
        column_names: &[String],
        text: &str,
    ) {
        let column_types: Vec<_> = column_names
            .iter()
            .map(|name| {
                let data_type = env
                    .columns
                    .iter()
                    .find(|column| &column.name == name)
                    .map(|column| column.data_type.as_str());
                classify_data_type_family(data_type)
            })
            .collect();
        record_clipboard_source(
            ClipboardSource {
                table_key: table_change_key(env.context),
                column_names: column_names.to_vec(),
                column_types,
            },
            Some(text),
        );
    }

    /// `buildMatrix`: the matrix + column names a copy target spans,
    /// before any serialization.
    fn build_matrix(
        &self,
        env: &GridClipboardEnv<'_>,
        target: &GridCopyTarget,
    ) -> Option<(Vec<Vec<Value>>, Vec<String>)> {
        match target {
            GridCopyTarget::Cell { range } => {
                let left = env
                    .column_order
                    .iter()
                    .position(|name| *name == range.left_column)?;
                let right = env
                    .column_order
                    .iter()
                    .position(|name| *name == range.right_column)?;
                if right < left {
                    return None;
                }
                let column_names: Vec<String> = env.column_order[left..=right].to_vec();
                let coords = cell_coords_in_range(
                    &CellRange {
                        start: CellCoord {
                            row_index: range.top_row,
                            column_name: range.left_column.clone(),
                        },
                        end: CellCoord {
                            row_index: range.bottom_row,
                            column_name: range.right_column.clone(),
                        },
                    },
                    env.column_order,
                );
                if coords.is_empty() {
                    return None;
                }
                Some((build_selection_matrix(env.rows, &coords), column_names))
            }
            GridCopyTarget::Row { row_indices } => {
                let matrix = build_rows_matrix(env.rows, env.column_order, row_indices);
                if matrix.is_empty() {
                    return None;
                }
                Some((matrix, env.column_order.to_vec()))
            }
        }
    }

    /// `buildText`: serialize the target and record where it came from.
    /// A row-mode copy is also the "copy as new rows" payload (DBR-02):
    /// the matrix travels alongside the text, because TSV cannot tell
    /// `null` from `''` and the drafts must carry the real values.
    fn build_text(
        &mut self,
        env: &GridClipboardEnv<'_>,
        target: &GridCopyTarget,
        format: ClipboardFormat,
        include_header: Option<bool>,
    ) -> Option<String> {
        let (matrix, column_names) = self.build_matrix(env, target)?;
        let text = serialize_cells(
            &matrix,
            &SerializeCellsOptions {
                format: Some(format),
                column_names: &column_names,
                include_header,
                ..SerializeCellsOptions::default()
            },
        );
        if !text.is_empty() {
            self.remember_source(env, &column_names, &text);
            if matches!(target, GridCopyTarget::Row { .. }) {
                record_row_copy(
                    RowCopyPayload {
                        table_key: table_change_key(env.context),
                        column_names,
                        rows: matrix,
                    },
                    &text,
                );
            }
        }
        Some(text)
    }

    /// `handlers.buildCopyText`: the native `copy` gesture's text, with
    /// the source recorded.
    pub fn build_copy_text(
        &mut self,
        env: &GridClipboardEnv<'_>,
        target: &GridCopyTarget,
    ) -> Option<String> {
        self.build_text(env, target, ClipboardFormat::Tsv, None)
    }

    /// `handlers.onCopied`: the native path's feedback (that path
    /// writes the clipboard itself, so it needs the timer but not the
    /// write).
    pub fn on_copied(&mut self, ok: bool, now_ms: i64) {
        self.native_copied = ok;
        self.native_clear_at_ms = None;
        if !ok {
            return;
        }
        self.native_clear_at_ms = Some(now_ms + COPY_FEEDBACK_MS);
    }

    /// `copySelectedCellsAs`: build the text, arm the feedback window,
    /// and hand the host the write to perform. HTML is the one format
    /// whose clipboard text is not what a paste reads back (DB-10): the
    /// rich flavor carries the `<table>`, the plain flavor — and the
    /// paste-back alignment source — is the TSV of the same cells.
    pub fn copy_selected_cells_as(
        &mut self,
        env: &GridClipboardEnv<'_>,
        format: ClipboardFormat,
        include_header: Option<bool>,
    ) -> Option<(u64, ClipboardCopyWrite)> {
        let target = self.copy_target(env)?;
        if format == ClipboardFormat::Html {
            let (matrix, column_names) = self.build_matrix(env, &target)?;
            let html = serialize_cells(
                &matrix,
                &SerializeCellsOptions {
                    format: Some(ClipboardFormat::Html),
                    column_names: &column_names,
                    include_header,
                    ..SerializeCellsOptions::default()
                },
            );
            let plain = serialize_cells(
                &matrix,
                &SerializeCellsOptions {
                    format: Some(ClipboardFormat::Tsv),
                    column_names: &column_names,
                    include_header,
                    ..SerializeCellsOptions::default()
                },
            );
            if !plain.is_empty() {
                self.remember_source(env, &column_names, &plain);
            }
            return self.arm_copy(env, &plain, Some(&html));
        }
        // The name-alignment bookkeeping has to see the same columns the
        // text does.
        let text = self.build_text(env, &target, format, include_header)?;
        self.arm_copy(env, &text, None)
    }

    /// `copyRowsAsNewRows` (DBR-02): the covered rows go on the
    /// clipboard as row data — plain TSV for every other consumer, plus
    /// the recorded payload a later "paste rows" verifies and turns into
    /// drafts. Cell mode acts on the rows the rectangle covers, row
    /// mode on the row set; both are `coveredRows`.
    pub fn copy_rows_as_new_rows(
        &mut self,
        env: &GridClipboardEnv<'_>,
    ) -> Option<(u64, ClipboardCopyWrite)> {
        if self.covered_rows(env).is_empty() {
            return None;
        }
        let target = GridCopyTarget::Row { row_indices: self.sorted_covered_rows(env) };
        let text = self.build_text(env, &target, ClipboardFormat::Tsv, None)?;
        self.arm_copy(env, &text, None)
    }

    /// `copyRowsAsDrafts` (DBR-02, ⌘D): the clipboard-free variant —
    /// copy the covered rows straight into drafts. ⌘D means "duplicate
    /// now", so it must not depend on an asynchronous clipboard write.
    pub fn copy_rows_as_drafts(&mut self, env: &GridClipboardEnv<'_>) -> bool {
        let Some(add_draft_rows) = env.add_draft_rows else {
            return false;
        };
        if !env.is_editable || self.covered_rows(env).is_empty() {
            return false;
        }
        let target = GridCopyTarget::Row { row_indices: self.sorted_covered_rows(env) };
        let Some((matrix, column_names)) = self.build_matrix(env, &target) else {
            return false;
        };
        let drafts = rows_to_draft_values(&column_names, &matrix, env.columns);
        if drafts.is_empty() {
            self.report(env, PasteProblem::new(paste_code::PASTE_ROWS_NO_WRITABLE_COLUMN));
            return false;
        }
        add_draft_rows(&drafts);
        true
    }

    /// The copy target of the current selection (`copySelectedCellsAs`'s
    /// target derivation): the cell rectangle, else the covered rows.
    fn copy_target(&self, env: &GridClipboardEnv<'_>) -> Option<GridCopyTarget> {
        if let Some(range) = self.selection_range(env) {
            return Some(GridCopyTarget::Cell {
                range: GridClipboardRange {
                    top_row: range.start.row_index,
                    bottom_row: range.end.row_index,
                    left_column: range.start.column_name.clone(),
                    right_column: range.end.column_name.clone(),
                },
            });
        }
        if self.covered_rows(env).is_empty() {
            return None;
        }
        Some(GridCopyTarget::Row { row_indices: self.sorted_covered_rows(env) })
    }

    /// Arm the copy feedback and produce the host's write. An empty
    /// copy is an immediate failure with no state change — and the hook
    /// reports it, as React's `if (!ok)` after the awaited copy did.
    fn arm_copy(
        &mut self,
        env: &GridClipboardEnv<'_>,
        text: &str,
        rich_html: Option<&str>,
    ) -> Option<(u64, ClipboardCopyWrite)> {
        match self.copy_feedback.copy(text, rich_html, env.now_ms, COPY_FEEDBACK_MS) {
            Ok(write) => Some(write),
            Err(()) => {
                self.report(env, PasteProblem::new(paste_code::CLIPBOARD_WRITE_FAILED));
                None
            }
        }
    }
}
