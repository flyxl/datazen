//! T385: the query-run request bridge — a mounted panel accepts a
//! statement immediately, a not-yet-mounted panel queues it in order and
//! the flush runs once on mount, and unregister drops the runner and
//! the queue.

use std::cell::RefCell;
use std::rc::Rc;

use datazen_ui::query_run_request_store::QueryRunRequestStore;

#[derive(Default)]
struct Host {
    ran: RefCell<Vec<String>>,
}

impl Host {
    fn runner(host: &Rc<Host>) -> Rc<dyn Fn(&mut Self, &str)> {
        let host = Rc::clone(host);
        Rc::new(move |_: &mut Self, sql: &str| {
            host.ran.borrow_mut().push(format!("run {sql}"));
        })
    }
}

fn runner_recording(host: &Rc<Host>) -> Rc<dyn Fn(&mut Host, &str)> {
    Host::runner(host)
}

#[test]
fn a_mounted_panel_accepts_immediately() {
    let host = Rc::new(Host::default());
    let mut store = QueryRunRequestStore::<Host>::default();
    let _registration =
        store.register_query_runner(&mut Host::default(), "q1", runner_recording(&host));
    let mut host_state = Host::default();
    let accepted = store.request_query_run(&mut host_state, "q1", "SELECT 1");
    assert!(accepted, "the mounted panel accepts");
    assert_eq!(*host.ran.borrow(), vec!["run SELECT 1".to_owned()]);
}

#[test]
fn a_request_before_mount_is_queued_and_flushed_in_order() {
    let host = Rc::new(Host::default());
    let mut store = QueryRunRequestStore::<Host>::default();
    let mut host_state = Host::default();
    assert!(
        !store.request_query_run(&mut host_state, "q2", "SELECT first"),
        "the panel is not mounted yet"
    );
    assert!(
        !store.request_query_run(&mut host_state, "q2", "SELECT second"),
        "still not mounted"
    );
    // Two quick clicks, one mount: both statements survive, in order.
    let _registration =
        store.register_query_runner(&mut host_state, "q2", runner_recording(&host));
    assert_eq!(
        *host.ran.borrow(),
        vec!["run SELECT first".to_owned(), "run SELECT second".to_owned()],
        "the flush runs the queue once, in order"
    );
    // The queue is gone; a later register does not re-run anything.
    let mut quiet = Host::default();
    let _again = store.register_query_runner(&mut quiet, "q2", runner_recording(&host));
    assert_eq!(host.ran.borrow().len(), 2, "no second flush");
}

#[test]
fn unregister_drops_the_runner_and_the_pending_queue() {
    let host = Rc::new(Host::default());
    let mut store = QueryRunRequestStore::<Host>::default();
    let mut host_state = Host::default();
    // A request queued before the panel goes away is dropped with it.
    assert!(!store.request_query_run(&mut host_state, "q3", "SELECT lost"));
    let registration =
        store.register_query_runner(&mut host_state, "q3", runner_recording(&host));
    // The mount flushed it (ran), then the panel goes away.
    store.unregister(&registration);
    // A fresh mount of the same id does not re-run the flushed queue…
    let _again =
        store.register_query_runner(&mut host_state, "q3", runner_recording(&host));
    assert_eq!(
        *host.ran.borrow(),
        vec!["run SELECT lost".to_owned()],
        "one run, one flush"
    );
    // …but a request queued after an unmount is a normal new queue for
    // the next mount. The second mount's runner has to go first.
    store.unregister(&_again);
    assert!(
        !store.request_query_run(&mut host_state, "q3", "SELECT queued"),
        "no runner is mounted now"
    );
    let _third = store.register_query_runner(&mut host_state, "q3", runner_recording(&host));
    assert_eq!(
        *host.ran.borrow(),
        vec!["run SELECT lost".to_owned(), "run SELECT queued".to_owned()],
        "the post-unmount queue flushes on the next mount"
    );
}

#[test]
fn two_panels_do_not_share_runners_or_queues() {
    let host_a = Rc::new(Host::default());
    let host_b = Rc::new(Host::default());
    let mut store = QueryRunRequestStore::<Host>::default();
    let mut host_state = Host::default();
    let _a = store.register_query_runner(&mut host_state, "a", runner_recording(&host_a));
    assert!(!store.request_query_run(&mut host_state, "b", "SELECT b-first"));
    assert!(store.request_query_run(&mut host_state, "a", "SELECT a"));
    let _b = store.register_query_runner(&mut host_state, "b", runner_recording(&host_b));
    assert_eq!(*host_a.ran.borrow(), vec!["run SELECT a".to_owned()]);
    assert_eq!(*host_b.ran.borrow(), vec!["run SELECT b-first".to_owned()]);
}
