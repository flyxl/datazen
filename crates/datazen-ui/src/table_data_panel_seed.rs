//! T254: `stores/tableData/panelSeed.ts` ported to Rune (67 lines).
//!
//! The first-frame seed of a panel (DB-07): a foreign-key jump opens
//! a tab that has to show the referenced row **right away** — with
//! the condition visible in the filter bar, not after a second round
//! trip.
//!
//! ## Why `seedPanel` exists (the React comment, verbatim in force)
//!
//! The obvious way (`setFilters`) cannot work here:
//!
//! * `setFilters` patches the panel and reloads it immediately;
//! * `patchPanel` is a **no-op for a panel that has no slice yet** —
//!   and a freshly created tab has no slice;
//! * so "create the tab, then set the filters" silently does nothing,
//!   the tab loads the whole table, and the user sees a full table
//!   flash before the filtered one arrives.
//!
//! [`seed_panel`] therefore **creates** the slice ([`TableState`])
//! with the conditions already in it and does **not** trigger a load:
//! the panel's own first load (the one `TableView` starts anyway)
//! picks the conditions up and the table is fetched exactly once —
//! with the filter.
//!
//! ## The slice factory's shape
//!
//! React's `createPanelSeedSlice(write)` hands the store's write
//! callback in; the Rune port keeps the logic as the pure
//! [`seed_panel`] and the store wiring task calls it with its own
//! write — the same function with one less closure.
//!
//! | React field | Rune default (identical to `emptyTableState`) |
//! |---|---|
//! | `filters` | `clone_filters(seed.filters)` |
//! | `draftFilters` | `clone_filters(filters)` (a second clone, React's own) |
//! | `filterLogic` / `draftFilterLogic` | `seed.filterLogic ?? And` |
//! | `filterPanelOpen` | `seed.openFilterPanel ?? true` |
//! | everything else | [`empty_table_state`] field-for-field |
//!
//! `requestRevision` never bumps and no reload ever runs here — the
//! one fetch is the panel's own first load.

// The store wiring task (the `useTableDataStore` port) is the
// consumer; until then the tests are the only callers.
#![allow(dead_code)]

use crate::table_data_connection_state::{
    empty_table_state, FilterCondition, FilterLogic, TableChangeContext, TableState,
};
use crate::table_data_filter_utils::clone_filters;

/// React `PanelSeed`: the first-frame conditions a foreign-key jump
/// opens the tab with.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelSeed {
    /// The conditions the first fetch has to carry.
    pub filters: Vec<FilterCondition>,
    /// How the conditions combine; `and` is the default.
    pub filter_logic: Option<FilterLogic>,
    /// Whether the filter panel starts open (the conditions stay
    /// visible).
    pub open_filter_panel: Option<bool>,
}

/// React `seedPanel`: the first-frame state of a freshly opened panel
/// — the empty slice with the seed's conditions already applied, both
/// in the live filters and their drafts, the panel open so the
/// conditions stay visible.
///
/// Creates the state the store writes; it never bumps
/// `requestRevision` and never reloads (the state itself carries no
/// load — the panel's own first load is the one fetch).
pub(crate) fn seed_panel(context: TableChangeContext, seed: &PanelSeed) -> TableState {
    // React 52: the filters clone once for the live list…
    let filters = clone_filters(&seed.filters);
    // …and once again for the drafts (`cloneFilters(filters)`, React
    // 58 — a second clone, so the two lists can never alias).
    let draft_filters = clone_filters(&filters);
    let logic = seed.filter_logic.unwrap_or_default();

    let mut state = empty_table_state(Some(context));
    state.filters = filters;
    state.draft_filters = draft_filters;
    state.filter_logic = logic;
    state.draft_filter_logic = logic;
    state.filter_panel_open = seed.open_filter_panel.unwrap_or(true);
    state
}
