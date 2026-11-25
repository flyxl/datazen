//! T233 (b): the keyset half of the table-data store — port of
//! `packages/frontend/src/stores/tableData/keysetActions.ts` (100 lines).
//!
//! Three things happen in that file, and none of them belongs in the
//! store itself:
//!
//! * **what a read carries** — the boundary of the page it reads, or
//!   nothing (`keysetRequestFor`);
//! * **what the answer does to the state** — the back end's word is the
//!   authority (`applyKeysetResult`);
//! * **what invalidates the boundaries** — a sort, a filter list or a
//!   page size that changed makes every boundary on the stack describe
//!   another question, so the stack is emptied (`invalidatedKeyset`).
//!
//! They are pure functions over the panel state, so they can be tested
//! without a store; the store only wires them.
//!
//! ## Deliberate boundaries (cross-segment)
//!
//! **`keysetRequestFor` and `applyKeysetResult` wait for T543**
//! (`lib/keysetPaging.ts`, segment 2): both fold through that lib's
//! functions — `nextRequestCursor`, `orderSignature`,
//! `filterSignature`, `applyPagingInfo`, `pushCursor`, `reanchor` — and
//! only its *state* mirror ([`KeysetPagingState`]) has landed here
//! (`table_data_connection_state.rs`). Carrying the lib's logic in this
//! segment would duplicate its 357-line task, so this module ports the
//! state-only third export now and the two cursor functions land with
//! the T543 rebase (the coordinator's cross-segment merge point). The
//! signatures and their semantics are pinned below for that hand-off.
//!
//! | React | here (after T543) |
//! |---|---|
//! | `KeysetRequest` (`afterKey?`, `reanchor`) | a struct with `after_key: Option<Vec<CursorValue>>` and `reanchor: bool` |
//! | `keysetRequestFor(state, page)` | `fn keyset_request_for(state: &TableState, page: usize) -> KeysetRequest` — `nextRequestCursor`'s plan decides (`fallbackToOffset` / no `afterKey` → an offset read) |
//! | `applyKeysetResult(state, res, sent)` | `fn apply_keyset_result(...) -> KeysetPagingState` — `applyPagingInfo`, then `reanchor` when `sent.reanchor`, then `pushCursor` |
//!
//! What is ported today:
//!
//! | React | here |
//! |---|---|
//! | `invalidatedKeyset(state)` | [`invalidated_keyset`] |

use crate::table_data_connection_state::KeysetPagingState;

/// The paging state of a panel whose sort, filters or page size changed
/// (React's `invalidatedKeyset`): every boundary on the stack was taken
/// under the old question, so none of them may be sent any more. The
/// strategy, the order and the drift flags survive — the back end
/// re-decides them on the next answer; only the boundaries and the
/// next-page boundary drop.
pub fn invalidated_keyset(state: &KeysetPagingState) -> KeysetPagingState {
    // React's spread `{...state, stack: [], anchorPage: 0, nextAfterKey: null}`.
    let mut next = state.clone();
    next.stack = Vec::new();
    next.anchor_page = 0;
    next.next_after_key = None;
    next
}
