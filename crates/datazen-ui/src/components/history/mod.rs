//! History UI components (query history entry card and filter bar).
//!
//! These components currently have no production consumer in the Rune app;
//! they are intended to be wired by the query-history dialog/view migration
//! track. Public(crate) re-exports are kept for that upcoming integration.

#[allow(unused_imports)] // consumers will be added by query-history view wiring
pub(crate) use history_entry_card::{HistoryEntryAction, HistoryEntryCard, HistoryEntryVm};
#[allow(unused_imports)] // consumers will be added by query-history view wiring
pub(crate) use history_filter_bar::{
    HistoryConnectionOption, HistoryFilterBar, HistoryFilterOpen, HistoryQueryState, HistoryRange,
    HistorySort,
};

pub(crate) mod history_entry_card;
pub(crate) mod history_filter_bar;
