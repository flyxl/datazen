//! The SQL text contract a driver owns.
//!
//! [`SqlDialect`] is deliberately narrow: identifier quoting, pagination
//! syntax, SQL literal rendering and the bound-parameter vocabulary are text
//! rules, not connection lifecycle. A consumer that only builds SQL can take
//! `&dyn SqlDialect` and never see [`crate::traits::DatabaseDriver`].
//!
//! Migration is one-way by construction. [`LegacySqlDialect`] borrows an
//! existing driver and forwards each rule to the old method, so the old trait
//! keeps the only implementation while call sites move. The new contract never
//! calls back into itself through the driver: a driver method's default body
//! either contains the rule or delegates to the shared builder, and the adapter
//! is only used by consumers of the new contract.

use crate::traits::DatabaseDriver;
use crate::types::{DriverError, PaginationSyntax, Value};

/// Where a statement's bound-parameter ceiling comes from.
///
/// The number matters for batch splitting; the *source* matters because it says
/// who owns the ceiling. A dialect that hard-codes a grammar limit (SQLite's
/// `SQLITE_MAX_VARIABLE_NUMBER`) cannot be raised by tuning a pool, while a
/// server- or implementation-imposed limit can move between versions. Callers
/// that only need the number use [`Self::limit`]; callers that report or
/// document the limit keep the distinction instead of flattening it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundParameterLimit {
    /// Fixed by the dialect's statement grammar, independent of any server.
    Dialect(usize),
    /// Imposed by the server or by this driver's binding implementation.
    Runtime(usize),
    /// Declared through the legacy [`DatabaseDriver`] API, which does not
    /// distinguish a grammar ceiling from a runtime one. Only
    /// [`LegacySqlDialect`] produces this value; a native dialect implementation
    /// states which of the other two it is.
    Declared(usize),
}

impl BoundParameterLimit {
    /// The ceiling as a plain count, for batch splitting.
    pub fn limit(self) -> usize {
        match self {
            Self::Dialect(limit) | Self::Runtime(limit) | Self::Declared(limit) => limit,
        }
    }

    /// `true` only when the ceiling is the dialect's own grammar.
    pub fn is_dialect_grammar(self) -> bool {
        matches!(self, Self::Dialect(_))
    }
}

/// The SQL text rules of one dialect.
///
/// Implementations must be pure: no method here may open a connection, acquire
/// a resource or depend on session state. That is what lets a SQL builder take
/// this trait without dragging the driver's resource lifecycle in, and it is
/// why `max_bound_parameters` is a declaration rather than a query against the
/// live server.
pub trait SqlDialect: Send + Sync {
    /// Quote one identifier — not a qualified name, and not a string literal.
    fn quote_ident(&self, name: &str) -> String;

    /// The pagination clause this dialect accepts after the ordering clause for
    /// at most `limit` rows starting at `offset`.
    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax;

    /// Render `value` as a literal this dialect accepts, or fail when the
    /// dialect has no injective rendering (or no literal grammar at all).
    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError>;

    /// Historical, infallible ANSI-style rendering, kept **only** for the
    /// deprecated `build_*_sql` string builders.
    ///
    /// New code must call [`Self::try_format_sql_literal`]. A driver that
    /// overrode the legacy `DatabaseDriver::format_sql_literal` keeps its
    /// rendering through [`LegacySqlDialect`], so migrating a consumer cannot
    /// change the SQL it emits. The default is the old crate-wide fallback; it
    /// exists so this method does not force every new dialect implementation to
    /// own a deprecated path.
    fn legacy_format_sql_literal(&self, value: &Option<Value>) -> String {
        crate::sql_builder::format_sql_literal_ansi(value)
    }

    /// Render the placeholder for bound parameter number `index` (1-based).
    /// `data_type` is the inspected target column type when known.
    fn parameter_placeholder(
        &self,
        index: usize,
        data_type: Option<&str>,
    ) -> Result<String, DriverError>;

    /// The maximum number of bound parameters one statement may carry, and
    /// where that ceiling comes from.
    fn max_bound_parameters(&self) -> BoundParameterLimit;
}

/// Borrowing adapter from an existing [`DatabaseDriver`] to [`SqlDialect`].
///
/// This is the migration seam, not a second source of truth: every method
/// forwards to the driver method of the same name, so a driver override keeps
/// working and there is exactly one implementation of each rule. Disappears
/// once each driver implements [`SqlDialect`] natively and every consumer holds
/// the narrow contract.
#[derive(Debug, Clone, Copy)]
pub struct LegacySqlDialect<'a, D: DatabaseDriver + ?Sized> {
    driver: &'a D,
}

impl<'a, D: DatabaseDriver + ?Sized> LegacySqlDialect<'a, D> {
    pub fn new(driver: &'a D) -> Self {
        Self { driver }
    }

    /// The wrapped driver, for callers that still need a driver-only rule
    /// (e.g. `skip_count_query`) at the boundary of a migrated consumer.
    pub fn driver(&self) -> &'a D {
        self.driver
    }
}

impl<D: DatabaseDriver + ?Sized> SqlDialect for LegacySqlDialect<'_, D> {
    fn quote_ident(&self, name: &str) -> String {
        self.driver.quote_ident(name)
    }

    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        self.driver.pagination_syntax(limit, offset)
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        self.driver.try_format_sql_literal(value)
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        self.driver.parameter_placeholder(index, data_type)
    }

    fn legacy_format_sql_literal(&self, value: &Option<Value>) -> String {
        self.driver.format_sql_literal(value)
    }

    fn max_bound_parameters(&self) -> BoundParameterLimit {
        // The legacy API reports a number only; claiming it is a grammar limit
        // or a runtime limit would invent provenance the driver never declared.
        BoundParameterLimit::Declared(self.driver.max_bound_parameters())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_driver::{MockDriver, MockDriverOptions};

    fn mock() -> std::sync::Arc<MockDriver> {
        MockDriver::new("sqlite", MockDriverOptions::default())
    }

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn adapter_is_object_safe_and_thread_safe() {
        assert_send_sync::<dyn SqlDialect>();
        let driver = mock();
        let dialect: &dyn SqlDialect = &LegacySqlDialect::new(driver.as_ref());
        assert_eq!(dialect.quote_ident("id"), "\"id\"");
    }

    #[test]
    fn adapter_forwards_text_rules_to_the_driver() {
        let driver = mock();
        let dialect = LegacySqlDialect::new(driver.as_ref());

        assert_eq!(dialect.quote_ident("a\"b"), "\"a\"\"b\"");

        let pagination = dialect.pagination_syntax(25, 50);
        assert_eq!(pagination.clause, "LIMIT 25 OFFSET 50");
        assert!(!pagination.requires_order_by);

        // SQLite's declared literal dialect comes through the driver method.
        let literal = dialect
            .try_format_sql_literal(&Some(Value::String("x".into())))
            .unwrap();
        assert_eq!(literal, "CAST(X'78' AS TEXT)");

        // A driver with no declared grammar still refuses; the adapter must not
        // invent an ANSI fallback on its behalf.
        let plain = MockDriver::new("redis", MockDriverOptions::default());
        let plain_dialect = LegacySqlDialect::new(plain.as_ref());
        assert!(matches!(
            plain_dialect.try_format_sql_literal(&Some(Value::String("x".into()))),
            Err(DriverError::Unsupported(_))
        ));
    }

    #[test]
    fn adapter_reports_legacy_limit_as_declared() {
        let driver = mock();
        let dialect = LegacySqlDialect::new(driver.as_ref());
        assert_eq!(
            dialect.max_bound_parameters(),
            BoundParameterLimit::Declared(driver.max_bound_parameters())
        );
        assert!(!dialect.max_bound_parameters().is_dialect_grammar());
    }

    #[test]
    fn adapter_forwards_parameter_placeholders() {
        let driver = MockDriver::new(
            "sqlite",
            MockDriverOptions {
                parameterized_writes: true,
                ..MockDriverOptions::default()
            },
        );
        let dialect = LegacySqlDialect::new(driver.as_ref());
        assert_eq!(dialect.parameter_placeholder(2, None).unwrap(), "?2");
    }
}
