//! `TableClipboardStatus`, ported 1:1 from
//! `packages/frontend/src/windows/connection/TableClipboardStatus.tsx` (DB-03).
//!
//! One line under the grid says how many cells moved, how many were skipped,
//! and whether staged changes were overwritten — plus the first problem with a
//! count. It never repeats pasted cell data. The Rune port keeps every decision
//! pure and emits `Dismiss` through an injected host port; the host owns the
//! feedback deadline (`visible_until`) and clears the snapshot in its poll, so
//! no timer or thread lives here.
//!
//! ## Removal stage
//!
//! No production host mounts this line yet; the table host will own the VM and
//! clear it on dismissal/expiry. Per-item allow annotations carry that removal
//! stage until then.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_X};

const ROOT: &str = "table-clipboard-status";
const MAX_PASTE_CELLS: &str = "100000";

/// React `PasteSummary` (the fields this line reads).
#[allow(dead_code)] // VM mirror; supplied by the clipboard host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PasteSummary {
    pub(crate) cells: usize,
    pub(crate) staged: usize,
    pub(crate) skipped: usize,
    pub(crate) overwritten_pending: usize,
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) cancelled: bool,
}

/// React `PasteProblem`.
#[allow(dead_code)] // VM mirror; supplied by the clipboard host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PasteProblem {
    pub(crate) code: String,
    pub(crate) row: Option<usize>,
    pub(crate) column: Option<String>,
    pub(crate) detail: Option<String>,
}

/// Host-owned projection of `TableClipboardBridge` plus the feedback deadline.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TableClipboardStatusVm {
    pub(crate) last_paste: Option<PasteSummary>,
    pub(crate) problems: Vec<PasteProblem>,
    /// Wall-clock deadline in seconds; `tick` clears the snapshot at expiry.
    pub(crate) visible_until: Option<f64>,
    pub(crate) now: f64,
}

/// React `hasResult`: staged or skipped cells actually moved.
#[allow(dead_code)] // pure helper; consumed by visibility and tests.
pub(crate) fn has_result(vm: &TableClipboardStatusVm) -> bool {
    vm.last_paste.as_ref().is_some_and(|paste| paste.staged > 0 || paste.skipped > 0)
}

/// React `visible`: a result, an overwritten-pending revert, or a problem.
#[allow(dead_code)] // pure helper; consumed by the view and tests.
pub(crate) fn visible(vm: &TableClipboardStatusVm) -> bool {
    let expired = vm.visible_until.is_some_and(|until| vm.now >= until);
    if expired {
        return false;
    }
    has_result(vm)
        || vm.last_paste.as_ref().is_some_and(|paste| paste.overwritten_pending > 0)
        || vm.problems.first().is_some()
}

/// Host poll hook for the recorded deadline (React's host clears the bridge).
/// Returns true when visible state changed.
#[allow(dead_code)] // host hook; removed with the table wiring.
pub(crate) fn tick(vm: &mut TableClipboardStatusVm, now: f64) -> bool {
    vm.now = now;
    if vm.visible_until.is_some_and(|until| now >= until) {
        vm.visible_until = None;
        vm.last_paste = None;
        vm.problems.clear();
        return true;
    }
    false
}

/// React `describePasteProblem`: exact locale keys, never pasted cell values.
#[allow(dead_code)] // pure helper; consumed by the status line and tests.
pub(crate) fn describe_problem(problem: &PasteProblem) -> String {
    let detail = problem.detail.as_deref().unwrap_or("");
    match problem.code.as_str() {
        "grid.paste.empty" => t("dataTable.pasteEmpty").to_owned(),
        "grid.paste.tooLarge" => t("dataTable.pasteTooLarge")
            .replace("{cells}", detail)
            .replace("{maxCells}", MAX_PASTE_CELLS),
        "grid.paste.raggedRow" => {
            if detail.is_empty() {
                t("dataTable.pasteRaggedRow")
                    .replace("{row}", "")
                    .replace("{count}", "")
                    .replace("{expected}", "")
                    .to_owned()
            } else {
                detail.to_owned()
            }
        }
        "grid.paste.unclosedQuote" => t("dataTable.pasteUnclosedQuote").to_owned(),
        "grid.paste.outOfBounds" => t("dataTable.pasteClippedToPage").replace("{count}", detail),
        "grid.paste.noWritableCell" => t("dataTable.pasteNoWritableColumn").to_owned(),
        "grid.paste.notNullEmpty" => t("dataTable.pasteNotAllowedEmpty")
            .replace("{row}", &(problem.row.unwrap_or(0) + 1).to_string())
            .replace("{column}", problem.column.as_deref().unwrap_or("")),
        "grid.paste.identityChanged" => t("dataTable.pasteIdentityChanged")
            .replace("{row}", &(problem.row.unwrap_or(0) + 1).to_string())
            .replace("{column}", problem.column.as_deref().unwrap_or("")),
        "grid.paste.columnMismatch" => t("dataTable.pasteDroppedColumns")
            .replace("{count}", &detail.split(", ").filter(|part| !part.is_empty()).count().to_string())
            .replace("{names}", detail),
        "grid.paste.crossTable" => t("dataTable.pasteCrossTableNotice").replace("{table}", detail),
        "grid.cell.readOnly" => match problem.column.as_deref() {
            Some(column) => t("dataTable.pasteReadOnlyColumn")
                .replace("{count}", "1")
                .replace("{columns}", column),
            None => t("dataTable.readOnlyPasteDisabled").to_owned(),
        },
        "grid.cell.noRowIdentity" => t("dataTable.pasteNoRowIdentity").to_owned(),
        "grid.clipboard.writeFailed" => t("dataTable.copyFailed").to_owned(),
        "grid.clipboard.readFailed" => t("dataTable.pasteReadFailed").replace("{shortcut}", "⌘V"),
        "grid.pasteRows.notRowData" => t("dataTable.pasteRowsNotRowData").to_owned(),
        "grid.pasteRows.noWritableColumn" => t("dataTable.pasteRowsNoWritableColumn").to_owned(),
        // Unknown codes keep their raw detail instead of being swallowed.
        _ => if detail.is_empty() { problem.code.clone() } else { detail.to_owned() },
    }
}

/// The message segments React renders, in order (no cell values repeated).
#[allow(dead_code)] // pure helper; consumed by the line and tests.
pub(crate) fn message_segments(vm: &TableClipboardStatusVm) -> Vec<String> {
    let mut segments = Vec::new();
    if let Some(paste) = vm.last_paste.as_ref() {
        if paste.staged > 0 || paste.skipped > 0 {
            segments.push(
                t("dataTable.pasteResult")
                    .replace("{count}", &paste.staged.to_string())
                    .replace("{skipped}", &paste.skipped.to_string()),
            );
        }
    }
    if let Some(paste) = vm.last_paste.as_ref() {
        if paste.overwritten_pending > 0 {
            segments.push(
                t("dataTable.pasteOverwrotePending")
                    .replace("{count}", &paste.overwritten_pending.to_string()),
            );
        }
    }
    if let Some(problem) = vm.problems.first() {
        let mut description = describe_problem(problem);
        if vm.problems.len() > 1 {
            description.push_str(&format!(" (+{})", vm.problems.len() - 1));
        }
        segments.push(description);
    }
    segments
}

/// The host intents this line emits.
#[allow(dead_code)] // host port; removed with the table wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableClipboardAction {
    Dismiss,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, TableClipboardAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: TableClipboardAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the one-line status plus the host-supplied confirm element.
#[allow(dead_code)] // consumer: the table host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &TableClipboardStatusVm,
    confirm_element: Option<El>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = crate::ui::kit::kit(dark);
    let mut root = col().key("table-clipboard-slot").w_full().min_w(0.0);
    if let Some(confirm) = confirm_element {
        root = root.child(confirm);
    }
    if visible(vm) {
        let staged = vm.last_paste.as_ref().map_or(0, |paste| paste.staged);
        let skipped = vm.last_paste.as_ref().map_or(0, |paste| paste.skipped);
        let mut line = row()
            .key(ROOT)
            .role("status")
            .w_full()
            .gap(8.0)
            .center_y()
            .px(12.0)
            .py(4.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                icon(if vm.problems.is_empty() { &ICON_CHECK } else { &ICON_X }, 12.0)
                    .color(if vm.problems.is_empty() { k.c.success.icon } else { k.c.danger.icon }),
            );
        for segment in message_segments(vm) {
            line = line.child(text(segment).color(k.c.text_muted).truncate().min_w(0.0));
        }
        line = line
            .child(row().grow(1.0).min_w(0.0))
            .child(
                text("×")
                    .key("table-clipboard-status-dismiss")
                    .role("button")
                    .focusable(true)
                    .label(t("common.close"))
                    .on_press(emit(&on_action, TableClipboardAction::Dismiss)),
            );
        // React's data-* attributes are mirrored by hidden keyed markers.
        line = line
            .child(el().key(format!("paste-data-staged-{staged}")).a11y_hidden().label(staged.to_string()))
            .child(el().key(format!("paste-data-skipped-{skipped}")).a11y_hidden().label(skipped.to_string()));
        root = root.child(line);
    }
    root
}
