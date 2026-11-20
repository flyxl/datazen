//! Port of `packages/frontend/src/hooks/useGridClipboard.ts` (T294) —
//! the grid's clipboard orchestration (DB-03 §5 Step 7).
//!
//! Everything that needs both worlds lives here: the selection model's
//! coordinates, the panel's `ColumnSchema`, the store's batch staging
//! and the confirmation dialog. `DataTable` only reports gestures (it
//! owns no schema), the table view only wires it up.
//!
//! ## What maps where
//!
//! - **The hook IS its state** (the confirm-dialog / copy-feedback /
//!   column-stats precedent): React's `useState` collection becomes
//!   [`GridClipboardHook`], and every `useCallback`/`useMemo` becomes a
//!   method taking the per-frame [`GridClipboardEnv`] — in immediate
//!   mode there is no closure to capture, the environment is re-supplied
//!   on every call.
//! - **Order matters and is enforced**: *plan → confirm → stage*. The
//!   async `await confirm(...)` becomes the [`PasteFlow`] state machine:
//!   `ConfirmCellPaste` holds the plan until the dialog answers
//!   ([`Self::on_confirm_answer`]), `ChunkedCellPaste` stages
//!   [`GRID_PASTE_CHUNK_CELLS`] cells per [`Self::pump`] (React's
//!   `await nextFrame()` — the host pumps once per frame while
//!   [`Self::staging_in_progress`]), `ConfirmRowsPaste` holds the
//!   verified row payload the same way. A second `begin_paste` while a
//!   flow is open supersedes it — exactly what React's replaced
//!   `setState`/leaked promise did.
//! - `await requirePlatformServices().readClipboard()` is host
//!   transport: [`Self::begin_paste_rows`] answers `NeedClipboard`, the
//!   host reads and reports back through [`Self::on_rows_clipboard_text`]
//!   (or [`Self::on_clipboard_read_failed`] — a missing binding or
//!   a denied permission is reported, never guessed around, B23).
//! - `copyFeedback.copy` is the ported [`ClipboardCopyFeedback`]
//!   machine: the copy methods arm the window and hand the host the
//!   write to perform plus its request id; the host reports the outcome
//!   through [`Self::on_copy_result`] (a failure also reports
//!   `writeFailed`, as React's awaiting continuation did).
//! - `stagePasteWrites` / `addDraftRows` / `onProblem` /
//!   `showReadOnlyTip` are injected actions, supplied per env — the
//!   panel's action table owns them (result grids omit `addDraftRows`
//!   and "paste rows" is simply not offered there).
//! - `nextFrame`'s hidden-frame timer guard has no Rust counterpart:
//!   the host's frame clock *is* the pump; there is no batch to stall.
//! - The native-copy feedback (`nativeCopied` + `nativeTimerRef`) rides
//!   this hook's own `native_clear_at_ms` deadline, cleared by
//!   [`Self::tick`] (the host's `wake_at`, from [`Self::wake_at_ms`]).
//! - `confirmElement` is [`Self::confirm`] + the `ConfirmDialogHost`
//!   the panel paints once (the `useConfirmDialog` port). The option
//!   builders live in `confirm_options.rs`; the summary shape, the
//!   copy-target vocabulary, `rowsToDraftValues` and
//!   `describePasteProblem` in `summary.rs`.
//! - `useI18n()` is `crate::i18n::t` (the T0-03 base; the `useI18n.ts`
//!   wrapper is T341's and changes nothing here).
//! - Plan problems are reported **after** the user agrees, because those
//!   lines describe what happened; the cross-table notice reads "pasted
//!   from another table into X", which is a lie once the user answers
//!   "no" — and the dialog already said it before the decision.
//!
//! ## Keys
//!
//! No elements are painted here (the hook is the machine; the panel and
//! `ConfirmDialogHost` paint). `cellErrors` keys are
//! `${rowIndex}:${columnName}` ([`pending_cell_key`]).

mod confirm_options;
mod copy_paths;
mod summary;

pub(crate) use summary::{
    describe_paste_problem, rows_to_draft_values, GridCopyTarget, PasteSummary, RowsPasteStart,
};

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::hooks::confirm_dialog::ConfirmDialogState;
use crate::hooks::grid_clipboard_helpers::{
    paste_code, parse_clipboard_matrix, ClipboardParseResult, GridClipboardAnchor,
    GridClipboardRange, PasteProblem, GRID_PASTE_CHUNK_CELLS, GRID_PASTE_CONFIRM_CELLS,
};
use crate::hooks::grid_paste_plan_helpers::{
    pending_cell_key, plan_paste, row_copy_for_clipboard_text, source_for_clipboard_text,
    table_change_key, CellPasteWrite, PasteOverflow, PastePlan, PastePlanOptions, RowCopyPayload,
    StagePasteWritesResult,
};
use crate::hooks::use_clipboard_copy_feedback::ClipboardCopyFeedback;
use crate::i18n::t;
use crate::table_data_connection_state::{
    ColumnSchema, GridSelection, PendingRowChange, TableChangeContext,
};

use confirm_options::{cell_paste_confirm_options, rows_paste_confirm_options};
use summary::MAX_REPORTED_PROBLEMS;

/// The per-frame environment: the React hook's options, re-supplied on
/// every call instead of captured. `stage_paste_writes` stages the
/// block; `add_draft_rows` appends prefilled draft rows (DBR-02) and is
/// `None` on result grids, where "paste rows" is simply not offered.
pub(crate) struct GridClipboardEnv<'a> {
    pub is_editable: bool,
    pub columns: &'a [ColumnSchema],
    /// Displayed column names in render order.
    pub column_order: &'a [String],
    /// The panel's row records, by column name.
    pub rows: &'a [HashMap<String, Value>],
    pub pending_changes: &'a HashMap<String, PendingRowChange>,
    /// Row index → staged identity key; used only to count overwritten
    /// staged changes.
    pub row_identity_anchors: &'a HashMap<usize, String>,
    pub selected_rows: &'a HashSet<usize>,
    pub grid_selection: &'a GridSelection,
    pub context: Option<&'a TableChangeContext>,
    /// Human-readable name of the target table, for the cross-table
    /// confirmation.
    pub table_label: &'a str,
    pub overflow: PasteOverflow,
    pub stage_paste_writes: &'a dyn Fn(&[CellPasteWrite]) -> StagePasteWritesResult,
    /// Report one problem (the owner renders it; null cells are
    /// highlighted by the grid).
    pub on_problem: Option<&'a dyn Fn(&PasteProblem)>,
    /// "Read-only" refusal, shared with the cell editor's tip.
    pub show_read_only_tip: Option<&'a dyn Fn()>,
    /// Append prefilled draft rows; returns their draft ids.
    pub add_draft_rows: Option<&'a dyn Fn(&[HashMap<String, Option<Value>>]) -> Vec<String>>,
    /// The frame's clock (epoch ms) — arms and clears feedback windows.
    pub now_ms: i64,
}

/// The in-flight paste (React's suspended `async` continuation, made
/// explicit so the host can drive it across frames).
#[derive(Debug, Default)]
enum PasteFlow {
    #[default]
    Idle,
    /// `await confirm(...)` for a cell paste: nothing is staged until
    /// the user answers.
    ConfirmCellPaste { plan: PastePlan },
    /// Staging chunk by chunk (`await nextFrame()` between them).
    ChunkedCellPaste { plan: PastePlan, next_chunk: usize, staged: usize, skipped: usize },
    /// `await confirm(...)` for a cross-table "paste rows".
    ConfirmRowsPaste { payload: RowCopyPayload },
}

/// The useGridClipboard machine (the hook IS its state).
#[derive(Debug, Default)]
pub(crate) struct GridClipboardHook {
    /// The owned `useConfirmDialog` (the paste flows' gate).
    confirm: ConfirmDialogState,
    /// The owned `useClipboardCopyFeedback` (the async copy path).
    copy_feedback: ClipboardCopyFeedback,
    /// Cells the last paste reported a problem for, as `row:column`.
    cell_errors: HashSet<String>,
    problems: Vec<PasteProblem>,
    last_paste: Option<PasteSummary>,
    /// The **native** copy path's feedback: that path writes the
    /// clipboard itself inside the gesture, so it needs the timer but
    /// not the write.
    native_copied: bool,
    native_clear_at_ms: Option<i64>,
    flow: PasteFlow,
}

impl GridClipboardHook {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The confirm dialog's state (paint it via `ConfirmDialogHost`).
    pub(crate) fn confirm(&self) -> &ConfirmDialogState {
        &self.confirm
    }

    /// `copyFeedback.copied`: native or async, whichever lit up.
    pub(crate) fn copied(&self) -> bool {
        self.native_copied || self.copy_feedback.copied()
    }

    pub(crate) fn cell_errors(&self) -> &HashSet<String> {
        &self.cell_errors
    }

    pub(crate) fn problems(&self) -> &[PasteProblem] {
        &self.problems
    }

    pub(crate) fn last_paste(&self) -> Option<PasteSummary> {
        self.last_paste
    }

    /// `canUseRowCopy`: editable + draft-capable.
    pub(crate) fn can_use_row_copy(env: &GridClipboardEnv<'_>) -> bool {
        env.is_editable && env.add_draft_rows.is_some()
    }

    /// `handlers.canCopy`: a selection exists.
    pub(crate) fn can_copy(&self, env: &GridClipboardEnv<'_>) -> bool {
        self.selection_range(env).is_some() || !self.covered_rows(env).is_empty()
    }

    /// `handlers.canPaste`: editable + a cell region.
    pub(crate) fn can_paste(&self, env: &GridClipboardEnv<'_>) -> bool {
        env.is_editable && self.selection_range(env).is_some()
    }

    /// `pendingKeys`: `(rowIndex, column)` keys already staged, to count
    /// what the paste will overwrite.
    fn pending_keys(&self, env: &GridClipboardEnv<'_>) -> HashSet<String> {
        let mut keys = HashSet::new();
        for (row_index, identity_key) in env.row_identity_anchors {
            let Some(change) = env.pending_changes.get(identity_key) else {
                continue;
            };
            for column in &change.changed_columns {
                keys.insert(pending_cell_key(*row_index, column));
            }
        }
        keys
    }

    /// `report`: mark the cell, cap the stored list, hand the problem
    /// to the owner.
    fn report(&mut self, env: &GridClipboardEnv<'_>, problem: PasteProblem) {
        if problem.row.is_some() && problem.column.is_some() {
            let key = pending_cell_key(
                problem.row.unwrap_or_default(),
                problem.column.as_deref().unwrap_or_default(),
            );
            self.cell_errors.insert(key);
        }
        if self.problems.len() < MAX_REPORTED_PROBLEMS {
            self.problems.push(problem.clone());
        }
        if let Some(on_problem) = env.on_problem {
            on_problem(&problem);
        }
    }

    /// `clearPasteResult`.
    pub(crate) fn clear_paste_result(&mut self) {
        self.problems.clear();
        self.cell_errors.clear();
        self.last_paste = None;
    }

    /// The frame clock: clears the native feedback window and drives
    /// the copy-feedback machine (the host's `wake_at`).
    pub(crate) fn tick(&mut self, now_ms: i64) {
        if self.native_copied && self.native_clear_at_ms.is_some_and(|clear_at| now_ms >= clear_at)
        {
            self.native_copied = false;
            self.native_clear_at_ms = None;
        }
        self.copy_feedback.tick(now_ms);
    }

    /// The host's report of an async clipboard write's outcome. A
    /// failure also reports `writeFailed` — React's awaiting
    /// continuation did the same for its own call's `false`, stale or
    /// not (the feedback window itself is id-guarded).
    pub(crate) fn on_copy_result(&mut self, env: &GridClipboardEnv<'_>, request_id: u64, ok: bool) {
        self.copy_feedback.on_copy_result(request_id, ok);
        if !ok {
            self.report(env, PasteProblem::new(paste_code::CLIPBOARD_WRITE_FAILED));
        }
    }

    /// The next frame's wake deadline, when one is armed (the feedback
    /// windows; the chunk pump is frame-driven by the host).
    pub(crate) fn wake_at_ms(&self) -> Option<i64> {
        self.native_clear_at_ms.min(self.copy_feedback.clear_at_ms())
    }

    /// `pasteText` (the paste flow itself): plan → confirm → stage.
    /// Always consumes the paste (the native handler already called
    /// `preventDefault`); the return is React's resolved `true`.
    pub(crate) fn begin_paste(
        &mut self,
        env: &GridClipboardEnv<'_>,
        text: &str,
        anchor: &GridClipboardAnchor,
    ) -> bool {
        self.problems.clear();
        self.cell_errors.clear();
        // A superseded flow is dropped wholesale — React's replaced
        // request could never land its continuation.
        self.flow = PasteFlow::Idle;
        if !env.is_editable {
            // Consume the event (so nothing else pastes into the page)
            // but write nothing.
            if let Some(show_read_only_tip) = env.show_read_only_tip {
                show_read_only_tip();
            }
            self.report(
                env,
                PasteProblem {
                    code: paste_code::CELL_READ_ONLY,
                    row: None,
                    column: None,
                    detail: Some("readOnly".to_owned()),
                },
            );
            return true;
        }

        let parsed = parse_clipboard_matrix(text);
        let matrix = match parsed {
            ClipboardParseResult::Ok { matrix, problems } => {
                for problem in problems {
                    self.report(env, problem);
                }
                matrix
            }
            ClipboardParseResult::Refused {
                code, cells, row, expected, actual, ..
            } => {
                if code == paste_code::RAGGED_ROW {
                    self.report(
                        env,
                        PasteProblem {
                            code,
                            row: None,
                            column: None,
                            detail: Some(
                                t("dataTable.pasteRaggedRow")
                                    .replace(
                                        "{row}",
                                        &(row.map(|row| row + 1).unwrap_or_default()).to_string(),
                                    )
                                    .replace("{count}", &actual.unwrap_or_default().to_string())
                                    .replace("{expected}", &expected.unwrap_or_default().to_string()),
                            ),
                        },
                    );
                } else {
                    self.report(
                        env,
                        PasteProblem {
                            code,
                            row: None,
                            column: None,
                            detail: Some(cells.to_string()),
                        },
                    );
                }
                return true;
            }
        };

        let pending_keys = self.pending_keys(env);
        let region = self.selection_range(env).map(|range| GridClipboardRange {
            top_row: range.start.row_index,
            bottom_row: range.end.row_index,
            left_column: range.start.column_name.clone(),
            right_column: range.end.column_name.clone(),
        });
        // Only a source the clipboard still holds may steer alignment or
        // waive the confirmation; a stale one is treated exactly like an
        // external paste.
        let source = source_for_clipboard_text(text);
        let plan = plan_paste(&PastePlanOptions {
            matrix: &matrix,
            anchor,
            column_order: env.column_order,
            columns: env.columns,
            region,
            page_row_count: env.rows.len(),
            overflow: env.overflow,
            null_literal: "NULL",
            pending_cell_keys: Some(&pending_keys),
            source: source.as_ref(),
            context: env.context,
        });

        if plan.writes.is_empty() {
            self.report_plan_problems(env, &plan);
            self.last_paste = Some(PasteSummary {
                cells: 0,
                staged: 0,
                skipped: plan.skipped,
                overwritten_pending: 0,
                rows: 0,
                columns: 0,
                cancelled: false,
            });
            return true;
        }

        if plan.requires_confirmation || plan.writes.len() > GRID_PASTE_CONFIRM_CELLS {
            let options = cell_paste_confirm_options(env, &plan);
            self.confirm.open(options);
            self.flow = PasteFlow::ConfirmCellPaste { plan };
            return true;
        }

        self.report_plan_problems(env, &plan);
        self.start_chunks(env, plan);
        true
    }

    /// `reportPlanProblems`: the plan's own diagnostics, reported only
    /// once the paste actually happens (they describe what *was* done).
    fn report_plan_problems(&mut self, env: &GridClipboardEnv<'_>, plan: &PastePlan) {
        if plan.cross_table != Some(false) {
            self.report(
                env,
                PasteProblem {
                    code: paste_code::CROSS_TABLE,
                    row: None,
                    column: None,
                    detail: Some(env.table_label.to_owned()),
                },
            );
        }
        for problem in &plan.problems {
            self.report(env, problem.clone());
        }
    }

    /// Enter the chunked staging loop and stage the first chunk (React
    /// stages the first chunk synchronously inside `pasteText`).
    fn start_chunks(&mut self, env: &GridClipboardEnv<'_>, plan: PastePlan) {
        self.flow = PasteFlow::ChunkedCellPaste { plan, next_chunk: 0, staged: 0, skipped: 0 };
        self.pump(env);
    }

    /// Stage the next chunk. Returns whether another pump is needed
    /// (React's `await nextFrame()` between chunks).
    pub(crate) fn pump(&mut self, env: &GridClipboardEnv<'_>) -> bool {
        // Snapshot the chunk first so `&mut self.flow` does not pin the
        // whole hook across the staging call and the reports.
        let (chunk, end, total_writes) = {
            let PasteFlow::ChunkedCellPaste { plan, next_chunk, .. } = &self.flow else {
                return false;
            };
            let total = plan.writes.len();
            if *next_chunk >= total {
                return false;
            }
            let end = (*next_chunk + GRID_PASTE_CHUNK_CELLS).min(total);
            (plan.writes[*next_chunk..end].to_vec(), end, total)
        };
        let result = (env.stage_paste_writes)(&chunk);
        let more = end < total_writes;
        {
            let PasteFlow::ChunkedCellPaste { next_chunk, staged, skipped, .. } = &mut self.flow
            else {
                return false;
            };
            *next_chunk = end;
            *staged += result.staged;
            *skipped += result.skipped;
        }
        let staged_problems: Vec<PasteProblem> = result
            .problems
            .into_iter()
            .map(|problem| PasteProblem {
                code: problem.code,
                row: Some(problem.row_index),
                column: Some(problem.column_name),
                detail: problem.detail,
            })
            .collect();
        for problem in staged_problems {
            self.report(env, problem);
        }
        if !more {
            let flow = std::mem::replace(&mut self.flow, PasteFlow::Idle);
            let PasteFlow::ChunkedCellPaste { plan, staged, skipped, .. } = flow else {
                return false;
            };
            self.last_paste = Some(self.chunked_summary(env, &plan, staged, skipped));
        }
        more
    }

    /// The finished chunked paste's summary (rows/columns from the
    /// plan's bounds, in render positions).
    fn chunked_summary(
        &self,
        env: &GridClipboardEnv<'_>,
        plan: &PastePlan,
        staged: usize,
        skipped: usize,
    ) -> PasteSummary {
        PasteSummary {
            cells: plan.writes.len(),
            staged,
            skipped,
            overwritten_pending: plan.overwritten_pending,
            rows: plan
                .bounds
                .as_ref()
                .map_or(0, |bounds| bounds.bottom_row - bounds.top_row + 1),
            columns: plan.bounds.as_ref().map_or(0, |bounds| {
                let left = env
                    .column_order
                    .iter()
                    .position(|name| *name == bounds.left_column);
                let right = env
                    .column_order
                    .iter()
                    .position(|name| *name == bounds.right_column);
                match (left, right) {
                    (Some(left), Some(right)) => right - left + 1,
                    _ => 0,
                }
            }),
            cancelled: false,
        }
    }

    /// Whether a chunked paste still has work (the host pumps until
    /// false).
    pub(crate) fn staging_in_progress(&self) -> bool {
        matches!(self.flow, PasteFlow::ChunkedCellPaste { .. })
    }

    /// The dialog's answer (the host's dialog buttons resolve the
    /// pending request). A stale answer — superseded by a newer
    /// `begin_paste` — lands nothing, exactly as React's replaced
    /// resolver could never fire.
    pub(crate) fn on_confirm_answer(&mut self, env: &GridClipboardEnv<'_>, confirmed: bool) {
        let flow = std::mem::replace(&mut self.flow, PasteFlow::Idle);
        match flow {
            PasteFlow::ConfirmCellPaste { plan } => {
                if self.confirm.answer_pending(confirmed).is_none() {
                    // Superseded: restore and ignore.
                    self.flow = PasteFlow::ConfirmCellPaste { plan };
                    return;
                }
                if !confirmed {
                    self.last_paste = Some(PasteSummary {
                        cells: plan.writes.len(),
                        staged: 0,
                        skipped: 0,
                        overwritten_pending: plan.overwritten_pending,
                        rows: 0,
                        columns: 0,
                        cancelled: true,
                    });
                    return;
                }
                self.report_plan_problems(env, &plan);
                self.start_chunks(env, plan);
            }
            PasteFlow::ConfirmRowsPaste { payload } => {
                if self.confirm.answer_pending(confirmed).is_none() {
                    self.flow = PasteFlow::ConfirmRowsPaste { payload };
                    return;
                }
                if confirmed {
                    self.apply_rows_payload(env, &payload);
                }
            }
            other => self.flow = other,
        }
    }

    /// `pasteRowsAsDrafts` (DBR-02): a verified DataZen row copy becomes
    /// new draft rows. The clipboard read is host transport.
    pub(crate) fn begin_paste_rows(&mut self, env: &GridClipboardEnv<'_>) -> RowsPasteStart {
        if env.add_draft_rows.is_none() {
            return RowsPasteStart::NotOffered;
        }
        self.problems.clear();
        self.cell_errors.clear();
        self.flow = PasteFlow::Idle;
        if !env.is_editable {
            if let Some(show_read_only_tip) = env.show_read_only_tip {
                show_read_only_tip();
            }
            self.report(
                env,
                PasteProblem {
                    code: paste_code::CELL_READ_ONLY,
                    row: None,
                    column: None,
                    detail: Some("readOnly".to_owned()),
                },
            );
            return RowsPasteStart::Consumed;
        }
        RowsPasteStart::NeedClipboard
    }

    /// The host's clipboard read failed: report it, never guess around
    /// it (B23) — there is no "fall back to something on the page".
    pub(crate) fn on_clipboard_read_failed(&mut self, env: &GridClipboardEnv<'_>) {
        self.report(env, PasteProblem::new(paste_code::CLIPBOARD_READ_FAILED));
    }

    /// The host's clipboard text (or `None`, when the platform answered
    /// nothing): verify the payload against it, then confirm or apply.
    pub(crate) fn on_rows_clipboard_text(
        &mut self,
        env: &GridClipboardEnv<'_>,
        text: Option<&str>,
    ) {
        let Some(payload) = text.and_then(row_copy_for_clipboard_text) else {
            self.report(env, PasteProblem::new(paste_code::PASTE_ROWS_NOT_ROW_DATA));
            return;
        };
        // What may be written: real target columns that are neither
        // primary key nor auto-increment (a new row gets its own
        // identity).
        let writable: HashSet<&str> = env
            .columns
            .iter()
            .filter(|column| !column.is_primary_key && !column.is_auto_increment)
            .map(|column| column.name.as_str())
            .collect();
        let matched_source: Vec<&str> = payload
            .column_names
            .iter()
            .filter(|name| writable.contains(name.as_str()))
            .map(String::as_str)
            .collect();
        if matched_source.is_empty() {
            self.report(env, PasteProblem::new(paste_code::PASTE_ROWS_NO_WRITABLE_COLUMN));
            return;
        }
        let dropped_columns: Vec<&str> = payload
            .column_names
            .iter()
            .filter(|name| !writable.contains(name.as_str()))
            .map(String::as_str)
            .collect();
        if payload.table_key != table_change_key(env.context) {
            let options =
                rows_paste_confirm_options(env, &payload, &matched_source, &dropped_columns);
            self.confirm.open(options);
            self.flow = PasteFlow::ConfirmRowsPaste { payload };
            return;
        }
        self.apply_rows_payload(env, &payload);
    }

    /// Turn a verified payload into drafts (the same-table path, and
    /// the confirmed cross-table path).
    fn apply_rows_payload(&mut self, env: &GridClipboardEnv<'_>, payload: &RowCopyPayload) {
        let Some(add_draft_rows) = env.add_draft_rows else {
            return;
        };
        let drafts = rows_to_draft_values(&payload.column_names, &payload.rows, env.columns);
        if !drafts.is_empty() {
            add_draft_rows(&drafts);
        }
    }
}
