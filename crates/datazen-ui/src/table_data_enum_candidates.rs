//! Enum-candidate cache transitions from
//! `stores/tableData/enumCandidates.ts` (T158, DB-05).
//!
//! The cache is **per panel and per column**, and it is filled only when
//! the user asks for it: opening an editor never queries anything. The
//! panel scope matters because the candidates belong to a table, and two
//! tabs on two tables must not see each other's values.
//!
//! The React `EnumCandidateStoreSlice` / `createEnumCandidateSlice` wiring
//! (the `patch` closure) is not ported: the Rune host owns the
//! [`TableState`] and calls these transitions directly, the same split the
//! sibling T158 module (`table_data_draft_inserts`) and the T089 / T127 /
//! T210 ports took.
//!
//! Deliberate difference: the transitions mutate instead of returning a
//! `Partial<TableState>` (no structural sharing to preserve in Rust);
//! `clear_enum_candidates` returns `bool` — `false` is React's empty
//! `Partial` on an already-empty cache (nothing changed).

#![allow(dead_code)] // wired by the table-data store track

use crate::table_data_connection_state::TableState;

/// Remember the candidates of one column (`setEnumCandidates`): the list
/// is copied into the panel, so the caller may reuse its buffer.
pub(crate) fn set_enum_candidates(ts: &mut TableState, column: &str, values: &[String]) {
    ts.enum_candidates
        .insert(column.to_owned(), values.to_vec());
}

/// Forget the candidates of the panel (`clearEnumCandidates` — page
/// change, table change). Returns whether anything was dropped: React
/// returns an empty `Partial` when the cache is already empty.
pub(crate) fn clear_enum_candidates(ts: &mut TableState) -> bool {
    if ts.enum_candidates.is_empty() {
        return false;
    }
    ts.enum_candidates.clear();
    true
}
