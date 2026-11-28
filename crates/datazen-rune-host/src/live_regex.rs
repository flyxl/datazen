//! One bounded latest-request slot and one supervising thread. Regexp compile,
//! search and expansion run only in a newly isolated child, never on Rune.
use std::{sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}}, thread, time::Duration};
use crossbeam_queue::ArrayQueue;
use datazen_ui_api::native_query_regex as ui;
use datazen_regex_worker as worker;

pub(super) trait Job: Send + 'static {
    type Output: Send + 'static;
    fn id(&self) -> u64;
    fn check_request(&self) -> Result<(),ui::Error>;
    fn execute(&self) -> Result<Self::Output,ui::Error>;
}
impl Job for ui::Request {
    type Output = ui::Outcome;
    fn id(&self)->u64 { self.id }
    fn check_request(&self)->Result<(),ui::Error> { self.check() }
    fn execute(&self)->Result<Self::Output,ui::Error> { run(self) }
}
impl Job for ui::MarkRequest {
    type Output = Vec<std::ops::Range<usize>>;
    fn id(&self)->u64 { self.id }
    fn check_request(&self)->Result<(),ui::Error> { self.check() }
    fn execute(&self)->Result<Self::Output,ui::Error> {
        let hits = worker::highlight(worker::HighlightRequest {
            viewport:self.viewport.clone(), document:&self.document, pattern:&self.pattern,
            case_sensitive:self.case_sensitive, whole_word:self.whole_word,
        }, Duration::from_millis(500)).map_err(engine_error)?;
        ui::check_marks(&self.document,&hits)?;
        Ok(hits)
    }
}

type Answer<J> = (u64, Result<<J as Job>::Output,ui::Error>);
struct Mailbox<J: Job> {
    next: ArrayQueue<J>,
    done: ArrayQueue<Answer<J>>,
    current: AtomicU64,
    stopped: AtomicBool,
}
pub(super) struct Supervisor<J: Job = ui::Request> {
    inner: Arc<Mailbox<J>>,
    runner: thread::JoinHandle<()>,
    last_id: u64,
}
impl<J:Job> Supervisor<J> {
    pub(super) fn new(name: &'static str) -> Option<Self> {
        let inner = Arc::new(Mailbox::<J> { next: ArrayQueue::new(1), done: ArrayQueue::new(1), current: AtomicU64::new(0), stopped: AtomicBool::new(false) });
        let box_ = Arc::clone(&inner);
        let runner = thread::Builder::new().name(name.into()).spawn(move || {
            while !box_.stopped.load(Ordering::Acquire) {
                let Some(job) = box_.next.pop() else { thread::park(); continue };
                if box_.current.load(Ordering::Acquire) != job.id() { continue; }
                let result = job.execute();
                if box_.current.load(Ordering::Acquire) == job.id() {
                    drop(box_.done.force_push((job.id(),result)));
                }
            }
        }).ok()?;
        Some(Self { inner, runner, last_id: 0 })
    }
    pub(super) fn available(&self) -> bool { !self.runner.is_finished() }
    pub(super) fn submit(&mut self, req: J) -> Result<(), ui::Error> {
        if !self.available() { return Err(ui::Error::Unavailable); }
        req.check_request()?;
        if req.id() <= self.last_id { return Err(ui::Error::InvalidReply); }
        self.last_id = req.id();
        self.inner.current.store(req.id(), Ordering::Release);
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
    pub(super) fn poll(&mut self) -> Option<Answer<J>> {
        let current = self.inner.current.load(Ordering::Acquire);
        if current == 0 { return None; }
        if let Some(done) = self.inner.done.pop().filter(|done| done.0 == current) {
            self.cancel(current);
            return Some(done);
        }
        if !self.available() {
            self.cancel(current);
            return Some((current,Err(ui::Error::Unavailable)));
        }
        None
    }
}
impl<J:Job> Drop for Supervisor<J> {
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
    }, Duration::from_millis(500)).map_err(engine_error)?;
    let outcome = match result {
        worker::Outcome::Select(ranges) => ui::Outcome::Select(ranges),
        worker::Outcome::Edits { edits, following } => ui::Outcome::Edits {
            edits: edits.into_iter().map(|e| ui::Edit { range:e.range, insert:e.insert }).collect(), following
        },
    };
    outcome.check(req.operation, &req.document, req.selection.clone())?;
    Ok(outcome)
}

fn engine_error(e:worker::Error)->ui::Error { match e {
        worker::Error::Empty => ui::Error::Empty,
        worker::Error::InvalidPattern => ui::Error::InvalidPattern,
        worker::Error::UnsupportedPattern => ui::Error::UnsupportedPattern,
        worker::Error::UnsupportedRange => ui::Error::UnsupportedRange,
        worker::Error::BudgetExceeded => ui::Error::Limit,
        worker::Error::TimedOut => ui::Error::Timeout,
        worker::Error::InvalidReply => ui::Error::InvalidReply,
        worker::Error::Unavailable => ui::Error::Unavailable,
} }
