//! T254 independent audit: fresh vectors, none shared with the coder
//! file. The angles here are the ones the coder file does not take:
//! the all-defaults seed (an empty condition list still opens the
//! panel — the seed's own default disagrees with `emptyTableState`,
//! and that disagreement is the contract), the per-condition connector
//! surviving the clone untouched, the `serde_json::Value` shapes
//! round-tripping through both lists, a whole-`TableState` equality
//! against `empty_table_state` plus exactly the five React overrides
//! (the spread, pinned field-for-field through `PartialEq`), seed
//! independence from one source object, and verbatim column/operator
//! strings.

use datazen_ui::table_data_connection_state::{
    empty_table_state, FilterCondition, FilterLogic, TableChangeContext,
};
use datazen_ui::table_data_panel_seed::{seed_panel, PanelSeed};
use serde_json::json;

// A deliberately different context from the coder file's: a
// schema-qualified Postgres target reached from a saved connection.
fn audit_context() -> TableChangeContext {
    TableChangeContext {
        connection_id: None,
        db_session_id: "sess-audit".to_owned(),
        driver_type: Some("postgres".to_owned()),
        database: Some("crm".to_owned()),
        schema: Some("sales".to_owned()),
        table: "invoices".to_owned(),
    }
}

fn raw(column: &str, operator: &str, value: Option<serde_json::Value>) -> FilterCondition {
    FilterCondition {
        column: column.to_owned(),
        operator: operator.to_owned(),
        value,
        connector: None,
    }
}

#[test]
fn a_default_seed_still_opens_the_filter_panel() {
    // React 63 with every field absent: `openFilterPanel ?? true`.
    // `emptyTableState` itself ships `filterPanelOpen: false` — the
    // seed's own default is the **opposite**, and that is the only sane
    // default for a jump (the condition must stay visible). This pins
    // the disagreement so neither side can silently converge.
    let state = seed_panel(audit_context(), &PanelSeed::default());

    assert_eq!(state.filter_panel_open, true);
    assert!(state.filters.is_empty());
    assert!(state.draft_filters.is_empty());
    assert_eq!(state.filter_logic, FilterLogic::And);
    assert_eq!(state.draft_filter_logic, FilterLogic::And);
    // The empty seed must not invent conditions either.
    assert_eq!(state.filters, state.draft_filters);
}

#[test]
fn each_condition_keeps_its_own_connector_through_the_clone() {
    // React 55/58/60: `cloneFilters` copies each condition whole — the
    // per-condition `connector` joins one condition to the one before
    // it and is **not** the request-wide `filterLogic`. A seed whose
    // wide logic is OR may still carry AND-joined condition pairs, and
    // the clone must preserve that mix in both lists untouched.
    let seed = PanelSeed {
        filters: vec![
            FilterCondition {
                column: "region".to_owned(),
                operator: "eq".to_owned(),
                value: Some(json!("emea")),
                connector: None,
            },
            FilterCondition {
                column: "amount".to_owned(),
                operator: "gte".to_owned(),
                value: Some(json!(1000)),
                connector: Some(FilterLogic::And),
            },
            FilterCondition {
                column: "amount".to_owned(),
                operator: "lte".to_owned(),
                value: Some(json!(5000)),
                connector: Some(FilterLogic::Or),
            },
        ],
        filter_logic: Some(FilterLogic::Or),
        open_filter_panel: Some(false),
    };
    let state = seed_panel(audit_context(), &seed);

    for list in [&state.filters, &state.draft_filters] {
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].connector, None);
        assert_eq!(list[1].connector, Some(FilterLogic::And));
        assert_eq!(list[2].connector, Some(FilterLogic::Or));
    }
    // The wide logic is the seed's, not the connectors' majority.
    assert_eq!(state.filter_logic, FilterLogic::Or);
    assert_eq!(state.draft_filter_logic, FilterLogic::Or);
}

#[test]
fn the_values_round_trip_unchanged_through_both_lists() {
    // The condition value is a `serde_json::Value` — the wire shape the
    // back end receives. Numbers, strings, booleans, arrays (set
    // operators), and the value-less (`isEmpty`) hole must all survive
    // the double clone into both lists.
    let seed = PanelSeed {
        filters: vec![
            raw("id", "eq", Some(json!(42))),
            raw("code", "eq", Some(json!("INV-009"))),
            raw("paid", "eq", Some(json!(true))),
            raw("tags", "in", Some(json!(["urgent", "review"]))),
            raw("memo", "isEmpty", None),
        ],
        filter_logic: None,
        open_filter_panel: None,
    };
    let state = seed_panel(audit_context(), &seed);

    for list in [&state.filters, &state.draft_filters] {
        assert_eq!(list[0].value, Some(json!(42)));
        assert_eq!(list[1].value, Some(json!("INV-009")));
        assert_eq!(list[2].value, Some(json!(true)));
        assert_eq!(list[3].value, Some(json!(["urgent", "review"])));
        assert_eq!(list[4].value, None, "the value-less operator stays value-less");
        assert_eq!(list[4].operator, "isEmpty");
    }
}

#[test]
fn the_seeded_state_equals_the_empty_state_with_only_the_five_overrides() {
    // React 57–63 is a spread of `emptyTableState(context)` with five
    // fields overridden. `TableState: PartialEq` lets the audit pin the
    // spread as a whole: any field the port sets outside the five (a
    // stray revision bump, a page reset, a loading flip) fails this
    // single equality.
    let ctx = audit_context();
    let seed = PanelSeed {
        filters: vec![raw("customer_id", "eq", Some(json!(7)))],
        filter_logic: Some(FilterLogic::Or),
        open_filter_panel: Some(false),
    };
    let seeded = seed_panel(ctx.clone(), &seed);

    let mut expected = empty_table_state(Some(ctx));
    expected.filters = seed.filters.clone();
    expected.draft_filters = seed.filters.clone();
    expected.filter_logic = FilterLogic::Or;
    expected.draft_filter_logic = FilterLogic::Or;
    expected.filter_panel_open = false;

    assert_eq!(seeded, expected);
}

#[test]
fn two_seeds_from_one_source_are_equal_and_independent() {
    // React's `cloneFilters` exists because the React objects share by
    // reference. In Rune the source `PanelSeed` is borrowed and the
    // states are owned: seeding twice from one seed yields two equal,
    // fully independent states, and tampering with one (or with the
    // source afterwards) cannot bleed into the other.
    let seed = PanelSeed {
        filters: vec![raw("a", "eq", Some(json!(1))), raw("b", "eq", Some(json!(2)))],
        filter_logic: Some(FilterLogic::And),
        open_filter_panel: Some(true),
    };
    let first = seed_panel(audit_context(), &seed);
    let mut second = seed_panel(audit_context(), &seed);

    assert_eq!(first, second);

    second.filters.clear();
    second.draft_filters.push(raw("c", "eq", Some(json!(3))));
    assert_eq!(first.filters.len(), 2, "the first seed's live list is untouched");
    assert_eq!(first.draft_filters.len(), 2);
    assert_eq!(seed.filters.len(), 2, "the borrowed source is untouched");
}

#[test]
fn awkward_columns_and_operators_arrive_verbatim() {
    // Nothing in the seed path is allowed to normalize, trim, or
    // re-case the strings: columns with spaces and capitals, an
    // operator id that reads like a logic word, a column that reads
    // like an operator — all verbatim in both lists.
    let seed = PanelSeed {
        filters: vec![
            raw("Customer Name", "eq", Some(json!("李 雷"))),
            raw("or", "or", Some(json!(0))),
            raw("between (a,b)", "between", Some(json!([1, 9]))),
        ],
        filter_logic: None,
        open_filter_panel: Some(true),
    };
    let state = seed_panel(audit_context(), &seed);

    for list in [&state.filters, &state.draft_filters] {
        assert_eq!(list[0].column, "Customer Name");
        assert_eq!(list[0].value, Some(json!("李 雷")));
        assert_eq!(list[1].column, "or");
        assert_eq!(list[1].operator, "or");
        assert_eq!(list[2].column, "between (a,b)");
        assert_eq!(list[2].operator, "between");
    }
}
