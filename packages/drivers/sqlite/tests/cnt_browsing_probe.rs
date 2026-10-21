//! Probe: the browsing contract as the host sees it for SQLite.
//!
//! SQLite is a driver that changed nothing, so its answer must be exactly
//! what it was before the count strategy and the default order existed: an
//! exact count, no estimate, the real key (or the first column) as the order.

use datazen_driver_api::{ColumnSchema, CountStrategy, DatabaseDriver};

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
fn an_unchanged_driver_keeps_the_behaviour_it_had() {
    let driver = datazen_driver_sqlite::SqliteDriver::new();
    assert!(!driver.skip_count_query());
    assert_eq!(driver.count_strategy(), CountStrategy::Exact);
    assert!(driver.count_estimate_sql("app", None, "orders").is_none());

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
