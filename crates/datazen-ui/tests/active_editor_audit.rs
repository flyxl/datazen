//! T405 audit (tester): independent checks of the one-open-editor registry.
//! Fresh vectors: replacement chains, unregister-after-replacement storms,
//! a clone of a replaced editor that still answers its own closure, and the
//! commit-verdict read paths.

use std::cell::Cell;
use std::rc::Rc;

use datazen_ui::components::data_table::editors::active_editor::{
    get_active_cell_editor, register_active_cell_editor, ActiveCellEditor,
};
use datazen_ui::support::cell_editor_kind::CellValueErrorReason;

#[derive(Clone, Default)]
struct AuditEditor {
    asked: Rc<Cell<usize>>,
    verdict: Rc<Cell<Option<CellValueErrorReason>>>,
}

impl AuditEditor {
    fn with_verdict(verdict: Option<CellValueErrorReason>) -> Self {
        Self {
            asked: Rc::new(Cell::new(0)),
            verdict: Rc::new(Cell::new(verdict)),
        }
    }

    fn build(&self, panel: &str) -> ActiveCellEditor {
        let asked = Rc::clone(&self.asked);
        let verdict = Rc::clone(&self.verdict);
        ActiveCellEditor::new(panel, move || {
            asked.set(asked.get() + 1);
            verdict.get()
        })
    }
}

#[test]
fn audit_a_replacement_chain_keeps_only_the_last() {
    let a = AuditEditor::with_verdict(None);
    let b = AuditEditor::with_verdict(None);
    let c = AuditEditor::with_verdict(None);

    let reg_a = register_active_cell_editor(a.build("p-a"));
    let reg_b = register_active_cell_editor(b.build("p-b"));
    let reg_c = register_active_cell_editor(c.build("p-c"));
    assert_eq!(get_active_cell_editor().unwrap().panel_id, "p-c");

    // Dropping the obsolete registrations in any order clears nothing: the
    // slot holds c, and neither a nor b is it.
    drop(reg_a);
    assert_eq!(get_active_cell_editor().unwrap().panel_id, "p-c");
    drop(reg_b);
    assert_eq!(get_active_cell_editor().unwrap().panel_id, "p-c");
    // Only the current one's cleanup empties the slot.
    drop(reg_c);
    assert!(get_active_cell_editor().is_none());
    assert_eq!(a.asked.get() + b.asked.get() + c.asked.get(), 0, "no editor was asked");
}

#[test]
fn audit_a_replaced_editors_clone_still_answers_its_own_closure() {
    // The React registry hands out the object itself; a caller holding a
    // reference to a replaced editor can still call its commit(). The Rc
    // keeps the closure alive even though the slot moved on.
    let first = AuditEditor::with_verdict(Some(CellValueErrorReason::TooLarge));
    let second = AuditEditor::with_verdict(None);

    let held = {
        let editor = first.build("p-1");
        let registration = register_active_cell_editor(editor);
        let held = get_active_cell_editor().unwrap();
        let _replaced = register_active_cell_editor(second.build("p-2"));
        // The slot moved to p-2, but `held` (a clone of the Rc) still
        // answers: the draft's verdict is reachable to its owner.
        assert_eq!(held.panel_id, "p-1");
        assert_eq!(held.commit(), Some(CellValueErrorReason::TooLarge));
        assert_eq!(get_active_cell_editor().unwrap().panel_id, "p-2");
        drop(registration);
        held
    };
    assert_eq!(held.commit(), Some(CellValueErrorReason::TooLarge));
    assert_eq!(first.asked.get(), 2);
    // The replacement was never consulted through the slot.
    assert_eq!(second.asked.get(), 0);
}

#[test]
fn audit_re_registration_after_an_unregister_works() {
    let editor = AuditEditor::with_verdict(None);
    let registration = register_active_cell_editor(editor.build("p-1"));
    drop(registration);
    assert!(get_active_cell_editor().is_none());

    // A fresh registration of a fresh editor re-fills the slot.
    let again = AuditEditor::with_verdict(Some(CellValueErrorReason::NotEditable));
    let registration = register_active_cell_editor(again.build("p-2"));
    assert_eq!(
        get_active_cell_editor().unwrap().commit(),
        Some(CellValueErrorReason::NotEditable)
    );
    drop(registration);
    assert!(get_active_cell_editor().is_none());
    assert_eq!(again.asked.get(), 1);
}

#[test]
fn audit_every_reason_is_distinct() {
    // The 11 reasons are 11 distinct verdicts; a commit verdict must be
    // distinguishable cell by cell.
    let reasons = [
        CellValueErrorReason::EmptyNotNull,
        CellValueErrorReason::InvalidNumber,
        CellValueErrorReason::InvalidInteger,
        CellValueErrorReason::InvalidBoolean,
        CellValueErrorReason::InvalidDate,
        CellValueErrorReason::InvalidTime,
        CellValueErrorReason::InvalidDateTime,
        CellValueErrorReason::InvalidJson,
        CellValueErrorReason::InvalidUuid,
        CellValueErrorReason::TooLarge,
        CellValueErrorReason::NotEditable,
    ];
    let mut spellings: Vec<&str> = reasons.iter().map(|r| r.as_str()).collect();
    let before = spellings.len();
    spellings.sort();
    spellings.dedup();
    assert_eq!(spellings.len(), before, "no two reasons spell the same");
    assert_eq!(spellings[0], "emptyNotNull");
    assert_eq!(spellings[before - 1], "tooLarge");
}
