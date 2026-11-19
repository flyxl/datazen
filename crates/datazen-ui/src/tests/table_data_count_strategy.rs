//! T089: `stores/tableData/countStrategy.ts`.

use crate::table_data_connection_state::CountStrategy;
use crate::table_data_count_strategy::{
    resolve_panel_count_strategy, CatalogTable, CountContext, CountSettings, RowCountMode,
    DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
};

fn table(name: &str, schema: Option<&str>, rows: Option<f64>) -> CatalogTable {
    CatalogTable {
        name: name.into(),
        schema: schema.map(str::to_owned),
        row_count: rows,
    }
}

fn ctx(table: &str, schema: Option<&str>) -> CountContext {
    CountContext {
        table: table.into(),
        schema: schema.map(str::to_owned),
    }
}

fn settings(mode: Option<RowCountMode>, threshold: Option<f64>) -> CountSettings {
    CountSettings { mode, threshold }
}

#[test]
fn catalog_match_is_first_compatible_schema_and_null_count_is_no_prior() {
    let tables = vec![
        table("users", Some("audit"), Some(10.0)),
        table("users", Some("public"), Some(40.0)),
        table("users", None, Some(7.0)),
        table("logs", Some("public"), None),
    ];
    let auto = settings(Some(RowCountMode::Auto), None);
    assert_eq!(
        resolve_panel_count_strategy(&ctx("users", Some("public")), Some(&tables), &auto),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx("users", None), Some(&tables), &auto),
        Some(CountStrategy::Exact),
        "a missing context schema matches the first same-named table"
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx("users", Some("")), Some(&tables), &auto),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx("missing", Some("public")), Some(&tables), &auto),
        None
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx("logs", Some("public")), Some(&tables), &auto),
        None,
        "null rowCount is no prior, so auto declines"
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx("users", Some("public")), None, &auto),
        None
    );
}

#[test]
fn modes_and_the_auto_threshold() {
    let tables = vec![table("t", Some("public"), Some(DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD))];
    let over = vec![table(
        "t",
        Some("public"),
        Some(DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD + 1.0),
    )];
    let ctx = ctx("t", Some("public"));
    assert_eq!(
        resolve_panel_count_strategy(&ctx, Some(&tables), &settings(Some(RowCountMode::None), None)),
        Some(CountStrategy::Unsupported)
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx, Some(&tables), &settings(Some(RowCountMode::Exact), None)),
        Some(CountStrategy::Exact)
    );
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx,
            Some(&tables),
            &settings(Some(RowCountMode::Estimated), None)
        ),
        Some(CountStrategy::Estimated)
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx, Some(&tables), &settings(None, None)),
        Some(CountStrategy::Exact),
        "equal to the default threshold is still exact"
    );
    assert_eq!(
        resolve_panel_count_strategy(&ctx, Some(&over), &settings(Some(RowCountMode::Auto), None)),
        Some(CountStrategy::Estimated)
    );
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx,
            Some(&over),
            &settings(Some(RowCountMode::Auto), Some(0.0))
        ),
        Some(CountStrategy::Exact),
        "threshold <= 0 always counts"
    );
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx,
            Some(&over),
            &settings(Some(RowCountMode::Auto), Some(f64::NAN))
        ),
        Some(CountStrategy::Exact)
    );
    let nan_rows = vec![table("t", Some("public"), Some(f64::NAN))];
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx,
            Some(&nan_rows),
            &settings(Some(RowCountMode::Auto), Some(10.0))
        ),
        None
    );
}

#[test]
fn an_info_schema_that_is_empty_matches_a_qualified_context() {
    let tables = vec![table("t", Some(""), Some(1.0))];
    assert_eq!(
        resolve_panel_count_strategy(
            &ctx("t", Some("public")),
            Some(&tables),
            &settings(Some(RowCountMode::Auto), Some(10.0))
        ),
        Some(CountStrategy::Exact)
    );
}
