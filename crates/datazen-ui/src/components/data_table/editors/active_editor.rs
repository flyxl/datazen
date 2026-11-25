//! The one editor that is open, if any — ported from React
//! `packages/frontend/src/components/DataTable/editors/activeEditor.ts`
//! (tasklist T405).
//!
//! The grid shows at most one editor at a time (05 §4.5 #1), so the open
//! editor can be registered in one place — and a caller that must not lose
//! the user's typing (a page change, a sort, a refresh) can ask it to
//! commit before the navigation happens.
//!
//! The registry holds a **callback**, not a value: the editor owns its
//! draft, and the caller only learns whether the draft is acceptable.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | the module-level `let activeEditor` | a `thread_local!` slot holding one `Rc` |
//! | `getActiveCellEditor()` | [`get_active_cell_editor`] |
//! | `registerActiveCellEditor(editor)` returning the cleanup closure | [`register_active_cell_editor`] returning [`ActiveCellEditorRegistration`], whose `Drop` **is** the closure |
//! | `activeEditor === editor` (the closure's identity guard) | `Rc::ptr_eq` — a replaced editor's cleanup is a no-op |
//! | `commit(): CellValueErrorReason \| null` | [`ActiveCellEditor::commit`] — `None` = written (or closed), `Some(reason)` = refused and the editor stays open |
//!
//! The TS interface's method becomes a captured closure, so the editor that
//! registers itself keeps owning its draft: the registry learns only the
//! verdict. `Rc` identity is the object identity the React guard compares.

use std::cell::RefCell;
use std::rc::Rc;

use crate::support::cell_editor_kind::CellValueErrorReason;

/// React `ActiveCellEditor`: the panel the editor belongs to, plus the
/// commit hook.
pub struct ActiveCellEditor {
    /// The panel the editor belongs to.
    pub panel_id: String,
    /// `commit()` — captures the draft; returns `None` when the draft was
    /// written (or the editor was closed), and the reason when it was
    /// refused — in which case the editor stays open with its inline error.
    commit: Rc<dyn Fn() -> Option<CellValueErrorReason>>,
}

impl ActiveCellEditor {
    /// Build the editor the way React's object literal does: the panel id
    /// plus the `commit` method.
    pub fn new(
        panel_id: impl Into<String>,
        commit: impl Fn() -> Option<CellValueErrorReason> + 'static,
    ) -> Self {
        Self {
            panel_id: panel_id.into(),
            commit: Rc::new(commit),
        }
    }

    /// Commit the draft. `None` = written; `Some(reason)` = refused, the
    /// editor stays open with its inline error.
    pub fn commit(&self) -> Option<CellValueErrorReason> {
        (self.commit)()
    }
}

thread_local! {
    /// The module-level `let activeEditor: ActiveCellEditor | null`.
    static ACTIVE_EDITOR: RefCell<Option<Rc<ActiveCellEditor>>> = const { RefCell::new(None) };
}

/// `getActiveCellEditor()`: the editor that is open right now, if any.
pub fn get_active_cell_editor() -> Option<Rc<ActiveCellEditor>> {
    ACTIVE_EDITOR.with(|cell| cell.borrow().clone())
}

/// The unregister function `registerActiveCellEditor` hands back.
///
/// Dropping it unregisters — but only when the slot still holds **this**
/// editor: a registration made obsolete by a newer one clears nothing
/// (`if (activeEditor === editor) activeEditor = null`). That is the React
/// closure's identity guard, spelled with `Rc::ptr_eq`.
#[must_use = "dropping the registration immediately unregisters the editor"]
pub struct ActiveCellEditorRegistration {
    editor: Rc<ActiveCellEditor>,
}

impl ActiveCellEditorRegistration {
    /// The closure's explicit call form (React's effect cleanup). Consuming
    /// self, it is exactly the drop path.
    pub fn unregister(self) {}
}

impl Drop for ActiveCellEditorRegistration {
    fn drop(&mut self) {
        ACTIVE_EDITOR.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.as_ref().is_some_and(|current| Rc::ptr_eq(current, &self.editor)) {
                *slot = None;
            }
        });
    }
}

/// `registerActiveCellEditor(editor)`: register the open editor and get
/// the function that removes it again.
///
/// The editor registers itself when it mounts and unregisters when it
/// unmounts; here mounting is the call and unmounting is the returned
/// registration's `Drop` (or its explicit [`unregister`]).
pub fn register_active_cell_editor(
    editor: ActiveCellEditor,
) -> ActiveCellEditorRegistration {
    let editor = Rc::new(editor);
    ACTIVE_EDITOR.with(|cell| *cell.borrow_mut() = Some(editor.clone()));
    ActiveCellEditorRegistration { editor }
}
