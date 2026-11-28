//! Narrow parent/worker interface for Native SQL regular-expression commands.
//! Regex input is only matched in an independently killed, restricted process.
#![cfg(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64"))]

use std::ops::Range;
mod isolation;
mod protocol;
mod transport;
mod worker;

pub use transport::search;
pub use worker::worker_main;

/// Dispatch this exact argument before starting the application, rendering, or
/// opening user state. A normal launch never calls the child entry point.
pub const WORKER_ARG: &str = "--datazen-editor-regex-worker-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Operation {
    Next,
    Previous,
    SelectAll,
    ReplaceNext,
    ReplaceAll,
}

impl Operation {
    fn replaces(self) -> bool {
        matches!(self, Self::ReplaceNext | Self::ReplaceAll)
    }
}

#[derive(Clone, Debug)]
pub struct Request<'a> {
    pub operation: Operation,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Ordered physical edges of the main selection, in UTF-8 bytes.
    pub selection: Range<usize>,
    /// Immutable LF document. The caller owns identity and any projection.
    pub document: &'a str,
    pub pattern: &'a str,
    pub replacement: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub insert: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A navigation/selection result; an empty vector means no match.
    Select(Vec<Range<usize>>),
    /// Original-document coordinates, including the optional following match
    /// for ReplaceNext. Map through the same atomic transaction at application.
    Edits {
        edits: Vec<Edit>,
        following: Option<Range<usize>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    InvalidPattern,
    UnsupportedRange,
    BudgetExceeded,
    TimedOut,
    Unavailable,
    InvalidReply,
}
