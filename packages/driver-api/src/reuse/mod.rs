//! Reuse drivers — thin wrappers that present an existing driver
//! under a different `DatabaseType` id.
//!
//! Used for engines that speak a compatible wire protocol:
//! - MySQL protocol: Doris, StarRocks, ManticoreSearch, OceanBase (MySQL mode)
//! - PostgreSQL wire: QuestDB, Cloudberry
//!
//! **Invariant: every `DatabaseDriver` method is decided here.** A method the
//! wrapper does not mention inherits a default body written for a generic
//! driver, so driver-specific rules (`has_multi_database`, `get_schemas`,
//! `qualify_sql_target`, `default_host`/`default_port`, the migration
//! renderers, target-qualified streaming) silently disappear while the product
//! metadata keeps advertising them. `forwarding_tests.rs` fails the build when
//! the trait grows and the forwarding tables do not, so the omission cannot
//! return. Only `driver_type` is deliberately *not* forwarded: the alias id is
//! the reason the wrapper exists. `test_connection` and the precise-cancel
//! methods override rather than forward, because they gate the alias' own
//! contract.
//!
//! The forwarding itself lives in [`driver_impl`]: one table, one impl block,
//! because the rule "the wrapper decides on every driver method" is only
//! checkable when the table is read as one. The behavioural tests live in
//! `tests.rs` and the source-text guard in `forwarding_tests.rs` — three
//! different concerns, three files.

use crate::*;
use std::sync::Arc;

pub struct ReuseDriver {
    inner: Arc<dyn DatabaseDriver>,
    db_type: DatabaseType,
    precise_query_execution_cancel: bool,
}

impl ReuseDriver {
    pub fn new(inner: Arc<dyn DatabaseDriver>, db_type: &str) -> Self {
        Self::new_with_precise_cancel(inner, db_type, true)
    }

    /// Build a compatibility wrapper with an explicit precise-cancel
    /// capability. The capability can only be enabled when the wrapped driver
    /// actually implements the execution-handle protocol.
    pub fn new_with_precise_cancel(
        inner: Arc<dyn DatabaseDriver>,
        db_type: &str,
        enabled: bool,
    ) -> Self {
        let precise_query_execution_cancel = enabled && inner.supports_query_execution_cancel();
        Self {
            inner,
            db_type: db_type.to_string(),
            precise_query_execution_cancel,
        }
    }
}

mod driver_impl;

#[cfg(test)]
mod tests;

/// Enforces "the reuse wrapper decides on every driver method" in CI rather
/// than in review. Lives beside the wrapper because it reads `traits.rs` and
/// the forwarding tables as text.
#[cfg(test)]
#[path = "forwarding_tests.rs"]
mod forwarding_tests;
