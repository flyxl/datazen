/**
 * The keyset half of the table-data store.
 *
 * Three things happen here, and none of them belongs in the store itself:
 *
 * * **what a read carries.** The boundary of the page it reads, or nothing.
 *   The boundary is never computed from a page number — it is what the back
 *   end reported for the previous page, sent back verbatim.
 * * **what the answer does to the state.** The back end's word is the
 *   authority: the mode, the order and the reason come from it.
 * * **what invalidates the boundaries.** A sort, a filter list or a page
 *   size that changed makes every boundary on the stack describe another
 *   question, so the stack is emptied.
 *
 * They are pure functions over the panel state, so they can be tested
 * without a store, and the store only wires them.
 */

import {
  applyPagingInfo,
  filterSignature,
  nextRequestCursor,
  orderSignature,
  pushCursor,
  reanchor,
  type KeysetPagingState,
} from '../../lib/keysetPaging';
import type { CursorValue, TableDataResult } from '../../types';
import type { TableState } from './types';

/** What a read carries: the boundary of the page, or nothing. */
export interface KeysetRequest {
  /** The boundary to send, or `undefined` for a read by offset. */
  afterKey?: CursorValue[];
  /**
   * The read starts over at the target page.
   *
   * A page that was never visited has no boundary, so it is read by offset
   * and the stack is re-anchored there.
   */
  reanchor: boolean;
}

/** The boundary of the page the panel is about to read. */
export function keysetRequestFor(state: TableState, page: number): KeysetRequest {
  const plan = nextRequestCursor(
    state.keyset,
    page,
    orderSignature(state.sorts),
    filterSignature(state.filters, state.filterLogic),
  );
  if (plan.fallbackToOffset || !plan.afterKey) {
    return { reanchor: plan.reanchor };
  }
  return { afterKey: plan.afterKey, reanchor: plan.reanchor };
}

/**
 * Fold the answer of a read into the paging state.
 *
 * `sent` is the request that produced the answer: the boundary it carried is
 * the boundary of the page that was read, and it is what the stack records.
 */
export function applyKeysetResult(
  state: TableState,
  res: TableDataResult,
  sent: KeysetRequest,
): KeysetPagingState {
  const orderSig = orderSignature(state.sorts);
  const filterSig = filterSignature(state.filters, state.filterLogic);

  let keyset = applyPagingInfo(state.keyset, res.paging);
  if (sent.reanchor) {
    keyset = reanchor(keyset, res.page, orderSig, filterSig);
  }
  const pushed = pushCursor(
    keyset,
    res.page,
    sent.afterKey ?? null,
    res.paging?.nextAfterKey ?? null,
    orderSig,
    filterSig,
  );
  return pushed.state;
}

/**
 * The paging state of a panel whose sort, filters or page size changed.
 *
 * Every boundary on the stack was taken under the old question, so none of
 * them may be sent any more.
 */
export function invalidatedKeyset(state: KeysetPagingState): KeysetPagingState {
  return {
    ...state,
    stack: [],
    anchorPage: 0,
    nextAfterKey: null,
  };
}
