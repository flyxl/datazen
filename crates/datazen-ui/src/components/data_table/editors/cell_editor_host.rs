//! The cell editor host, ported from React
//! `packages/frontend/src/components/DataTable/editors/CellEditorHost.tsx`
//! (T297).
//!
//! The one place that decides **when** a cell edit is written. The
//! per-family editors only edit a value; they never commit, never
//! cancel and never look at the keyboard. Everything about timing —
//! Enter, Escape, Tab, the blur, the IME composition — lives here, so
//! there is one state machine instead of one per control (05 §4.3).
//!
//! Three rules shape it (the React doc comment, verbatim in intent):
//!
//! 1. **The IME owns the keyboard.** While a composition is running,
//!    `Enter` confirms the candidate word and `Escape` dismisses it —
//!    neither may reach the grid.
//! 2. **A blur is not always the end.** Focus may move into a surface
//!    the editor owns, into the context menu, or out of an unfinished
//!    composition. None of those may write a value the user did not
//!    confirm.
//! 3. **Unmounting never writes.** A row that disappears takes its
//!    draft with it.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | the `useState` cells (`draft`, `error`, `candidates`, …) | [`CellEditorHostState`] — the host-held state machine, one per open editor |
//! | `commit()` / `cancel()` / `pick()` | [`CellEditorHostState::commit`] / [`Self::cancel`] / [`Self::pick`] — pure transitions returning [`EditAction`], the host applies them |
//! | `handleKeyDown` (256-277) | [`CellEditorHostState::key`] — the Enter/Escape/Tab rules with the composition guard |
//! | `handleBlur` (279-294) | [`CellEditorHostState::blur`] — the owned-surface / context-menu / composition guards |
//! | `isOwnedEditorTarget` (148-151) | [`is_owned_editor_target`] — the same four selectors as key predicates |
//! | the `mousedown` button-2 listener (306-312) | [`CellEditorHostState::context_menu_opened`] — the host calls it on the right press |
//! | `registerActiveCellEditor` (327-333) | the T405 port, real — [`CellEditorHost::register`] returns the registration whose Drop unregisters |
//! | `handleLoadCandidates` (336-353) | [`CellEditorHostState::candidates_*`] — the load state machine; the async halves are the host's seams |
//! | `temporalKindOf` (122-126) | [`temporal_kind_of`] |
//! | the eight-control switch (401-420) | [`CellEditorHost::control`] — binary and boolean real (T278), the other six labelled seams (T315/T330) |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | the host root (React test id) | `cell-editor` |
//! | the control mount (internal) | `cell-editor-control` |
//! | the inline error (React test id) | `cell-editor-error` |
//! | one pending-family seam | `cell-editor-seam-{kind}` |
//!
//! ## Documented deviations from React
//!
//! * **The blur's related target is a key predicate.** React reads
//!   `event.relatedTarget.closest(selector)` on four DOM selectors;
//!   Rune has no DOM, so [`is_owned_editor_target`] takes the four
//!   key predicates the ported surfaces carry — the editor's own
//!   root (`data-dt-editor-owned`), the shared select's listbox
//!   (`select-listbox`), the temporal picker's popover
//!   (`cell-editor-temporal-popover`), and any menu (`role=menu`).
//!   The host calls it with the target's key and role.
//! * **The `done` guard is the state's own field**, checked by every
//!   transition — the ref's own semantics, held as data.
//! * **The candidates' async halves are host seams**: the loader
//!   itself is async IPC; the state machine (loading → landed /
//!   failed with the message) is the port's.
//! * **The six pending controls mount as labelled seams** naming their
//!   owning tasks — the two ported controls (binary, boolean) are the
//!   real mounts.

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::Kit;
use serde_json::Value;

use crate::components::data_table::editable_cell::EditableCellKind;
use crate::components::data_table::editors::active_editor::{
    register_active_cell_editor, ActiveCellEditor, ActiveCellEditorRegistration,
};
use crate::components::data_table::editors::binary_cell_editor::BinaryCellEditor;
use crate::components::data_table::editors::boolean_cell_editor::BooleanCellEditor;
use crate::i18n::t;
use crate::support::cell_editor_kind::{
    coerce_value, CellEditorKind, CellValueErrorReason, CoerceInput,
};
use crate::support::cell_types::CellType;
use crate::support::to_edit_string::to_edit_string;
use crate::table_data_dirty_index::normalize_edit_value;

/// The temporal shape of a cell type, for the date editor (React
/// 122-126).
pub fn temporal_kind_of(cell_type: CellType) -> &'static str {
    match cell_type {
        CellType::Time => "time",
        CellType::DateTime => "datetime",
        _ => "date",
    }
}

/// Whether the focus moved into a surface the editor owns (React
/// 148-151) — the four selectors as key predicates.
pub fn is_owned_editor_target(key: Option<&str>, role: Option<&str>) -> bool {
    let key = key.unwrap_or_default();
    // The editor's own root, and anything it marks as its own
    // (`[data-dt-editor-owned]` — the host's root and its subtree).
    if key == "cell-editor" {
        return true;
    }
    // The shared select renders its option list into a portal
    // (`[data-testid="select-listbox"]`).
    if key == "select-listbox" {
        return true;
    }
    // The temporal picker renders its pop-up into a portal
    // (`[data-testid="cell-editor-temporal-popover"]`).
    if key == "cell-editor-temporal-popover" {
        return true;
    }
    // The host's web context menu (the "Set Value" menu,
    // `[role="menu"]`).
    role == Some("menu")
}

/// What one state-machine transition asked the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum EditAction {
    /// Nothing (guarded, composing, or already done).
    None,
    /// `onCancel()` — the editor closes without writing.
    Cancel,
    /// `onCommit(value)` — the write (already coerced and normalized),
    /// optionally with the move (`onCommitAndMove`).
    Commit {
        value: Option<Value>,
        direction: Option<EditMove>,
    },
    /// The input was refused: the reason stays in the state (the
    /// inline error) and the host reports it (`onInvalid`).
    Refused(CellValueErrorReason),
    /// The candidates loader should run.
    LoadCandidates,
}

/// `onCommitAndMove`'s direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMove {
    Next,
    Prev,
}

/// The host's state machine — React's `useState`/`useRef` cells as one
/// host-held struct.
#[derive(Clone, Debug)]
pub struct CellEditorHostState {
    /// `initial` — the seed text (re-seeded when the value changes).
    pub initial: String,
    /// `draft` — the control's text.
    pub draft: String,
    /// `error` — the last refusal (the inline error).
    pub error: Option<CellValueErrorReason>,
    /// `candidates` (the enum control's list).
    pub candidates: Option<Vec<String>>,
    /// `candidatesLoading`.
    pub candidates_loading: bool,
    /// `candidatesFailed`.
    pub candidates_failed: bool,
    /// `candidatesMessage` — a loader that explains itself is quoted.
    pub candidates_message: Option<String>,
    /// `done` — the editor is done; it must not write twice.
    pub done: bool,
    /// `composingRef` — an IME composition is running.
    pub composing: bool,
    /// `contextMenuRef` — a right-click is on its way.
    pub context_menu: bool,
    /// The value before the edit (the coercion's input).
    pub original_value: Value,
    /// The column accepts SQL `NULL`.
    pub nullable: bool,
    /// The column holds whole numbers.
    pub integer_only: bool,
    /// The resolved editor kind + the cell type (the coercion's input).
    pub kind: CellEditorKind,
    pub cell_type: CellType,
    /// The boolean editor's three-state select listbox open flag (the
    /// host's cell — the T278 contract; the toggle flips it).
    pub select_open: bool,
}

impl CellEditorHostState {
    /// The state over the resolved kind (`toEditString(originalValue)`
    /// seeds both the initial and the draft).
    pub fn new(kind: &EditableCellKind, original_value: &Value, nullable: bool) -> Self {
        Self::seed(kind.kind, kind.cell_type, original_value, nullable)
    }

    /// The seed itself (the value-change effect's re-seed, React
    /// 184-188).
    pub fn seed(
        kind: CellEditorKind,
        cell_type: CellType,
        original_value: &Value,
        nullable: bool,
    ) -> Self {
        let text = to_edit_string(original_value);
        Self {
            initial: text.clone(),
            draft: text,
            error: None,
            candidates: None,
            candidates_loading: false,
            candidates_failed: false,
            candidates_message: None,
            done: false,
            composing: false,
            context_menu: false,
            original_value: original_value.clone(),
            nullable,
            integer_only: false,
            kind,
            cell_type,
            select_open: false,
        }
    }

    /// The value-change effect (React 184-188): re-seed the initial
    /// and the draft from the new value.
    pub fn value_changed(&mut self, original_value: &Value) {
        let text = to_edit_string(original_value);
        self.initial = text.clone();
        self.draft = text;
        self.original_value = original_value.clone();
    }

    /// `cancel` (React 194-198): the editor closes without writing —
    /// at most once.
    pub fn cancel(&mut self) -> EditAction {
        if self.done {
            return EditAction::None;
        }
        self.done = true;
        EditAction::Cancel
    }

    /// `commit` (React 200-242): the write gate — nothing changed
    /// closes without writing; a refused coercion keeps the editor
    /// open with its inline error; a landed write sets done, clears
    /// the error, and commits (optionally moving).
    pub fn commit(&mut self, direction: Option<EditMove>) -> EditAction {
        if self.done {
            return EditAction::None;
        }
        // Nothing changed: the editor closes without writing anything.
        if self.draft == self.initial {
            self.done = true;
            return EditAction::Cancel;
        }
        let result = coerce_value(&CoerceInput {
            kind: self.kind,
            cell_type: self.cell_type,
            text: self.draft.as_str(),
            nullable: self.nullable,
            original_value: Some(&self.original_value),
            integer_only: self.integer_only,
        });
        match result {
            crate::support::cell_editor_kind::CoerceResult::Refused(reason) => {
                self.error = Some(reason);
                EditAction::Refused(reason)
            }
            crate::support::cell_editor_kind::CoerceResult::Ok(value) => {
                self.done = true;
                self.error = None;
                EditAction::Commit {
                    value: normalize_edit_value(Some(&value)),
                    direction,
                }
            }
        }
    }

    /// `pick` (React 245-254): a discrete choice (boolean / enum) is
    /// written right away — `onPick(value | null)`: `null` IS the SQL
    /// `NULL` (both arms write; `normalizeEditValue` passes the null
    /// through). The port's `Option<Value>` mirrors the T278 boolean
    /// editor's own channel, whose `None` is React's `null`.
    pub fn pick(&mut self, value: Option<Value>) -> EditAction {
        if self.done {
            return EditAction::None;
        }
        self.done = true;
        self.error = None;
        EditAction::Commit {
            value: normalize_edit_value(Some(&value.unwrap_or(Value::Null))),
            direction: None,
        }
    }

    /// `onDraftChange` (React 358-364): the control's text — a change
    /// clears the error.
    pub fn draft_changed(&mut self, next: impl Into<String>) {
        self.draft = next.into();
        if self.error.is_some() {
            self.error = None;
        }
    }

    /// `handleKeyDown` (React 256-277): Enter commits, Escape cancels,
    /// Tab commits and moves (shift = previous) — none while composing
    /// (the IME owns the keyboard).
    pub fn key(&mut self, key: &str, shift: bool) -> EditAction {
        if self.composing {
            return EditAction::None;
        }
        match key {
            "Enter" => self.commit(None),
            "Escape" => self.cancel(),
            "Tab" => self.commit(Some(if shift { EditMove::Prev } else { EditMove::Next })),
            _ => EditAction::None,
        }
    }

    /// `handleBlur` (React 279-294): the blur commits — unless an
    /// unfinished composition owns the focus, a right-click is on its
    /// way (the context menu may still change this very cell), or the
    /// focus moved into a surface the editor owns.
    pub fn blur(&mut self, target_key: Option<&str>, target_role: Option<&str>) -> EditAction {
        if self.composing {
            return EditAction::None;
        }
        if self.context_menu {
            self.context_menu = false;
            return EditAction::None;
        }
        if is_owned_editor_target(target_key, target_role) {
            return EditAction::None;
        }
        self.commit(None)
    }

    /// The right-click's mousedown (React 306-312): the host calls it
    /// before the blur the menu causes.
    pub fn context_menu_opened(&mut self) {
        self.context_menu = true;
    }

    /// `handleCompositionStart` / `handleCompositionEnd`.
    pub fn composition_started(&mut self) {
        self.composing = true;
    }
    pub fn composition_ended(&mut self) {
        self.composing = false;
    }

    /// `handleLoadCandidates` (React 336-353): the load's begin —
    /// loading, not failed, no message; the host runs the loader and
    /// reports back through `candidates_landed` /
    /// `candidates_failed`.
    pub fn load_candidates(&mut self) -> EditAction {
        if self.candidates_loading {
            return EditAction::None;
        }
        self.candidates_loading = true;
        self.candidates_failed = false;
        self.candidates_message = None;
        EditAction::LoadCandidates
    }

    /// The loader's success half: the values land, the loading ends.
    pub fn candidates_landed(&mut self, values: Vec<String>) {
        self.candidates = Some(values);
        self.candidates_loading = false;
    }

    /// The loader's failure half: the failure flags and the message —
    /// a loader that explains itself (the selector's uniqueness
    /// refusal, DB-20) is quoted; a silent one keeps the generic
    /// wording.
    pub fn candidates_failed(&mut self, message: Option<String>) {
        self.candidates_failed = true;
        self.candidates_loading = false;
        self.candidates_message = message;
    }
}

/// The host — the state machine's mount: the control for the resolved
/// kind (binary and boolean real, the six pending families as labelled
/// seams), the inline error, and the active-editor registration.
pub struct CellEditorHost<'a, S: 'static> {
    pub state: &'a CellEditorHostState,
    /// The column's aria label base (React `cellEditor.ariaLabel`).
    pub aria_label: String,
    /// The host's panel id (the navigation guard's registration; none
    /// disarms the guard silently — the same omission the React
    /// source reports in dev).
    pub panel_id: Option<String>,
    /// The host's commit action — the registration's commit hook
    /// runs it (the navigation guard's ask, 05 §4.4).
    on_commit: Option<Rc<dyn Fn(&mut S, Option<Value>)>>,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
    on_commit_and_move: Option<Rc<dyn Fn(&mut S, EditMove)>>,
    on_toggle_select: Option<Rc<dyn Fn(&mut S)>>,
    on_close_select: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> CellEditorHost<'a, S> {
    /// The host over the open state.
    pub fn new(state: &'a CellEditorHostState) -> Self {
        Self {
            state,
            aria_label: t("cellEditor.ariaLabel").to_owned(),
            panel_id: None,
            on_commit: None,
            on_cancel: None,
            on_commit_and_move: None,
            on_toggle_select: None,
            on_close_select: None,
        }
    }

    /// React `panelId` — the registration's owner.
    pub fn panel_id(mut self, panel_id: impl Into<String>) -> Self {
        self.panel_id = Some(panel_id.into());
        self
    }

    /// React `onCommit`.
    pub fn on_commit(mut self, f: impl Fn(&mut S, Option<Value>) + 'static) -> Self {
        self.on_commit = Some(Rc::new(f));
        self
    }

    /// React `onCancel`.
    pub fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// React `onCommitAndMove`.
    pub fn on_commit_and_move(mut self, f: impl Fn(&mut S, EditMove) + 'static) -> Self {
        self.on_commit_and_move = Some(Rc::new(f));
        self
    }

    /// The boolean control's three-state select listbox toggle (the
    /// host's `select_open` cell — the enabled-select rule).
    pub fn on_toggle_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_select = Some(Rc::new(f));
        self
    }

    /// Presses outside and Escape close the boolean select's listbox.
    pub fn on_close_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_select = Some(Rc::new(f));
        self
    }

    /// The navigation guard's registration (React 318-334): the
    /// active-editor registry entry whose commit hook drives the
    /// state's commit — `run_commit` runs the state machine's
    /// [`CellEditorHostState::commit`] and returns the reason when
    /// refused (the editor stays open with its inline error; `None`
    /// when written or already closed). **Without a panel id the
    /// editor is not registered, and the guard is silently
    /// disarmed** — the omission the React source reports in dev.
    pub fn register(
        &self,
        run_commit: impl Fn() -> Option<CellValueErrorReason> + 'static,
    ) -> Option<ActiveCellEditorRegistration> {
        let panel_id = self.panel_id.as_ref()?;
        let editor = ActiveCellEditor::new(panel_id.clone(), move || run_commit());
        Some(register_active_cell_editor(editor))
    }

    /// The host root: the control mount and the inline error (React
    /// 422-441). The boolean control's listbox rides `layer`; the
    /// caller roots once.
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut root = col()
            .key("cell-editor")
            .w_full()
            .min_w(0.0)
            .child(self.control(k, layer));
        if let Some(error) = state.error {
            root = root.child(
                text(t(error.message_key()))
                    .key("cell-editor-error")
                    .role("alert")
                    .font(TextStyle {
                        size: 10.0,
                        ..k.ty.small
                    })
                    .color(k.c.danger.fg)
                    .truncate(),
            );
        }
        root
    }

    /// The control mount (React 401-420): the eight-family switch —
    /// binary and boolean real (T278), the other six as labelled
    /// seams naming their owning tasks (T315/T330).
    fn control(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        match state.kind {
            // The two ported controls keep their own root keys
            // (`cell-editor-binary`, `cell-editor-boolean-row`): a
            // `.key` on a built element overwrites the builder's own,
            // and the controls' testids are the contract.
            CellEditorKind::Binary => {
                BinaryCellEditor::new(self.aria_label.clone(), state.error.is_some()).build(k)
            }
            CellEditorKind::Boolean => {
                let mut editor = BooleanCellEditor::<S>::new(
                    state.draft.clone(),
                    state.nullable,
                    self.aria_label.clone(),
                )
                .select_open(state.select_open);
                // The enabled-select rule: the toggle is always present
                // (wired when the host provides the cell flip, a no-op
                // when it does not — the unwired select never opens).
                match &self.on_toggle_select {
                    Some(on_toggle) => {
                        let action = Rc::clone(on_toggle);
                        editor = editor.on_select_toggle(move |s: &mut S| action(s));
                    }
                    None => {
                        editor = editor.on_select_toggle(|_s: &mut S| {});
                    }
                }
                if let Some(on_close) = &self.on_close_select {
                    let action = Rc::clone(on_close);
                    editor = editor.on_select_close(move |s: &mut S| action(s));
                }
                editor.build(k, layer)
            }
            kind @ (CellEditorKind::Text
            | CellEditorKind::LongText
            | CellEditorKind::Number
            | CellEditorKind::Enum
            | CellEditorKind::Date
            | CellEditorKind::Json) => {
                row()
                    .key(format!("cell-editor-seam-{}", kind.as_str()))
                    .w_full()
                    .h(28.0)
                    .center_y()
                    .child(
                        text(state.draft.clone())
                            .key(format!("cell-editor-seam-{}-text", kind.as_str()))
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .truncate(),
                    )
                    .label(format!(
                        "{}|draft={:?}|nullable={}|invalid={}|candidates={:?}",
                        kind.as_str(),
                        state.draft,
                        state.nullable,
                        state.error.is_some(),
                        state.candidates.as_ref().map(Vec::len).unwrap_or(0)
                    ))
            }
        }
    }
}
