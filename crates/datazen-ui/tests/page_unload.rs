//! T154: `lib/session/pageUnload.ts` ported to a host-owned registry value.
//!
//! The probes below are pure value journeys: register closers, release, and read
//! the emitted report / call log. No window event is faked and no thread is
//! involved; a "thrown" closer is an `Err(CloserError)` the registry must record
//! and swallow so the remaining closers still fire.

use std::cell::RefCell;
use std::rc::Rc;

use datazen_ui::session::page_unload::{
    CloserError, PageUnloadCloser, PageUnloadRegistry, UnloadEvent,
};

type Log = Rc<RefCell<Vec<String>>>;

fn log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

fn counting(label: &'static str, calls: Log) -> PageUnloadCloser {
    Box::new(move || {
        calls.borrow_mut().push(label.to_owned());
        Ok(())
    })
}

fn failing(label: &'static str, message: &'static str, calls: Log) -> PageUnloadCloser {
    Box::new(move || {
        calls.borrow_mut().push(label.to_owned());
        Err(CloserError { closer_id: 0, message: message.to_owned() })
    })
}

#[test]
fn register_then_release_fires_each_closer_once_and_clears_the_registry() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    let first = Rc::clone(&calls);
    let second = Rc::clone(&calls);
    let t1 = registry.register(counting("first", first));
    let t2 = registry.register(counting("second", second));

    assert_eq!(registry.len(), 2);
    assert!(registry.is_installed(), "registration installs the unload listeners");

    let report = registry.release_owned_sessions();
    assert_eq!(report.fired, vec![t1.id, t2.id], "registration order is preserved");
    assert!(report.errors.is_empty());
    assert_eq!(*calls.borrow(), vec!["first", "second"]);
    assert_eq!(registry.release_count(), 1);
    assert!(registry.is_empty(), "release clears the registered closers");
}

#[test]
fn a_second_release_is_a_noop_because_the_first_cleared_the_set() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    registry.register(counting("only", Rc::clone(&calls)));
    let first = registry.release_owned_sessions();
    let second = registry.release_owned_sessions();

    assert_eq!(first.fired.len(), 1);
    assert!(second.fired.is_empty(), "a cleared registry has nothing left to fire");
    assert_eq!(*calls.borrow(), vec!["only"], "no closer runs twice");
    assert_eq!(registry.release_count(), 2, "both releases are observable");
}

#[test]
fn unregister_prevents_only_that_closer_from_firing() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    let removed = registry.register(counting("removed", Rc::clone(&calls)));
    let kept = registry.register(counting("kept", Rc::clone(&calls)));

    assert!(registry.unregister(removed), "a live registration is removed");
    assert!(!registry.unregister(removed), "unregistering twice is a no-op");
    assert_eq!(registry.len(), 1);

    let report = registry.release_owned_sessions();
    assert_eq!(report.fired, vec![kept.id]);
    assert_eq!(*calls.borrow(), vec!["kept"]);
    assert!(!registry.unregister(kept), "a released registration is already gone");
}

#[test]
fn a_failing_closer_is_captured_and_does_not_stop_the_others() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    registry.register(failing("broken", "this closer is broken", Rc::clone(&calls)));
    registry.register(counting("survivor", Rc::clone(&calls)));
    registry.register(counting("last", Rc::clone(&calls)));

    let report = registry.release_owned_sessions();
    assert!(!report.fired.is_empty());
    assert_eq!(report.fired, vec![1, 2, 3], "every closer fires even after a failure");
    assert_eq!(
        report.errors,
        vec![CloserError { closer_id: 0, message: "this closer is broken".to_owned() }],
        "the throw is captured with its message instead of panicking"
    );
    assert_eq!(registry.last_errors(), report.errors.as_slice());
    assert_eq!(*calls.borrow(), vec!["broken", "survivor", "last"]);

    // The broken closer last still cannot stop the ones ahead of it.
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    registry.register(counting("first", Rc::clone(&calls)));
    registry.register(failing("broken-last", "late failure", Rc::clone(&calls)));
    let report = registry.release_owned_sessions();
    assert_eq!(report.fired, vec![1, 2]);
    assert_eq!(*calls.borrow(), vec!["first", "broken-last"]);
    assert_eq!(report.errors.len(), 1);
}

#[test]
fn install_is_idempotent_and_covers_pagehide_and_beforeunload() {
    let mut registry = PageUnloadRegistry::new();
    assert!(!registry.is_installed());
    assert!(registry.install(), "the first install attaches the listeners");
    assert!(!registry.install(), "a second install does not attach them twice");
    assert_eq!(
        registry.listener_events(),
        &[UnloadEvent::PageHide, UnloadEvent::BeforeUnload],
        "both React listeners are modeled"
    );

    let mut registry = PageUnloadRegistry::new();
    registry.register(Box::new(|| Ok(())));
    assert!(registry.is_installed(), "register installs on first use, as React does");
}
