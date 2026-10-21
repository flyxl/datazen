//! Independent verification of the browsing contract (count strategy and
//! default order).
//!
//! The contract lives in the driver API; the *decisions* live in the shared
//! layer (`services::table_browsing`). These tests are written against the
//! public surface only, so they check what a host — desktop today, web later
//! — actually gets, not the internals of one caller.
//!
//! Three things are pinned here:
//!
//! * a driver that writes nothing keeps the behaviour it had (the
//!   compatibility argument of the whole contract);
//! * the six combinations of "what the response says about the row count"
//!   stay distinguishable — in particular "the caller reused the total" is
//!   **not** "this database cannot count";
//! * the disagreement between the two sources of the primary key is really
//!   reported, with the table name and both sources in the message.

use std::sync::{Arc, Mutex};

use datazen_driver_api::filters::FilterOperator;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ColumnSchema, CountStrategy, DatabaseDriver, Value};
use datazen_feature_core::services::table_browsing;

fn column(name: &str, is_primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: "text".to_string(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key,
        is_auto_increment: false,
    }
}

fn mock(options: MockDriverOptions) -> Arc<MockDriver> {
    MockDriver::new("sqlite", options)
}

/* ------------------------------------------------------------------ */
/* A driver that writes nothing.                                        */
/* ------------------------------------------------------------------ */

#[test]
fn a_driver_that_writes_nothing_keeps_the_behaviour_it_had() {
    let driver = mock(MockDriverOptions::default());

    // It counted exactly, as it always did.
    assert!(!driver.skip_count_query());
    assert_eq!(driver.count_strategy(), CountStrategy::Exact);

    // It has no estimate, so the host never guesses.
    assert!(driver.count_estimate_sql("app", None, "orders").is_none());
    assert!(driver
        .count_estimate_sql("app", Some("public"), "orders")
        .is_none());

    // The order is the real key, or the first column when there is none.
    let columns = vec![column("id", true), column("name", false)];
    assert_eq!(
        driver.default_order_columns(&columns, &["id".to_string()]),
        vec!["id".to_string()]
    );
    assert_eq!(
        driver.default_order_columns(&columns, &[]),
        vec!["id".to_string()]
    );
    assert!(driver.default_order_columns(&[], &[]).is_empty());

    // A table without a primary key is still ordered by its first column.
    assert!(driver.allow_first_column_fallback_order());
}

#[test]
fn a_driver_that_skips_the_count_says_so() {
    let driver = mock(MockDriverOptions {
        skip_count_query: true,
        ..Default::default()
    });
    assert_eq!(driver.count_strategy(), CountStrategy::Unsupported);
}

/* ------------------------------------------------------------------ */
/* The six combinations.                                                */
/* ------------------------------------------------------------------ */

fn complete_filter() -> datazen_feature_core::services::query_executor::FilterCondition {
    datazen_feature_core::services::query_executor::FilterCondition {
        column: "id".into(),
        operator: FilterOperator::Eq,
        value: Value::Integer(1),
        connector: None,
    }
}

#[test]
fn the_six_combinations_stay_distinguishable() {
    // 1. The caller reuses the total it already has: the answer says nothing
    //    about the count.
    let plain = mock(MockDriverOptions::default());
    assert_eq!(
        table_browsing::resolve_count_strategy(
            plain.as_ref(),
            None,
            true,
            false,
            "app",
            None,
            "orders"
        ),
        CountStrategy::Unsupported,
        "the decision layer answers 'no count'; the response carries no kind at all"
    );

    // 2. The driver declares that it cannot count: the same decision, but the
    //    response says "unsupported" — a different fact from "reuse".
    let blind = mock(MockDriverOptions {
        skip_count_query: true,
        ..Default::default()
    });
    assert_eq!(
        table_browsing::resolve_count_strategy(
            blind.as_ref(),
            None,
            false,
            false,
            "app",
            None,
            "orders"
        ),
        CountStrategy::Unsupported
    );

    // 3. The default: an exact count.
    assert_eq!(
        table_browsing::resolve_count_strategy(
            plain.as_ref(),
            None,
            false,
            false,
            "app",
            None,
            "orders"
        ),
        CountStrategy::Exact
    );

    // 4. An estimate with a statement.
    let estimating = mock(MockDriverOptions {
        count_strategy: Some(CountStrategy::Estimated),
        count_estimate_rows: Some(4242),
        ..Default::default()
    });
    assert_eq!(
        table_browsing::resolve_count_strategy(
            estimating.as_ref(),
            None,
            false,
            false,
            "app",
            None,
            "orders"
        ),
        CountStrategy::Estimated
    );

    // 5. An estimate without a statement degrades to an exact count.
    let promising = mock(MockDriverOptions {
        count_strategy: Some(CountStrategy::Estimated),
        ..Default::default()
    });
    assert_eq!(
        table_browsing::resolve_count_strategy(
            promising.as_ref(),
            None,
            false,
            false,
            "app",
            None,
            "orders"
        ),
        CountStrategy::Exact
    );

    // 6. An estimate cannot answer "how many rows match this filter".
    assert_eq!(
        table_browsing::resolve_count_strategy(
            estimating.as_ref(),
            None,
            false,
            true,
            "app",
            None,
            "orders"
        ),
        CountStrategy::Exact
    );
}

#[test]
fn a_driver_that_cannot_count_is_never_overruled() {
    let blind = mock(MockDriverOptions {
        skip_count_query: true,
        ..Default::default()
    });
    for requested in [
        CountStrategy::Exact,
        CountStrategy::Estimated,
        CountStrategy::Unsupported,
    ] {
        assert_eq!(
            table_browsing::resolve_count_strategy(
                blind.as_ref(),
                Some(requested),
                false,
                false,
                "app",
                None,
                "orders"
            ),
            CountStrategy::Unsupported,
            "{requested:?} must not overrule a driver that says it cannot count"
        );
    }
}

#[test]
fn the_filters_follow_the_statement_builder() {
    assert!(!table_browsing::has_effective_filters(None));
    assert!(!table_browsing::has_effective_filters(Some(&[])));
    assert!(table_browsing::has_effective_filters(Some(&[
        complete_filter()
    ])));
}

/* ------------------------------------------------------------------ */
/* The disagreement between the two sources of the primary key.         */
/* ------------------------------------------------------------------ */

/// A subscriber that keeps every field of every event it sees.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

struct FieldVisitor(Arc<Mutex<Vec<String>>>);

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .lock()
            .expect("the capture is not poisoned")
            .push(format!("{}={:?}", field.name(), value));
    }
}

impl tracing::Subscriber for Capture {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::Id {
        tracing::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::Id, _follows: &tracing::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut visitor = FieldVisitor(Arc::clone(&self.0));
        event.record(&mut visitor);
    }
    fn enter(&self, _span: &tracing::Id) {}
    fn exit(&self, _span: &tracing::Id) {}
}

#[test]
fn the_disagreement_is_reported_with_the_table_and_both_sources() {
    let capture = Capture::default();
    // The flags name one column and the normalised list names another: the
    // two sources disagree about the same fact.
    let columns = vec![column("id", true)];
    let primary_keys = vec!["other".to_string()];

    {
        let _guard = tracing::subscriber::with_default(capture.clone(), || {
            table_browsing::report_pk_source_disagreement("orders", &columns, &primary_keys);
        });
    }

    let events = capture
        .0
        .lock()
        .expect("the capture is not poisoned")
        .clone();
    assert!(
        !events.is_empty(),
        "the disagreement must be reported, not swallowed"
    );
    let joined = events.join("\n");
    assert!(
        joined.contains("orders"),
        "the table name is missing: {joined}"
    );
    assert!(
        joined.contains("column_flags"),
        "the column flags are missing: {joined}"
    );
    assert!(
        joined.contains("primary_keys"),
        "the normalised primary keys are missing: {joined}"
    );
    assert!(
        joined.contains("\"id\""),
        "the flags are not in the message: {joined}"
    );
    assert!(
        joined.contains("\"other\""),
        "the keys are not in the message: {joined}"
    );
}

#[test]
fn a_table_whose_two_sources_agree_is_not_reported() {
    let capture = Capture::default();
    let columns = vec![column("id", true)];
    let primary_keys = vec!["id".to_string()];

    {
        let _guard = tracing::subscriber::with_default(capture.clone(), || {
            table_browsing::report_pk_source_disagreement("orders", &columns, &primary_keys);
        });
    }

    let events = capture
        .0
        .lock()
        .expect("the capture is not poisoned")
        .clone();
    assert!(
        events.is_empty(),
        "a table whose sources agree must not produce a report: {events:?}"
    );
}
