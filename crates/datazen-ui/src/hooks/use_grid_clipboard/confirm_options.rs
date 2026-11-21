//! The two confirmation dialogs' option builders (React's
//! `confirmOptionsFor` and the "paste rows" cross-table options built
//! inline in `pasteRowsAsDrafts`). Split out of `mod.rs` to respect the
//! 800-line module cap; the wording — including which numbers land in
//! which sentence — is the dialog contract, re-derived from the React
//! source line by line.

use crate::hooks::confirm_dialog::ConfirmOptionsVm;
use crate::hooks::grid_clipboard_helpers::GRID_PASTE_CONFIRM_CELLS;
use crate::hooks::grid_paste_plan_helpers::{peek_clipboard_source, PastePlan, RowCopyPayload};
use crate::hooks::use_grid_clipboard::GridClipboardEnv;
use crate::i18n::t;
use crate::ui::confirm_dialog::ConfirmKind;

/// `confirmOptionsFor` (the cell-paste dialog).
pub(crate) fn cell_paste_confirm_options(
    env: &GridClipboardEnv<'_>,
    plan: &PastePlan,
) -> ConfirmOptionsVm {
    let rows_written =
        plan.bounds.as_ref().map_or(0, |bounds| bounds.bottom_row - bounds.top_row + 1);
    let columns_written =
        if !plan.dropped_columns.is_empty() { plan.matched_columns } else { plan.total_columns };
    let source = peek_clipboard_source();
    let body = if plan.cross_table.is_none() {
        t("dataTable.pasteUnknownSourceBody")
            .replace("{total}", &columns_written.to_string())
            .replace("{target}", env.table_label)
    } else {
        t("dataTable.pasteCrossTableBody")
            .replace("{source}", source.as_ref().map(|s| s.table_key.as_str()).unwrap_or(""))
            .replace("{target}", env.table_label)
            .replace("{matched}", &plan.matched_columns.to_string())
            .replace("{total}", &plan.total_columns.to_string())
    };
    let dropped = if !plan.dropped_columns.is_empty() {
        format!(
            " {}",
            t("dataTable.pasteDroppedColumns")
                .replace("{count}", &plan.dropped_columns.len().to_string())
                .replace(
                    "{names}",
                    &plan.dropped_columns.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                )
        )
    } else {
        String::new()
    };
    let large = plan.writes.len() > GRID_PASTE_CONFIRM_CELLS;
    ConfirmOptionsVm {
        title: if large {
            t("dataTable.pasteConfirmTitle").replace("{cells}", &plan.writes.len().to_string())
        } else {
            t("dataTable.pasteCrossTableTitle").to_owned()
        },
        message: if large {
            t("dataTable.pasteConfirmMessage")
                .replace("{rows}", &rows_written.to_string())
                .replace("{cols}", &columns_written.to_string())
                .replace("{overwritten}", &plan.overwritten_pending.to_string())
        } else {
            body
        },
        // React: `dropped.trim() || undefined` — an all-whitespace
        // description is no description.
        description: (!dropped.trim().is_empty()).then(|| dropped.trim().to_owned()),
        // The expensive mistake is a wrong write, so the safe answer is
        // the default.
        confirm_label: Some(t("dataTable.pasteToSelectedCells").to_owned()),
        cancel_label: Some(t("common.cancel").to_owned()),
        kind: ConfirmKind::Warning,
        badge: None,
        code_preview: None,
    }
}

/// The "paste rows" cross-table dialog (built inline in React's
/// `pasteRowsAsDrafts`): the column-mapping summary first (PRD 5.1).
pub(crate) fn rows_paste_confirm_options(
    env: &GridClipboardEnv<'_>,
    payload: &RowCopyPayload,
    matched_source: &[&str],
    dropped_columns: &[&str],
) -> ConfirmOptionsVm {
    let description = if !dropped_columns.is_empty() {
        Some(
            t("dataTable.pasteDroppedColumns")
                .replace("{count}", &dropped_columns.len().to_string())
                .replace(
                    "{names}",
                    &dropped_columns.iter().take(5).copied().collect::<Vec<_>>().join(", "),
                ),
        )
    } else {
        None
    };
    ConfirmOptionsVm {
        title: t("dataTable.pasteRowsCrossTableTitle").to_owned(),
        message: t("dataTable.pasteRowsCrossTableBody")
            .replace("{source}", &payload.table_key)
            .replace("{target}", env.table_label)
            .replace("{matched}", &matched_source.len().to_string())
            .replace("{total}", &payload.column_names.len().to_string()),
        description,
        confirm_label: Some(t("dataTable.pasteRows").to_owned()),
        cancel_label: Some(t("common.cancel").to_owned()),
        kind: ConfirmKind::Warning,
        badge: None,
        code_preview: None,
    }
}
