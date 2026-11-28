//! One compute thread per live native formatter. Draft ownership and driver
//! admission remain in the adapter; this worker only accepts typed SQL text.
//! No queue, callback or returned error exposes draft text to logs.

#![cfg(feature = "native-sql-formatter")]

use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use crossbeam_queue::ArrayQueue;
use datazen_ui_api::{
    native_sql_format::{
        NativeSqlFormatDelivery, NativeSqlFormatError as Error, NATIVE_SQL_FORMAT_MAX_INPUT_BYTES,
        NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES,
    },
    SqlFormatRequest,
};

// The compute provider can be stricter than the public UI job envelope.
const MAX_INPUT: usize = if super::INPUT_LIMIT < NATIVE_SQL_FORMAT_MAX_INPUT_BYTES {
    super::INPUT_LIMIT
} else {
    NATIVE_SQL_FORMAT_MAX_INPUT_BYTES
};
const MAX_OUTPUT: usize = if super::OUTPUT_LIMIT < NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES {
    super::OUTPUT_LIMIT
} else {
    NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES
};
const MAX_OPTION_BYTES: usize = 32;

struct Work {
    serial: u64,
    generation: u64,
    sql: Box<str>,
    options: SqlFormatRequest,
}

struct Done {
    serial: u64,
    delivery: NativeSqlFormatDelivery,
}

struct Mailbox {
    // force_push replaces the older queued item; the formatter may also have
    // one owned job in flight. No lock protects either slot across compute.
    pending: ArrayQueue<Work>,
    ready: ArrayQueue<Done>,
    current: AtomicU64,
    closed: AtomicBool,
}

struct Cancellation<'a> {
    mailbox: &'a Mailbox,
    serial: u64,
}

impl Cancellation<'_> {
    fn is_cancelled(&self) -> bool {
        self.mailbox.current.load(Ordering::Acquire) != self.serial
    }
}

pub(crate) struct FormatterWorker {
    mailbox: Arc<Mailbox>,
    thread: JoinHandle<()>,
    next_serial: u64,
    // The app's generation is a delivery label; use a separate private serial
    // so a stale result can never become valid by reusing a request label.
    latest: Option<(u64, u64)>,
}

impl FormatterWorker {
    /// Launch at native adapter initialization, outside the frame loop. The
    /// dedicated thread can run only the application's fixed local provider.
    pub(crate) fn launch() -> io::Result<Self> {
        Self::launch_with(|sql, options, cancel| {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            // The provider enforces its own time/memory limits. Do not
            // promise forced interruption of in-flight engine code.
            super::format_native_sql(sql, options).map_err(provider_error)
        })
    }

    fn launch_with(
        format: impl Fn(&str, &SqlFormatRequest, &Cancellation<'_>) -> Result<String, Error>
            + Send
            + 'static,
    ) -> io::Result<Self> {
        let mailbox = Arc::new(Mailbox {
            pending: ArrayQueue::new(1),
            ready: ArrayQueue::new(1),
            current: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let worker = Arc::clone(&mailbox);
        let thread = thread::Builder::new()
            .name("datazen-native-sql-format".into())
            .spawn(move || run(worker, format))?;
        Ok(Self { mailbox, thread, next_serial: 0, latest: None })
    }

    /// Publish the latest accepted input. Never wait for compute, queue space,
    /// or delivery. The caller validates the session and selected draft.
    pub(crate) fn submit(
        &mut self,
        generation: u64,
        sql: String,
        options: SqlFormatRequest,
    ) -> Result<(), Error> {
        if self.thread.is_finished() {
            return Err(Error::Unavailable);
        }
        if sql.is_empty() {
            return Err(Error::Rejected);
        }
        if sql.len() > MAX_INPUT {
            return Err(Error::Limit);
        }
        let options = bounded_options(options)?;
        let serial = self.next_serial.checked_add(1).ok_or(Error::Unavailable)?;
        self.next_serial = serial;
        self.latest = Some((generation, serial));
        self.mailbox.current.store(serial, Ordering::Release);
        drop(self.mailbox.ready.pop());
        drop(self.mailbox.pending.force_push(Work {
            serial,
            generation,
            // Release excess capacity rather than retaining an oversized
            // original draft allocation in either the queue or the engine.
            sql: sql.into_boxed_str(),
            options,
        }));
        self.thread.thread().unpark();
        Ok(())
    }

    /// Invalidate only this exact caller generation, not its replacement.
    pub(crate) fn cancel(&mut self, generation: u64) {
        if self.latest.is_some_and(|(current, _)| current == generation) {
            self.clear();
        }
    }

    pub(crate) fn poll(&mut self) -> Option<NativeSqlFormatDelivery> {
        if let Some(done) = self.mailbox.ready.pop() {
            if self.latest == Some((done.delivery.generation, done.serial)) {
                self.clear();
                return Some(done.delivery);
            }
        }
        // An unexpectedly stopped thread cannot keep the latest UI
        // request pending indefinitely. Do not surface an unwind payload.
        if self.thread.is_finished() {
            let (generation, _) = self.latest?;
            self.clear();
            return Some(NativeSqlFormatDelivery { generation, result: Err(Error::Unavailable) });
        }
        None
    }

    fn clear(&mut self) {
        self.latest = None;
        self.mailbox.current.store(0, Ordering::Release);
        drop(self.mailbox.pending.pop());
        drop(self.mailbox.ready.pop());
    }
}

impl Drop for FormatterWorker {
    fn drop(&mut self) {
        self.mailbox.closed.store(true, Ordering::Release);
        self.clear();
        self.thread.thread().unpark();
        // Dropping JoinHandle detaches. An in-flight bounded formatter can
        // finish cleanup, but window shutdown never joins/waits for it.
    }
}

fn run(
    mailbox: Arc<Mailbox>,
    format: impl Fn(&str, &SqlFormatRequest, &Cancellation<'_>) -> Result<String, Error>,
) {
    while !mailbox.closed.load(Ordering::Acquire) {
        let Some(work) = mailbox.pending.pop() else {
            // unpark has a token, so publish between pop and park is not lost.
            thread::park();
            continue;
        };
        let cancel = Cancellation { mailbox: &mailbox, serial: work.serial };
        if cancel.is_cancelled() {
            continue;
        }
        let result = format(&work.sql, &work.options, &cancel).and_then(|formatted| {
            if formatted.len() > MAX_OUTPUT {
                Err(Error::Limit)
            } else {
                Ok(formatted.into_boxed_str().into_string())
            }
        });
        if !cancel.is_cancelled() && !mailbox.closed.load(Ordering::Acquire) {
            // A cancel may still race this final push, so poll also checks
            // the private serial. A newer completion replaces a stale one.
            drop(mailbox.ready.force_push(Done {
                serial: work.serial,
                delivery: NativeSqlFormatDelivery { generation: work.generation, result },
            }));
        }
    }
}

fn bounded_options(options: SqlFormatRequest) -> Result<SqlFormatRequest, Error> {
    fn compact(value: String) -> Result<String, Error> {
        if value.len() > MAX_OPTION_BYTES {
            return Err(Error::Limit);
        }
        Ok(value.into_boxed_str().into_string())
    }

    Ok(SqlFormatRequest {
        language: compact(options.language)?,
        keyword_case: compact(options.keyword_case)?,
        indent_style: compact(options.indent_style)?,
        logical_operator_newline: compact(options.logical_operator_newline)?,
        ..options
    })
}

fn provider_error(error: super::NativeSqlFormatError) -> Error {
    use super::NativeSqlFormatError::*;
    match error {
        InvalidOptions | Lexical | Unsupported => Error::Rejected,
        InputLimit | OutputLimit => Error::Limit,
        Deadline => Error::Timeout,
    }
}

#[cfg(test)]
mod tests;
