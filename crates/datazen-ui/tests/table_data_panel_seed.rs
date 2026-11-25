//! T254: the panel-seed coder tests — the first-frame conditions of a
//! foreign-key jump (DB-07): both the live list and the drafts carry
//! the seed's conditions, the wide logic and the panel-open flag
//! default exactly as React does, the second clone gives the drafts a
//! life of their own, and the seeded state is the empty table state
//! with the five overrides and nothing else.

use datazen_ui::table_data_connection_state::{
    FilterCondition, FilterLogic, TableChangeContext,
};
use datazen_ui::table_data_panel_seed::{seed_panel, PanelSeed};
use serde_json::json;

// The context a foreign-key jump opens the tab with: a real session
// pinned to a concrete database, no schema (MySQL has none), the
// referenced table.
fn context() -> TableChangeContext {
    TableChangeContext {
        connection_id: Some("conn-1".to_owned()),
        db_session_id: "session-7".to_owned(),
        driver_type: Some("mysql".to_owned()),
        database: Some("shop".to_owned()),
        schema: None,
        table: "customers".to_owned(),
    }
}

fn cond(column: &str, operator: &str, value: serde_json::Value) -> FilterCondition {
    FilterCondition {
        column: column.to_owned(),
        operator: operator.to_owned(),
        value: Some(value),
        connector: None,
    }
}

// The seed a foreign-key jump on `customer.id` produces: "show me the
// rows of `orders` whose customer is this one", panel open so the
// condition stays visible in the filter bar.
fn fk_jump_seed() -> PanelSeed {
    PanelSeed {
        filters: vec![
            cond("customer_id", "eq", json!(42)),
            cond("status", "eq", json!("open")),
        ],
        filter_logic: Some(FilterLogic::And),
        open_filter_panel: Some(true),
    }
}

// ---------------------------------------------------------------------------
// React 54–65: the seeded state
// ---------------------------------------------------------------------------

#[test]
fn the_seed_carries_the_conditions_into_both_the_live_list_and_the_drafts() {
    // React 55/57–60: `cloneFilters(seed.filters)` lands in `filters`
    // and a second clone lands in `draftFilters` — the foreign-key
    // jump's tab opens with the condition already applied **and**
    // already sitting in the filter bar's editor.
    let state = seed_panel(context(), &fk_jump_seed());

    assert_eq!(state.filters.len(), 2);
    assert_eq!(state.filters[0].column, "customer_id");
    assert_eq!(state.filters[0].operator, "eq");
    assert_eq!(state.filters[0].value, Some(json!(42)));
    assert_eq!(state.filters[1].value, Some(json!("open")));
    // The drafts are content-equal clones (React 60: a separate
    // `cloneFilters` call, not a reference to the live list).
    assert_eq!(state.draft_filters, state.filters);
}

#[test]
fn an_absent_logic_defaults_to_and_on_both_fields() {
    // React 56/61–62: `seed.filterLogic ?? 'and'` — one default feeding
    // both the applied logic and the draft logic.
    let seed = PanelSeed { filters: fk_jump_seed().filters, ..Default::default() };
    let state = seed_panel(context(), &seed);

    assert_eq!(state.filter_logic, FilterLogic::And);
    assert_eq!(state.draft_filter_logic, FilterLogic::And);
}

#[test]
fn an_explicit_or_logic_reaches_the_applied_and_the_draft_field() {
    // React 61–62: the same `logic` value written twice — an OR jump
    // ("either of these two customers") must not degrade to AND in the
    // editor the user is about to look at.
    let mut seed = fk_jump_seed();
    seed.filter_logic = Some(FilterLogic::Or);
    let state = seed_panel(context(), &seed);

    assert_eq!(state.filter_logic, FilterLogic::Or);
    assert_eq!(state.draft_filter_logic, FilterLogic::Or);
}

#[test]
fn the_panel_defaults_to_open_and_an_explicit_false_is_kept() {
    // React 63: `seed.openFilterPanel ?? true` — the conditions stay
    // visible after a jump (that is the point of DB-07); an explicit
    // false is a deliberate choice and survives.
    let mut seed = fk_jump_seed();
    seed.open_filter_panel = None;
    assert_eq!(seed_panel(context(), &seed).filter_panel_open, true);

    seed.open_filter_panel = Some(false);
    assert_eq!(seed_panel(context(), &seed).filter_panel_open, false);
}

#[test]
fn the_second_clone_gives_the_drafts_a_life_of_their_own() {
    // React 58/60: `draftFilters: cloneFilters(filters)` — the second
    // clone exists so editing a draft in the filter bar can never leak
    // into the applied list the first fetch is about to carry. Owned
    // vectors make the aliasing impossible; this pins the observable
    // contract so it cannot regress into a shared buffer.
    let seed = fk_jump_seed();
    let mut state = seed_panel(context(), &seed);
    let before = state.filters.clone();

    state.draft_filters.clear();

    assert_eq!(state.filters, before, "clearing the drafts leaves the live list alone");
    assert!(state.draft_filters.is_empty());
    // And the source seed is never consumed by the seeding.
    assert_eq!(seed.filters.len(), 2);
}

#[test]
fn the_seed_is_the_empty_state_plus_the_overrides_and_nothing_else() {
    // React 57–58: `{ ...emptyTableState(context), filters, … }` — the
    // spread means every field the seed does not name is the empty
    // state's own default, and the context is the one passed in.
    let state = seed_panel(context(), &fk_jump_seed());

    assert_eq!(state.context, Some(context()));
    assert!(state.columns.is_empty());
    assert!(state.rows.is_empty());
    assert_eq!(state.total_rows, 0);
    assert_eq!(state.page, 0);
    assert_eq!(state.page_size, 50);
    assert!(state.sorts.is_empty());
    assert!(state.selected_rows.is_empty());
    assert!(state.pending_changes.is_empty());
    assert!(state.raw_where.is_none());
    assert!(state.detail_row_index.is_none());
    assert!(state.editing_cell.is_none());
}

#[test]
fn the_seed_never_bumps_the_revision_and_never_loads() {
    // React 43–45: `seedPanel` "never bumps `requestRevision` and never
    // reloads" — the one fetch is the panel's own first load. A seed
    // that bumped the revision (or set loading) would race that load
    // and fetch the table twice — the exact flash DB-07 exists to kill.
    let state = seed_panel(context(), &fk_jump_seed());

    assert_eq!(state.request_revision, 0);
    assert!(!state.loading);
    assert!(state.loading_revision.is_none());
    assert!(state.error.is_none());
}
