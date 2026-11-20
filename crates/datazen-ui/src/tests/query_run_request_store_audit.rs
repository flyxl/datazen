//! T385 independent audit (tester): fresh vectors for the query-run
//! bridge — a re-registration replacing the runner, a queue that
//! survives a wrong panel's mount, the return-value contract, and a
//! multi-statement flush.

use std::cell::RefCell;
use std::rc::Rc;

use crate::query_run_request_store::QueryRunRequestStore;

#[derive(Default)]
struct AuditHost {
    log: RefCell<Vec<String>>,
}

impl AuditHost {
    fn runner(tag: &'static str) -> Rc<dyn Fn(&mut Self, &str)> {
        Rc::new(move |host: &mut Self, sql: &str| {
            host.log.borrow_mut().push(format!("{tag}:{sql}"));
        })
    }
}

#[test]
fn audit_a_re_registration_replaces_the_runner() {
    let mut store = QueryRunRequestStore::<AuditHost>::default();
    let mut state = AuditHost::default();
    let _first = store.register_query_runner(&mut state, "p", AuditHost::runner("old"));
    assert!(store.request_query_run(&mut state, "p", "one"));
    // The panel re-mounts with a fresh runner: the old registration's
    // handle is gone, the new runner serves.
    let _second = store.register_query_runner(&mut state, "p", AuditHost::runner("new"));
    assert!(store.request_query_run(&mut state, "p", "two"));
    assert_eq!(
        *state.log.borrow(),
        vec!["old:one".to_owned(), "new:two".to_owned()],
        "the newest runner wins"
    );
}

#[test]
fn audit_a_queue_survives_another_panels_mount() {
    let mut store = QueryRunRequestStore::<AuditHost>::default();
    let mut state = AuditHost::default();
    assert!(!store.request_query_run(&mut state, "target", "SELECT t"));
    // A different panel mounts: the target's queue is untouched.
    let _other = store.register_query_runner(&mut state, "other", AuditHost::runner("other"));
    assert!(state.log.borrow().is_empty(), "the other panel's mount runs nothing of the target's queue");
    // Then the target mounts: its queue flushes through its own runner.
    let _target = store.register_query_runner(&mut state, "target", AuditHost::runner("target"));
    assert_eq!(*state.log.borrow(), vec!["target:SELECT t".to_owned()]);
}

#[test]
fn audit_the_return_value_is_the_acceptance_contract() {
    let mut store = QueryRunRequestStore::<AuditHost>::default();
    let mut state = AuditHost::default();
    // Every request before any mount returns false.
    for sql in ["a", "b", "c"] {
        assert!(!store.request_query_run(&mut state, "p", sql));
    }
    // The single flush drains them all, in order.
    let _mount = store.register_query_runner(&mut state, "p", AuditHost::runner("p"));
    assert_eq!(
        *state.log.borrow(),
        vec!["p:a".to_owned(), "p:b".to_owned(), "p:c".to_owned()]
    );
    // And afterwards every request returns true.
    assert!(store.request_query_run(&mut state, "p", "d"));
    assert_eq!(state.log.borrow().last().map(String::as_str), Some("p:d"));
}

#[test]
fn audit_the_same_sql_twice_queues_twice() {
    let mut store = QueryRunRequestStore::<AuditHost>::default();
    let mut state = AuditHost::default();
    assert!(!store.request_query_run(&mut state, "p", "SELECT 1"));
    assert!(!store.request_query_run(&mut state, "p", "SELECT 1"));
    let _mount = store.register_query_runner(&mut state, "p", AuditHost::runner("p"));
    assert_eq!(
        *state.log.borrow(),
        vec!["p:SELECT 1".to_owned(), "p:SELECT 1".to_owned()],
        "duplicates are distinct requests"
    );
}
