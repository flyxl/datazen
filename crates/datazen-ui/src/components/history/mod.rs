//! History UI components (query history entry card and filter bar).
//!
//! These components currently have no production consumer in the Rune app;
//! they are intended to be wired by the query-history dialog/view migration
//! track. Public(crate) re-exports are kept for that upcoming integration.

#[allow(unused_imports)] // consumers will be added by query-history view wiring
pub use history_entry_card::{HistoryEntryAction, HistoryEntryCard, HistoryEntryVm};
#[allow(unused_imports)] // consumers will be added by query-history view wiring
pub use history_filter_bar::{
    HistoryConnectionOption, HistoryFilterBar, HistoryFilterOpen, HistoryQueryState, HistoryRange,
    HistorySort,
};

pub mod global_query_history_dialog;
pub mod history_entry_card;
pub mod history_filter_bar;
// T099: the pure helpers behind the dialog (range cut, arg bag, status
// filter, truncation reports, .sql export).
pub mod history_query;
