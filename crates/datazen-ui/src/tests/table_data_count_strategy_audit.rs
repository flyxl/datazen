//! Independent audit of T089.
//!
//! The boundary is strict `>`: a prior equal to the threshold is exact.
//! Two same-named tables: the first compatible schema wins, so a later
//! bigger table does not flip the strategy. A negative threshold still
//! counts exactly.

use crate::table_data_connection_state::CountStrategy;
use crate::table_data_count_strategy::{
    resolve_panel_count_strategy, CatalogTable, CountContext, CountSettings, RowCountMode,
};

fn table(name: &str, schema: Option<&str>, rows: f64) -> CatalogTable {
    CatalogTable {
        name: name.into(),
        schema: schema.map(str::to_owned),
        row_count: Some(rows),
    }
}

#[test]
fn audit_equal_threshold_is_exact_and_the_first_schema_wins() {
    let tables = vec![
        table("orders", Some("sales"), 200_000.0),
        table("orders", Some("sales"), 9_000_000.0),
    ];
    let ctx = CountContext {
        table: "orders".into(),
        schema: Some("sales".into()),
    };
    let settings = CountSettings {
        mode: Some(RowCountMode::Auto),
        threshold: Some(200_000.0),
    };
    assert_eq!(
        resolve_panel_count_strategy(&ctx, Some(&tables), &settings),
        Some(CountStrategy::Exact)
    );
    let settings = CountSettings {
        mode: Some(RowCountMode::Auto),
        threshold: Some(-5.0),
    };
    assert_eq!(
        resolve_panel_count_strategy(&ctx, Some(&tables), &settings),
        Some(CountStrategy::Exact)
    );
}

#[test]
fn audit_a_different_schema_does_not_match_and_estimated_ignores_the_prior() {
    let tables = vec![table("orders", Some("sales"), 1.0)];
    let ctx = CountContext {
        table: "orders".into(),
        schema: Some("audit".into()),
    };
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx,
            Some(&tables),
            &CountSettings {
                mode: Some(RowCountMode::Auto),
                threshold: Some(1.0),
            }
        ),
        None
    );
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx,
            Some(&tables),
            &CountSettings {
                mode: Some(RowCountMode::Estimated),
                threshold: Some(1.0),
            }
        ),
        Some(CountStrategy::Estimated)
    );
}
