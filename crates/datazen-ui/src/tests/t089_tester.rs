//! Independent T089 retest. Rules are copied from
//! `lib/rowCountStrategy.ts` and `stores/tableData/countStrategy.ts`,
//! with numbers that are not the audit fixtures.

use crate::table_data_connection_state::CountStrategy;
use crate::table_data_count_strategy::{
    catalog_row_count, resolve_count_strategy, resolve_panel_count_strategy, CatalogTable,
    CountContext, CountSettings, RowCountMode, DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
    DEFAULT_ROW_COUNT_MODE,
};

fn ctx(table: &str, schema: Option<&str>) -> CountContext {
    CountContext {
        table: table.to_owned(),
        schema: schema.map(str::to_owned),
    }
}

fn table(name: &str, schema: Option<&str>, row_count: Option<f64>) -> CatalogTable {
    CatalogTable {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        row_count,
    }
}

#[test]
fn defaults_are_auto_and_two_hundred_thousand() {
    assert_eq!(DEFAULT_ROW_COUNT_MODE, RowCountMode::Auto);
    assert_eq!(DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD, 200_000.0);
}

#[test]
fn mode_mapping_matches_row_count_strategy() {
    assert_eq!(
        resolve_count_strategy(RowCountMode::None, 10.0, Some(1.0)),
        Some(CountStrategy::Unsupported)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Exact, f64::NAN, None),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Estimated, 1.0, Some(0.0)),
        Some(CountStrategy::Estimated)
    );

    assert_eq!(resolve_count_strategy(RowCountMode::Auto, 80.0, None), None);
    assert_eq!(resolve_count_strategy(RowCountMode::Auto, 80.0, Some(f64::NAN)), None);
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 80.0, Some(f64::INFINITY)),
        None
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, f64::NAN, Some(5.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, f64::INFINITY, Some(5.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 0.0, Some(5.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, -3.5, Some(9_999.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 100.0, Some(100.0)),
        Some(CountStrategy::Exact),
        "equal to the threshold is still an exact count"
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 100.0, Some(100.5)),
        Some(CountStrategy::Estimated)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 10.0, Some(-1.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 200_000.0, Some(200_000.0)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_count_strategy(RowCountMode::Auto, 200_000.0, Some(200_001.0)),
        Some(CountStrategy::Estimated)
    );
}

#[test]
fn catalog_prior_uses_the_first_schema_compatible_table_and_settings_null_falls_back() {
    let tables = [
        table("orders", Some("other"), Some(9.0)),
        table("orders", Some(""), Some(40.0)),
        table("orders", Some("app"), Some(500_000.0)),
        table("orders", None, Some(1.0)),
    ];
    // context schema "app": "other" mismatches, "" is falsy so it matches first.
    assert_eq!(
        catalog_row_count(&ctx("orders", Some("app")), Some(&tables)),
        Some(40.0)
    );
    assert_eq!(
        catalog_row_count(&ctx("orders", Some("")), Some(&tables)),
        Some(9.0),
        "a missing context schema matches the first name"
    );
    assert_eq!(catalog_row_count(&ctx("missing", None), Some(&tables)), None);
    assert_eq!(catalog_row_count(&ctx("orders", None), None), None);

    let zero = [table("t", None, Some(0.0))];
    assert_eq!(catalog_row_count(&ctx("t", None), Some(&zero)), Some(0.0));
    let absent = [table("t", None, None)];
    assert_eq!(catalog_row_count(&ctx("t", None), Some(&absent)), None);

    let settings = CountSettings {
        mode: None,
        threshold: None,
    };
    assert_eq!(
        resolve_panel_count_strategy(&ctx("orders", Some("app")), Some(&tables), &settings),
        Some(CountStrategy::Exact),
        "prior 40 is under the default 200_000 threshold"
    );
    let big = [table("orders", Some("warehouse"), Some(200_001.0))];
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx("orders", Some("warehouse")),
            Some(&big),
            &CountSettings {
                mode: Some(RowCountMode::Auto),
                threshold: None,
            }
        ),
        Some(CountStrategy::Estimated)
    );
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx("orders", None),
            Some(&tables),
            &CountSettings {
                mode: Some(RowCountMode::None),
                threshold: Some(1.0),
            }
        ),
        Some(CountStrategy::Unsupported)
    );
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx("nope", None),
            Some(&tables),
            &CountSettings {
                mode: Some(RowCountMode::Auto),
                threshold: Some(1.0),
            }
        ),
        None
    );
}
