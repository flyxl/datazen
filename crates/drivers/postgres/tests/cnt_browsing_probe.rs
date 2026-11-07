//! Probe: the browsing contract as the host sees it for PostgreSQL.
//!
//! PostgreSQL is the one tree-in driver that declares something of its own
//! (`CountStrategy::Estimated`), and the driver the host holds for an alias
//! (questdb / cloudberry) is the `ReuseDriver` wrapper the factory hands out
//! — so this probe covers both the declaration and the wrapper's forwarding
//! of it.

use std::sync::Arc;

use datazen_driver_api::{ColumnSchema, CountStrategy, DatabaseDriver, ReuseDriver};

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

#[test]
fn the_postgres_driver_declares_the_estimate_strategy() {
    let driver = datazen_driver_postgres::PostgresDriver::new();
    assert_eq!(driver.count_strategy(), CountStrategy::Estimated);
    assert!(driver
        .count_estimate_sql("app", Some("public"), "orders")
        .is_some());
    // The default order is still the trait's own: the real key wins, and a
    // table without one falls back to its first column.
    let columns = vec![column("id", true), column("name", false)];
    assert_eq!(
        driver.default_order_columns(&columns, &["id".to_string()]),
        vec!["id".to_string()]
    );
    assert_eq!(
        driver.default_order_columns(&columns, &[]),
        vec!["id".to_string()]
    );
    assert!(driver.allow_first_column_fallback_order());
}

/// The wrapper the alias factories hand out must carry the driver's own
/// declaration; a wrapper that fell back to the trait default would report
/// `Exact` and the grid would ask the database for a `COUNT(*)` it never
/// wanted.
#[test]
fn the_wrapped_postgres_driver_still_declares_the_estimate_strategy() {
    let inner: Arc<dyn DatabaseDriver> = Arc::new(datazen_driver_postgres::PostgresDriver::new());
    let wrapper = ReuseDriver::new_with_precise_cancel(inner.clone(), "questdb", true);
    assert_eq!(wrapper.count_strategy(), CountStrategy::Estimated);
    assert_eq!(
        wrapper.count_estimate_sql("app", Some("public"), "orders"),
        inner.count_estimate_sql("app", Some("public"), "orders")
    );
    let columns = vec![column("id", true)];
    assert_eq!(
        wrapper.default_order_columns(&columns, &["id".to_string()]),
        inner.default_order_columns(&columns, &["id".to_string()])
    );
    assert_eq!(
        wrapper.allow_first_column_fallback_order(),
        inner.allow_first_column_fallback_order()
    );
}
