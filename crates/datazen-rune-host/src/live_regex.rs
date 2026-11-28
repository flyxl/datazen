//! One bounded latest-request slot and one supervising thread. Regexp compile,
//! search and expansion run only in a newly isolated child, never on Rune.
use std::{sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}}, thread, time::Duration};
use crossbeam_queue::ArrayQueue;
use datazen_ui_api::native_query_regex as ui;
use datazen_regex_worker as worker;

struct Mailbox {
    next: ArrayQueue<ui::Request>,
    done: ArrayQueue<ui::Delivery>,
    current: AtomicU64,
    stopped: AtomicBool,
}
pub(super) struct Supervisor {
    inner: Arc<Mailbox>,
    runner: thread::JoinHandle<()>,
    last_id: u64,
}
impl Supervisor {
    pub(super) fn new() -> Option<Self> {
        let inner = Arc::new(Mailbox { next: ArrayQueue::new(1), done: ArrayQueue::new(1), current: AtomicU64::new(0), stopped: AtomicBool::new(false) });
        let box_ = Arc::clone(&inner);
        let runner = thread::Builder::new().name("query-find-supervisor".into()).spawn(move || {
            while !box_.stopped.load(Ordering::Acquire) {
                let Some(job) = box_.next.pop() else { thread::park(); continue };
                if box_.current.load(Ordering::Acquire) != job.id { continue; }
                let result = run(&job);
                if box_.current.load(Ordering::Acquire) == job.id {
                    drop(box_.done.force_push(ui::Delivery { id: job.id, result }));
                }
            }
        }).ok()?;
        Some(Self { inner, runner, last_id: 0 })
    }
    pub(super) fn available(&self) -> bool { !self.runner.is_finished() }
    pub(super) fn submit(&mut self, req: ui::Request) -> Result<(), ui::Error> {
        if !self.available() { return Err(ui::Error::Unavailable); }
        req.check()?;
        if req.id <= self.last_id { return Err(ui::Error::InvalidReply); }
        self.last_id = req.id;
        self.inner.current.store(req.id, Ordering::Release);
        drop(self.inner.done.pop());
        drop(self.inner.next.force_push(req));
        self.runner.thread().unpark();
        Ok(())
    }
    pub(super) fn cancel(&mut self, id: u64) {
        if self.inner.current.compare_exchange(id,0,Ordering::AcqRel,Ordering::Relaxed).is_ok() {
            drop(self.inner.next.pop());
            drop(self.inner.done.pop());
        }
    }
    pub(super) fn poll(&mut self) -> Option<ui::Delivery> {
        let current = self.inner.current.load(Ordering::Acquire);
        if current == 0 { return None; }
        if let Some(done) = self.inner.done.pop().filter(|done| done.id == current) {
            self.cancel(current);
            return Some(done);
        }
        if !self.available() {
            self.cancel(current);
            return Some(ui::Delivery { id:current, result:Err(ui::Error::Unavailable) });
        }
        None
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.inner.current.store(0, Ordering::Release);
        self.inner.stopped.store(true, Ordering::Release);
        drop(self.inner.next.pop());
        self.runner.thread().unpark();
        // Drop detaches instead of joining a child launch/reap on the UI thread.
    }
}
fn run(req: &ui::Request) -> Result<ui::Outcome, ui::Error> {
    let result = worker::search(worker::Request {
        operation: match req.operation {
            ui::Operation::Next => worker::Operation::Next,
            ui::Operation::Previous => worker::Operation::Previous,
            ui::Operation::SelectAll => worker::Operation::SelectAll,
            ui::Operation::ReplaceNext => worker::Operation::ReplaceNext,
            ui::Operation::ReplaceAll => worker::Operation::ReplaceAll,
        },
        document: &req.document, pattern:&req.pattern, replacement:&req.replacement,
        selection:req.selection.clone(), case_sensitive:req.case_sensitive, whole_word:req.whole_word,
    }, Duration::from_millis(500)).map_err(|e| match e {
        worker::Error::Empty => ui::Error::Empty,
        worker::Error::InvalidPattern => ui::Error::InvalidPattern,
        worker::Error::UnsupportedRange => ui::Error::UnsupportedRange,
        worker::Error::BudgetExceeded => ui::Error::Limit,
        worker::Error::TimedOut => ui::Error::Timeout,
        worker::Error::InvalidReply => ui::Error::InvalidReply,
        worker::Error::Unavailable => ui::Error::Unavailable,
    })?;
    let outcome = match result {
        worker::Outcome::Select(ranges) => ui::Outcome::Select(ranges),
        worker::Outcome::Edits { edits, following } => ui::Outcome::Edits {
            edits: edits.into_iter().map(|e| ui::Edit { range:e.range, insert:e.insert }).collect(), following
        },
    };
    outcome.check(req.operation, &req.document, req.selection.clone())?;
    Ok(outcome)
}
