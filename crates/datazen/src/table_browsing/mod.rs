//! Table browsing vertical slice: the read path from browse requests to
//! filtered/paged SQL and driver result assembly (D4-c, moved from
//! `datazen::services::{query_executor, table_browsing,
//! table_paging}`).
//!
//! Read path only. The write path (ChangeSet / insert plan / row identity /
//! write exec) is the `table_editing` module (D4-d).

pub mod api;
pub mod browsing;
pub mod executor;
pub mod paging;

pub use browsing::*;
pub use executor::*;
pub use paging::*;
