use super::*;
use std::{
    sync::mpsc::{channel, sync_channel, Sender},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);

fn options() -> SqlFormatRequest {
    SqlFormatRequest {
        language: "sqlite".into(),
        keyword_case: "upper".into(),
        indent_style: "standard".into(),
        tab_width: 2,
        use_tabs: false,
        logical_operator_newline: "before".into(),
        lines_between_queries: 1,
    }
}

fn wait<T>(mut find: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(value) = find() {
            return value;
        }
        assert!(Instant::now() < deadline, "bounded worker did not report");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn latest_pending_work_replaces_superseded_work_without_waiting() {
    let (started, starts) = channel();
    let (release, held) = sync_channel(1);
    let (cancelled, saw_cancel) = channel();
    let mut worker = FormatterWorker::launch_with(move |sql, _, cancel| {
        started.send(sql.to_owned()).unwrap();
        if sql == "first" {
            held.recv_timeout(WAIT).unwrap();
            cancelled.send(cancel.is_cancelled()).unwrap();
        }
        Ok(format!("[{sql}]"))
    }).unwrap();

    worker.submit(1, "first".into(), options()).unwrap();
    assert_eq!(starts.recv_timeout(WAIT).unwrap(), "first");
    worker.submit(2, "superseded".into(), options()).unwrap();
    worker.submit(3, "latest".into(), options()).unwrap();
    worker.cancel(1); // A stale UI cancellation must not erase the latest one.
    assert_eq!(worker.mailbox.pending.len(), 1);
    assert!(worker.poll().is_none());
    release.send(()).unwrap();
    assert!(saw_cancel.recv_timeout(WAIT).unwrap());
    assert_eq!(starts.recv_timeout(WAIT).unwrap(), "latest");
    let done = wait(|| worker.poll());
    assert_eq!(done.generation, 3);
    assert_eq!(done.result.as_deref(), Ok("[latest]"));
    assert!(starts.try_recv().is_err());
    assert!(worker.poll().is_none());
}

#[test]
fn cancel_and_reuse_of_public_generation_do_not_deliver_old_output() {
    let mut worker = FormatterWorker::launch_with(|sql, _, _| Ok(sql.to_owned())).unwrap();
    worker.submit(7, "cancelled delivery".into(), options()).unwrap();
    wait(|| (!worker.mailbox.ready.is_empty()).then_some(()));
    worker.cancel(7);
    assert!(worker.poll().is_none());

    worker.submit(7, "already ready but obsolete".into(), options()).unwrap();
    wait(|| (!worker.mailbox.ready.is_empty()).then_some(()));
    worker.submit(7, "fresh result despite reused generation".into(), options()).unwrap();
    let done = wait(|| worker.poll());
    assert_eq!(done.generation, 7);
    assert_eq!(done.result.as_deref(), Ok("fresh result despite reused generation"));
    assert!(worker.poll().is_none());
}

#[test]
fn budgets_bound_inputs_before_queue_and_outputs_before_delivery() {
    let mut worker = FormatterWorker::launch_with(|_, _, _| Ok("a".repeat(MAX_OUTPUT + 1))).unwrap();
    assert_eq!(worker.submit(1, "".into(), options()), Err(Error::Rejected));
    assert_eq!(worker.submit(2, "b".repeat(MAX_INPUT + 1), options()), Err(Error::Limit));
    let mut bad = options();
    bad.language = "c".repeat(MAX_OPTION_BYTES + 1);
    assert_eq!(worker.submit(3, "select 1".into(), bad), Err(Error::Limit));
    assert!(worker.mailbox.pending.is_empty() && worker.poll().is_none());

    worker.submit(4, "select 1".into(), options()).unwrap();
    let done = wait(|| worker.poll());
    assert_eq!(done.generation, 4);
    assert_eq!(done.result, Err(Error::Limit));
}

struct OnExit(Sender<()>);

impl Drop for OnExit {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn dropping_worker_invalidates_inflight_job_and_does_not_join() {
    let (started, starts) = channel();
    let (release, held) = sync_channel(1);
    let (cancelled, saw_cancel) = channel();
    let (exiting, exited) = channel();
    let exit_probe = OnExit(exiting);
    let mut worker = FormatterWorker::launch_with(move |_, _, cancel| {
        let _keep = &exit_probe;
        started.send(()).unwrap();
        // Permit even a broken joining implementation to finish after the
        // test reports failure, rather than keeping a stranded test thread.
        let _ = held.recv_timeout(WAIT * 2);
        cancelled.send(cancel.is_cancelled()).unwrap();
        Ok("must not deliver after shutdown".into())
    }).unwrap();
    let mailbox = Arc::clone(&worker.mailbox);

    worker.submit(9, "in flight".into(), options()).unwrap();
    starts.recv_timeout(WAIT).unwrap();
    let (dropped, did_drop) = channel();
    let dropping = std::thread::spawn(move || {
        drop(worker);
        dropped.send(()).unwrap();
    });
    let returned_before_release = did_drop.recv_timeout(WAIT);
    release.send(()).unwrap();
    returned_before_release.expect("window drop must not wait for the formatter to return");
    dropping.join().unwrap();
    assert!(saw_cancel.recv_timeout(WAIT).unwrap());
    exited.recv_timeout(WAIT).unwrap();
    assert!(mailbox.ready.is_empty() && mailbox.pending.is_empty());
}
