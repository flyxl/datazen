//! T154 independent audit (tester, not coder).
//!
//! Uncovered branches of the host-owned closer registry: several failures in
//! one release (all recorded, in order, without stopping the rest), a fresh
//! registration after a release (clear-then-fires and id monotonicity), the
//! per-release reset of `last_errors`, and unregistering an unknown token.
//! Every assertion reads the report / call log — observable values, not a
//! helper self-report.

use std::cell::RefCell;
use std::rc::Rc;

use crate::session::page_unload::{
    CloserError, PageUnloadCloser, PageUnloadRegistry, PageUnloadToken,
};

type Log = Rc<RefCell<Vec<&'static str>>>;

fn log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

fn counting(label: &'static str, calls: Log) -> PageUnloadCloser {
    Box::new(move || {
        calls.borrow_mut().push(label);
        Ok(())
    })
}

fn failing(label: &'static str, id: u64, message: &'static str, calls: Log) -> PageUnloadCloser {
    Box::new(move || {
        calls.borrow_mut().push(label);
        Err(CloserError { closer_id: id, message: message.to_owned() })
    })
}

// React swallows each throw independently: two failures must both be recorded,
// in registration order, and the healthy closer between them must still run.
#[test]
fn multiple_failures_are_all_recorded_in_order_without_stopping_the_rest() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    registry.register(failing("first-bad", 11, "boom-a", Rc::clone(&calls)));
    registry.register(counting("healthy", Rc::clone(&calls)));
    registry.register(failing("last-bad", 33, "boom-b", Rc::clone(&calls)));

    let report = registry.release_owned_sessions();
    assert_eq!(report.fired, vec![1, 2, 3], "every closer fires exactly once");
    assert_eq!(*calls.borrow(), vec!["first-bad", "healthy", "last-bad"]);
    assert_eq!(
        report.errors,
        vec![
            CloserError { closer_id: 11, message: "boom-a".to_owned() },
            CloserError { closer_id: 33, message: "boom-b".to_owned() },
        ],
        "both failures are captured in firing order"
    );
    assert_eq!(registry.last_errors(), report.errors.as_slice());
    assert!(registry.is_empty());
}

// After a release clears the set, a new registration must fire on the next
// release with a never-reused id (React's cleared Set accepts fresh closers).
#[test]
fn a_closer_registered_after_a_release_fires_next_time_with_a_fresh_id() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    let first = registry.register(counting("first", Rc::clone(&calls)));
    let first_report = registry.release_owned_sessions();
    assert_eq!(first_report.fired, vec![first.id]);
    assert!(registry.is_empty(), "release clears the registry");

    let second = registry.register(counting("second", Rc::clone(&calls)));
    assert!(second.id > first.id, "registration ids are never reused");
    let second_report = registry.release_owned_sessions();
    assert_eq!(second_report.fired, vec![second.id]);
    assert_eq!(*calls.borrow(), vec!["first", "second"]);
    assert!(registry.is_empty());
}

// `last_errors` is per-release: a clean release after a failing one reports no
// errors and leaves no stale diagnostics behind.
#[test]
fn a_clean_release_clears_the_errors_of_the_previous_release() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    registry.register(failing("bad", 7, "boom", Rc::clone(&calls)));
    let first = registry.release_owned_sessions();
    assert_eq!(first.errors.len(), 1);
    assert_eq!(registry.last_errors().len(), 1);

    registry.register(counting("good", Rc::clone(&calls)));
    let second = registry.release_owned_sessions();
    assert!(second.errors.is_empty(), "a clean release reports no errors");
    assert!(registry.last_errors().is_empty(), "stale errors are cleared");
    assert_eq!(second.fired.len(), 1);
    assert_eq!(*calls.borrow(), vec!["bad", "good"]);
}

// Unregistering a token this registry never issued is a harmless no-op.
#[test]
fn unregistering_an_unknown_token_is_a_noop() {
    let mut registry = PageUnloadRegistry::new();
    let calls = log();
    let live = registry.register(counting("live", Rc::clone(&calls)));
    let unknown = PageUnloadToken { id: live.id + 1000 };
    assert!(!registry.unregister(unknown));
    assert_eq!(registry.len(), 1, "the live registration is untouched");

    let report = registry.release_owned_sessions();
    assert_eq!(report.fired, vec![live.id]);
    assert_eq!(*calls.borrow(), vec!["live"]);
}
