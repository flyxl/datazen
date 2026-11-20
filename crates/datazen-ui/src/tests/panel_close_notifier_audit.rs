//! T293 independent audit (tester): fresh vectors for the tab-closed
//! notification — multi-panel closes in one diff, closing and re-adding
//! in the same change, unsubscribe of a never-registered handler, and
//! handler independence.

use std::rc::Rc;

use crate::panel_close_notifier::PanelCloseNotifier;

#[derive(Default)]
struct Tape {
    events: std::cell::RefCell<Vec<String>>,
}

impl Tape {
    fn handler(self: &Rc<Self>, tag: &str) -> Rc<dyn Fn()> {
        let tape = Rc::clone(self);
        let tag = tag.to_owned();
        Rc::new(move || tape.events.borrow_mut().push(tag.clone()))
    }
}

#[test]
fn audit_a_multi_close_diff_fires_each_panel_once() {
    let tape = Rc::new(Tape::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("a", tape.handler("a"));
    notifier.on_panel_closed("b", tape.handler("b"));
    notifier.on_panel_closed("c", tape.handler("c"));
    let fired = notifier.panels_changed(&["a", "b", "c", "d"], &["c", "d"]);
    assert_eq!(
        fired,
        vec!["a".to_owned(), "b".to_owned()],
        "only the disappearing ids fire, in diff order"
    );
    assert_eq!(
        *tape.events.borrow(),
        vec!["a".to_owned(), "b".to_owned()],
        "the survivor c does not fire"
    );
}

#[test]
fn audit_closing_and_readding_in_one_change_stays_open() {
    // An id present before and after was never closed — even when the
    // panel object itself was replaced (the id is the only signal).
    // q has a registration too, so its close is observable.
    let tape = Rc::new(Tape::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("p", tape.handler("p"));
    notifier.on_panel_closed("q", tape.handler("q"));
    let fired = notifier.panels_changed(&["p", "q"], &["p", "r"]);
    assert_eq!(fired, vec!["q".to_owned()], "q closed");
    assert_eq!(
        *tape.events.borrow(),
        vec!["q".to_owned()],
        "p survived the same change"
    );
}

#[test]
fn audit_unsubscribing_a_stranger_is_harmless() {
    let tape = Rc::new(Tape::default());
    let mut notifier = PanelCloseNotifier::new();
    let keep = notifier.on_panel_closed("p", tape.handler("keep"));
    // A subscription that was never made (a clone of the same handler
    // registered under another id, say) must not disturb the real one.
    let stranger = notifier.on_panel_closed("other", tape.handler("stranger"));
    notifier.unsubscribe(&stranger);
    notifier.unsubscribe(&stranger);
    notifier.panels_changed(&["p"], &[]);
    assert_eq!(*tape.events.borrow(), vec!["keep".to_owned()]);
    let _ = keep;
}

#[test]
fn audit_handlers_of_different_panels_are_independent() {
    let tape = Rc::new(Tape::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("x", tape.handler("x1"));
    notifier.on_panel_closed("x", tape.handler("x2"));
    notifier.on_panel_closed("y", tape.handler("y1"));
    notifier.panels_changed(&["x", "y"], &["y"]);
    assert_eq!(
        *tape.events.borrow(),
        vec!["x1".to_owned(), "x2".to_owned()],
        "both x handlers fire in registration order, y stays"
    );
    // x is gone; firing again through a stale diff is a no-op.
    notifier.panels_changed(&["x"], &[]);
    assert_eq!(tape.events.borrow().len(), 2);
    notifier.panels_changed(&["y"], &[]);
    assert_eq!(
        *tape.events.borrow(),
        vec!["x1".to_owned(), "x2".to_owned(), "y1".to_owned()],
        "y closes later on its own diff"
    );
}

#[test]
fn audit_the_same_handler_on_two_panels_fires_per_panel() {
    let tape = Rc::new(Tape::default());
    let shared = tape.handler("shared");
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("m", Rc::clone(&shared));
    notifier.on_panel_closed("n", Rc::clone(&shared));
    notifier.panels_changed(&["m", "n"], &[]);
    assert_eq!(*tape.events.borrow(), vec!["shared".to_owned(), "shared".to_owned()]);
}
