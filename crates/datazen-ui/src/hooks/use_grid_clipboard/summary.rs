//! The hook's exported contracts and pure functions: the summary shape,
//! the copy target vocabulary, `rowsToDraftValues` (DBR-02) and
//! `describePasteProblem`. Split out of `mod.rs` to respect the
//! 800-line module cap; every piece here is 1:1 with the React hook
//! file's own exports (`packages/frontend/src/hooks/useGridClipboard.ts`).

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::hooks::grid_clipboard_helpers::{
    paste_code, GridClipboardRange, PasteProblem, GRID_PASTE_MAX_CELLS,
};
use crate::i18n::t;
use crate::table_data_connection_state::ColumnSchema;

/// How long "copied" stays visible; the grid's own window length.
pub const COPY_FEEDBACK_MS: i64 = 1_200;
/// Stored problems are capped: a 5 000-row block can produce thousands
/// of identical ones.
pub const MAX_REPORTED_PROBLEMS: usize = 100;

/// `PasteSummary`: what the last paste did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PasteSummary {
    /// Cells the plan wanted to write.
    pub cells: usize,
    /// Cells that really entered `pendingChanges`.
    pub staged: usize,
    /// Cells that were skipped, for any reason.
    pub skipped: usize,
    /// Staged changes that were overwritten.
    pub overwritten_pending: usize,
    pub rows: usize,
    pub columns: usize,
    pub cancelled: bool,
}

/// What a copy gesture should put on the clipboard (`GridCopyTarget`,
/// `components/DataTable/gridClipboardBridge.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridCopyTarget {
    Row { row_indices: Vec<usize> },
    Cell { range: GridClipboardRange },
}

/// `begin_paste_rows`'s answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowsPasteStart {
    /// No `addDraftRows`: "paste rows" is not offered (result grids).
    NotOffered,
    /// The host must read the system clipboard and report back.
    NeedClipboard,
    /// The paste was consumed (read-only refusal already reported).
    Consumed,
}

/// `rowsToDraftValues` (DBR-02): map a copied row matrix onto draft-row
/// values. Primary-key and auto-increment columns are never copied — a
/// new row earns its own identity — and unknown columns of the target
/// table are skipped. A `null` cell stays an explicit NULL; the caller
/// keeps the "absent key = don't write" invariant by only setting keys
/// that exist in the matrix.
pub fn rows_to_draft_values(
    column_names: &[String],
    matrix: &[Vec<Value>],
    columns: &[ColumnSchema],
) -> Vec<HashMap<String, Option<Value>>> {
    let writable: HashSet<&str> = columns
        .iter()
        .filter(|column| !column.is_primary_key && !column.is_auto_increment)
        .map(|column| column.name.as_str())
        .collect();
    matrix
        .iter()
        .map(|row| {
            let mut values: HashMap<String, Option<Value>> = HashMap::new();
            for (index, name) in column_names.iter().enumerate() {
                if !writable.contains(name.as_str()) {
                    continue;
                }
                match row.get(index) {
                    // A null cell is an explicit NULL; an absent key
                    // keeps meaning "don't write".
                    None | Some(Value::Null) => {
                        values.insert(name.clone(), None);
                    }
                    Some(cell) => {
                        values.insert(name.clone(), Some(cell.clone()));
                    }
                }
            }
            values
        })
        .collect()
}

/// `describePasteProblem`: render one structured paste problem as text
/// (shared by the banner and the tests). An unknown code is shown, not
/// swallowed (00-contracts §8.1): the raw detail is the only thing the
/// user has.
pub fn describe_paste_problem(problem: &PasteProblem) -> String {
    match problem.code {
        paste_code::EMPTY => t("dataTable.pasteEmpty").to_owned(),
        paste_code::TOO_LARGE => t("dataTable.pasteTooLarge")
            .replace("{cells}", problem.detail.as_deref().unwrap_or("0"))
            // From the constant the parser enforces. A literal here
            // drifts silently the day the limit moves, and the user
            // would be told a number the code does not obey.
            .replace("{maxCells}", &GRID_PASTE_MAX_CELLS.to_string()),
        paste_code::RAGGED_ROW => problem.detail.clone().unwrap_or_else(|| {
            t("dataTable.pasteRaggedRow")
                .replace("{row}", "")
                .replace("{count}", "")
                .replace("{expected}", "")
                .to_owned()
        }),
        paste_code::UNCLOSED_QUOTE => t("dataTable.pasteUnclosedQuote").to_owned(),
        paste_code::OUT_OF_BOUNDS => t("dataTable.pasteClippedToPage")
            .replace("{count}", problem.detail.as_deref().unwrap_or("0")),
        paste_code::NO_WRITABLE_CELL => t("dataTable.pasteNoWritableColumn").to_owned(),
        paste_code::NOT_NULL_EMPTY => t("dataTable.pasteNotAllowedEmpty")
            .replace("{row}", &(problem.row.map(|row| row + 1).unwrap_or(0)).to_string())
            .replace("{column}", problem.column.as_deref().unwrap_or("")),
        paste_code::IDENTITY_CHANGED => t("dataTable.pasteIdentityChanged")
            .replace("{row}", &(problem.row.map(|row| row + 1).unwrap_or(0)).to_string())
            .replace("{column}", problem.column.as_deref().unwrap_or("")),
        paste_code::COLUMN_MISMATCH => t("dataTable.pasteDroppedColumns")
            .replace(
                "{count}",
                &problem
                    .detail
                    .as_deref()
                    .map(|names| names.split(", ").count())
                    .unwrap_or(0)
                    .to_string(),
            )
            .replace("{names}", problem.detail.as_deref().unwrap_or("")),
        paste_code::CROSS_TABLE => t("dataTable.pasteCrossTableNotice")
            // The status line's wording, not the dialog's title: after
            // the user confirmed, a line reading "Paste into another
            // table?" would look like an unanswered question.
            .replace("{table}", problem.detail.as_deref().unwrap_or("")),
        paste_code::CELL_READ_ONLY => {
            // No column ⇒ the whole panel is read-only; with a column ⇒
            // that one column cannot be written and the rest still
            // lands. The **column name** is in `column`; `detail`
            // carries the *reason* (`auto-increment`), not a name list.
            match problem.column.as_deref() {
                Some(column) => t("dataTable.pasteReadOnlyColumn")
                    .replace("{count}", "1")
                    .replace("{columns}", column),
                None => t("dataTable.readOnlyPasteDisabled").to_owned(),
            }
        }
        paste_code::CELL_AMBIGUOUS_IDENTITY => t("dataTable.pasteNoRowIdentity").to_owned(),
        paste_code::CLIPBOARD_WRITE_FAILED => t("dataTable.copyFailed").to_owned(),
        paste_code::CLIPBOARD_READ_FAILED => {
            t("dataTable.pasteReadFailed").replace("{shortcut}", "⌘V")
        }
        paste_code::PASTE_ROWS_NOT_ROW_DATA => t("dataTable.pasteRowsNotRowData").to_owned(),
        paste_code::PASTE_ROWS_NO_WRITABLE_COLUMN => {
            t("dataTable.pasteRowsNoWritableColumn").to_owned()
        }
        _ => problem.detail.clone().unwrap_or_else(|| problem.code.to_owned()),
    }
}
