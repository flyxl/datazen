//! T293: the tab-closed notification — the id-set diff fires exactly the
//! handlers of the panels that disappeared, unregistering drops a
//! handler, and re-registering the same handler is a no-op.

use std::rc::Rc;

use crate::panel_close_notifier::PanelCloseNotifier;

#[derive(Default)]
struct Recorder {
    fired: std::cell::RefCell<Vec<&'static str>>,
}

fn handler(recorder: &Rc<Recorder>, tag: &'static str) -> Rc<dyn Fn()> {
    let recorder = Rc::clone(recorder);
    Rc::new(move || recorder.fired.borrow_mut().push(tag))
}

#[test]
fn a_disappearing_panel_fires_only_its_handlers() {
    let recorder = Rc::new(Recorder::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("p1", handler(&recorder, "p1-a"));
    notifier.on_panel_closed("p1", handler(&recorder, "p1-b"));
    notifier.on_panel_closed("p2", handler(&recorder, "p2-a"));

    // p2 closes: only its handler fires.
    let fired = notifier.panels_changed(&["p1", "p2"], &["p1"]);
    assert_eq!(fired, vec!["p2".to_owned()]);
    assert_eq!(*recorder.fired.borrow(), vec!["p2-a"]);
    // The registration is gone; a second fire cannot happen.
    let fired = notifier.panels_changed(&["p1"], &[]);
    assert_eq!(fired, vec!["p1".to_owned()]);
    assert_eq!(
        *recorder.fired.borrow(),
        vec!["p2-a", "p1-a", "p1-b"],
        "both p1 handlers fire, in registration order"
    );
}

#[test]
fn an_unsubscribe_drops_only_that_handler() {
    let recorder = Rc::new(Recorder::default());
    let mut notifier = PanelCloseNotifier::new();
    let keep = notifier.on_panel_closed("p1", handler(&recorder, "keep"));
    let drop_me = notifier.on_panel_closed("p1", handler(&recorder, "drop"));
    notifier.unsubscribe(&drop_me);
    notifier.panels_changed(&["p1"], &[]);
    assert_eq!(*recorder.fired.borrow(), vec!["keep"]);
    // Unsubscribing twice is harmless.
    notifier.unsubscribe(&drop_me);
    let _ = keep;
}

#[test]
fn re_registering_the_same_handler_is_a_no_op() {
    let recorder = Rc::new(Recorder::default());
    let mut notifier = PanelCloseNotifier::new();
    let same = handler(&recorder, "same");
    notifier.on_panel_closed("p1", Rc::clone(&same));
    notifier.on_panel_closed("p1", Rc::clone(&same));
    notifier.panels_changed(&["p1"], &[]);
    assert_eq!(
        *recorder.fired.borrow(),
        vec!["same"],
        "the duplicate registration dedupes by handler identity"
    );
}

#[test]
fn registering_for_an_already_closed_panel_is_allowed_and_silent() {
    let recorder = Rc::new(Recorder::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("gone", handler(&recorder, "gone"));
    // The panel is already absent; nothing ever fires.
    let fired = notifier.panels_changed(&["other"], &["other"]);
    assert!(fired.is_empty());
    assert!(recorder.fired.borrow().is_empty());
}

#[test]
fn an_added_panel_does_not_fire_anything() {
    let recorder = Rc::new(Recorder::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("p1", handler(&recorder, "p1"));
    let fired = notifier.panels_changed(&["p1"], &["p1", "p2"]);
    assert!(fired.is_empty(), "an added tab closes nothing");
    assert!(recorder.fired.borrow().is_empty());
}

#[test]
fn an_empty_change_is_a_no_op() {
    let recorder = Rc::new(Recorder::default());
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("p1", handler(&recorder, "p1"));
    let fired = notifier.panels_changed(&["p1"], &["p1"]);
    assert!(fired.is_empty());
    assert!(recorder.fired.borrow().is_empty());
}

#[test]
fn a_panicking_handler_neither_breaks_the_rest_nor_re_fires() {
    let recorder = Rc::new(Recorder::default());
    let boom: Rc<dyn Fn()> = Rc::new(|| panic!("cleanup failed"));
    let mut notifier = PanelCloseNotifier::new();
    notifier.on_panel_closed("p1", boom);
    notifier.on_panel_closed("p1", handler(&recorder, "after"));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let fired = notifier.panels_changed(&["p1"], &[]);
    std::panic::set_hook(prev);
    assert_eq!(fired, vec!["p1".to_owned()], "the close still reports");
    assert_eq!(*recorder.fired.borrow(), vec!["after"], "the handler after the panicking one runs");
    // And the panicking registration is gone (dropped before running).
    let fired = notifier.panels_changed(&["p1", "p1"], &[]);
    assert!(fired.is_empty(), "no registration survived the fire");
}
