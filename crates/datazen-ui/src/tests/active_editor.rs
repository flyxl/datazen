//! T405: the one-open-editor registry — registration, replacement, the
//! identity-guarded unregister, and the commit verdict.

use std::cell::Cell;
use std::rc::Rc;

use crate::components::data_table::editors::active_editor::{
    get_active_cell_editor, register_active_cell_editor, ActiveCellEditor,
};
use crate::support::cell_editor_kind::CellValueErrorReason;

/// An editor whose commit is scripted and observable: how many times it
/// was asked, and what it answers this time. The counters are `Rc<Cell>`
/// so the closure and the test observe the same cells.
#[derive(Clone, Default)]
struct Scripted {
    asked: Rc<Cell<usize>>,
    answer: Rc<Cell<Option<CellValueErrorReason>>>,
}

impl Scripted {
    fn answering(answer: Option<CellValueErrorReason>) -> Self {
        Self {
            asked: Rc::new(Cell::new(0)),
            answer: Rc::new(Cell::new(answer)),
        }
    }

    /// The `commit()` method: increments the asked counter, answers as
    /// scripted.
    fn editor(&self, panel: &str) -> ActiveCellEditor {
        let asked = Rc::clone(&self.asked);
        let answer = Rc::clone(&self.answer);
        ActiveCellEditor::new(panel, move || {
            asked.set(asked.get() + 1);
            answer.get()
        })
    }
}

#[test]
fn without_registration_there_is_no_open_editor() {
    assert!(get_active_cell_editor().is_none());
}

#[test]
fn registration_puts_the_editor_in_the_slot() {
    let scripted = Scripted::answering(None);
    let editor = scripted.editor("panel-1");
    let _registration = register_active_cell_editor(editor);
    let active = get_active_cell_editor().expect("the editor is registered");
    assert_eq!(active.panel_id, "panel-1");
    // A written draft answers None.
    assert_eq!(active.commit(), None);
    assert_eq!(scripted.asked.get(), 1);
}

#[test]
fn a_refused_draft_reports_its_reason() {
    let scripted = Scripted::answering(Some(CellValueErrorReason::InvalidNumber));
    let editor = scripted.editor("panel-1");
    let _registration = register_active_cell_editor(editor);
    assert_eq!(
        get_active_cell_editor().unwrap().commit(),
        Some(CellValueErrorReason::InvalidNumber)
    );
}

#[test]
fn a_second_registration_replaces_the_first() {
    let first = Scripted::answering(None);
    let second = Scripted::answering(None);
    let first_registration = register_active_cell_editor(first.editor("panel-1"));
    let _second_registration = register_active_cell_editor(second.editor("panel-2"));

    // The slot now holds the second editor.
    assert_eq!(get_active_cell_editor().unwrap().panel_id, "panel-2");
    // The first editor's cleanup is a no-op: the identity guard.
    first_registration.unregister();
    assert_eq!(
        get_active_cell_editor().unwrap().panel_id,
        "panel-2",
        "an obsolete cleanup clears nothing"
    );
    assert_eq!(first.asked.get(), 0, "the replaced editor is never asked");
}

#[test]
fn dropping_the_registration_unregisters_the_current_editor() {
    let scripted = Scripted::answering(None);
    let registration = register_active_cell_editor(scripted.editor("panel-1"));
    assert!(get_active_cell_editor().is_some());
    drop(registration);
    assert!(get_active_cell_editor().is_none());
}

#[test]
fn the_current_editor_commits_before_a_navigation() {
    // The caller that must not lose typing (05 §4.5 #1): it asks the open
    // editor to commit; a written draft (None) lets the navigation pass,
    // a refused draft (Some) keeps the editor open.
    let scripted = Scripted::answering(None);
    let _registration = register_active_cell_editor(scripted.editor("panel-1"));
    let can_navigate = get_active_cell_editor()
        .map(|editor| editor.commit())
        .is_none_or(|reason| reason.is_none());
    assert!(can_navigate);
    assert_eq!(scripted.asked.get(), 1, "commit was asked exactly once");
}
