//! Port of `components/sql-editor/extensions/executionState.ts` and
//! `extensions/types.ts` (T170) — the statement frame/gutter/execution
//! extensions' shared contracts.
//!
//! ## What maps where
//!
//! - `executionState.ts` is a re-export shell over extension-points'
//! `sql-editor/executionState.ts`: the StateField (status, target range,
//! document version) and its three effects. The CodeMirror StateField
//! becomes [`EditorExecutionState`] plus [`EditorExecutionState::update`]
//! — the field's `update` loop verbatim (the transaction's **first**
//! effect decides; later ones never run). `matchesRunningTarget` /
//! `isIdleOrCancelled` are the pure helpers, verbatim.
//! - `types.ts`'s re-export trio: `SqlTextRange` stays the single source
//! in `completion::semantic_model` (semanticTypes' shape, T062);
//! `SqlStatementRange` / `SqlExecutionTarget` land here (the same
//! semanticTypes contracts) because this shell is their first engine
//! consumer — the statement scanner itself (`buildStatementRanges`) is a
//! separate track.
//! - `ExecutionStatus` stays T105's single source in [`contracts`] (the
//!   three states are one vocabulary; `contracts.ts` and `types.ts`
//!   declare the same union).
//! - `isMacOS()` reads `navigator` in React; the engine takes the
//!   platform (the T386 seam, carried at `sql_editor_context_menu` since
//!   T155 — the React file itself points there for consistency).
//!   `executeShortcutLabel()` composes `formatModKey` + Enter the same
//!   way.
//! - `statementExtensions.test.ts`'s `createStatementExtensions` counting
//!   (2 core extensions vs 3 with the enhanced-EP registered) is
//!   CodeMirror assembly machinery — T139's `editor_extensions` seam
//!   table owns that registry decision; nothing to port here.
//!
//! Known deltas: `shouldHideFrame`'s `docLineCount` option is declared
//! but never read by the React body — carried verbatim as an unread
//! field (dropping it would change the call shape the host copies).

use crate::components::sql_editor::completion::SqlTextRange;
use crate::components::sql_editor::contracts::ExecutionStatus;
use crate::components::sql_editor::sql_editor_context_menu::{Platform, format_mod_key};

// ---------------------------------------------------------------------------
// types.ts — the re-export shell's semanticTypes contracts
// ---------------------------------------------------------------------------

/// `SqlStatementKind` (semanticTypes): the coarse statement vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the statement scanner track wires it
pub enum SqlStatementKind {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
    Ddl,
    Transaction,
    Other,
}

/// `confidence`: how the range was derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the statement scanner track wires it
pub enum SqlStatementConfidence {
    Exact,
    Degraded,
}

/// `SqlStatementRange` (semanticTypes): one statement's full geometry —
/// content span, delimiter span, the first executable line, kind hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the statement scanner track wires it
pub struct SqlStatementRange {
    pub from: usize,
    pub to: usize,
    pub index: usize,
    pub content_from: usize,
    pub content_to: usize,
    pub delimiter_from: Option<usize>,
    pub delimiter_to: Option<usize>,
    pub first_executable_line: usize,
    pub kind_hint: Option<SqlStatementKind>,
    pub confidence: SqlStatementConfidence,
}

/// `SqlExecutionTarget['source']` — where the execute request came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the execute dispatch track wires it
pub enum SqlExecutionSource {
    Selection,
    CurrentStatement,
    Gutter,
    All,
}

/// `SqlExecutionTarget` (semanticTypes): what an execute dispatch carries.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; the execute dispatch track wires it
pub struct SqlExecutionTarget {
    pub source: SqlExecutionSource,
    pub sql: String,
    pub range: Option<SqlTextRange>,
    pub statement_index: Option<usize>,
    pub document_version: u64,
}

// ---------------------------------------------------------------------------
// types.ts — the pure frame helpers
// ---------------------------------------------------------------------------

/// The default degrade threshold (React `isDocumentDegraded`'s
/// `maxLineCount = 500`).
pub const DEGRADE_MAX_LINES_DEFAULT: usize = 500;

/// `isDocumentDegraded`: a document beyond the threshold degrades the
/// statement frame. Pure — no DOM / CodeMirror dependency (extracted for
/// testability, the React comment says the same).
pub fn is_document_degraded(line_count: usize, max_line_count: usize) -> bool {
    line_count > max_line_count
}

/// `shouldHideFrame`'s options, verbatim (including the declared-but-
/// unread `doc_line_count`).
pub struct ShouldHideFrameOpts<'a> {
    pub degraded: bool,
    pub selection_empty: bool,
    pub selection_from: usize,
    pub selection_to: usize,
    /// Declared by the React type, never read by the body — kept so the
    /// call shape matches.
    pub doc_line_count: usize,
    pub doc_text: &'a str,
}

/// `shouldHideFrame`: the frame hides when the document is degraded, or
/// when a non-empty selection spans more than one line (a newline inside
/// the selected text).
pub fn should_hide_frame(opts: &ShouldHideFrameOpts<'_>) -> bool {
    if opts.degraded {
        return true;
    }
    if !opts.selection_empty && opts.selection_from != opts.selection_to {
        // Check if the selection spans more than one line by counting
        // newlines (byte offsets, the CodeMirror currency).
        let from = opts.selection_from.min(opts.selection_to);
        let to = opts.selection_from.max(opts.selection_to);
        let text = &opts.doc_text[from.min(opts.doc_text.len())..to.min(opts.doc_text.len())];
        if text.contains('\n') {
            return true;
        }
    }
    false
}

/// `isMacOS()` — platform detection, consistent with
/// `lib/sqlEditorContextMenu.ts` (the React comment); the engine takes
/// the platform the host resolved instead of reading `navigator`.
pub fn is_macos(platform: Platform) -> bool {
    platform == Platform::Macos
}

/// `executeShortcutLabel()` — the platform-aware shortcut label for the
/// execute action (`⌘+Enter` on macOS, `Ctrl+Enter` elsewhere).
pub fn execute_shortcut_label(platform: Platform) -> String {
    format!("{}+Enter", format_mod_key(platform))
}

// ---------------------------------------------------------------------------
// executionState.ts — the state field and its effects
// ---------------------------------------------------------------------------

/// `EditorExecutionState`: the immutable snapshot of the running
/// execution (§Track S4-A step 6 — the spinner matches
/// documentVersion + targetRange).
pub type EditorExecutionState = EditorExecutionStateValue;

/// The state value (React's `EditorExecutionState` shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorExecutionStateValue {
    pub status: ExecutionStatus,
    pub target_range: Option<SqlTextRange>,
    pub document_version: Option<u64>,
}

/// `INITIAL_EXECUTION_STATE`.
pub const INITIAL_EXECUTION_STATE: EditorExecutionState = EditorExecutionState {
    status: ExecutionStatus::Idle,
    target_range: None,
    document_version: None,
};

/// The field's effects: Start (idle → running with the pinned target),
/// Cancel (→ cancelling, target kept), Finish (→ idle, everything
/// cleared).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionEffect {
    /// `StartExecutionEffect.of({ targetRange, documentVersion })`.
    Start { target_range: Option<SqlTextRange>, document_version: u64 },
    /// `CancelExecutionEffect.of()`.
    Cancel,
    /// `FinishExecutionEffect.of()`.
    Finish,
}

impl EditorExecutionStateValue {
    /// The StateField's `update`: walk the transaction's effects and
    /// return at the **first** one that matches (a transaction carrying
    /// several effects lets the first decide — the React loop's shape).
    pub fn update(&self, effects: &[ExecutionEffect]) -> Self {
        for effect in effects {
            match effect {
                ExecutionEffect::Start { target_range, document_version } => {
                    return Self {
                        status: ExecutionStatus::Running,
                        target_range: *target_range,
                        document_version: Some(*document_version),
                    };
                }
                ExecutionEffect::Cancel => {
                    return Self { status: ExecutionStatus::Cancelling, ..*self };
                }
                ExecutionEffect::Finish => {
                    return INITIAL_EXECUTION_STATE;
                }
            }
        }
        *self
    }
}

/// `matchesRunningTarget` (aka `isStatementRunning`): the spinner shows
/// only when the status is running AND the documentVersion AND the
/// targetRange match exactly.
pub fn matches_running_target(
    state: &EditorExecutionState,
    document_version: u64,
    range: SqlTextRange,
) -> bool {
    if state.status != ExecutionStatus::Running {
        return false;
    }
    if state.document_version != Some(document_version) {
        return false;
    }
    let Some(target) = state.target_range else {
        return false;
    };
    target.from == range.from && target.to == range.to
}

/// `isIdleOrCancelled`: the execution is no longer running (idle or
/// cancelling).
pub fn is_idle_or_cancelled(state: &EditorExecutionState) -> bool {
    matches!(state.status, ExecutionStatus::Idle | ExecutionStatus::Cancelling)
}
