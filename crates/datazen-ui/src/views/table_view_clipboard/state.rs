//! Host-owned clipboard state and the row-copy / feedback decisions kept out
//! of the main decision file (800-line ceiling). Same T630 removal stage: no
//! table host mounts these yet; every reusable item is `allow(dead_code)` until
//! it does. No timer or clipboard call — the host records the text and clock.

use std::collections::BTreeMap;

use crate::i18n::t;
use crate::views::table_clipboard_status::{PasteProblem, PasteSummary};

use super::{
    build_rows_matrix, rows_to_draft_values, ClipValue, ClipboardColumn, ClipboardRow, ConfirmTexts,
    RowCopy, COPY_FEEDBACK_MS, MAX_REPORTED_PROBLEMS,
};

/// React's `GRID_PASTE_CHUNK_CELLS` loop: a confirmed paste is staged in this
/// many patches. Exposed separately because the gate asks for confirmation first.
#[allow(dead_code)] // pure helper; remove with the host.
pub(crate) fn stage_chunks(writes: usize) -> usize {
    writes.div_ceil(crate::views::table_view_clipboard::GRID_PASTE_CHUNK_CELLS)
}

fn fill(key: &str, pairs: &[(&str, String)]) -> String {
    let mut text = t(key).to_owned();
    for (name, value) in pairs {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// React `normalizeClipboardText`: CRLF and bare CR both become LF. The OS is
/// free to rewrite a copied trailing newline; both sides are normalized first.
#[allow(dead_code)] // deps mirror; remove with the host.
fn normalize_clipboard_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\r' {
            out.push('\n');
            if chars.get(index + 1) == Some(&'\n') {
                index += 2;
            } else {
                index += 1;
            }
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

/// React `rowCopyForClipboardText`: only a payload whose recorded text still
/// matches what the clipboard holds may steer a paste-rows (a stale Excel copy
/// must be treated as unknown text).
#[allow(dead_code)] // pure helper; remove with the host.
pub(crate) fn verify_row_copy(recorded: Option<(&RowCopy, &str)>, current: &str) -> Option<RowCopy> {
    let (payload, text) = recorded?;
    if normalize_clipboard_text(text) == normalize_clipboard_text(current) {
        Some(payload.clone())
    } else {
        None
    }
}

/// React `copyRowsAsDrafts`'s gate, with the matrix-to-drafts mapping folded in.
#[allow(dead_code)] // decision; remove with the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DraftCopyDecision {
    NoCapability,
    NoRows,
    NoWritableColumn,
    Ready(Vec<BTreeMap<String, ClipValue>>),
}

#[allow(dead_code)] // decision; remove with the host.
pub(crate) fn decide_copy_rows_as_drafts(
    is_editable: bool,
    has_add_drafts: bool,
    covered: &[usize],
    rows: &[ClipboardRow],
    column_order: &[String],
    columns: &[ClipboardColumn],
) -> DraftCopyDecision {
    if !is_editable || !has_add_drafts || covered.is_empty() {
        return if covered.is_empty() {
            DraftCopyDecision::NoRows
        } else {
            DraftCopyDecision::NoCapability
        };
    }
    let matrix = build_rows_matrix(rows, column_order, covered);
    let drafts = rows_to_draft_values(column_order, &matrix, columns);
    if drafts.is_empty() {
        DraftCopyDecision::NoWritableColumn
    } else {
        DraftCopyDecision::Ready(drafts)
    }
}

/// React `pasteRowsAsDrafts`'s ordered decision. A read failure, an unrecognized
/// clipboard and a cross-table confirmation are all host-facing intents.
#[allow(dead_code)] // decision; remove with the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PasteRowsDecision {
    NoCapability,
    ReadOnly,
    ReadFailed,
    NotRowData,
    NoWritableColumn,
    NeedConfirm(ConfirmTexts),
    Ready(Vec<BTreeMap<String, ClipValue>>),
}

#[allow(dead_code)] // decision; remove with the host.
pub(crate) fn decide_paste_rows(
    is_editable: bool,
    has_add_drafts: bool,
    recorded: Option<(&RowCopy, &str)>,
    clipboard_text: Option<&str>,
    columns: &[ClipboardColumn],
    context_key: &str,
    table_label: &str,
) -> PasteRowsDecision {
    if !has_add_drafts {
        return PasteRowsDecision::NoCapability;
    }
    if !is_editable {
        return PasteRowsDecision::ReadOnly;
    }
    let Some(text) = clipboard_text else {
        return PasteRowsDecision::ReadFailed;
    };
    let Some(payload) = verify_row_copy(recorded, text) else {
        return PasteRowsDecision::NotRowData;
    };
    let writable: Vec<&str> = columns
        .iter()
        .filter(|column| !column.is_primary_key && !column.is_auto_increment)
        .map(|column| column.name.as_str())
        .collect();
    let matched = payload
        .column_names
        .iter()
        .filter(|name| writable.iter().any(|candidate| candidate == name))
        .count();
    if matched == 0 {
        return PasteRowsDecision::NoWritableColumn;
    }
    let drafts = rows_to_draft_values(&payload.column_names, &payload.rows, columns);
    if payload.table_key != context_key {
        let dropped: Vec<String> = payload
            .column_names
            .iter()
            .filter(|name| !writable.iter().any(|candidate| candidate == name))
            .cloned()
            .collect();
        let description = if dropped.is_empty() {
            None
        } else {
            Some(fill(
                "dataTable.pasteDroppedColumns",
                &[
                    ("count", dropped.len().to_string()),
                    ("names", dropped.iter().take(5).cloned().collect::<Vec<_>>().join(", ")),
                ],
            ))
        };
        return PasteRowsDecision::NeedConfirm(ConfirmTexts {
            title: t("dataTable.pasteRowsCrossTableTitle").to_owned(),
            message: fill(
                "dataTable.pasteRowsCrossTableBody",
                &[
                    ("source", payload.table_key.clone()),
                    ("target", table_label.to_owned()),
                    ("matched", matched.to_string()),
                    ("total", payload.column_names.len().to_string()),
                ],
            ),
            description,
            confirm_label: t("dataTable.pasteRows").to_owned(),
            cancel_label: t("common.cancel").to_owned(),
        });
    }
    PasteRowsDecision::Ready(drafts)
}

/// Host-owned projection of the grid clipboard's mutable feedback state.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default)]
pub(crate) struct ClipboardVm {
    pub(crate) problems: Vec<PasteProblem>,
    /// `row:column` keys of cells the last paste reported a problem for.
    pub(crate) cell_errors: Vec<String>,
    pub(crate) last_paste: Option<PasteSummary>,
    pub(crate) copied: bool,
    pub(crate) copied_until: Option<f64>,
    /// Monotonic request id: a late failure must not erase a newer success.
    pub(crate) copy_request: u64,
}

#[allow(dead_code)] // pure state update; remove with the host.
pub(crate) fn report_problem(vm: &mut ClipboardVm, problem: PasteProblem) {
    if let (Some(row), Some(column)) = (problem.row.as_ref(), problem.column.as_ref()) {
        let key = format!("{row}:{column}");
        if !vm.cell_errors.contains(&key) {
            vm.cell_errors.push(key);
        }
    }
    if vm.problems.len() < MAX_REPORTED_PROBLEMS {
        vm.problems.push(problem);
    }
}

#[allow(dead_code)] // pure state update; remove with the host.
pub(crate) fn clear_paste_result(vm: &mut ClipboardVm) {
    vm.problems.clear();
    vm.cell_errors.clear();
    vm.last_paste = None;
}

/// React `copyFeedback.copy`'s optimistic start: light up now, record the
/// deadline, and bump the request id so older failures become stale.
#[allow(dead_code)] // pure state update; remove with the host.
pub(crate) fn begin_copy(vm: &mut ClipboardVm, now: f64) -> u64 {
    vm.copy_request = vm.copy_request.wrapping_add(1);
    vm.copied = true;
    vm.copied_until = Some(now + COPY_FEEDBACK_MS);
    vm.copy_request
}

/// React's request-bound rollback: a failure only clears a still-current
/// window; a stale failure leaves the newer copy's feedback alone.
#[allow(dead_code)] // pure state update; remove with the host.
pub(crate) fn end_copy(vm: &mut ClipboardVm, request_id: u64, ok: bool) {
    if !ok && request_id == vm.copy_request {
        vm.copied = false;
        vm.copied_until = None;
    }
}

/// React's native-copy path: it writes inside the gesture, so the host reports
/// success directly and this only owns the 1200ms window.
#[allow(dead_code)] // pure state update; remove with the host.
pub(crate) fn native_copied(vm: &mut ClipboardVm, ok: bool, now: f64) {
    if ok {
        vm.copied = true;
        vm.copied_until = Some(now + COPY_FEEDBACK_MS);
    } else {
        vm.copied = false;
        vm.copied_until = None;
    }
}

/// React's feedback window at the recorded deadline (host clock, no timer).
#[allow(dead_code)] // pure state update; remove with the host.
pub(crate) fn copy_visible(vm: &mut ClipboardVm, now: f64) -> bool {
    if vm.copied_until.is_some_and(|until| now >= until) {
        vm.copied = false;
        vm.copied_until = None;
    }
    vm.copied
}

/// React `useClipboardCopyFeedback`: an empty text (and no HTML flavor) is a
/// failure — never light up "copied" for a write that puts nothing there.
#[allow(dead_code)] // pure gate; remove with the host.
pub(crate) fn empty_copy_allowed(text: &str, html: Option<&str>) -> bool {
    !text.is_empty() || html.is_some_and(|rich| !rich.is_empty())
}
