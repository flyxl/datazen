//! Generic SQL dump helpers used by [`crate::DatabaseDriver`] defaults.
//!
//! Does **not** emit `CREATE DATABASE` — drivers that support that option
//! prepend their dialect preamble in `dump_database` before calling
//! [`dump_sql_database`].

mod dump;
mod parser;
mod restore;

#[cfg(test)]
mod tests;

pub use crate::sql_split::{
    find_dollar_tag, is_comment_only_or_empty, split_sql_statements, SqlStatementScanner,
    Utf8ChunkDecoder,
};

pub use dump::*;
pub use parser::created_relation_ident;
pub use restore::*;
